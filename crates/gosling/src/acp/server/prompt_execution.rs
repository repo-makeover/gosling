//! ACP prompt conversion, execution, completion, and usage projection.
//!
//! Maintainers: preserve run fencing, stream ordering, and terminal-state persistence here.
//! Clients: prompt cancellation, notifications, usage, and response semantics remain stable.

use super::*;

/// Shown in the chat, and carried as the prompt error, when a Deep Research
/// turn ends on a question to the operator instead of a report.
const RESEARCH_AWAITING_REPLY_NOTICE: &str =
    "Deep Research is waiting for your reply. Answer the question above and it will continue and write the report.";
const RESEARCH_AWAITING_REPLY_REASON: &str = "deep_research_awaiting_reply";

fn to_nonnegative_u64(value: Option<i32>) -> Option<u64> {
    value.and_then(|v| u64::try_from(v).ok())
}

pub(super) fn build_prompt_usage(session: &Session) -> Option<Usage> {
    let total = to_nonnegative_u64(session.usage.total_tokens)?;
    let input = to_nonnegative_u64(session.usage.input_tokens).unwrap_or(0);
    let output = to_nonnegative_u64(session.usage.output_tokens).unwrap_or(0);
    Some(Usage::new(total, input, output))
}

pub(in crate::acp) struct UsageUpdates {
    pub(in crate::acp) custom: GoslingSessionNotification,
    pub(in crate::acp) standard: UsageUpdate,
}

pub(in crate::acp) fn build_usage_updates(session: &Session) -> Option<UsageUpdates> {
    let used = session.usage.total_tokens.unwrap_or(0).max(0) as u64;
    let ctx_limit = session.model_config.as_ref()?.context_limit() as u64;
    let accumulated_input_tokens =
        to_nonnegative_u64(session.accumulated_usage.input_tokens).unwrap_or(0);
    let accumulated_output_tokens =
        to_nonnegative_u64(session.accumulated_usage.output_tokens).unwrap_or(0);
    Some(UsageUpdates {
        custom: GoslingSessionNotification {
            session_id: session.id.clone(),
            update: GoslingSessionUpdate::UsageUpdate(SessionUsageUpdate {
                used,
                context_limit: ctx_limit,
                accumulated_input_tokens,
                accumulated_output_tokens,
                accumulated_cost: session.accumulated_cost,
            }),
        },
        standard: {
            let mut standard = UsageUpdate::new(used, ctx_limit);
            if let Some(amount) = session.accumulated_cost {
                standard = standard.cost(Cost::new(amount, "USD"));
            }
            standard
        },
    })
}

