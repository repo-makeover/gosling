//! Reply preparation, conversation repair, tool categorization, and permission routing.
//!
//! Maintainers: preserve inspection order and provider-input preparation here.
//! Clients: tool approval classification and reply context remain stable.

use super::*;

impl Agent {
    /// Create a tool inspection manager with default inspectors
    pub(super) fn create_tool_inspection_manager(
        permission_manager: Arc<PermissionManager>,
        provider: SharedProvider,
        session_manager: Arc<SessionManager>,
    ) -> ToolInspectionManager {
        let mut tool_inspection_manager = ToolInspectionManager::new();

        // Add security inspector (highest priority - runs first)
        tool_inspection_manager.add_inspector(Box::new(SecurityInspector::new()));
        tool_inspection_manager
            .add_inspector(Box::new(EgressInspector::new(permission_manager.clone())));

        // Add adversary inspector (LLM-based review, enabled by ~/.config/gosling/adversary.md)
        tool_inspection_manager.add_inspector(Box::new(AdversaryInspector::new(
            provider.clone(),
            session_manager.clone(),
        )));

        // Add permission inspector (medium-high priority)
        tool_inspection_manager.add_inspector(Box::new(PermissionInspector::new(
            permission_manager,
            provider,
            session_manager.clone(),
        )));

        // Opt-in, off by default: flags out-of-scope paths when a session has
        // "restrict tools to working directories" turned on.
        tool_inspection_manager
            .add_inspector(Box::new(WorkingDirScopeInspector::new(session_manager)));

        tool_inspection_manager.add_inspector(Box::new(RepetitionInspector::new(Some(3))));

        tool_inspection_manager
    }

    pub(super) async fn load_project_instructions(&self, session: &Session) -> Option<String> {
        let project_id = session.project_id.as_deref()?;
        let entry = crate::sources::read_project(project_id).ok()?;
        let mut parts = Vec::new();
        parts.push(format!("# Project: {}", entry.name));
        if !entry.description.is_empty() {
            parts.push(entry.description.clone());
        }
        if !entry.content.is_empty() {
            parts.push(entry.content.clone());
        }
        Some(parts.join("\n\n"))
    }

    pub(super) async fn prepare_reply_context(
        &self,
        session_id: &str,
        unfixed_conversation: Conversation,
        working_dir: &std::path::Path,
        additional_working_dirs: &[std::path::PathBuf],
    ) -> Result<ReplyContext> {
        // Only clone the pre-fix conversation when the debug-fix log can
        // actually fire: this clone previously ran unconditionally on every
        // turn to feed a debug!() line that's usually filtered out, doubling
        // as the same full-conversation-clone-for-nothing shape already
        // fixed once under MEM-GSL-006.
        let debug_fix_enabled = tracing::enabled!(tracing::Level::DEBUG);
        let unfixed_messages = debug_fix_enabled.then(|| unfixed_conversation.messages().clone());
        let (conversation, issues) = fix_conversation(unfixed_conversation);
        if !issues.is_empty() {
            if let Some(unfixed_messages) = unfixed_messages {
                debug!(
                    "Conversation issue fixed: {}",
                    debug_conversation_fix(
                        unfixed_messages.as_slice(),
                        conversation.messages(),
                        &issues
                    )
                );
            }
        }

        let (tools, toolshim_tools, system_prompt, model_config) = self
            .prepare_tools_and_prompt_with_additional_dirs(
                session_id,
                working_dir,
                additional_working_dirs,
            )
            .await?;

        let gosling_mode = *self.current_gosling_mode.lock().await;

        if gosling_mode == GoslingMode::SmartApprove {
            self.tool_inspection_manager.apply_tool_annotations(&tools);
        }

        let tool_call_cut_off = match Config::global()
            .get_param::<usize>("GOSLING_TOOL_CALL_CUTOFF")
        {
            Ok(v) => v,
            Err(_) => {
                let context_limit = match self.provider().await {
                    Ok(provider) => provider
                        .get_context_limit(&model_config)
                        .await
                        .unwrap_or_else(|_| model_config.context_limit()),
                    Err(_) => gosling_providers::model::DEFAULT_CONTEXT_LIMIT,
                };
                let compaction_threshold = Config::global()
                    .get_param::<f64>("GOSLING_AUTO_COMPACT_THRESHOLD")
                    .unwrap_or(crate::context_mgmt::DEFAULT_COMPACTION_THRESHOLD);
                crate::context_mgmt::compute_tool_call_cutoff(context_limit, compaction_threshold)
            }
        };

        Ok(ReplyContext {
            conversation,
            tools,
            toolshim_tools,
            system_prompt,
            gosling_mode,
            tool_call_cut_off,
            model_config,
        })
    }

