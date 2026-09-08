//! ACP live message, tool lifecycle, elicitation, and permission event projection.
//!
//! Maintainers: preserve event ordering and permission response handling in this seam.
//! Clients: message chunks, tool updates, and confirmation choices remain stable.

use super::*;

impl GoslingAcpAgent {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_message_content(
        &self,
        content_item: &MessageContent,
        session_id: &SessionId,
        session_id_str: &str,
        message_id: Option<&str>,
        message_created: i64,
        role: &Role,
        steer: bool,
        agent: &Arc<Agent>,
        session: &mut GoslingAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), agent_client_protocol::Error> {
        match content_item {
            MessageContent::Text(text) => {
                let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(
                    presentation::project_live_text(&text.text, "Message text"),
                )))
                .meta(message_update_meta(message_id, message_created, steer));
                let update = match role {
                    Role::User => SessionUpdate::UserMessageChunk(chunk),
                    Role::Assistant => SessionUpdate::AgentMessageChunk(chunk),
                };
                cx.send_notification(SessionNotification::new(session_id.clone(), update))?;
            }
            MessageContent::ToolRequest(tool_request) => {
                self.handle_tool_request(
                    tool_request,
                    session_id,
                    session_id_str,
                    message_id,
                    session,
                    cx,
                )
                .await?;
            }
            MessageContent::ToolResponse(tool_response) => {
                self.handle_tool_response(
                    tool_response,
                    session_id,
                    session_id_str,
                    message_id,
                    session,
                    cx,
                )
                .await?;
            }
            MessageContent::Thinking(thinking) => {
                cx.send_notification(SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentThoughtChunk(
                        ContentChunk::new(ContentBlock::Text(TextContent::new(
                            presentation::project_live_text(&thinking.thinking, "Thinking content"),
                        )))
                        .meta(message_update_meta(
                            message_id,
                            message_created,
                            steer,
                        )),
                    ),
                ))?;
            }
            MessageContent::ActionRequired(action_required) => match &action_required.data {
                ActionRequiredData::ToolConfirmation {
                    id,
                    tool_name,
                    arguments,
                    prompt,
                    domain,
                } => {
                    self.handle_tool_permission_request(
                        cx,
                        agent,
                        session_id,
                        id.clone(),
                        tool_name.clone(),
                        arguments.clone(),
                        prompt.clone(),
                        domain.clone(),
                    )?;
                }
                ActionRequiredData::Elicitation {
                    id,
                    message,
                    requested_schema,
                } => {
                    self.handle_form_elicitation(
                        cx,
                        session_id,
                        id,
                        message,
                        requested_schema,
                        message_update_meta(message_id, message_created, false),
                    )
                    .await?;
                }
                ActionRequiredData::ElicitationResponse { .. } => {}
            },
            MessageContent::SystemNotification(notification) => {
                send_status_message_update(
                    cx,
                    self.supports_gosling_custom_notifications(),
                    session_id.0.as_ref(),
                    notification,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_tool_request(
        &self,
        tool_request: &crate::conversation::message::ToolRequest,
        session_id: &SessionId,
        session_id_for_persist: &str,
        message_id: Option<&str>,
        session: &mut GoslingAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), agent_client_protocol::Error> {
        session
            .tool_requests
            .insert(tool_request.id.clone(), tool_request.clone());

        let pending_tool_call = pending_tool_call_from_request(tool_request);
        let initial_tool_call = pending_tool_call
            .tool_call
            .meta(pending_tool_call.identity_meta.clone());
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCall(initial_tool_call),
        ))?;

        if Config::global()
            .get_gosling_disable_tool_call_summary()
            .unwrap_or(false)
        {
            return Ok(());
        }

        if let Ok(tool_call) = &tool_request.tool_call {
            let agent = session.agent.clone();
            let sid = session_id.clone();
            let request_id = tool_request.id.clone();
            let cx = cx.clone();
            let name = tool_call.name.to_string();
            let identity_meta = pending_tool_call.identity_meta.clone();
            let fallback_title = pending_tool_call.fallback_title.clone();
            let session_id_for_persist = session_id_for_persist.to_string();
            let message_id_for_persist = message_id.map(|s| s.to_string());
            let session_manager = self.session_manager.clone();
            let args_json = tool_call
                .arguments
                .as_ref()
                .map(|a| {
                    let s = serde_json::to_string(a).unwrap_or_default();
                    if s.len() > 300 {
                        format!("{}…", crate::utils::safe_truncate(&s, 300))
                    } else {
                        s
                    }
                })
                .unwrap_or_default();

            tokio::spawn(async move {
                let (title, from_llm) = match agent.provider().await {
                    Ok(provider) => {
                        if provider.manages_own_context() {
                            return;
                        }

                        let system =
                            "Summarize this tool call in a short lowercase phrase (3-8 words). \
                             No punctuation. No quotes. Examples: reading project configuration, \
                             checking network connectivity, listing files in src directory";
                        let user_text = format!("Tool: {name}\nArguments: {args_json}");
                        let message = Message::user().with_text(&user_text);
                        let model_config = match agent.model_config_for_session(&sid.0).await {
                            Ok(config) => config,
                            Err(_) => return,
                        };
                        let fast_model_config = match crate::model_config::get_fast_model(
                            provider.get_name(),
                            &model_config,
                        )
                        .await
                        {
                            Ok(config) => config,
                            Err(_) => return,
                        };
                        // The fast model occasionally returns an empty response
                        // under load (rate limiting, transient network). One
                        // retry with a short backoff is enough to recover the
                        // common cases without paying for the regular model.
                        let mut llm_outcome: Option<String> = None;
                        for attempt in 0..2 {
                            match crate::session_context::with_session_id(
                                Some(sid.0.to_string()),
                                provider.complete(
                                    &fast_model_config,
                                    system,
                                    std::slice::from_ref(&message),
                                    &[],
                                ),
                            )
                            .await
                            {
                                Ok((response, _)) => {
                                    let summary: String = response
                                        .content
                                        .iter()
                                        .filter_map(|c: &MessageContent| c.as_text())
                                        .collect::<String>()
                                        .trim()
                                        .to_string();
                                    if !summary.is_empty() {
                                        llm_outcome = Some(summary);
                                        break;
                                    }
                                    if attempt == 0 {
                                        warn!(
                                            "tool call summary: fast_complete returned empty for {request_id} ({name}), retrying once",
                                        );
                                        tokio::time::sleep(std::time::Duration::from_millis(150))
                                            .await;
                                    }
                                }
                                Err(e) => {
                                    if attempt == 0 {
                                        warn!(
                                            "tool call summary: fast_complete errored for {request_id} ({name}): {e}, retrying once",
                                        );
                                        tokio::time::sleep(std::time::Duration::from_millis(150))
                                            .await;
                                    } else {
                                        warn!(
                                            "tool call summary: fast_complete errored for {request_id} ({name}) after retry: {e}",
                                        );
                                    }
                                }
                            }
                        }
                        match llm_outcome {
                            Some(summary) => (summary, true),
                            None => {
                                warn!(
                                    "tool call summary: falling back to deterministic title for {request_id} ({name}) — replay will not show an LLM summary for this call",
                                );
                                (fallback_title.clone(), false)
                            }
                        }
                    }
                    Err(e) => {
                        warn!("tool call summary: failed to get provider: {e}");
                        (fallback_title.clone(), false)
                    }
                };

                let title = presentation::project_tool_title(&title);
                let fields = ToolCallUpdateFields::new().title(title.clone());
                let _ = cx.send_notification(SessionNotification::new(
                    sid,
                    SessionUpdate::ToolCallUpdate(
                        ToolCallUpdate::new(
                            ToolCallId::new(presentation::project_identifier(&request_id)),
                            fields,
                        )
                        .meta(identity_meta),
                    ),
                ));

                // Best-effort persistence: only persist the LLM-generated title
                // (not the deterministic fallback) so reload uses fallback_title
                // for older or failed cases just like today.
                if from_llm {
                    if let Some(msg_id) = message_id_for_persist {
                        let patch = serde_json::json!({
                            crate::conversation::message::TOOL_META_TITLE_KEY: title,
                        });
                        if let Err(e) = session_manager
                            .update_tool_request_meta(
                                &session_id_for_persist,
                                &msg_id,
                                &request_id,
                                patch,
                            )
                            .await
                        {
                            warn!(
                                "tool call summary: persist failed for {request_id} in {msg_id}: {e}",
                            );
                        }
                    } else {
                        warn!(
                            "tool call summary: missing message_id for {request_id} — title will not survive reload",
                        );
                    }
                }
            });
        }

        Ok(())
    }

    async fn handle_tool_response(
        &self,
        tool_response: &crate::conversation::message::ToolResponse,
        session_id: &SessionId,
        session_id_str: &str,
        message_id: Option<&str>,
        session: &mut GoslingAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), agent_client_protocol::Error> {
        let status = match &tool_response.tool_result {
            Ok(result) if result.is_error == Some(true) => ToolCallStatus::Failed,
            Ok(_) => ToolCallStatus::Completed,
            Err(_) => ToolCallStatus::Failed,
        };

        let mut presented_response = tool_response.clone();
        presented_response.tool_result =
            presentation::project_tool_result_for_update(&tool_response.tool_result);

        let mut fields = ToolCallUpdateFields::new().status(status);
        if let Some(raw_output) = extract_tool_raw_output(&presented_response.tool_result) {
            fields = fields.raw_output(raw_output);
        }
        if !presented_response
            .tool_result
            .as_ref()
            .is_ok_and(|r| r.is_acp_aware())
        {
            let content = build_tool_call_content(&presented_response.tool_result);
            fields = fields.content(content);

            let locations = extract_locations_from_meta(tool_response).unwrap_or_else(|| {
                if let Some(tool_request) = session.tool_requests.get(&tool_response.id) {
                    extract_tool_locations(tool_request, tool_response)
                } else {
                    Vec::new()
                }
            });
            if !locations.is_empty() {
                fields = fields.locations(locations);
            }
        }

        let update = ToolCallUpdate::new(
            ToolCallId::new(presentation::project_identifier(&tool_response.id)),
            fields,
        )
        .meta(extract_tool_call_update_meta(&presented_response));
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCallUpdate(update),
        ))?;

        // Chain summarization: when this response completes a multi-tool
        // chain, fire one LLM summary covering the run.
        session.responded_tool_ids.insert(tool_response.id.clone());
        self.maybe_summarize_chain(&tool_response.id, session_id, session_id_str, session, cx);
        let _ = message_id;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_tool_permission_request(
        &self,
        cx: &ConnectionTo<Client>,
        agent: &Arc<Agent>,
        session_id: &SessionId,
        request_id: String,
        tool_name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        prompt: Option<String>,
        domain: Option<String>,
    ) -> Result<(), agent_client_protocol::Error> {
        let cx = cx.clone();
        let agent = agent.clone();
        let session_id = session_id.clone();

        let formatted_name = presentation::project_tool_title(&format_tool_name(&tool_name));

        // A security prompt means an inspector flagged this specific call. A
        // persistent "always allow" would carry that one-off decision forward
        // to every future call of the tool, so the option is withheld -- the
        // CLI already does this via `security_prompt.is_none()`, but ACP
        // clients (Desktop, TUI) were offered it. (WFG-GOS-006)
        let is_security_prompt = prompt.is_some();

        let mut fields = ToolCallUpdateFields::new()
            .title(formatted_name)
            .kind(ToolKind::default())
            .status(ToolCallStatus::Pending)
            .raw_input(presentation::project_tool_input(
                &serde_json::Value::Object(arguments),
            ));
        if let Some(p) = prompt {
            fields = fields.content(vec![ToolCallContent::Content(Content::new(
                ContentBlock::Text(TextContent::new(presentation::project_live_text(
                    &p,
                    "Permission prompt",
                ))),
            ))]);
        }
        let tool_call_update = ToolCallUpdate::new(
            ToolCallId::new(presentation::project_identifier(&request_id)),
            fields,
        )
        .meta(Some(
            serde_json::json!({
                "gosling": {
                    "toolCall": { "toolName": presentation::project_identifier(&tool_name) },
                    "permission": { "domain": domain },
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        ));

        fn option(kind: PermissionOptionKind) -> PermissionOption {
            let id = serde_json::to_value(kind)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            PermissionOption::new(id.clone(), id, kind)
        }
        let mut options = Vec::new();
        if !is_security_prompt {
            options.push(option(PermissionOptionKind::AllowAlways));
        } else if let Some(domain) = &domain {
            // A domain-scoped grant is a narrower, more auditable claim than
            // the tool-wide always-allow withheld above by WFG-GOS-006: it
            // covers only the flagged destination, not every future call of
            // the tool. The ACP `PermissionOptionKind` enum has no
            // domain-scoped variant, so this reuses `AllowAlways` for the
            // kind hint but carries a distinct `option_id` so the response
            // can be told apart from a tool-wide grant.
            options.push(PermissionOption::new(
                PermissionDecision::AllowAlwaysDomain.to_string(),
                format!("Always allow {domain}"),
                PermissionOptionKind::AllowAlways,
            ));
        }
        options.extend([
            option(PermissionOptionKind::AllowOnce),
            option(PermissionOptionKind::RejectOnce),
            option(PermissionOptionKind::RejectAlways),
        ]);

        let permission_request =
            RequestPermissionRequest::new(session_id, tool_call_update, options);

        cx.send_request(permission_request)
            .on_receiving_result(move |result| async move {
                match result {
                    Ok(response) => {
                        agent
                            .handle_confirmation(
                                request_id,
                                outcome_to_confirmation(&response.outcome),
                            )
                            .await;
                        Ok(())
                    }
                    Err(e) => {
                        error!(error = ?e, "permission request failed");
                        agent
                            .handle_confirmation(
                                request_id,
                                PermissionConfirmation {
                                    principal_type: PrincipalType::Tool,
                                    permission: Permission::Cancel,
                                },
                            )
                            .await;
                        Ok(())
                    }
                }
            })?;

        Ok(())
    }
}
