//! Active ACP prompt-run registration, cancellation, steering, and close cleanup.
//!
//! Maintainers: AgentManager busy pins and the ACP run map must change atomically.
//! Clients: run identifiers, steer fences, cancellation, and close behavior remain stable.

use super::*;

pub(super) struct ActivePromptRun {
    run_id: String,
    cancel_token: CancellationToken,
}

pub(super) async fn register_active_prompt_run(
    active_prompt_runs: &Mutex<HashMap<String, ActivePromptRun>>,
    agent_manager: &AgentManager,
    session_id: &str,
    run_id: String,
    cancel_token: CancellationToken,
) -> Result<(), agent_client_protocol::Error> {
    {
        let active_prompt_runs = active_prompt_runs.lock().await;
        if let Some(active_run) = active_prompt_runs.get(session_id) {
            return Err(agent_client_protocol::Error::invalid_params().data(format!(
                "session already has active run `{}`; use _gosling/unstable/session/steer",
                active_run.run_id.as_str()
            )));
        }
    }

    agent_manager
        .try_register_cancel_token(session_id, cancel_token.clone())
        .await
        .map_err(|error| agent_client_protocol::Error::invalid_params().data(error.to_string()))?;

    active_prompt_runs.lock().await.insert(
        session_id.to_string(),
        ActivePromptRun {
            run_id,
            cancel_token,
        },
    );
    Ok(())
}

/// Returns `Some(was_cancelled)` if `run_id` matched the session's active run
/// (and has now been cleared), or `None` if there was nothing to clear.
pub(super) async fn unregister_active_prompt_run(
    active_prompt_runs: &Mutex<HashMap<String, ActivePromptRun>>,
    agent_manager: &AgentManager,
    session_id: &str,
    run_id: &str,
) -> Option<bool> {
    let was_cancelled = {
        let mut active_prompt_runs = active_prompt_runs.lock().await;
        let active_run = active_prompt_runs.get(session_id)?;
        if active_run.run_id != run_id {
            return None;
        }
        let was_cancelled = active_run.cancel_token.is_cancelled();
        active_prompt_runs.remove(session_id);
        was_cancelled
    };
    agent_manager.unregister_cancel_token(session_id).await;
    Some(was_cancelled)
}

impl GoslingAcpAgent {
    pub(super) async fn start_active_run(
        &self,
        session_id: &str,
        run_id: String,
        cancel_token: CancellationToken,
    ) -> Result<(), agent_client_protocol::Error> {
        if self.closed_session_ids.lock().await.contains(session_id) {
            return Err(agent_client_protocol::Error::resource_not_found(Some(
                session_id.to_string(),
            ))
            .data(format!("Session not found: {}", session_id)));
        }

        register_active_prompt_run(
            &self.active_prompt_runs,
            &self.agent_manager,
            session_id,
            run_id,
            cancel_token,
        )
        .await
    }

    pub(super) async fn clear_active_run(&self, session_id: &str, run_id: &str) {
        let Some(was_cancelled) = unregister_active_prompt_run(
            &self.active_prompt_runs,
            &self.agent_manager,
            session_id,
            run_id,
        )
        .await
        else {
            return;
        };

        // A steer queued for this run only belongs to a *future* run when the
        // run it targeted was explicitly cancelled — an uncancelled run drains
        // its own pending steers before it lets itself finish (see
        // `reply_stream.rs`'s `has_pending_steers` checks), so anything left
        // here on a normal completion is, at worst, a narrow race that should
        // still reach the user on the next turn rather than vanish silently.
        if was_cancelled {
            let agent = {
                let sessions = self.sessions.lock().await;
                sessions
                    .get(session_id)
                    .map(|session| session.agent.clone())
            };
            if let Some(agent) = agent {
                agent.discard_pending_steers(session_id).await;
            }
        }

        if self.closed_session_ids.lock().await.contains(session_id) {
            self.sessions.lock().await.remove(session_id);
            if let Err(error) = self
                .agent_manager
                .remove_session_if_loaded(session_id)
                .await
            {
                warn!(
                    session_id,
                    %error,
                    "Failed to remove in-memory agent for closed session"
                );
            }
        }
    }

    pub(super) async fn require_active_run(
        &self,
        session_id: &str,
        expected_run_id: &str,
    ) -> Result<String, agent_client_protocol::Error> {
        if expected_run_id.is_empty() {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("expectedRunId must not be empty"));
        }

