// Owns delegate task, provider, model, turn-limit, and working-directory configuration.
// Extracted from `summon.rs` in a behavior-preserving modularization.
// The `summon` compatibility facade keeps these details behind `SummonClient`.

use super::*;

/// How long a synchronous `delegate` may run before the parent turn reclaims
/// its tool call (REL-GSL-004).
///
/// An async delegate is already bounded: `load` waits five minutes and then
/// hands the model an explicit "still running, wait again or cancel" result. A
/// synchronous delegate had no bound at all, so a delegate that never returned
/// held its parent's tool call -- and therefore the parent's turn and session
/// turn lease -- open indefinitely.
///
/// Thirty minutes is deliberately far above any delegate that is making
/// progress and far below "forever". It bounds the failure, it does not pace
/// the work: a delegate is already limited by `max_turns`, so the only tasks
/// this ends are ones that are stuck rather than slow.
const DEFAULT_SYNC_DELEGATE_TIMEOUT_SECS: u64 = 1800;

/// The wall-clock budget for a synchronous delegate, or `None` when the
/// operator has set the budget to `0` to opt out of the bound entirely.
pub(super) fn sync_delegate_timeout() -> Option<Duration> {
    sync_delegate_timeout_from(Config::global())
}

/// Split out from [`sync_delegate_timeout`] so tests can resolve the budget
/// against an isolated config instead of the operator's real one.
pub(super) fn sync_delegate_timeout_from(config: &Config) -> Option<Duration> {
    let secs = config
        .get_param::<u64>("GOSLING_SYNC_DELEGATE_TIMEOUT_SECS")
        .unwrap_or(DEFAULT_SYNC_DELEGATE_TIMEOUT_SECS);
    (secs > 0).then(|| Duration::from_secs(secs))
}

pub(super) fn delegate_mode(executes_tools_outside_gosling: bool) -> GoslingMode {
    if executes_tools_outside_gosling {
        GoslingMode::Chat
    } else {
        GoslingMode::Auto
    }
}

pub(super) fn delegate_mode_notice(mode: GoslingMode) -> &'static str {
    if mode == GoslingMode::Chat {
        " Delegate mode: Chat; this provider executes tools outside Gosling's inspection pipeline, so delegated tool calls are disabled."
    } else {
        ""
    }
}

/// The pieces shared by the synchronous and background delegate paths, which
/// differ only in how they run the resulting task and report progress.
pub(super) struct PreparedDelegate {
    pub(super) spec: DelegateSpec,
    pub(super) task_config: TaskConfig,
    pub(super) subagent_mode: GoslingMode,
    pub(super) agent_config: AgentConfig,
    pub(super) subagent_session: crate::session::Session,
}

impl SummonClient {
    /// Resolve a delegate's spec, task config, and subagent mode, then create
    /// the subagent session and record its parent linkage.
    pub(super) async fn prepare_delegate(
        &self,
        session_id: &str,
        params: &DelegateParams,
        session: &crate::session::Session,
        subagent_session_name: String,
    ) -> Result<PreparedDelegate, String> {
        let working_dir = session.working_dir.clone();
        let spec = self.build_delegate_spec(params, &working_dir).await?;

        let task_config = self
            .build_task_config(params, &spec, session)
            .await
            .map_err(|e| format!("Failed to build task config: {}", e))?;
        let subagent_mode = delegate_mode(task_config.provider.executes_tools_outside_gosling());

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
                subagent_session_name,
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

        Ok(PreparedDelegate {
            spec,
            task_config,
            subagent_mode,
            agent_config,
            subagent_session,
        })
    }
}

impl SummonClient {
    pub(super) async fn build_task_config(
        &self,
        params: &DelegateParams,
        spec: &DelegateSpec,
        session: &crate::session::Session,
    ) -> Result<TaskConfig, anyhow::Error> {
        let (provider, model_config) = self.resolve_provider(params, spec, session).await?;

        let parent_extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            Config::global(),
        );
        let extensions =
            resolve_delegate_extensions(parent_extensions, spec, params.extensions.as_deref())
                .map_err(anyhow::Error::msg)?;

        let max_turns = params.max_turns.unwrap_or_else(|| self.resolve_max_turns());

        if max_turns == 0 || max_turns > u32::MAX as usize {
            anyhow::bail!(
                "max_turns must be between 1 and {} (got {})",
                u32::MAX,
                max_turns
            );
        }