    pub(super) async fn categorize_tools(
        &self,
        response: &Message,
        tools: &[rmcp::model::Tool],
        suppress_replayed_thinking: bool,
    ) -> ToolCategorizeResult {
        // Categorize tool requests
        let (frontend_requests, remaining_requests, filtered_response) = self
            .categorize_tool_requests(response, tools, suppress_replayed_thinking)
            .await;

        ToolCategorizeResult {
            frontend_requests,
            remaining_requests,
            filtered_response,
        }
    }

    pub(super) async fn handle_approved_and_denied_tools(
        &self,
        permission_check_result: &PermissionCheckResult,
        inspection_results: &[crate::tool_inspection::InspectionResult],
        request_to_response_map: &mut HashMap<String, Message>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        session: &Session,
    ) -> Result<Vec<(String, ToolStream)>> {
        let mut tool_futures: Vec<(String, ToolStream)> = Vec::new();

        // Handle pre-approved and read-only tools
        for request in &permission_check_result.approved {
            if let Ok(tool_call) = request.tool_call.clone() {
                let (req_id, tool_result) = self
                    .dispatch_conversation_tool_call(
                        tool_call,
                        request.id.clone(),
                        cancel_token.clone(),
                        session,
                    )
                    .await;

                tool_futures.push((
                    req_id,
                    match tool_result {
                        Ok(result) => tool_stream(
                            result
                                .notification_stream
                                .unwrap_or_else(|| Box::new(stream::empty())),
                            result
                                .action_required_stream
                                .unwrap_or_else(|| Box::new(stream::empty())),
                            result.result,
                        ),
                        Err(e) => tool_stream(
                            Box::new(stream::empty()),
                            Box::new(stream::empty()),
                            futures::future::ready(Err(e)),
                        ),
                    },
                ));
            }
        }

        Self::handle_denied_tools(
            permission_check_result,
            inspection_results,
            request_to_response_map,
        );
        Ok(tool_futures)
    }

    fn handle_denied_tools(
        permission_check_result: &PermissionCheckResult,
        inspection_results: &[crate::tool_inspection::InspectionResult],
        request_to_response_map: &mut HashMap<String, Message>,
    ) {
        for request in &permission_check_result.denied {
            if let Some(response) = request_to_response_map.get_mut(&request.id) {
                response.add_tool_response_with_metadata(
                    request.id.clone(),
                    Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                        inspection_results
                            .iter()
                            .find(|result| {
                                result.tool_request_id == request.id
                                    && matches!(
                                        result.action,
                                        crate::tool_inspection::InspectionAction::Deny
                                    )
                            })
                            .map(|result| format!("Tool denied by policy: {}", result.reason))
                            .unwrap_or_else(|| "Tool denied by current permissions.".into()),
                    )])),
                    request.metadata.as_ref(),
                );
            }
        }
    }

    pub(super) fn record_chat_mode_tool_skip(
        request: &ToolRequest,
        request_to_response_map: &mut HashMap<String, Message>,
    ) {
        if request.tool_call.is_err() {
            return;
        }
        if let Some(response) = request_to_response_map.get_mut(&request.id) {
            response.add_tool_response_with_metadata(
                request.id.clone(),
                Ok(CallToolResult::error(vec![Content::text(
                    CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                )])),
                request.metadata.as_ref(),
            );
        }
    }

    /// Subagents run in `GoslingMode::Auto` with nothing that can ever answer
    /// an approval prompt (`get_agent_messages` does not forward
    /// `ActionRequired` to the parent). A tool call an inspector still flags
    /// as `RequireApproval` even after Auto mode's default downgrade (i.e. a
    /// fail-closed inspector such as security/egress/adversary) must
    /// therefore be answered as denied here rather than left to hang forever
    /// on an unanswerable confirmation channel.
    pub(super) fn redirect_unapprovable_subagent_requests(
        gosling_mode: GoslingMode,
        session_type: SessionType,
        permission_check_result: &mut PermissionCheckResult,
        request_to_response_map: &mut HashMap<String, Message>,
    ) {
        if gosling_mode != GoslingMode::Auto || session_type != SessionType::SubAgent {
            return;
        }
        for request in permission_check_result.needs_approval.drain(..) {
            if let Some(response) = request_to_response_map.get_mut(&request.id) {
                response.add_tool_response_with_metadata(
                    request.id.clone(),
                    Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                        SUBAGENT_APPROVAL_UNAVAILABLE_RESPONSE,
                    )])),
                    request.metadata.as_ref(),
                );
            }
        }
    }
}
