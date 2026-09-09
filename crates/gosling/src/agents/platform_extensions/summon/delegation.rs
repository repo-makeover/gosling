// Owns the delegate tool schema, validation, spec construction, and foreground execution.
// Extracted from `summon.rs` in a behavior-preserving modularization.
// The `summon` compatibility facade keeps delegation behind `SummonClient` and MCP dispatch.

use super::delegate_config::{delegate_mode_notice, sync_delegate_timeout, PreparedDelegate};
use super::*;

/// How long a timed-out synchronous delegate is given to unwind after its
/// cancellation token fires, before the task is aborted outright. Matches the
/// grace the `load(cancel: true)` path already uses so both cancellation routes
/// behave the same.
const SYNC_DELEGATE_CANCEL_GRACE: Duration = Duration::from_secs(5);

impl SummonClient {
    pub(super) fn create_delegate_tool(&self) -> Tool {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "instructions": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Task instructions. Required for ad-hoc tasks."
                },
                "source": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Name of an existing agent to run. For an ad-hoc task, omit this argument entirely; never send an empty, null, or dummy source. A compatibility placeholder of source: \"dummy\" is ignored only when instructions, provider, and model explicitly identify an ad-hoc task."
                },
                "extensions": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Extensions to enable. Ad-hoc delegates default to none. Source-based delegates are limited to the role's versioned capabilities policy; an explicit list can only narrow that policy."
                },
                "provider": {
                    "type": "string",
                    "description": "Override LLM provider."
                },
                "model": {
                    "type": "string",
                    "description": "Override model."
                },
                "temperature": {
                    "type": "number",
                    "description": "Override temperature."
                },
                "max_turns": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum turns for this delegate. Overrides GOSLING_SUBAGENT_MAX_TURNS."
                },
                "context": {
                    "type": "string",
                    "description": "Reference context to inject into the delegate's system prompt. Use for background information, file contents, or constraints the delegate needs but that aren't part of the task instructions."
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the delegate. Must be within the parent session's working directory. Defaults to the parent's working directory."
                },
                "async": {
                    "type": "boolean",
                    "default": false,
                    "description": "Run in background (default: false)."
                }
            }
        });

        Tool::new(
            "delegate",
            "Delegate a task to a subagent that runs independently with its own context.\n\n\
             Modes:\n\
             1. Ad-hoc: Provide `instructions` for a custom task and omit `source` entirely\n\
             2. Source-based: Provide `source` name to run an agent\n\
             3. Combined: Pair a source with a task (e.g., source: \"reviewer\", instructions: \"review the auth module\")\n\n\
             Effective Delegation:\n\
             - Delegates know only instructions + source content\n\
             - Delegates cannot coordinate. Same-file work = conflicts.\n\
             - Parallel: async: true, then load(taskId) to wait and get results. Single: sync.\n\n\
             Research (read-only): parallelize freely - delegates explore and report back.\n\
             Work (writes): partition files strictly - no two delegates touch the same file.\n\n\
             Validation failures are final for that launch attempt; do not retry with empty or alternate source values.\n\n\
             Decompose → async delegates → load(taskId) for each → synthesize."
                .to_string(),
            schema.as_object().unwrap().clone(),
        )
    }

    pub(super) async fn handle_delegate(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, String> {
        self.cleanup_completed_tasks().await;

        let params: DelegateParams = arguments
            .map(|args| serde_json::from_value::<DelegateParams>(serde_json::Value::Object(args)))
            .transpose()
            .map_err(|e| format!("Invalid parameters: {}", e))?
            .unwrap_or_default()
            .normalize();

        self.validate_delegate_params(&params)?;

        let session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|e| format!("Failed to get session: {}", e))?;

        if session.session_type == SessionType::SubAgent {
            return Err("Delegated tasks cannot spawn further delegations".to_string());
        }

        if params.r#async {
            let (content, task_id) = self.handle_async_delegate(session_id, params).await?;
            let mut meta = Meta::new();
            meta.0.insert(
                "subagent_session_id".to_string(),
                serde_json::Value::String(task_id),
            );
            return Ok(CallToolResult::success(content).with_meta(Some(meta)));
        }

        let PreparedDelegate {
            spec,
            task_config,
            subagent_mode,
            agent_config,
            subagent_session,
        } = self
            .prepare_delegate(session_id, &params, &session, "Delegated task".to_string())
            .await?;

        let (notif_tx, notif_rx) = tokio::sync::mpsc::unbounded_channel::<ServerNotification>();
        Self::spawn_notification_bridge(
            notif_rx,
            Arc::clone(&self.notification_subscribers),
            Arc::new(Mutex::new(Vec::new())),
        );

        let subagent_session_id = subagent_session.id.clone();

        // The delegate runs on its own task so a timeout can cancel it
        // cooperatively and then reclaim the tool call. Awaiting the future
        // inline and dropping it on timeout would tear the subagent down at an
        // arbitrary await point, leaving its own tool operations `started`.
        // The token is a child of the parent tool call's, so cancelling the
        // parent still stops the delegate, while the timeout stops only this
        // delegate.
        let delegate_token = cancellation_token.child_token();
        let run_token = delegate_token.clone();
        let mut handle = tokio::spawn(async move {
            run_subagent_task(SubagentRunParams {
                config: agent_config,
                task: SubagentTask {
                    instructions: spec.instructions.clone(),
                    prompt: spec.prompt.clone(),
                },
                task_config,
                return_last_only: true,
                session_id: subagent_session.id,
                cancellation_token: Some(run_token),
                on_message: None,
                notification_tx: Some(notif_tx),
            })
            .await
        });

        // `Err(budget)` is the timeout, carrying the budget it exceeded.
        let outcome = match sync_delegate_timeout() {
            Some(budget) => tokio::select! {
                joined = &mut handle => Ok(joined),
                _ = tokio::time::sleep(budget) => Err(budget),
            },
            None => Ok((&mut handle).await),
        };

        let mut meta = Meta::new();
        meta.0.insert(
            "subagent_session_id".to_string(),
            serde_json::Value::String(subagent_session_id.clone()),
        );

        let joined = match outcome {
            Ok(joined) => joined,
            Err(budget) => {
                delegate_token.cancel();
                if tokio::time::timeout(SYNC_DELEGATE_CANCEL_GRACE, &mut handle)
                    .await
                    .is_err()
                {
                    handle.abort();
                }
                warn!(
                    "Synchronous delegate {} exceeded its {}s budget and was cancelled",
                    subagent_session_id,
                    budget.as_secs()
                );
                meta.0.insert(
                    "delegate_status".to_string(),
                    serde_json::Value::String("timed_out".to_string()),
                );
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Delegation timed out after {}s and was cancelled. It was not retried \
                     automatically -- re-running it would repeat any side effects the delegate \
                     already performed. Whatever the delegate completed before cancellation is \
                     durable in session {}; read that session to decide how to proceed. For \
                     work that legitimately runs longer, launch it with async: true and poll \
                     it with load(), or raise GOSLING_SYNC_DELEGATE_TIMEOUT_SECS.",
                    budget.as_secs(),
                    subagent_session_id
                ))])
                .with_meta(Some(meta)));
            }
        };

        let result = match joined {
            Ok(result) => result,
            Err(join_error) => Err(anyhow::anyhow!("delegate task panicked: {join_error}")),
        };

        match result {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(format!(
                "{}{}",
                text,
                delegate_mode_notice(subagent_mode)
            ))])
            .with_meta(Some(meta))),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Delegation failed: {}",
                e
            ))])
            .with_meta(Some(meta))),
        }
    }

    pub(super) fn validate_delegate_params(&self, params: &DelegateParams) -> Result<(), String> {
        if params.instructions.is_none() && params.source.is_none() {
            return Err("Must provide 'instructions' or 'source' (or both)".to_string());
        }

        if let Some(max) = params.max_turns {
            if max < 1 {
                return Err("'max_turns' must be at least 1".to_string());
            }
        }

        Ok(())
    }

    pub(super) async fn build_delegate_spec(
        &self,
        params: &DelegateParams,
        working_dir: &Path,
    ) -> Result<DelegateSpec, String> {
        let mut spec = if let Some(source_name) = &params.source {
            self.build_source_spec(source_name, params, working_dir)
                .await?
        } else {
            self.build_adhoc_spec(params)?
        };

        if let Some(ref context) = params.context {
            let existing = spec.instructions.unwrap_or_default();
            spec.instructions = Some(build_instructions_with_context(context, &existing));
        }

        Ok(spec)
    }

    fn build_adhoc_spec(&self, params: &DelegateParams) -> Result<DelegateSpec, String> {
        let task = params
            .instructions
            .as_ref()
            .ok_or("Instructions required for ad-hoc task")?;

        Ok(DelegateSpec {
            prompt: Some(task.clone()),
            ..Default::default()
        })
    }

    async fn build_source_spec(
        &self,
        source_name: &str,
        params: &DelegateParams,
        working_dir: &Path,
    ) -> Result<DelegateSpec, String> {
        let source = self
            .resolve_source(source_name, working_dir)
            .await?
            .ok_or_else(|| format!("Source '{}' not found", source_name))?;

        let mut spec = match source.source_type {
            SourceType::Agent => self.build_spec_from_agent(&source, params)?,
            _ => {
                return Err(format!(
                    "Source '{}' has kind '{}' which cannot be delegated from summon",
                    source_name, source.source_type
                ));
            }
        };

        if let Some(extra_instructions) = &params.instructions {
            if spec.prompt.is_some() {
                let current_prompt = spec.prompt.take().unwrap();
                spec.prompt = Some(format!("{}\n\n{}", current_prompt, extra_instructions));
            } else {
                spec.prompt = Some(extra_instructions.clone());
            }
        }

        Ok(spec)
    }

    pub(super) fn build_spec_from_agent(
        &self,
        source: &SourceEntry,
        params: &DelegateParams,
    ) -> Result<DelegateSpec, String> {
        let agent_content = if source.path.is_empty() {
            return Err("Agent source has no path".to_string());
        } else {
            std::fs::read_to_string(&source.path)
                .map_err(|e| format!("Failed to read agent file: {}", e))?
        };

        let (metadata, _): (AgentMetadata, String) = parse_frontmatter(&agent_content)
            .map_err(|e| format!("Failed to parse agent frontmatter: {}", e))?
            .ok_or("No frontmatter found in agent file")?;

        let prompt = params
            .instructions
            .is_none()
            .then(|| "Proceed with your expertise to produce a useful result.".to_string());

        // A capability policy is an allowlist authored by the agent file. Only
        // the operator's own global agents (~/.gosling, ~/.claude, ~/.agents,
        // the config dir) are trusted to author one. A repo-committed agent
        // file (`source.global == false`) is untrusted config, so its
        // declared policy is dropped and treated the same as no policy at
        // all — `Some(Vec::new())`, the existing legacy-source result —
        // instead of being honored as a grant (AOC-GOS-004).
        let trusted_capabilities = if source.global {
            metadata.capabilities
        } else {
            if metadata.capabilities.is_some() {
                tracing::warn!(
                    security.event_type = "delegate_capability_policy_untrusted_source",
                    security.source_path = %source.path,
                    "repo-committed agent file declares a capability policy; repo \
                     content cannot grant extensions on its own, ignoring it"
                );
            }
            None
        };
        let role_extensions = Some(validate_capability_policy(trusted_capabilities)?);

        Ok(DelegateSpec {
            instructions: Some(source.content.clone()),
            prompt,
            model: metadata.model,
            role_extensions,
        })
    }
}