impl GoslingAcpAgent {
    /// Convert ACP prompt content blocks into a user message.
    pub(super) fn convert_acp_prompt_to_message(prompt: &[ContentBlock]) -> Message {
        let mut message = Message::user();
        for block in prompt {
            match block {
                ContentBlock::Text(text) => {
                    let annotated = if let Some(ref ann) = text.annotations {
                        let audience: Vec<Role> = ann
                            .audience
                            .as_ref()
                            .map(|roles| {
                                roles
                                    .iter()
                                    .filter_map(|r| match r {
                                        agent_client_protocol::schema::v1::Role::Assistant => {
                                            Some(Role::Assistant)
                                        }
                                        agent_client_protocol::schema::v1::Role::User => {
                                            Some(Role::User)
                                        }
                                        _ => None,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let raw = RawTextContent {
                            text: sanitize_unicode_tags(&text.text),
                            meta: None,
                        };
                        if audience.is_empty() {
                            raw.no_annotation()
                        } else {
                            raw.no_annotation().with_audience(audience)
                        }
                    } else {
                        // No annotations — regular user text.
                        let sanitized = sanitize_unicode_tags(&text.text);
                        RawTextContent {
                            text: sanitized,
                            meta: None,
                        }
                        .no_annotation()
                    };
                    message = message.with_content(MessageContent::Text(annotated));
                }
                ContentBlock::Image(image) => {
                    message = message.with_image(&image.data, &image.mime_type);
                }
                ContentBlock::Resource(resource) => {
                    if let EmbeddedResourceResource::TextResourceContents(text_resource) =
                        &resource.resource
                    {
                        let header = format!("--- Resource: {} ---\n", text_resource.uri);
                        let content = format!("{}{}\n---\n", header, text_resource.text);
                        message = message.with_text(&content);
                    }
                }
                ContentBlock::ResourceLink(link) => {
                    if let Some(text) = read_resource_link(link.clone()) {
                        message = message.with_text(text);
                    }
                }
                ContentBlock::Audio(..) | _ => (),
            }
        }
        message
    }

    async fn record_acp_prompt_state(
        &self,
        session_id: &str,
        state: AcpPromptRunState,
    ) -> Result<(), agent_client_protocol::Error> {
        let value = state.to_value().map_err(|error| {
            agent_client_protocol::Error::internal_error()
                .data(format!("failed to serialize ACP prompt state: {error}"))
        })?;
        let key = format!(
            "{}.{}",
            AcpPromptRunState::EXTENSION_NAME,
            AcpPromptRunState::VERSION
        );
        self.session_manager
            .merge_extension_state(session_id, &key, value)
            .await
            .map_err(|error| {
                agent_client_protocol::Error::internal_error()
                    .data(format!("failed to persist ACP prompt state: {error}"))
            })
    }

    pub(super) async fn on_prompt(
        &self,
        cx: &ConnectionTo<Client>,
        args: PromptRequest,
    ) -> Result<PromptResponse, agent_client_protocol::Error> {
        // The ACP session_id IS the thread ID.
        let session_id = args.session_id.0.to_string();
        let sid = sid_short(&session_id);
        let t_start = std::time::Instant::now();
        let research_run_started_at = chrono::Utc::now() - chrono::Duration::seconds(1);

        let run_id = format!("run_{}", Uuid::new_v4());
        let cancel_token = CancellationToken::new();
        self.start_active_run(&session_id, run_id.clone(), cancel_token.clone())
            .await?;

        let agent = match self.get_session_agent(&session_id).await {
            Ok(agent) => agent,
            Err(error) => {
                self.clear_active_run(&session_id, &run_id).await;
                return Err(error);
            }
        };

        if cancel_token.is_cancelled() {
            self.clear_active_run(&session_id, &run_id).await;
            Self::send_active_run_update(cx, &args.session_id, None)?;
            return Ok(PromptResponse::new(StopReason::Cancelled));
        }

        if let Err(error) = Self::send_active_run_update(cx, &args.session_id, Some(&run_id)) {
            self.clear_active_run(&session_id, &run_id).await;
            return Err(error);
        }

        if let Err(error) = self
            .record_acp_prompt_state(&session_id, AcpPromptRunState::InProgress)
            .await
        {
            self.clear_active_run(&session_id, &run_id).await;
            let _ = Self::send_active_run_update(cx, &args.session_id, None);
            return Err(error);
        }

        let user_message = Self::convert_acp_prompt_to_message(&args.prompt);
        let (compacted_context, tail_limit) = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(&session_id)
                .map(|session| (session.compacted_context, session.tail_limit))
                .unwrap_or((false, DEFAULT_SESSION_TAIL_LIMIT))
        };

        let session_config = SessionConfig {
            id: session_id.clone(),
            max_turns: None,
            compacted_context,
            tail_limit: Some(tail_limit),
        };

        let mut stream = match agent
            .reply(user_message, session_config, Some(cancel_token.clone()))
            .await
        {
            Ok(stream) => stream,
            Err(error) => {
                let persisted = self
                    .record_acp_prompt_state(&session_id, AcpPromptRunState::Failed)
                    .await;
                self.clear_active_run(&session_id, &run_id).await;
                let _ = Self::send_active_run_update(cx, &args.session_id, None);
                persisted?;
                return Err(agent_client_protocol::Error::internal_error()
                    .data(format!("Error getting agent reply: {error}")));
            }
        };

        let mut was_cancelled = false;
        let mut first_event_logged = false;
        let mut event_count: u32 = 0;
        // Streaming chain buffer: tracks consecutive tool requests across
        // `AgentEvent::Message` events so chains that span multiple rows are
        // still registered. Sequential tool use (Bedrock/Anthropic) yields
        // request → response → request → response across separate
        // assistant/user messages, so tool responses are chain-neutral; only
        // non-tool content (text, thinking, image, etc.) breaks the run.
        // Holds `(tool_call_id, message_id_of_owning_row)` in arrival order;
        // re-registered eagerly each time a request arrives so
        // `handle_tool_response` finds the chain when subsequent responses
        // are processed.
        let mut chain_buffer: Vec<(String, String)> = Vec::new();
        let mut stream_error = None;
        let mut terminal_assistant_text = String::new();
        let mut current_assistant_message_ids = HashSet::new();

        loop {
            let event = tokio::select! {
                biased;
                _ = cancel_token.cancelled() => {
                    was_cancelled = true;
                    break;
                }
                event = stream.next() => event,
            };
            let Some(event) = event else {
                break;
            };
            event_count += 1;
            if !first_event_logged {
                debug!(
                    target: "perf",
                    sid = %sid,
                    ttft_ms = t_start.elapsed().as_millis() as u64,
                    "perf: prompt first stream event (time-to-first-token from prompt start)"
                );
                first_event_logged = true;
            }

            match event {
                Ok(crate::agents::AgentEvent::Message(message)) => {
                    // Agent persists messages via session_manager.add_message() internally.
                    let stored_message_id = message.id.clone();

                    if message.role == Role::Assistant {
                        if let Some(message_id) = stored_message_id.as_ref() {
                            current_assistant_message_ids.insert(message_id.clone());
                        }
                        let mut message_text = String::new();
                        for content_item in &message.content {
                            if let MessageContent::Text(text) = content_item {
                                message_text.push_str(&text.text);
                                message_text.push('\n');
                            }
                        }
                        if !message_text.is_empty() {
                            terminal_assistant_text = message_text;
                        }
                    }

                    let mut sessions = self.sessions.lock().await;
                    let Some(session) = sessions.get_mut(&session_id) else {
                        stream_error = Some(
                            agent_client_protocol::Error::invalid_params()
                                .data(format!("Session not found: {}", session_id)),
                        );
                        break;
                    };

                    for content_item in &message.content {
                        if let Some(error) = prompt_error_from_message_content(content_item) {
                            stream_error = Some(error);
                            break;
                        }

                        match content_item {
                            MessageContent::ToolRequest(tr) => {
                                if let Some(msg_id) = stored_message_id.as_deref() {
                                    chain_buffer.push((tr.id.clone(), msg_id.to_string()));
                                    // Re-register eagerly so the chain is in
                                    // place by the time the matching
                                    // `tool_response` triggers
                                    // `maybe_summarize_chain` (sequential
                                    // tool use interleaves request/response
                                    // events).
                                    extend_chain_membership(
                                        &chain_buffer,
                                        &mut session.chain_membership,
                                    );
                                }
                            }
                            MessageContent::ToolResponse(_) => {
                                // Chain-neutral: a response between two
                                // requests doesn't break the run, matching
                                // the frontend's `groupContentSections`.
                            }
                            _ => {
                                // Text, thinking, image, etc. end the run.
                                chain_buffer.clear();
                            }
                        }

                        if let Err(error) = self
                            .handle_message_content(
                                content_item,
                                &args.session_id,
                                &session_id,
                                stored_message_id.as_deref(),
                                message.created,
                                &message.role,
                                message.metadata.steer,
                                &agent,
                                session,
                                cx,
                            )
                            .await
                        {
                            stream_error = Some(error);
                            break;
                        }
                    }
                    if stream_error.is_none() {
                        stream_error = prompt_error_from_message(&message);
                    }
                    if stream_error.is_some() {
                        break;
                    }
                }
                Ok(crate::agents::AgentEvent::McpNotification((request_id, notification))) => {
                    if let Some(update) =
                        tool_notifications::tool_notification_update(request_id, notification)
                    {
                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            update,
                        ))?;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    stream_error = Some(
                        agent_client_protocol::Error::internal_error()
                            .data(format!("Error in agent response stream: {}", e)),
                    );
                    break;
                }
            }
        }

        {
            let mut sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get_mut(&session_id) {
                // Final safety net: in case the stream ended without any
                // chain-breaking content, make sure a multi-tool buffer is
                // registered. (Eager registration during the loop usually
                // covers this.)
                extend_chain_membership(&chain_buffer, &mut session.chain_membership);
            }
        }
        self.clear_active_run(&session_id, &run_id).await;
        Self::send_active_run_update(cx, &args.session_id, None)?;
        was_cancelled |= cancel_token.is_cancelled();
        if stream_error.is_none() && !was_cancelled {
            match research_completion::verify_deep_research_completion(
                &self.session_manager,
                &session_id,
                &terminal_assistant_text,
                research_run_started_at,
                &current_assistant_message_ids,
            )
            .await
            {
                Ok(research_completion::ResearchOutcome::AwaitingReply) => {
                    let message = Message::assistant().with_system_notification(
                        SystemNotificationType::InlineMessage,
                        RESEARCH_AWAITING_REPLY_NOTICE,
                    );
                    self.session_manager
                        .add_message(&session_id, &message)
                        .await
                        .internal_err_ctx("Failed to record the research status")?;
                    for content in &message.content {
                        if let MessageContent::SystemNotification(notification) = content {
                            send_status_message_update(
                                cx,
                                self.supports_gosling_custom_notifications(),
                                &session_id,
                                notification,
                            )?;
                        }
                    }
                    stream_error = Some(
                        agent_client_protocol::Error::new(-32603, "Waiting for your reply").data(
                            serde_json::json!({
                                "reason": RESEARCH_AWAITING_REPLY_REASON,
                                "message": RESEARCH_AWAITING_REPLY_NOTICE,
                            }),
                        ),
                    );
                }
                Ok(research_completion::ResearchOutcome::Verified(notes)) => {
                    for note in notes {
                        let message = Message::assistant()
                            .with_system_notification(SystemNotificationType::InlineMessage, note);
                        self.session_manager
                            .add_message(&session_id, &message)
                            .await
                            .internal_err_ctx("Failed to record the research closeout")?;
                        for content in &message.content {
                            if let MessageContent::SystemNotification(notification) = content {
                                send_status_message_update(
                                    cx,
                                    self.supports_gosling_custom_notifications(),
                                    &session_id,
                                    notification,
                                )?;
                            }
                        }
                    }
                }
                Err(error) => {
                    stream_error = Some(
                        agent_client_protocol::Error::internal_error()
                            .data(format!("Deep Research was not completed: {error}")),
                    );
                }
            }
        }
        let terminal_state = if stream_error.is_some() {
            AcpPromptRunState::Failed
        } else if was_cancelled {
            AcpPromptRunState::Cancelled
        } else {
            AcpPromptRunState::Completed
        };
        self.record_acp_prompt_state(&session_id, terminal_state)
            .await?;
        if let Some(error) = stream_error {
            return Err(error);
        }

        let session = self
            .session_manager
            .get_session(&session_id, false)
            .await
            .internal_err_ctx("Failed to load session")?;
        if let Some(updates) = build_usage_updates(&session) {
            if self.supports_gosling_custom_notifications() {
                cx.send_notification(updates.custom)?;
            }
            // Standard ACP notification — emitted alongside the custom one for
            // backwards compatibility. Remove once all known clients have
            // migrated to `_gosling/unstable/session/update`.
            cx.send_notification(SessionNotification::new(
                args.session_id.clone(),
                SessionUpdate::UsageUpdate(updates.standard),
            ))?;
        }
        if self.supports_gosling_custom_notifications() {
            let page = self
                .session_manager
                .list_session_artifacts(&session_id, None, 200)
                .await
                .internal_err_ctx("Failed to load session artifacts")?;
            for artifact in page.artifacts {
                cx.send_notification(GoslingSessionNotification {
                    session_id: session_id.clone(),
                    update: GoslingSessionUpdate::ArtifactUpdate(ArtifactUpdate {
                        artifact: session_artifact_dto(artifact),
                    }),
                })?;
            }
        }

        debug!(
            target: "perf",
            sid = %sid,
            ms = t_start.elapsed().as_millis() as u64,
            events = event_count,
            cancelled = was_cancelled,
            "perf: prompt done"
        );
        let stop_reason = if was_cancelled {
            StopReason::Cancelled
        } else {
            StopReason::EndTurn
        };

        let mut response = PromptResponse::new(stop_reason);
        if let Some(usage) = build_prompt_usage(&session) {
            response = response.usage(usage);
        }
        Ok(response)
    }
}
