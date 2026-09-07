//! Public reply entry, provider readiness, compaction, and context-manager integration.
//!
//! Maintainers: preserve pre-persistence checks and context fallback behavior here.
//! Clients: reply submission, compaction notices, and provider errors remain stable.

use super::*;

impl Agent {
    /// Get a reference count clone to the provider
    pub async fn provider(&self) -> Result<Arc<dyn Provider>, anyhow::Error> {
        match &*self.provider.lock().await {
            Some(provider) => Ok(Arc::clone(provider)),
            None => Err(anyhow!("Provider not set")),
        }
    }

    /// Resolve the active model config for a session.
    ///
    /// The session is the source of truth for the selected model and its
    /// settings. When the session has no stored config (e.g. before the
    /// provider has been persisted), fall back to the configured provider
    /// defaults.
    pub async fn model_config_for_session(
        &self,
        session_id: &str,
    ) -> Result<gosling_providers::model::ModelConfig> {
        if let Ok(session) = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
        {
            if let Some(model_config) = session.model_config {
                return Ok(model_config);
            }
        }

        let config = Config::global();
        let provider_name = config
            .get_gosling_provider()
            .map_err(|_| anyhow!("Could not resolve model config: missing provider"))?;
        let model_name = config
            .get_gosling_model()
            .map_err(|_| anyhow!("Could not resolve model config: missing model"))?;
        crate::model_config::model_config_from_user_config(&provider_name, &model_name)
            .map_err(|e| anyhow!("Could not resolve model config: {e}"))
    }

    /// Handle a confirmation response for a tool request
    pub async fn handle_confirmation(
        &self,
        request_id: String,
        confirmation: PermissionConfirmation,
    ) {
        let provider = self.provider.lock().await.clone();
        if let Some(provider) = provider.as_ref() {
            if provider.permission_routing() == PermissionRouting::ActionRequired
                && provider
                    .handle_permission_confirmation(&request_id, &confirmation)
                    .await
            {
                return;
            }
        }
        if !self
            .tool_confirmation_router
            .deliver(request_id, confirmation)
            .await
        {
            error!("Failed to deliver confirmation");
        }
    }

    pub async fn supports_action_required_permissions(&self) -> bool {
        if let Some(provider) = self.provider.lock().await.as_ref() {
            return provider.permission_routing() == PermissionRouting::ActionRequired;
        }
        false
    }

    /// Pre-flight for paths that reach the provider. Must run before the user
    /// message is persisted: bailing after `add_message` leaves a stray copy in
    /// the conversation that gets replayed to the provider once a later submit
    /// succeeds.
    async fn ensure_provider_ready(&self, restrict_to_working_dirs: bool) -> Result<()> {
        let provider = self.provider().await?;
        if restrict_to_working_dirs && provider.executes_tools_outside_gosling() {
            anyhow::bail!(
                "Provider '{}' runs tools outside Gosling's inspection pipeline, so it can't be used while this session restricts tools to working directories. Turn off \"Restrict tools to working directories\" for this session to allow it — the toggle is in the working-directories menu (folder icon in the chat's top-right corner).",
                provider.get_name()
            );
        }
        Ok(())
    }