        let effective_working_dir = match &params.working_dir {
            Some(dir) => resolve_working_dir(&session.working_dir, dir)?,
            None => session.working_dir.clone(),
        };

        let task_config = TaskConfig::new(
            provider,
            model_config,
            &session.id,
            &effective_working_dir,
            extensions,
        )
        .with_max_turns(Some(max_turns));

        Ok(task_config)
    }

    pub(super) fn resolve_model_config(
        &self,
        params: &DelegateParams,
        spec: &DelegateSpec,
        session: &crate::session::Session,
        provider_name: &str,
    ) -> Result<gosling_providers::model::ModelConfig, anyhow::Error> {
        let mut model_config = session.model_config.clone().map(Ok).unwrap_or_else(|| {
            crate::model_config::model_config_from_user_config(provider_name, "default")
        })?;

        let override_model = params
            .model
            .clone()
            .or_else(|| spec.model.clone())
            .or_else(|| {
                Config::global()
                    .get_param::<String>("GOSLING_SUBAGENT_MODEL")
                    .ok()
            });

        if let Some(model) = override_model {
            if model != model_config.model_name {
                // Build the new config from scratch so canonical fields
                // (context_limit, max_tokens, reasoning) and env-derived
                // overrides (GOSLING_CONTEXT_LIMIT, GOSLING_MAX_TOKENS) match the
                // overridden model, then preserve session-level state that is
                // not model-specific from the parent.
                let parent = model_config;
                let mut cfg =
                    crate::model_config::model_config_from_user_config(provider_name, &model)?;
                cfg.toolshim = parent.toolshim;
                cfg.toolshim_model = parent.toolshim_model;
                cfg.temperature = cfg.temperature.or(parent.temperature);
                if let Some(parent_params) = parent.request_params {
                    let merged = cfg.request_params.get_or_insert_with(Default::default);
                    for (k, v) in parent_params {
                        merged.insert(k, v);
                    }
                }
                model_config = cfg;
            }
        }

        if let Some(temp) = params.temperature {
            model_config = model_config.with_temperature(Some(temp));
        }

        Ok(model_config)
    }

    async fn resolve_provider(
        &self,
        params: &DelegateParams,
        spec: &DelegateSpec,
        session: &crate::session::Session,
    ) -> Result<
        (
            Arc<dyn crate::providers::base::Provider>,
            gosling_providers::model::ModelConfig,
        ),
        anyhow::Error,
    > {
        let provider_name = params
            .provider
            .clone()
            .or_else(|| {
                Config::global()
                    .get_param::<String>("GOSLING_SUBAGENT_PROVIDER")
                    .ok()
            })
            .or_else(|| session.provider_name.clone())
            .ok_or_else(|| anyhow::anyhow!("No provider configured"))?;

        let model_config = self.resolve_model_config(params, spec, session, &provider_name)?;
        let provider = providers::create(&provider_name, Vec::new()).await?;
        Ok((provider, model_config))
    }

    pub(super) fn resolve_max_turns(&self) -> usize {
        std::env::var("GOSLING_SUBAGENT_MAX_TURNS")
            .ok()
            .and_then(|v| v.parse().ok())
            .or_else(|| {
                Config::global()
                    .get_param::<usize>("GOSLING_SUBAGENT_MAX_TURNS")
                    .ok()
            })
            .unwrap_or(DEFAULT_SUBAGENT_MAX_TURNS)
    }
}

/// Resolve a requested `working_dir` override against the parent session
/// directory. Relative paths are joined to the parent dir; the result must
/// canonicalize to an existing directory contained within the parent dir.
pub(super) fn resolve_working_dir(
    parent_dir: &Path,
    requested: &str,
) -> Result<PathBuf, anyhow::Error> {
    let requested_path = PathBuf::from(requested);
    let resolved = if requested_path.is_absolute() {
        requested_path
    } else {
        parent_dir.join(&requested_path)
    };
    let canonical = resolved
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("working_dir '{}' could not be resolved: {}", requested, e))?;
    let parent_canonical = parent_dir
        .canonicalize()
        .unwrap_or_else(|_| parent_dir.to_path_buf());
    if !canonical.starts_with(&parent_canonical) {
        anyhow::bail!(
            "working_dir '{}' is outside the parent session directory",
            requested
        );
    }
    if !canonical.is_dir() {
        anyhow::bail!("working_dir '{}' is not a directory", requested);
    }
    Ok(canonical)
}