        let active_prompt_runs = self.active_prompt_runs.lock().await;
        let active_run = active_prompt_runs.get(session_id).ok_or_else(|| {
            agent_client_protocol::Error::invalid_params().data("no active run to steer")
        })?;
        if active_run.run_id != expected_run_id {
            return Err(
                agent_client_protocol::Error::invalid_params().data(serde_json::json!({
                    "message": format!(
                        "expected active run id `{expected_run_id}` but found `{}`",
                        active_run.run_id.as_str()
                    ),
                    "expectedRunId": expected_run_id,
                    "actualRunId": active_run.run_id.as_str(),
                })),
            );
        }
        Ok(active_run.run_id.clone())
    }

    fn active_run_meta(active_run_id: Option<&str>) -> Meta {
        let mut gosling = serde_json::Map::new();
        gosling.insert(
            "activeRunId".to_string(),
            active_run_id
                .map(|run_id| serde_json::Value::String(run_id.to_string()))
                .unwrap_or(serde_json::Value::Null),
        );

        let mut meta = serde_json::Map::new();
        meta.insert("gosling".to_string(), serde_json::Value::Object(gosling));
        meta
    }

    pub(super) fn send_active_run_update(
        cx: &ConnectionTo<Client>,
        session_id: &SessionId,
        active_run_id: Option<&str>,
    ) -> Result<(), agent_client_protocol::Error> {
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::SessionInfoUpdate(
                SessionInfoUpdate::new().meta(Self::active_run_meta(active_run_id)),
            ),
        ))
    }

    fn send_queued_steer_update(
        cx: &ConnectionTo<Client>,
        session_id: &SessionId,
        message_id: &str,
        run_id: &str,
    ) -> Result<(), agent_client_protocol::Error> {
        let mut gosling = serde_json::Map::new();
        gosling.insert(
            "queuedSteer".to_string(),
            serde_json::json!({
                "messageId": message_id,
                "runId": run_id,
            }),
        );
        let mut meta = serde_json::Map::new();
        meta.insert("gosling".to_string(), serde_json::Value::Object(gosling));

        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
        ))
    }

    pub(super) async fn on_steer_session(
        &self,
        req: SteerSessionRequest,
    ) -> Result<SteerSessionResponse, agent_client_protocol::Error> {
        if req.prompt.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("prompt must not be empty")
            );
        }

        self.require_active_run(&req.session_id, &req.expected_run_id)
            .await?;
        let agent = self.get_session_agent(&req.session_id).await?;
        let active_run_id = self
            .require_active_run(&req.session_id, &req.expected_run_id)
            .await?;

        let message = Self::convert_acp_prompt_to_message(&req.prompt);
        if message.content.is_empty() {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("prompt must contain steerable content"));
        }

        let message_id = format!("steer_{}", Uuid::new_v4());
        let message = message.with_id(message_id.clone());
        agent.steer(&req.session_id, message).await;

        if let Some(cx) = self.client_cx.get() {
            let _ = Self::send_queued_steer_update(
                cx,
                &SessionId::new(req.session_id.clone()),
                &message_id,
                &active_run_id,
            );
        }

        Ok(SteerSessionResponse {
            run_id: active_run_id,
            message_id,
        })
    }

    pub(super) async fn on_cancel(
        &self,
        args: CancelNotification,
    ) -> Result<(), agent_client_protocol::Error> {
        debug!(?args, "cancel request");

        let session_id = args.session_id.0.to_string();
        let token = {
            let active_prompt_runs = self.active_prompt_runs.lock().await;
            active_prompt_runs
                .get(&session_id)
                .map(|active_run| active_run.cancel_token.clone())
        };

        if let Some(token) = token {
            info!(session_id = %session_id, "prompt cancelled");
            token.cancel();
        } else if !self.sessions.lock().await.contains_key(&session_id) {
            warn!(session_id = %session_id, "cancel request for unknown session");
        }

        Ok(())
    }

    /// Blocks until `session_id` has no active prompt run. A provider/model switch
    /// applied while a turn is mid-flight can race an in-flight request that already
    /// captured the pre-switch provider/model pair (e.g. the request is sent to the
    /// newly-switched provider's endpoint but still carries the old model name), so
    /// callers changing provider or model wait here first and apply the change
    /// between turns instead. Unblocks on normal completion, error, or cancellation,
    /// since all of those paths clear the session's active run.
    pub(super) async fn wait_for_session_idle(&self, session_id: &str) {
        loop {
            let busy = self
                .active_prompt_runs
                .lock()
                .await
                .contains_key(session_id);
            if !busy {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }

    pub(super) async fn on_close_session(
        &self,
        session_id: &str,
    ) -> Result<CloseSessionResponse, agent_client_protocol::Error> {
        self.closed_session_ids
            .lock()
            .await
            .insert(session_id.to_string());

        let active_run_token = {
            let active_prompt_runs = self.active_prompt_runs.lock().await;
            active_prompt_runs
                .get(session_id)
                .map(|active_run| active_run.cancel_token.clone())
        };

        if let Some(token) = active_run_token {
            token.cancel();
        }

        let mut sessions = self.sessions.lock().await;
        sessions.remove(session_id);
        drop(sessions);

        self.agent_manager
            .remove_session_if_loaded(session_id)
            .await
            .internal_err_ctx("Failed to remove in-memory agent")?;

        info!(session_id = %session_id, "ACP session closed");
        Ok(CloseSessionResponse::new())
    }
}
