//! Agent streaming turn-loop state machine.
//!
//! Maintainers: this cohesive loop intentionally stays intact; preserve retry, tool,
//! cancellation, persistence, compaction, and stop-hook ordering as one state machine.
//! Clients: streamed events, retries, tool execution, and terminal behavior remain stable.

use super::*;

impl Agent {
    async fn inference_metadata_for(
        provider: &Arc<dyn Provider>,
        model_config: &ModelConfig,
    ) -> Option<InferenceMetadata> {
        let requested_model = model_config.model_name.clone();
        let resolved_model = provider
            .fetch_model_info(&requested_model)
            .await
            .ok()
            .and_then(|model_info| model_info.resolved_model);
        Some(InferenceMetadata {
            provider: provider.get_name().to_string(),
            requested_model,
            resolved_model,
        })
    }

    pub(super) async fn reply_internal(
        &self,
        conversation: Conversation,
        session_config: SessionConfig,
        session: Session,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let context = self
            .prepare_reply_context(
                &session.id,
                conversation,
                session.working_dir.as_path(),
                &session.additional_working_dirs,
            )
            .await?;
        let ReplyContext {
            mut conversation,
            mut tools,
            mut toolshim_tools,
            mut system_prompt,
            tool_call_cut_off,
            gosling_mode,
            model_config,
        } = context;

        // Kept separately (rather than only the merged `system_prompt`) so the
        // Context Manager can account for system vs. project-instructions
        // tokens as distinct slots instead of double-counting the addendum.
        let base_system_prompt = system_prompt.clone();
        let project_addendum = self.load_project_instructions(&session).await;
        if let Some(ref addendum) = project_addendum {
            system_prompt = format!("{system_prompt}\n\n{addendum}");
        }

        let primary_provider = self.provider().await?;
        let inference = Self::inference_metadata_for(&primary_provider, &model_config).await;
        let failover_target = self.provider_failover_target();
        let session_manager = self.config.session_manager.clone();
        let session_id = session_config.id.clone();
        if !self.config.disable_session_naming {
            let provider = primary_provider.clone();
            let manager_for_spawn = session_manager.clone();
            let session_name_update_tx = self.config.session_name_update_tx.clone();
            tokio::spawn(async move {
                match manager_for_spawn
                    .maybe_update_name(&session_id, provider)
                    .await
                {
                    Ok(Some(update)) => {
                        if let Some(tx) = session_name_update_tx {
                            if tx.send(update).is_err() {
                                warn!("Failed to publish generated session name");
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => warn!("Failed to generate session description: {}", e),
                }
            });
        }

        // Count tool calls present before this reply — everything added during
        // the reply loop is part of the current turn and should not be summarized.
        let pre_turn_tool_count = conversation
            .messages()
            .iter()
            .flat_map(|m| m.content.iter())
            .filter(|c| matches!(c, MessageContent::ToolRequest(_)))
            .count();

        let working_dir = session.working_dir.clone();
        let reply_stream_span = tracing::info_span!(
            target: "gosling::agents::agent",
            "reply_stream",
            trace_output = tracing::field::Empty,
            session.id = %session_config.id,
            session.user = %crate::session_context::session_user(),
            session.host = %crate::session_context::session_host(),
            session.agent_type = "gosling",
        );
        let inner = Box::pin(async_stream::try_stream! {
            let mut turns_taken = 0u32;
            let max_turns = session_config.max_turns.unwrap_or_else(|| {
                Config::global()
                    .get_param::<u32>("GOSLING_MAX_TURNS")
                    .unwrap_or(DEFAULT_MAX_TURNS)
            });
            let mut compaction_attempts = 0;
            let mut last_assistant_text = String::new();
            let mut goal_check_pending = false;
            let mut grind_nudges_sent = 0u32;
            let mut tool_pair_summarization_done = false;
            let mut stop_hook_handled_for_exit = false;
            let mut retrying_after_stop_hook_denial = false;
            let mut mid_stream_retries = 0usize;
            let mut retrying_stream = false;
            let mut active_provider = primary_provider.clone();
            let mut active_model_config = model_config.clone();
            let mut inference = inference;
            let mut failover_target = failover_target;
            let mut failover_attempted = false;
            let mut consecutive_stop_hook_blocks = 0u32;
            let stop_hook_block_cap = self.stop_hook_block_cap();
            let mut can_drain_pending_steers = false;
            let turn_started_at = chrono::Utc::now() - chrono::Duration::seconds(1);
            let research_state = crate::session::DeepResearchState::from_extension_data(&session.extension_data);
            let mut research_nudge_sent = false;

            loop {
                if is_token_cancelled(&cancel_token) {
                    break;
                }

                if can_drain_pending_steers {
                    for message in self.drain_pending_steers(&session_config.id).await {
                        let message_text = message.as_concat_text();
                        if self
                            .hook_manager
                            .has_hooks(crate::hooks::HookEvent::UserPromptSubmit)
                        {
                            let ctx = crate::hooks::HookContext::new(
                                crate::hooks::HookEvent::UserPromptSubmit,
                                &session_config.id,
                            )
                            .with_message(message_text);
                            self.hook_manager
                                .emit(crate::hooks::HookEvent::UserPromptSubmit, ctx)
                                .await;
                        }
                        session_manager.add_message(&session_config.id, &message).await?;
                        conversation.push(message.clone());
                        yield AgentEvent::Message(message);
                    }
                }

                // Neither a stop-hook retry nor a re-issued stream is a new turn:
                // counting them would spend the user's `max_turns` budget on
                // recovery rather than on work.
                if retrying_after_stop_hook_denial {
                    retrying_after_stop_hook_denial = false;
                } else if retrying_stream {
                    retrying_stream = false;
                } else {
                    turns_taken += 1;
                }
                if turns_taken > max_turns {
                    last_assistant_text = MAX_TURNS_MESSAGE.to_string();
                    yield AgentEvent::Message(Message::assistant().with_text(last_assistant_text.clone()));
                    break;
                }

                // Proactively compact if the conversation has grown past the threshold since
                // the check in reply(). This catches growth during tool loops, including
                // long approval-pending waits.
                // Reload the session to get current token counts — the stale snapshot
                // passed into reply_internal won't reflect updates from update_session_metrics.
                let current_session_for_compact = session_manager.get_session(&session_config.id, false).await?;
                if check_if_compaction_needed(
                    active_provider.as_ref(),
                    &conversation,
                    None,
                    &current_session_for_compact,
                )
                .await?
                {
                    let config = Config::global();
                    let threshold = config
                        .get_param::<f64>("GOSLING_AUTO_COMPACT_THRESHOLD")
                        .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
                    let threshold_percentage = (threshold * 100.0) as u32;

                    yield AgentEvent::Message(
                        Message::assistant().with_system_notification(
                            SystemNotificationType::InlineMessage,
                            format!(
                                "Exceeded auto-compact threshold of {}%. Performing auto-compaction...",
                                threshold_percentage
                            ),
                        )
                    );
                    yield AgentEvent::Message(
                        Message::assistant().with_system_notification(
                            SystemNotificationType::ThinkingMessage,
                            COMPACTION_THINKING_TEXT,
                        )
                    );

                    let auto_compact_budget = crate::context_mgmt::auto_compact_reduction_budget(
                        active_provider.as_ref(),
                        &conversation,
                        &current_session_for_compact,
                        None,
                        None,
                    )
                    .await?;

                    match self.perform_compact_with_provider(
                        active_provider.clone(),
                        &active_model_config,
                        &session_config,
                        &conversation,
                        auto_compact_budget,
                    ).await {
                        Ok(compacted_conversation) => {
                            conversation = compacted_conversation;
                            yield AgentEvent::HistoryReplaced(conversation.clone());
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    "Compaction complete",
                                )
                            );
                        }
                        Err(e) => {
                            yield AgentEvent::Message(
                                Message::assistant()
                                    .with_text(crate::context_mgmt::compaction_failure_message(&e))
                            );
                            break;
                        }
                    }
                }

                let conversation_with_moim = crate::agents::moim::inject_moim(
                    &session_config.id,
                    &conversation,
                    &self.extension_manager,
                    turns_taken,
                    max_turns,
                )
                .await;
                let conversation_for_context = conversation_with_moim.as_ref().unwrap_or(&conversation);

                let (provider_system_prompt, provider_messages) = self
                    .apply_context_manager(
                        active_provider.as_ref(),
                        &session_config.id,
                        &base_system_prompt,
                        project_addendum.as_deref(),
                        &system_prompt,
                        conversation_for_context,
                        &active_model_config,
                        &working_dir,
                    )
                    .await;

                let mut stream = Self::stream_response_from_provider(
                    active_provider.clone(),
                    active_model_config.clone(),
                    &session_config.id,
                    &provider_system_prompt,
                    &provider_messages,
                    &tools,
                    &toolshim_tools,
                ).await?;
                last_assistant_text.clear();

                let current_turn_tool_count = conversation.messages().iter()
                    .flat_map(|m| m.content.iter())
                    .filter(|c| matches!(c, MessageContent::ToolRequest(_)))
                    .count()
                    .saturating_sub(pre_turn_tool_count);

                let tool_pair_summarization_task = if tool_pair_summarization_done {
                    None
                } else {
                    crate::context_mgmt::maybe_summarize_tool_pairs(
                        active_provider.clone(),
                        active_model_config.clone(),
                        session_config.id.clone(),
                        &conversation,
                        tool_call_cut_off,
                        current_turn_tool_count,
                    )
                };

                let mut no_tools_called = true;
                let mut messages_to_add = Conversation::default();
                let mut tools_updated = false;
                let mut did_recovery_compact_this_iteration = false;
                let mut exit_chat = false;
                let stream_message_id = format!("msg_{}", Uuid::new_v4());
                let mut last_stream_checkpoint_at: Option<Instant> = None;
                let mut last_stream_checkpoint_id: Option<String> = None;
                // First message this stream persisted, so a mid-stream failure can
                // truncate the session back to where the stream began.
                let mut stream_rollback_anchor: Option<String> = None;

                // Track whether this provider turn has already emitted visible
                // thinking so a later tool-call chunk can suppress replayed
                // reasoning without hiding final-only non-streaming thoughts.
                let mut surfaced_thinking_in_turn = false;

                while let Some(next) = stream.next().await {
                    if is_token_cancelled(&cancel_token) || exit_chat {
                        break;
                    }

                    match next {
                        Ok((response, usage)) => {
                            compaction_attempts = 0;

                            if let Some(ref usage) = usage {
                                self.update_session_metrics(&session_config.id, usage, false).await?;
                                yield AgentEvent::Usage(usage.clone());
                            }

                            if let Some(response) = response {
                                let response = if response.id.is_some() {
                                    response
                                } else {
                                    response.with_id(stream_message_id.clone())
                                };
                                let ToolCategorizeResult {
                                    frontend_requests,
                                    remaining_requests,
                                    filtered_response,
                                } = self
                                    .categorize_tools(
                                        &response,
                                        &tools,
                                        surfaced_thinking_in_turn,
                                    )
                                    .await;

                                let mut filtered_response = if let Some(inference) = inference.as_ref() {
                                    filtered_response.with_inference(inference.clone())
                                } else {
                                    filtered_response
                                };
                                let mut response = if let Some(inference) = inference.as_ref() {
                                    response.with_inference(inference.clone())
                                } else {
                                    response
                                };

                                if gosling_mode == GoslingMode::Auto {
                                    let mut permission_request_ids =
                                        take_tool_confirmation_requests(&mut response);
                                    for request_id in
                                        take_tool_confirmation_requests(&mut filtered_response)
                                    {
                                        if !permission_request_ids.contains(&request_id) {
                                            permission_request_ids.push(request_id);
                                        }
                                    }

                                    for request_id in permission_request_ids {
                                        self.handle_confirmation(
                                            request_id,
                                            PermissionConfirmation {
                                                principal_type: PrincipalType::Tool,
                                                permission: Permission::DenyOnce,
                                            },
                                        )
                                        .await;
                                    }

                                    if filtered_response.content.is_empty() {
                                        continue;
                                    }
                                }

                                surfaced_thinking_in_turn |= filtered_response.content.iter().any(
                                    |content| {
                                        matches!(
                                            content,
                                            MessageContent::Thinking(_)
                                                | MessageContent::RedactedThinking(_)
                                        )
                                    },
                                );

                                let num_tool_requests = frontend_requests.len() + remaining_requests.len();
                                if num_tool_requests == 0 {
                                    let text = filtered_response.as_concat_text();
                                    if !text.is_empty() {
                                        last_assistant_text.push_str(&text);
                                    }
                                    messages_to_add.push(response);

                                    if let Some(message) = messages_to_add.last() {
                                        let is_new_message = message.id.as_deref()
                                            != last_stream_checkpoint_id.as_deref();
                                        let checkpoint_due = last_stream_checkpoint_at
                                            .map(|checkpoint| checkpoint.elapsed() >= STREAM_CHECKPOINT_INTERVAL)
                                            .unwrap_or(true);
                                        if is_new_message || checkpoint_due {
                                            session_manager
                                                .upsert_message(&session_config.id, message)
                                                .await?;
                                            last_stream_checkpoint_at = Some(Instant::now());
                                            last_stream_checkpoint_id = message.id.clone();
                                            if stream_rollback_anchor.is_none() {
                                                stream_rollback_anchor = message.id.clone();
                                            }
                                        }
                                    }

                                    yield AgentEvent::Message(filtered_response.clone());
                                    tokio::task::yield_now().await;
                                    continue;
                                }

                                yield AgentEvent::Message(filtered_response.clone());
                                tokio::task::yield_now().await;

                                let mut request_to_response_map = HashMap::new();
                                let mut request_metadata: HashMap<String, Option<ProviderMetadata>> = HashMap::new();
                                for request in frontend_requests.iter().chain(remaining_requests.iter()) {
                                    request_to_response_map.insert(request.id.clone(), Message::user().with_generated_id());
                                    request_metadata.insert(request.id.clone(), request.metadata.clone());
                                }

                                let direct_thinking: Vec<MessageContent> = response
                                    .content
                                    .iter()
                                    .filter(|content| {
                                        matches!(
                                            content,
                                            MessageContent::Thinking(_)
                                                | MessageContent::RedactedThinking(_)
                                        )
                                    })
                                    .cloned()
                                    .collect();
                                if !direct_thinking.is_empty() {
                                    let thinking_msg = Message::new(
                                        response.role.clone(),
                                        response.created,
                                        direct_thinking.clone(),
                                    )
                                    .with_id(format!("msg_{}", Uuid::new_v4()));
                                    session_manager
                                        .upsert_message(&session_config.id, &thinking_msg)
                                        .await?;
                                    messages_to_add.push(thinking_msg);
                                }
                                let response_thinking = if direct_thinking.is_empty() {
                                    messages_to_add
                                        .messages()
                                        .iter()
                                        .rev()
                                        .find(|message| {
                                            message.role == response.role
                                                && !message.content.is_empty()
                                                && message.content.iter().all(|content| {
                                                    matches!(
                                                        content,
                                                        MessageContent::Thinking(_)
                                                            | MessageContent::RedactedThinking(_)
                                                    )
                                                })
                                        })
                                        .map(|message| message.content.clone())
                                        .unwrap_or_default()
                                } else {
                                    direct_thinking
                                };

                                let mut request_msg = Message::assistant()
                                    .with_id(format!("msg_{}", Uuid::new_v4()));
                                if let Some(inference) = inference.as_ref() {
                                    request_msg = request_msg.with_inference(inference.clone());
                                }
                                for thinking in &response_thinking {
                                    request_msg = request_msg.with_content(thinking.clone());
                                }
                                for content in response.content.iter().filter(|content| {
                                    matches!(content, MessageContent::Text(_) | MessageContent::Image(_))
                                }) {
                                    request_msg = request_msg.with_content(content.clone());
                                }
                                for request in frontend_requests.iter().chain(remaining_requests.iter()) {
                                    let history_tool_call = match &request.tool_call {
                                        Ok(_) => request.tool_call.clone(),
                                        Err(_) => Ok(CallToolRequestParams::new(
                                            "unparseable_tool_call",
                                        )
                                        .with_arguments(serde_json::Map::new())),
                                    };
                                    request_msg = request_msg.with_tool_request_with_metadata(
                                        request.id.clone(),
                                        history_tool_call,
                                        request.metadata.as_ref(),
                                        request.tool_meta.clone(),
                                    );
                                    if let Some(response_placeholder) =
                                        request_to_response_map.get(&request.id)
                                    {
                                        if request_msg.created > response_placeholder.created {
                                            request_msg.created = response_placeholder.created;
                                        }
                                    }
                                }
                                session_manager
                                    .upsert_message(&session_config.id, &request_msg)
                                    .await?;
                                messages_to_add.push(request_msg);

                                // Chat mode must run no tools at all. This loop
                                // used to sit above the `GoslingMode::Chat`
                                // branch below, which only skipped
                                // `remaining_requests` — so frontend tool
                                // requests still executed in the one mode whose
                                // entire contract is "answer, don't act".
                                // (STT-GOS-001)
                                if gosling_mode == GoslingMode::Chat {
                                    for request in frontend_requests.iter() {
                                        Self::record_chat_mode_tool_skip(
                                            request,
                                            &mut request_to_response_map,
                                        );
                                    }
                                } else {
                                    for request in frontend_requests.iter() {
                                        let response_msg = request_to_response_map.get_mut(&request.id)
                                            .ok_or_else(|| anyhow::anyhow!("missing response entry for request {}", request.id))?;
                                        let mut frontend_tool_stream = self.handle_frontend_tool_request(
                                            request,
                                            response_msg,
                                            &session,
                                        );

                                        while let Some(msg) = frontend_tool_stream.try_next().await? {
                                            yield AgentEvent::Message(msg);
                                        }
                                    }
                                }
                                if gosling_mode == GoslingMode::Chat {
                                    for request in remaining_requests.iter() {
                                        Self::record_chat_mode_tool_skip(
                                            request,
                                            &mut request_to_response_map,
                                        );
                                    }
                                } else {
                                    let inspection_results = self
                                        .tool_inspection_manager
                                        .inspect_tools(
                                            &session_config.id,
                                            &remaining_requests,
                                            conversation.messages(),
                                            gosling_mode,
                                        )
                                        .await?;

                                    let mut permission_check_result = self
                                        .tool_inspection_manager
                                        .process_inspection_results_with_permission_inspector(
                                            &remaining_requests,
                                            &inspection_results,
                                        )
                                        .unwrap_or_else(|| {
                                            let mut result = PermissionCheckResult {
                                                approved: vec![],
                                                needs_approval: vec![],
                                                denied: vec![],
                                            };
                                            result
                                                .needs_approval
                                                .extend(remaining_requests.iter().cloned());
                                            result
                                        });

                                    Self::redirect_unapprovable_subagent_requests(
                                        gosling_mode,
                                        session.session_type,
                                        &mut permission_check_result,
                                        &mut request_to_response_map,
                                    );

                                    // Track extension requests
                                    let mut enable_extension_request_ids = vec![];
                                    for request in &remaining_requests {
                                        if let Ok(tool_call) = &request.tool_call {
                                            if tool_call.name == MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE {
                                                enable_extension_request_ids.push(request.id.clone());
                                            }
                                        }
                                    }

                                    let mut tool_futures = self.handle_approved_and_denied_tools(
                                        &permission_check_result,
                                        &mut request_to_response_map,
                                        cancel_token.clone(),
                                        &session,
                                    ).await?;

                                    {
                                        let mut tool_approval_stream = self.handle_approval_tool_requests(
                                            &permission_check_result.needs_approval,
                                            &mut tool_futures,
                                            &mut request_to_response_map,
                                            cancel_token.clone(),
                                            &session,
                                            &inspection_results,
                                        );

                                        while let Some(msg) = tool_approval_stream.try_next().await? {
                                            yield AgentEvent::Message(msg);
                                        }
                                    }

                                    let with_id = tool_futures
                                        .into_iter()
                                        .map(|(request_id, stream)| {
                                            stream.map(move |item| (request_id.clone(), item))
                                        })
                                        .collect::<Vec<_>>();

                                    let mut combined = stream::select_all(with_id);
                                    let mut all_install_successful = true;
                                    let mut tool_persistence_error = None;

                                    loop {
                                        if is_token_cancelled(&cancel_token) {
                                            break;
                                        }

                                        tokio::select! {
                                            biased;

                                            tool_item = combined.next() => {
                                                match tool_item {
                                                    Some((request_id, item)) => {
                                                        match item {
                                                            ToolStreamItem::ActionRequired(mut msg) => {
                                                                if msg.id.is_none() {
                                                                    msg = msg.with_generated_id();
                                                                }
                                                                if let Err(e) = session_manager.add_message(&session_config.id, &msg).await {
                                                                    warn!("Failed to save elicitation message to session: {}", e);
                                                                }
                                                                yield AgentEvent::Message(msg);
                                                            }
                                                            ToolStreamItem::Result(output) => {
                                                                if let Ok(ref call_result) = output {
                                                                    if let Some(ref meta) = call_result.meta {
                                                                        if let Some(notification_data) = meta.0.get("platform_notification") {
                                                                            if let Some(method) = notification_data.get("method").and_then(|v| v.as_str()) {
                                                                                let params = notification_data.get("params").cloned();
                                                                                let custom_notification = rmcp::model::CustomNotification::new(
                                                                                    method.to_string(),
                                                                                    params,
                                                                                );

                                                                                let server_notification = rmcp::model::ServerNotification::CustomNotification(custom_notification);
                                                                                yield AgentEvent::McpNotification((request_id.clone(), server_notification));
                                                                            }
                                                                        }
                                                                    }
                                                                }

                                                                if enable_extension_request_ids.contains(&request_id)
                                                                    && output.is_err()
                                                                {
                                                                    all_install_successful = false;
                                                                }
                                                                if let Some(response) = request_to_response_map.get_mut(&request_id) {
                                                                    let metadata = request_metadata.get(&request_id).and_then(|m| m.as_ref());
                                                                    response.add_tool_response_with_metadata(request_id.clone(), output, metadata);
                                                                    if let Err(error) = session_manager
                                                                        .persist_tool_operation_response(
                                                                            &session_config.id,
                                                                            &request_id,
                                                                            response,
                                                                        )
                                                                        .await
                                                                    {
                                                                        tool_persistence_error = Some(error);
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                            ToolStreamItem::Message(msg) => {
                                                                yield AgentEvent::McpNotification((request_id, msg));
                                                            }
                                                        }
                                                    }
                                                    None => break,
                                                }
                                            }

                                            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                                        }
                                    }

                                    if let Some(error) = tool_persistence_error {
                                        Err(error)?;
                                    }

                                    if all_install_successful && !enable_extension_request_ids.is_empty() {
                                        if let Err(e) = self.save_extension_state(&session_config).await {
                                            warn!("Failed to save extension state after runtime changes: {}", e);
                                        }
                                        tools_updated = true;
                                    }
                                }

                                for request in frontend_requests.iter().chain(remaining_requests.iter()) {
                                    let final_response = match &request.tool_call {
                                        Ok(_) => {
                                            let Some(response) =
                                                request_to_response_map.remove(&request.id)
                                            else {
                                                continue;
                                            };
                                            let has_tool_response =
                                                response.content.iter().any(|c| {
                                                    matches!(c, MessageContent::ToolResponse(r) if r.id == request.id)
                                                });
                                            if !has_tool_response {
                                                // Cancelled before this tool call's result
                                                // arrived: the placeholder is still empty.
                                                // Leave it out of this turn's persisted
                                                // history rather than pairing an
                                                // already-durable ToolRequest with a
                                                // misleading empty response
                                                // (AOC-ORCH-002); `recover_tool_operations`
                                                // synthesizes the correct in-doubt response
                                                // for it on the next `reply()` call.
                                                continue;
                                            }
                                            response
                                        }
                                        Err(error) => {
                                            error!("Tool call could not be parsed: {error}");
                                            let mut response = request_to_response_map
                                                .remove(&request.id)
                                                .unwrap_or_else(|| Message::user().with_generated_id());
                                            // Only feed the parse error back if this id isn't
                                            // already answered. In Chat mode the skip branch above
                                            // already added a tool response for it; adding another
                                            // here would duplicate the tool_call_id (which strict
                                            // providers reject).
                                            let already_answered = response.content.iter().any(|c| {
                                                matches!(c, MessageContent::ToolResponse(r) if r.id == request.id)
                                            });
                                            if !already_answered {
                                                response.add_tool_response_with_metadata(
                                                    request.id.clone(),
                                                    Err(error.clone()),
                                                    request.metadata.as_ref(),
                                                );
                                            }
                                            response
                                        }
                                    };

                                    yield AgentEvent::Message(final_response.clone());
                                    messages_to_add.push(final_response);
                                }

                                no_tools_called = false;
                                // Agent is actively working — re-check goal when it next finishes
                                goal_check_pending = false;
                            }
                        }
                        #[allow(unused_variables)]
                        Err(ref provider_err @ ProviderError::ContextLengthExceeded(_)) => {
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            compaction_attempts += 1;

                            if compaction_attempts >= 2 {
                                error!("Context limit exceeded after compaction - prompt too large");
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    "Unable to continue: Context limit still exceeded after compaction. Try using a shorter message, a model with a larger context window, or start a new session."
                                ).with_terminal_error("Context limit still exceeded after compaction")
                            );
                                break;
                            }

                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    "Context limit reached. Compacting to continue conversation...",
                                )
                            );
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::ThinkingMessage,
                                    COMPACTION_THINKING_TEXT,
                                )
                            );

                            match self
                                .perform_compact_with_provider(
                                    active_provider.clone(),
                                    &active_model_config,
                                    &session_config,
                                    &conversation,
                                    // The provider already hit its hard context limit, so this
                                    // must fully resolve in one pass rather than take a soft,
                                    // budget-capped trim — same as a manual /compact.
                                    None,
                                )
                                .await
                            {
                                Ok(compacted_conversation) => {
                                    conversation = compacted_conversation;
                                    did_recovery_compact_this_iteration = true;
                                    yield AgentEvent::HistoryReplaced(conversation.clone());
                                    break;
                                }
                                Err(e) => {
                                    #[cfg(feature = "telemetry")]
                                    crate::posthog::emit_error("compaction_failed", &e.to_string());
                                    error!("Compaction failed: {}", e);
                                    yield AgentEvent::Message(
                                        Message::assistant()
                                            .with_text(crate::context_mgmt::compaction_failure_message(&e))
                                            .with_terminal_error(e.to_string())
                                    );
                                    break;
                                }
                            }
                        }
                        Err(ref provider_err @ ProviderError::CreditsExhausted { details: _, ref top_up_url }) => {
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);

                            let user_msg = if top_up_url.is_some() {
                                "Please add credits to your account, then resend your message to continue.".to_string()
                            } else {
                                "Please check your account with your provider to add more credits, then resend your message to continue.".to_string()
                            };

                            let notification_data = serde_json::json!({
                                "top_up_url": top_up_url,
                            });

                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification_with_data(
                                    SystemNotificationType::CreditsExhausted,
                                    user_msg,
                                    notification_data,
                                ).with_terminal_error(provider_err.to_string())
                            );
                            break;
                        }
                        Err(ref provider_err @ ProviderError::Refusal { ref details, ref category }) => {
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);

                            let category = category.as_deref().map(|c| format!("\n\nCategory: {c}")).unwrap_or_default();
                            yield AgentEvent::Message(
                                Message::assistant().with_text(format!(
                                    "The provider refused this request.\n\n{details}{category}\n\nPlease start a new session to continue — resending this conversation is likely to be refused again."
                                )).with_terminal_error(provider_err.to_string())
                            );
                            // A refusal is terminal: skip goal/grind nudges,
                            // which would resend the same refused conversation.
                            exit_chat = true;
                            break;
                        }
                        // A stream that dies before any of its tools have run can be
                        // re-issued: the partial assistant message is rolled back out
                        // of the session and the UI, and the outer loop asks the
                        // provider again from the same conversation. Once a tool has
                        // run this arm is skipped — replaying it could repeat a side
                        // effect — and the error falls through to the arms below.
                        Err(ref provider_err) if no_tools_called
                            && mid_stream_retries < MAX_MID_STREAM_RETRIES
                            && should_retry(provider_err, &RetryConfig::default()) => {
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());

                            mid_stream_retries += 1;
                            warn!(
                                "Provider stream failed mid-response, retrying ({}/{}): {}",
                                mid_stream_retries, MAX_MID_STREAM_RETRIES, provider_err
                            );

                            if let Some(anchor) = stream_rollback_anchor.take() {
                                session_manager
                                    .truncate_conversation_from_message(&session_config.id, &anchor)
                                    .await?;
                            }
                            // Dropping this keeps the partial answer out of the
                            // conversation the retry is built from — the tail of
                            // this iteration would otherwise extend it in.
                            messages_to_add = Conversation::default();
                            yield AgentEvent::HistoryReplaced(conversation.clone());

                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    format!(
                                        "The model's response was interrupted. Retrying ({mid_stream_retries}/{MAX_MID_STREAM_RETRIES})..."
                                    ),
                                )
                            );

                            let backoff = RetryConfig::default().delay_for_attempt(mid_stream_retries);
                            match cancel_token.as_ref() {
                                Some(token) => {
                                    tokio::select! {
                                        _ = tokio::time::sleep(backoff) => {}
                                        _ = token.cancelled() => {}
                                    }
                                }
                                None => tokio::time::sleep(backoff).await,
                            }

                            retrying_stream = true;
                            break;
                        }
                        Err(ref provider_err) if no_tools_called
                            && !failover_attempted
                            && failover_target.is_some()
                            && should_retry(provider_err, &RetryConfig::default()) => {
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());

                            failover_attempted = true;
                            let target = failover_target
                                .take()
                                .expect("failover target checked above");
                            match self
                                .resolve_provider_failover(
                                    target,
                                    &session,
                                    primary_provider.as_ref(),
                                    &model_config,
                                )
                                .await
                            {
                                Ok(failover) => {
                                    let fallback_provider_name = failover.provider.get_name().to_string();
                                    let fallback_model_name = failover.model_config.model_name.clone();
                                    warn!(
                                        "Primary provider remained unavailable; switching this turn to fallback provider '{}' model '{}'",
                                        fallback_provider_name,
                                        fallback_model_name
                                    );

                                    if let Some(anchor) = stream_rollback_anchor.take() {
                                        session_manager
                                            .truncate_conversation_from_message(&session_config.id, &anchor)
                                            .await?;
                                    }
                                    messages_to_add = Conversation::default();
                                    yield AgentEvent::HistoryReplaced(conversation.clone());

                                    active_provider = failover.provider;
                                    active_model_config = failover.model_config;
                                    inference = Self::inference_metadata_for(
                                        &active_provider,
                                        &active_model_config,
                                    ).await;
                                    mid_stream_retries = 0;
                                    retrying_stream = true;

                                    yield AgentEvent::Message(
                                        Message::assistant().with_system_notification(
                                            SystemNotificationType::InlineMessage,
                                            format!(
                                                "The primary model remained unavailable. Continuing this turn from the last checkpoint with fallback {fallback_provider_name} / {fallback_model_name}."
                                            ),
                                        )
                                    );
                                }
                                Err(failover_error) => {
                                    error!("Configured provider failover is unavailable: {failover_error}");
                                    yield AgentEvent::Message(provider_failure_message(
                                        provider_err,
                                        &format!(
                                            "Ran into this error: {provider_err}.\n\nThe configured failover could not start: {failover_error}"
                                        ),
                                        true,
                                    ));
                                }
                            }
                            break;
                        }
                        Err(ref provider_err @ ProviderError::NetworkError(_)) => {
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);
                            yield AgentEvent::Message(provider_failure_message(
                                provider_err,
                                &format!("{provider_err}\n\nPlease resend your message to try again."),
                                no_tools_called,
                            ));
                            break;
                        }
                        Err(ref provider_err) => {
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);
                            yield AgentEvent::Message(provider_failure_message(
                                provider_err,
                                &format!("Ran into this error: {provider_err}.\n\nPlease retry if you think this is a transient or recoverable error."),
                                no_tools_called,
                            ));
                            break;
                        }
                    }
                }
                can_drain_pending_steers = true;

                // The budget is for consecutive failures. A stream that ran to
                // completion means the connection recovered, so the next blip
                // starts from a full allowance rather than an exhausted one.
                if !retrying_stream {
                    mid_stream_retries = 0;
                }

                if tools_updated {
                    (tools, toolshim_tools, system_prompt, _) = self
                        .prepare_tools_and_prompt_with_additional_dirs(
                            &session_config.id,
                            &session.working_dir,
                            &session.additional_working_dirs,
                        )
                        .await?;
                }

                {
                    let hint_text = self
                        .subdirectory_hint_tracker
                        .lock()
                        .await
                        .collect_new_hints(&working_dir);
                    if let Some(hints) = hint_text {
                        messages_to_add
                            .push(Message::user().with_text(hints).with_visibility(false, true));
                    }
                }

                if no_tools_called && !exit_chat {
                    if did_recovery_compact_this_iteration || retrying_stream {
                        // continue from last user message after recovery compact,
                        // or re-issue the request the failed stream was serving —
                        // in neither case has the assistant actually answered yet
                    } else if self.has_pending_steers(&session_config.id).await {
                    } else {
                        // Clone out of the mutexes before branching: an `if let`
                        // scrutinee that locks keeps its guard alive for the whole
                        // if/else chain, which would deadlock against
                        // set_goal/set_grind in the final arm.
                        let goal_nudge = if goal_check_pending {
                            None
                        } else {
                            self.goal.lock().await.clone()
                        };
                        let grind_nudge = self.grind.lock().await.clone();
                        let research_report_missing = match &research_state {
                            Some(state) if !research_nudge_sent => {
                                !crate::session::research::turn_wrote_output_deliverable(
                                    &session_manager,
                                    &session_config.id,
                                    state,
                                    turn_started_at,
                                    &last_assistant_text,
                                )
                                .await
                            }
                            _ => false,
                        };
                        if let Some(goal) = goal_nudge {
                            goal_check_pending = true;
                            let nudge = format!(
                                "Before finishing, check whether the following goal has been fully met:\n\n\
                                 **Goal:** {goal}\n\n\
                                 If not, continue working toward it."
                            );
                            let message = Message::user().with_text(&nudge)
                                .with_visibility(false, true);
                            messages_to_add.push(message);
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    format!("Goal: {goal}"),
                                )
                            );
                        } else if let Some(grind) = grind_nudge {
                            if grind_nudges_sent < DEFAULT_MAX_GRIND_NUDGES {
                                grind_nudges_sent += 1;
                                let nudge = format!(
                                    "Keep working. The grind goal is not yet complete:\n\n\
                                     **Goal:** {grind}\n\n\
                                     Continue until it is fully done."
                                );
                                let message = Message::user().with_text(&nudge)
                                    .with_visibility(false, true);
                                messages_to_add.push(message);
                                yield AgentEvent::Message(
                                    Message::assistant().with_system_notification(
                                        SystemNotificationType::InlineMessage,
                                        format!("Grind: {grind}"),
                                    )
                                );
                            } else {
                                self.set_goal(None).await;
                                self.set_grind(None).await;
                                yield AgentEvent::Message(
                                    Message::assistant().with_text(MAX_GRIND_NUDGES_MESSAGE)
                                );
                                exit_chat = true;
                            }
                        } else if research_report_missing {
                            // A research turn that ends with the model announcing the
                            // report instead of writing it would otherwise fail the
                            // completion gate and wait on the operator. One hidden
                            // nudge finishes it; a second end is honoured as-is.
                            research_nudge_sent = true;
                            messages_to_add.push(
                                Message::user()
                                    .with_text(crate::session::research::RESEARCH_DELIVERABLE_NUDGE)
                                    .with_visibility(false, true),
                            );
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    "No report in Session Outputs yet; asking for it before finishing.",
                                )
                            );
                        } else {
                            self.set_goal(None).await;
                            self.set_grind(None).await;
                            exit_chat = true;
                        }
                    }
                }

                if is_token_cancelled(&cancel_token) {
                    if let Some(ref task) = tool_pair_summarization_task {
                        task.abort();
                    }
                }

                if let Some(task) = tool_pair_summarization_task {
                    tool_pair_summarization_done = true;
                    if let Ok(summaries) = task.await {
                        for (summary_msg, tool_id) in summaries {
                            let matching_ids: Vec<String> = conversation.messages()
                                .iter()
                                .filter(|msg| {
                                    msg.id.is_some() && msg.content.iter().any(|c| match c {
                                        MessageContent::ToolRequest(req) => req.id == tool_id,
                                        MessageContent::ToolResponse(resp) => resp.id == tool_id,
                                        _ => false,
                                    })
                                })
                                .filter_map(|msg| msg.id.clone())
                                .collect();

                            if matching_ids.len() == 2 {
                                for id in &matching_ids {
                                    SessionManager::update_message_metadata(&session_config.id, id, |metadata| {
                                        metadata.with_agent_invisible()
                                    }).await?;
                                }
                                session_manager.add_message(&session_config.id, &summary_msg).await?;
                            } else {
                                warn!("Expected a tool request/reply pair, but found {} matching messages",
                                    matching_ids.len());
                            }
                        }
                    }
                }

                let messages_to_add = if let Some(ref inference) = inference {
                    Conversation::new_unvalidated(
                        messages_to_add
                            .into_iter()
                            .map(|message| message.with_inference_if_assistant(inference.clone())),
                    )
                } else {
                    messages_to_add
                };

                for msg in &messages_to_add {
                    session_manager.upsert_message(&session_config.id, msg).await?;
                    session_manager
                        .register_completed_assistant_artifacts(&session_config.id, msg)
                        .await?;
                }
                conversation.extend(messages_to_add);

                if exit_chat && self.has_pending_steers(&session_config.id).await {
                    exit_chat = false;
                }

                if exit_chat {
                    match self
                        .emit_stop_hook_blocking(&session_config.id, &last_assistant_text)
                        .await
                    {
                        crate::hooks::HookDecision::Allow => {
                            stop_hook_handled_for_exit = true;
                            break;
                        }
                        crate::hooks::HookDecision::Deny { reason, plugin } => {
                            consecutive_stop_hook_blocks += 1;
                            if consecutive_stop_hook_blocks > stop_hook_block_cap {
                                let message = stop_hook_block_cap_warning(&plugin, stop_hook_block_cap);
                                session_manager.add_message(&session_config.id, &message).await?;
                                yield AgentEvent::Message(message);
                                stop_hook_handled_for_exit = true;
                                break;
                            }
                            let message = stop_hook_denial_context_message(&plugin, &reason);
                            session_manager.add_message(&session_config.id, &message).await?;
                            conversation.push(message);
                            yield AgentEvent::Message(stop_hook_denial_notification(&plugin));
                            retrying_after_stop_hook_denial = true;
                        }
                    }
                }

                tokio::task::yield_now().await;
            }

            if !last_assistant_text.is_empty() {
                tracing::Span::current().record("trace_output", last_assistant_text.as_str());
            }

            if !stop_hook_handled_for_exit {
                self.emit_stop_hook(&session_config.id, &last_assistant_text).await;
            }

            summarizer::spawn_session_rollup(
                summarizer::summarizer_mode(),
                session_manager.clone(),
                session_config.id.clone(),
                session_config.tail_limit.unwrap_or(DEFAULT_SESSION_TAIL_LIMIT),
            );
        }.instrument(reply_stream_span));
        Ok(inner)
    }
}
