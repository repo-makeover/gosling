//! Durable tool inspection, dispatch, hook, and terminal-result recording.
//!
//! Maintainers: preserve begin/replay/in-doubt/complete ordering and hook fences here.
//! Clients: tool request ids, errors, and result streams remain stable.

use super::*;

impl Agent {
    pub async fn dispatch_app_tool_call(
        &self,
        session_id: &str,
        tool_call: CallToolRequestParams,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ErrorData> {
        let request_id = format!("app_tool_{}", Uuid::new_v4().simple());
        let request = ToolRequest {
            id: request_id.clone(),
            tool_call: Ok(tool_call.clone()),
            metadata: None,
            tool_meta: None,
        };
        let requests = vec![request];
        let gosling_mode = self.gosling_mode().await;
        let inspection_results = self
            .tool_inspection_manager
            .inspect_tools(session_id, &requests, &[], gosling_mode)
            .await
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;

        let permission_result = self
            .tool_inspection_manager
            .process_inspection_results_with_permission_inspector(&requests, &inspection_results)
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Tool permission inspector is unavailable".to_string(),
                    None,
                )
            })?;

        if let Some(denied) = permission_result.denied.first() {
            let tool_name = denied
                .tool_call
                .as_ref()
                .map(|call| call.name.to_string())
                .unwrap_or_else(|_| "tool".to_string());
            return Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!("Tool `{tool_name}` is denied by current permissions"),
                None,
            ));
        }

        if let Some(needs_approval) = permission_result.needs_approval.first() {
            let tool_name = needs_approval
                .tool_call
                .as_ref()
                .map(|call| call.name.to_string())
                .unwrap_or_else(|_| "tool".to_string());
            return Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!("Tool `{tool_name}` requires approval before app clients can call it"),
                None,
            ));
        }

        if permission_result.approved.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                "Tool call was not approved by current permissions".to_string(),
                None,
            ));
        }

        let session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
        let (_, result) = self
            .dispatch_tool_call(tool_call, request_id, Some(cancellation_token), &session)
            .await;
        result
    }

    /// Dispatch a single tool call to the appropriate client
    #[instrument(skip(self, tool_call, request_id, cancellation_token, session), fields(input, output, session.id = %session.id))]
    pub async fn dispatch_tool_call(
        &self,
        tool_call: CallToolRequestParams,
        request_id: String,
        cancellation_token: Option<CancellationToken>,
        session: &Session,
    ) -> (String, Result<ToolCallResult, ErrorData>) {
        self.dispatch_tool_call_scoped(tool_call, request_id, cancellation_token, session, false)
            .await
    }

    pub(crate) async fn dispatch_conversation_tool_call(
        &self,
        tool_call: CallToolRequestParams,
        request_id: String,
        cancellation_token: Option<CancellationToken>,
        session: &Session,
    ) -> (String, Result<ToolCallResult, ErrorData>) {
        self.dispatch_tool_call_scoped(tool_call, request_id, cancellation_token, session, true)
            .await
    }

    async fn dispatch_tool_call_scoped(
        &self,
        tool_call: CallToolRequestParams,
        request_id: String,
        cancellation_token: Option<CancellationToken>,
        session: &Session,
        conversation_bound: bool,
    ) -> (String, Result<ToolCallResult, ErrorData>) {
        let input_summary = serde_json::json!({
            "tool": tool_call.name,
            "arguments": tool_call.arguments,
        });
        tracing::Span::current().record("input", tracing::field::display(&input_summary));

        let operation_id = match self
            .config
            .session_manager
            .begin_tool_operation(&session.id, &request_id, &tool_call, conversation_bound)
            .await
        {
            Ok(ToolOperationStart::Execute { operation_id }) => operation_id,
            Ok(ToolOperationStart::Replay { result, .. }) => {
                return (request_id, Ok(ToolCallResult::from(result)));
            }
            Ok(ToolOperationStart::InDoubt { operation_id }) => {
                return (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        "Tool execution was already durably started and its status is in doubt; Gosling will not dispatch it again automatically.".to_string(),
                        Some(serde_json::json!({
                            "tool_operation_id": operation_id,
                            "status": "in_doubt",
                            "retryable": false
                        })),
                    )),
                );
            }
            Err(error) => {
                return (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Could not durably begin tool operation: {error}"),
                        None,
                    )),
                );
            }
        };
        let mut operation_guard =
            ToolOperationGuard::new(self.config.session_manager.clone(), operation_id.clone());

        if self
            .hook_manager
            .has_hooks(crate::hooks::HookEvent::PreToolUse)
        {
            let ctx =
                crate::hooks::HookContext::new(crate::hooks::HookEvent::PreToolUse, &session.id)
                    .with_tool(
                        tool_call.name.to_string(),
                        tool_call
                            .arguments
                            .as_ref()
                            .map(|a| serde_json::Value::Object(a.clone())),
                    )
                    .with_working_dir(session.working_dir.to_string_lossy().to_string());
            if let crate::hooks::HookDecision::Deny { reason, plugin } = self
                .hook_manager
                .emit_blocking(crate::hooks::HookEvent::PreToolUse, ctx)
                .await
            {
                let denial = ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!(
                        "Tool call denied by policy hook `{plugin}`: {reason}. \
                         Do not retry; this is a policy denial, not a transient failure."
                    ),
                    None,
                );
                if let Err(error) = self
                    .config
                    .session_manager
                    .complete_tool_operation(&operation_id, &Err(denial.clone()))
                    .await
                {
                    return (
                        request_id,
                        Err(ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Could not durably complete denied tool operation: {error}"),
                            None,
                        )),
                    );
                }
                operation_guard.disarm();
                return (request_id, Err(denial));
            }
        }

        self.subdirectory_hint_tracker
            .lock()
            .await
            .record_tool_arguments(&tool_call.arguments, &session.working_dir);

        let tool_input_for_extended = tool_call
            .arguments
            .as_ref()
            .map(|a| serde_json::Value::Object(a.clone()));
        self.emit_pre_tool_extended_hooks(
            &tool_call.name,
            tool_input_for_extended.as_ref(),
            session,
        )
        .await;

        let output_capture = self
            .config
            .session_manager
            .prepare_output_capture(session, &tool_call, &request_id)
            .await;

        let ctx = crate::agents::tool_execution::ToolCallContext::new(
            session.id.clone(),
            Some(session.working_dir.clone()),
            Some(request_id.clone()),
        )
        .with_tool_operation_id(operation_id.clone());

        debug!("WAITING_TOOL_START: {}", tool_call.name);
        let result: ToolCallResult = if self.is_frontend_tool(&tool_call.name).await {
            ToolCallResult::from(Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Frontend tool execution required".to_string(),
                None,
            )))
        } else {
            let result = self
                .extension_manager
                .dispatch_tool_call(
                    &ctx,
                    tool_call.clone(),
                    cancellation_token.unwrap_or_default(),
                )
                .await;
            result.unwrap_or_else(|e| {
                #[cfg(feature = "telemetry")]
                crate::posthog::emit_error(
                    "tool_execution_failed",
                    &format!("{}: {}", tool_call.name, e),
                );
                let error_data = e.downcast::<ErrorData>().unwrap_or_else(|e| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                });
                ToolCallResult::from(Err(error_data))
            })
        };

        debug!("WAITING_TOOL_END: {}", tool_call.name);

        let result = self.with_post_tool_hook(result, &tool_call, session);
        let session_manager = self.config.session_manager.clone();
        let ToolCallResult {
            result,
            notification_stream,
            action_required_stream,
        } = result;
        let durable_result = async move {
            let mut terminal_result = result.await;
            if let Ok(output) = terminal_result.as_mut() {
                if output.is_error != Some(true) {
                    let captured = match output_capture {
                        Ok(Some(capture)) => {
                            session_manager.finish_output_capture(capture, output).await
                        }
                        Ok(None) => Ok(()),
                        Err(error) => Err(error),
                    };
                    if let Err(error) = captured {
                        output.content.push(Content::text(format!("The tool completed, but output history could not be fully recorded: {error}")));
                    }
                }
            }
            session_manager
                .complete_tool_operation(&operation_id, &terminal_result)
                .await
                .map_err(|error| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!(
                            "Tool finished but its terminal result could not be durably recorded: {error}. Its status is in doubt and it must not be retried automatically."
                        ),
                        None,
                    )
                })?;
            operation_guard.disarm();
            terminal_result
        };

        (
            request_id,
            Ok(ToolCallResult {
                result: Box::new(durable_result.boxed()),
                notification_stream,
                action_required_stream,
            }),
        )
    }
}
