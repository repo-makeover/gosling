// Owns background delegation startup, progress counters, and task registration.
// Extracted from `summon.rs` in a behavior-preserving modularization.
// The `summon` compatibility facade exposes this behavior through MCP delegation only.

use super::delegate_config::{delegate_mode, delegate_mode_notice};
use super::*;

impl SummonClient {
    pub(super) async fn handle_async_delegate(
        &self,
        session_id: &str,
        params: DelegateParams,
    ) -> Result<(Vec<Content>, String), String> {
        let task_slot = self.try_reserve_background_task_slot()?;

        let session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|e| format!("Failed to get session: {}", e))?;

        let working_dir = session.working_dir.clone();
        let spec = self.build_delegate_spec(&params, &working_dir).await?;

        let task_config = self
            .build_task_config(&params, &spec, &session)
            .await
            .map_err(|e| format!("Failed to build task config: {}", e))?;
        let authority_summary = delegate_authority_summary(&task_config.extensions);
        let subagent_mode = delegate_mode(task_config.provider.executes_tools_outside_gosling());

        let description = safe_truncate(&Self::get_task_description(&params), TASK_LABEL_BUDGET);

        // Hosted-tool subagents use Auto because no UI is attached to answer
        // approval prompts. External-tool providers cannot safely use Auto;
        // Chat mode keeps those providers available for bounded text work while
        // rejecting their delegated tool calls at the ACP boundary.
        let agent_config = AgentConfig::new(
            self.context.session_manager.clone(),
            crate::config::permission::PermissionManager::instance(),
            subagent_mode,
            true, // disable session naming for subagents
            crate::agents::GoslingPlatform::GoslingCli,
        )
        .with_code_execution_runtime(self.context.code_execution_runtime)
        .with_use_login_shell_path(self.context.use_login_shell_path);

        let subagent_session = self
            .context
            .session_manager
            .create_session(
                task_config.parent_working_dir.clone(),
                description.clone(),
                SessionType::SubAgent,
                subagent_mode,
            )
            .await
            .map_err(|e| format!("Failed to create subagent session: {}", e))?;
        self.context
            .session_manager
            .merge_extension_state(
                &subagent_session.id,
                "output_agent.v1",
                serde_json::json!({ "name": params.source, "parentSessionId": session_id }),
            )
            .await
            .map_err(|e| format!("Failed to record subagent identity: {e}"))?;

        let task_id = subagent_session.id.clone();

        let turns = Arc::new(AtomicU32::new(0));
        let last_activity = Arc::new(AtomicU64::new(current_epoch_millis()));

        let turns_clone = Arc::clone(&turns);
        let last_activity_clone = Arc::clone(&last_activity);

        let on_message: OnMessageCallback = Arc::new(move |_msg| {
            turns_clone.fetch_add(1, Ordering::Relaxed);
            last_activity_clone.store(current_epoch_millis(), Ordering::Relaxed);
        });

        let task_token = CancellationToken::new();
        let task_token_clone = task_token.clone();

        let notification_buffer = Arc::new(Mutex::new(Vec::new()));

        let (notif_tx, notif_rx) = tokio::sync::mpsc::unbounded_channel::<ServerNotification>();
        Self::spawn_notification_bridge(
            notif_rx,
            Arc::clone(&self.notification_subscribers),
            Arc::clone(&notification_buffer),
        );

        let mut background_tasks = self.background_tasks.lock().await;
        let handle = tokio::spawn(async move {
            run_subagent_task(SubagentRunParams {
                config: agent_config,
                task: SubagentTask {
                    instructions: spec.instructions.clone(),
                    prompt: spec.prompt.clone(),
                },
                task_config,
                return_last_only: true,
                session_id: subagent_session.id,
                cancellation_token: Some(task_token_clone),
                on_message: Some(on_message),
                notification_tx: Some(notif_tx),
            })
            .await
        });

        let task = BackgroundTask {
            id: task_id.clone(),
            description: description.clone(),
            started_at: Instant::now(),
            turns,
            last_activity,
            handle,
            cancellation_token: task_token,
            notification_buffer,
            _slot: task_slot,
        };

        background_tasks.insert(task_id.clone(), task);

        let content = vec![Content::text(format!(
            "Task {} started in background: \"{}\"\n\
             Resolved delegate authority: extensions = {}.{}\n\
             Continue with other work. When you need the result, use load(source: \"{}\").",
            task_id,
            description,
            authority_summary,
            delegate_mode_notice(subagent_mode),
            task_id
        ))];
        Ok((content, task_id))
    }
}