    #[instrument(
        skip(self, user_message, session_config, cancel_token),
        fields(user_message, trace_input, session.id = %session_config.id)
    )]
    pub async fn reply(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        if is_token_cancelled(&cancel_token) {
            return Ok(Box::pin(futures::stream::empty()));
        }

        let session_manager = self.config.session_manager.clone();
        session_manager
            .recover_tool_operations(&session_config.id)
            .await?;

        let message_text_for_trace = user_message.as_concat_text();
        tracing::Span::current().record("user_message", message_text_for_trace.as_str());
        tracing::Span::current().record("trace_input", message_text_for_trace.as_str());

        for content in &user_message.content {
            if let MessageContent::ActionRequired(action_required) = content {
                if let ActionRequiredData::ElicitationResponse {
                    id,
                    user_data,
                    action,
                } = &action_required.data
                {
                    // Surface stale/cancelled/timed-out elicitations as a hard
                    // error so callers (e.g. the HTTP handler) can propagate
                    // failure to the client instead of silently reporting
                    // success while the blocked tool call stays unblocked.
                    // The success path returns an empty stream after the MCP
                    // server receives the user's accept/decline/cancel action.
                    let response = match action {
                        ElicitationAction::Accept => ElicitationOutcome::Accept(user_data.clone()),
                        ElicitationAction::Decline => ElicitationOutcome::Decline,
                        ElicitationAction::Cancel => ElicitationOutcome::Cancel,
                    };
                    crate::elicitation::complete_elicitation_with_message(
                        &session_manager,
                        &session_config.id,
                        id,
                        response,
                        &user_message,
                    )
                    .await
                    .map_err(|e| {
                        error!("Failed to submit elicitation response: {}", e);
                        anyhow!("Failed to submit elicitation response: {}", e)
                    })?;
                    return Ok(Box::pin(futures::stream::empty()));
                }
            }
        }

        let turn_lease = session_manager
            .acquire_session_turn_lease(&session_config.id, cancel_token.as_ref())
            .await?;
        // Everything below runs under the lease's token, not the caller's: it
        // is a child of the caller's, so an explicit cancel still propagates,
        // and it additionally fires if another process takes this session's
        // turn lease over (REL-GSL-005).
        let cancel_token = Some(turn_lease.turn_cancel_token());

        let message_text = user_message.as_concat_text();

        let session = session_manager
            .get_session(&session_config.id, false)
            .await?;
        let is_first_turn = session.message_count == 0;
        if is_first_turn {
            self.emit_hook(crate::hooks::HookEvent::SessionStart, &session_config.id)
                .await;
        }

        if self
            .hook_manager
            .has_hooks(crate::hooks::HookEvent::UserPromptSubmit)
        {
            let ctx = crate::hooks::HookContext::new(
                crate::hooks::HookEvent::UserPromptSubmit,
                &session_config.id,
            )
            .with_message(message_text.clone());
            self.hook_manager
                .emit(crate::hooks::HookEvent::UserPromptSubmit, ctx)
                .await;
        }

        let command_result = self
            .execute_command(&message_text, &session_config.id)
            .await;

        let mut command_preamble: Vec<AgentEvent> = Vec::new();

        match command_result {
            Err(e) => {
                let error_message = Message::assistant()
                    .with_text(e.to_string())
                    .with_visibility(true, false);
                return Ok(Box::pin(stream::once(async move {
                    Ok(AgentEvent::Message(error_message))
                })));
            }
            Ok(Some(response))
                if response.role == rmcp::model::Role::Assistant
                    && crate::agents::execute_commands::command_starts_turn(&message_text) =>
            {
                // Setting a goal/grind should immediately start a turn so the
                // agent begins pursuing it, rather than waiting for the next
                // user prompt. Record the command and its confirmation as
                // user-visible only, then inject an agent-visible kickoff and
                // fall through into the reply loop.
                self.ensure_provider_ready(session.restrict_tools_to_working_dirs)
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &user_message.clone().with_visibility(true, false),
                    )
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &response.clone().with_visibility(true, false),
                    )
                    .await?;
                let goal_text = crate::agents::execute_commands::parse_slash_command(&message_text)
                    .map(|parsed| parsed.params_str.to_string())
                    .unwrap_or_default();
                let kickoff = Message::user()
                    .with_text(format!(
                        "Start working toward this goal now:\n\n**Goal:** {goal_text}"
                    ))
                    .with_visibility(false, true);
                session_manager
                    .add_message(&session_config.id, &kickoff)
                    .await?;

                command_preamble = vec![
                    AgentEvent::Message(user_message.clone()),
                    AgentEvent::Message(response.clone()),
                ];
            }
            Ok(Some(response)) if response.role == rmcp::model::Role::Assistant => {
                session_manager
                    .add_message(
                        &session_config.id,
                        &user_message.clone().with_visibility(true, false),
                    )
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &response.clone().with_visibility(true, false),
                    )
                    .await?;

                // Check if this was a command that modifies conversation history
                let modifies_history = crate::agents::execute_commands::COMPACT_TRIGGERS
                    .contains(&message_text.trim())
                    || message_text.trim() == "/clear";

                return Ok(Box::pin(async_stream::try_stream! {
                    let _turn_lease = turn_lease;
                    yield AgentEvent::Message(user_message);
                    yield AgentEvent::Message(response);

                    // After commands that modify history, notify UI that history was replaced
                    if modifies_history {
                        let updated_session = session_manager.get_session(&session_config.id, true)
                            .await
                            .map_err(|e| anyhow!("Failed to fetch updated session: {}", e))?;
                        let updated_conversation = updated_session
                            .conversation
                            .ok_or_else(|| anyhow!("Session has no conversation after history modification"))?;
                        yield AgentEvent::HistoryReplaced(updated_conversation);
                    }
                }));
            }
            Ok(Some(resolved_message)) => {
                self.ensure_provider_ready(session.restrict_tools_to_working_dirs)
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &user_message.clone().with_visibility(true, false),
                    )
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &resolved_message.clone().with_visibility(false, true),
                    )
                    .await?;
            }
            Ok(None) => {
                self.ensure_provider_ready(session.restrict_tools_to_working_dirs)
                    .await?;
                session_manager
                    .add_message(&session_config.id, &user_message)
                    .await?;
            }
        }
        let session = if session_config.compacted_context {
            session_manager
                .get_session_for_compacted_resume(
                    &session_config.id,
                    session_config
                        .tail_limit
                        .unwrap_or(DEFAULT_SESSION_TAIL_LIMIT),
                )
                .await?
        } else {
            session_manager
                .get_session(&session_config.id, true)
                .await?
        };
        let provider = self.provider().await?;
        let conversation = session
            .conversation
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Session {} has no conversation", session_config.id))?;

        let needs_auto_compact =
            check_if_compaction_needed(provider.as_ref(), &conversation, None, &session).await?;

        let conversation_to_compact = conversation.clone();

        Ok(Box::pin(async_stream::try_stream! {
            let _turn_lease = turn_lease;
            for event in command_preamble {
                yield event;
            }

            let final_conversation = if !needs_auto_compact {
                conversation
            } else {
                let config = Config::global();
                let threshold = config
                    .get_param::<f64>("GOSLING_AUTO_COMPACT_THRESHOLD")
                    .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
                let threshold_percentage = (threshold * 100.0) as u32;

                let inline_msg = format!(
                    "Exceeded auto-compact threshold of {}%. Performing auto-compaction...",
                    threshold_percentage
                );

                yield AgentEvent::Message(
                    Message::assistant().with_system_notification(
                        SystemNotificationType::InlineMessage,
                        inline_msg,
                    )
                );

                yield AgentEvent::Message(
                    Message::assistant().with_system_notification(
                        SystemNotificationType::ThinkingMessage,
                        COMPACTION_THINKING_TEXT,
                    )
                );

                let compact_model_config = self.model_config_for_session(&session_config.id).await?;
                match self
                    .perform_compact(&compact_model_config, &session_config, &conversation_to_compact)
                    .await
                {
                    Ok(compacted_conversation) => {
                        yield AgentEvent::HistoryReplaced(compacted_conversation.clone());
                        yield AgentEvent::Message(
                            Message::assistant().with_system_notification(
                                SystemNotificationType::InlineMessage,
                                "Compaction complete",
                            )
                        );
                        compacted_conversation
                    }
                    Err(e) => {
                        yield AgentEvent::Message(
                            Message::assistant()
                                .with_text(crate::context_mgmt::compaction_failure_message(&e))
                        );
                        return;
                    }
                }
            };

            let mut reply_stream = self.reply_internal(final_conversation, session_config, session, cancel_token).await?;
            while let Some(event) = reply_stream.next().await {
                yield event?;
            }
        }))
    }

    pub(super) async fn perform_compact(
        &self,
        model_config: &gosling_providers::model::ModelConfig,
        session_config: &SessionConfig,
        conversation: &Conversation,
    ) -> Result<Conversation> {
        self.perform_compact_with_provider(
            self.provider().await?,
            model_config,
            session_config,
            conversation,
        )
        .await
    }

    pub(super) async fn perform_compact_with_provider(
        &self,
        provider: Arc<dyn Provider>,
        model_config: &gosling_providers::model::ModelConfig,
        session_config: &SessionConfig,
        conversation: &Conversation,
    ) -> Result<Conversation> {
        let (compacted_conversation, usage) = compact_messages(
            provider.as_ref(),
            model_config,
            &session_config.id,
            conversation,
            false,
        )
        .await?;
        let session_manager = self.config.session_manager.clone();
        session_manager
            .replace_conversation(&session_config.id, &compacted_conversation)
            .await?;
        self.update_session_metrics(&session_config.id, &usage, true)
            .await?;
        Ok(compacted_conversation)
    }

    /// Runs the Context Manager (`GOSLING_CONTEXT_MANAGER`) ahead of a provider
    /// call and decides what to actually send. `off` skips packet assembly
    /// entirely so behavior and cost are unchanged; `shadow` builds and logs
    /// the packet but still returns the pre-existing prompt/messages; `on`
    /// returns the packet's own prompt/messages. Falls back to the
    /// pre-existing prompt/messages on any build error so this can never make
    /// a turn fail that would otherwise have succeeded.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn apply_context_manager(
        &self,
        provider: &dyn Provider,
        session_id: &str,
        base_system_prompt: &str,
        project_addendum: Option<&str>,
        merged_system_prompt: &str,
        conversation: &Conversation,
        model_config: &gosling_providers::model::ModelConfig,
        working_dir: &std::path::Path,
    ) -> (String, Vec<Message>) {
        let mode = context_manager_mode();
        let fallback = || {
            (
                merged_system_prompt.to_string(),
                conversation.messages().clone(),
            )
        };

        if mode == ContextManagerMode::Off {
            return fallback();
        }

        // A self-managing backend (Claude Code, Codex/ACP, Gemini CLI) runs
        // its own agent loop and compaction, so a Gosling-curated packet
        // driving its input is wasted or counterproductive. Cap `on` to
        // shadow — still build and log the packet, but hand the backend its
        // own prompt/messages — and route the summarizer's extracted facts to
        // the backend's durable file instead of the (unused) packet.
        let self_managing = provider.manages_own_context();
        let summarizer_target = summarizer::target_for_provider(provider, working_dir);
        let effective_mode = if self_managing && mode == ContextManagerMode::On {
            debug!(
                "Context Manager capped to shadow: provider manages its own context; skipping packet takeover"
            );
            ContextManagerMode::Shadow
        } else {
            mode
        };

        let context_limit = provider
            .get_context_limit(model_config)
            .await
            .unwrap_or_else(|_| model_config.context_limit());
        let reserved_response_tokens = model_config
            .max_tokens
            .filter(|tokens| *tokens > 0)
            .map(|tokens| tokens as usize)
            .unwrap_or(crate::context_mgmt::budget::DEFAULT_RESERVED_RESPONSE_TOKENS);

        // This is the memory retrieval point: FileMemorySource recalls from
        // the local memories.jsonl (GOSLING_MEMORY_FILE to override); with no
        // file present it recalls nothing. Swap the source here to back the
        // RetrievedMemory slot with something richer.
        let memory_query = MemoryQuery {
            session_id,
            messages: conversation.messages(),
            reserved_tokens: crate::context_mgmt::ContextBudgetPolicy::new(
                context_limit,
                reserved_response_tokens,
            )
            .retrieved_memory_reserved_tokens(),
        };
        let retrieved_memory = FileMemorySource::from_config().retrieve(&memory_query);

        let request = ContextBuildRequest {
            system_prompt: base_system_prompt.to_string(),
            project_instructions: project_addendum.map(|s| s.to_string()),
            conversation_messages: conversation.messages().clone(),
            context_limit,
            reserved_response_tokens,
            retrieved_memory,
        };

        match ContextManager::build(request).await {
            Ok(packet) => {
                crate::context_mgmt::telemetry::log_context_packet(effective_mode, &packet);
                self.maybe_dispatch_summarizer(session_id, &packet, summarizer_target);
                resolve_provider_input(
                    effective_mode,
                    &packet,
                    merged_system_prompt,
                    conversation.messages(),
                )
            }
            Err(e) => {
                warn!("Context Manager failed to build context packet, falling back to existing behavior: {e}");
                fallback()
            }
        }
    }

    /// Fires the local-LLM summarizer worker (`GOSLING_SUMMARIZER`) over any
    /// blocks the packet just rendered with the naive truncation stub.
    /// Spawned rather than awaited so it never sits on the critical path to
    /// the provider call. `target` (chosen from the current provider) decides
    /// where the output lands: a raw API provider caches a better digest for
    /// the *next* turn's packet (see `summarize_group` in
    /// `context_mgmt::packet`) and appends facts to `memories.jsonl`; a
    /// self-managing backend takes no digest handoff and routes facts to its
    /// durable file (`CLAUDE.md` / `AGENTS.md`). In `shadow` mode it only
    /// logs; a no-op in `off` mode and whenever nothing needed summarizing.
    pub(super) fn maybe_dispatch_summarizer(
        &self,
        session_id: &str,
        packet: &crate::context_mgmt::ContextPacket,
        target: summarizer::SummarizerTarget,
    ) {
        let mode = summarizer::summarizer_mode();
        if mode == SummarizerMode::Off || packet.metadata.pending_summaries.is_empty() {
            return;
        }

        let session_id = session_id.to_string();
        let pending = packet.metadata.pending_summaries.clone();
        tokio::spawn(async move {
            summarizer::run_pending(mode, &session_id, pending, target).await;
        });
    }
}
