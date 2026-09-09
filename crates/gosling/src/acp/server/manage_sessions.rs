use super::*;

impl GoslingAcpAgent {
    pub(super) async fn on_update_working_dir(
        &self,
        req: UpdateWorkingDirRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let working_dir = req.working_dir.trim().to_string();
        if working_dir.is_empty() {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("working directory cannot be empty"));
        }
        let path = std::path::PathBuf::from(&working_dir);
        validate_absolute_cwd(&path)?;
        let session_id = &req.session_id;

        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;
        reject_workspace_folder_policy_mutation(&session)?;

        if path == session.working_dir {
            return Ok(EmptyResponse {});
        }

        let agent = self.get_session_agent(session_id).await?;

        self.session_manager
            .update(session_id)
            .working_dir(path)
            .apply()
            .await
            .internal_err_ctx("Failed to update session working directory")?;

        let updated_session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .internal_err_ctx("Failed to reload session")?;

        let transition_error =
            if let Err(error) = agent.restore_provider_from_session(&updated_session).await {
                Some(format!("failed to refresh provider: {error}"))
            } else if let Err(error) = agent
                .extension_manager
                .update_working_dirs(
                    &updated_session.working_dir,
                    &updated_session.additional_working_dirs,
                )
                .await
            {
                Some(format!(
                    "failed to update extension working directories: {error}"
                ))
            } else {
                None
            };

        if let Some(transition_error) = transition_error {
            return Err(self
                .rollback_working_dir_transition(&agent, &session, transition_error)
                .await);
        }

        Ok(EmptyResponse {})
    }

    async fn rollback_working_dir_transition(
        &self,
        agent: &Arc<Agent>,
        previous_session: &Session,
        transition_error: String,
    ) -> agent_client_protocol::Error {
        let mut rollback_errors = Vec::new();
        if let Err(error) = self
            .session_manager
            .update(&previous_session.id)
            .working_dir(previous_session.working_dir.clone())
            .additional_working_dirs(previous_session.additional_working_dirs.clone())
            .workspace_context(previous_session.workspace_context.clone())
            .apply()
            .await
        {
            rollback_errors.push(format!("session: {error}"));
        }
        if let Err(error) = agent.restore_provider_from_session(previous_session).await {
            rollback_errors.push(format!("provider: {error}"));
        }
        if let Err(error) = agent
            .extension_manager
            .update_working_dirs(
                &previous_session.working_dir,
                &previous_session.additional_working_dirs,
            )
            .await
        {
            rollback_errors.push(format!("extensions: {error}"));
        }

        let rollback_detail = if rollback_errors.is_empty() {
            "rollback completed".to_string()
        } else {
            format!("rollback errors: {}", rollback_errors.join("; "))
        };
        agent_client_protocol::Error::internal_error().data(format!(
            "Working directory transition failed: {transition_error}; {rollback_detail}"
        ))
    }

    pub(super) async fn on_add_session_working_dir(
        &self,
        req: AddSessionWorkingDirRequest,
    ) -> Result<SessionWorkingDirsResponse, agent_client_protocol::Error> {
        let working_dir = req.working_dir.trim().to_string();
        if working_dir.is_empty() {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("working directory cannot be empty"));
        }
        let requested_path = std::path::PathBuf::from(&working_dir);
        validate_absolute_cwd(&requested_path)?;
        let path = std::fs::canonicalize(&requested_path).map_err(|_| {
            agent_client_protocol::Error::invalid_params().data("invalid directory path")
        })?;
        let session_id = &req.session_id;

        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;
        let mut additional_working_dirs = session.additional_working_dirs.clone();
        if path != session.working_dir && !additional_working_dirs.contains(&path) {
            additional_working_dirs.push(path.clone());
        }
        let workspace_context = workspace_context_with_added_root(&session, &path);

        let agent = self.get_session_agent(session_id).await?;

        let mut update = self
            .session_manager
            .update(session_id)
            .additional_working_dirs(additional_working_dirs.clone());
        if let Some(workspace_context) = &workspace_context {
            update = update.workspace_context(Some(workspace_context.clone()));
        }
        update
            .apply()
            .await
            .internal_err_ctx("Failed to add session working directory")?;

        if let Err(error) = agent
            .extension_manager
            .update_working_dirs(&session.working_dir, &additional_working_dirs)
            .await
        {
            return Err(self
                .rollback_working_dir_transition(
                    &agent,
                    &session,
                    format!("failed to update extension working directories: {error}"),
                )
                .await);
        }

        Ok(session_working_dirs_response(
            &session.working_dir,
            &additional_working_dirs,
            workspace_context.as_ref(),
        ))
    }

    pub(super) async fn on_remove_session_working_dir(
        &self,
        req: RemoveSessionWorkingDirRequest,
    ) -> Result<SessionWorkingDirsResponse, agent_client_protocol::Error> {
        let working_dir = req.working_dir.trim().to_string();
        if working_dir.is_empty() {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("working directory cannot be empty"));
        }
        let path = std::path::PathBuf::from(&working_dir);
        if !path.is_absolute() {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("working directory must be an absolute path"));
        }
        let session_id = &req.session_id;

        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;
        reject_workspace_folder_policy_mutation(&session)?;

        let additional_working_dirs: Vec<_> = session
            .additional_working_dirs
            .iter()
            .filter(|&dir| dir != &path)
            .cloned()
            .collect();

        let agent = self.get_session_agent(session_id).await?;

        self.session_manager
            .update(session_id)
            .additional_working_dirs(additional_working_dirs.clone())
            .apply()
            .await
            .internal_err_ctx("Failed to remove session working directory")?;

        if let Err(error) = agent
            .extension_manager
            .update_working_dirs(&session.working_dir, &additional_working_dirs)
            .await
        {
            return Err(self
                .rollback_working_dir_transition(
                    &agent,
                    &session,
                    format!("failed to update extension working directories: {error}"),
                )
                .await);
        }

        Ok(session_working_dirs_response(
            &session.working_dir,
            &additional_working_dirs,
            session.workspace_context.as_ref(),
        ))
    }

    pub(super) async fn on_set_session_working_dir_restriction(
        &self,
        req: SetSessionWorkingDirRestrictionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let session_id = &req.session_id;
        // The working-directory *set* stays pinned for workspace sessions (see the
        // other endpoints), but the restriction flag is intentionally togglable:
        // turning it off is the opt-in that lets providers which run their own
        // tools (Claude Code CLI, Codex CLI, …) be used in a workspace. The
        // workspace folder-policy inspector still enforces the boundary for any
        // tool Gosling routes through its own pipeline.
        self.session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;

        self.session_manager
            .update(session_id)
            .restrict_tools_to_working_dirs(req.restrict)
            .apply()
            .await
            .internal_err_ctx("Failed to update working directory restriction")?;

        Ok(EmptyResponse {})
    }

    pub(super) async fn on_set_session_system_prompt(
        &self,
        req: SetSessionSystemPromptRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        let agent = self.get_session_agent(session_id).await?;
        match req.mode {
            SessionSystemPromptMode::Set => {
                if req.text.trim().is_empty() {
                    agent.clear_system_prompt_override().await;
                } else {
                    agent.override_system_prompt(req.text).await;
                }
            }
            SessionSystemPromptMode::Append => {
                let key = req
                    .key
                    .as_deref()
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .ok_or_else(|| {
                        agent_client_protocol::Error::invalid_params()
                            .data("key cannot be empty for append mode")
                    })?;
                if req.text.trim().is_empty() {
                    agent.remove_system_prompt_extra(key).await;
                } else {
                    agent
                        .extend_system_prompt(key.to_string(), req.text.clone())
                        .await;
                }
                self.persist_system_prompt_extra(session_id, key, req.text)
                    .await?;
            }
        }

        Ok(EmptyResponse {})
    }

    async fn persist_system_prompt_extra(
        &self,
        session_id: &str,
        key: &str,
        text: String,
    ) -> Result<(), agent_client_protocol::Error> {
        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .internal_err_ctx("Failed to load session")?;
        let mut state =
            crate::session::SystemPromptExtrasState::from_extension_data(&session.extension_data)
                .unwrap_or_default();
        if text.trim().is_empty() {
            state.remove(key);
        } else {
            state.upsert(key, text);
        }
        let value = state
            .to_value()
            .internal_err_ctx("Failed to serialize system prompt extras")?;
        self.session_manager
            .merge_extension_state(
                session_id,
                &crate::session::SystemPromptExtrasState::state_key(),
                value,
            )
            .await
            .internal_err_ctx("Failed to persist system prompt extras")
    }

    pub(super) async fn on_delete_session(
        &self,
        req: DeleteSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .delete_session(&req.session_id)
            .await
            .internal_err()?;
        self.sessions.lock().await.remove(&req.session_id);
        self.agent_manager
            .remove_session_if_loaded(&req.session_id)
            .await
            .internal_err_ctx("Failed to remove in-memory agent")?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_export_session(
        &self,
        req: ExportSessionRequest,
    ) -> Result<ExportSessionResponse, agent_client_protocol::Error> {
        let data = self
            .session_manager
            .export_session(&req.session_id)
            .await
            .internal_err()?;
        Ok(ExportSessionResponse { data })
    }

    pub(super) async fn on_import_session(
        &self,
        req: ImportSessionRequest,
    ) -> Result<ImportSessionResponse, agent_client_protocol::Error> {
        let is_nostr = match req.source {
            SessionImportSource::Auto => is_nostr_session_link(&req.input),
            SessionImportSource::Json => false,
            SessionImportSource::Nostr => true,
        };
        let (data, session_type, transport) = if is_nostr {
            (
                import_nostr_session_json(&req.input).await?,
                Some(SessionType::User),
                crate::session::import_formats::SessionImportTransport::Nostr,
            )
        } else {
            (
                req.input,
                None,
                crate::session::import_formats::SessionImportTransport::Json,
            )
        };

        let session = self
            .session_manager
            .import_session(
                &data,
                session_type,
                std::path::PathBuf::from(req.working_dir),
                transport,
            )
            .await
            .internal_err()?;

        let msg_count = session.message_count as u64;

        Ok(ImportSessionResponse {
            session_id: session.id,
            title: Some(session.name),
            updated_at: Some(session.updated_at.to_rfc3339()),
            message_count: msg_count,
        })
    }

    pub(super) async fn on_share_session_nostr(
        &self,
        req: ShareSessionNostrRequest,
    ) -> Result<ShareSessionNostrResponse, agent_client_protocol::Error> {
        let data = self
            .session_manager
            .export_session(&req.session_id)
            .await
            .internal_err()?;

        let share = publish_session_to_nostr(&data, req.relays).await?;

        Ok(ShareSessionNostrResponse {
            deeplink: share.deeplink,
            nevent: share.nevent,
            event_id: share.event_id,
            relays: share.relays,
        })
    }

    pub(super) async fn on_get_session_info(
        &self,
        req: GetSessionInfoRequest,
    ) -> Result<GetSessionInfoResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;

        let resume_integrity = if AcpPromptRunState::from_extension_data(&session.extension_data)
            .is_some_and(|state| state.has_terminal_outcome())
        {
            "clean"
        } else {
            "uncertain"
        };
        let mut info = build_session_info(session);
        let mut meta = info.meta.take().unwrap_or_default();
        meta.insert(
            "gosling".to_string(),
            serde_json::json!({ "resumeIntegrity": resume_integrity }),
        );
        info.meta = Some(meta);

        Ok(GetSessionInfoResponse { session: info })
    }

    pub(super) async fn on_record_session_model_switch(
        &self,
        req: RecordSessionModelSwitchRequest,
    ) -> Result<RecordSessionModelSwitchResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        let message_text = req.message.trim();
        if message_text.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("message cannot be empty")
            );
        }

        self.session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;

        let message = self
            .session_manager
            .add_model_switch_record(session_id, message_text)
            .await
            .internal_err_ctx("Failed to record model switch")?;

        Ok(RecordSessionModelSwitchResponse {
            message: serde_json::to_value(message).internal_err()?,
        })
    }

    pub(super) async fn on_list_session_messages(
        &self,
        req: ListSessionMessagesRequest,
    ) -> Result<ListSessionMessagesResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }
        let limit = req
            .limit
            .unwrap_or(DEFAULT_SESSION_TAIL_LIMIT)
            .min(presentation::ACP_HISTORY_PAGE_LIMIT);
        let page = self
            .session_manager
            .get_session_message_page(session_id, req.before_cursor.as_deref(), limit)
            .await
            .map_err(|error| {
                if error.to_string().contains("Invalid before cursor") {
                    agent_client_protocol::Error::invalid_params().data(error.to_string())
                } else {
                    agent_client_protocol::Error::internal_error().data(error.to_string())
                }
            })?;
        let messages = presentation::project_history_page(page.messages)
            .into_iter()
            .map(|message| serde_json::to_value(message).unwrap_or(serde_json::Value::Null))
            .collect();
        Ok(ListSessionMessagesResponse {
            messages,
            next_before_cursor: page.next_before_cursor,
            total_count: page.total_count,
            oldest_row_id: page.oldest_row_id,
            newest_row_id: page.newest_row_id,
        })
    }

    pub(super) async fn on_list_session_artifacts(
        &self,
        req: ListSessionArtifactsRequest,
    ) -> Result<ListSessionArtifactsResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }
        let page = self
            .session_manager
            .list_session_artifacts(session_id, req.cursor.as_deref(), req.limit.unwrap_or(100))
            .await
            .map_err(|error| {
                if error.to_string().contains("invalid digit") {
                    agent_client_protocol::Error::invalid_params().data("invalid artifact cursor")
                } else {
                    agent_client_protocol::Error::internal_error().data(error.to_string())
                }
            })?;
        Ok(ListSessionArtifactsResponse {
            artifacts: page
                .artifacts
                .into_iter()
                .map(session_artifact_dto)
                .collect(),
            next_cursor: page.next_cursor,
            total_count: page.total_count,
        })
    }

    pub(super) async fn on_search_session_messages(
        &self,
        req: SearchSessionMessagesRequest,
    ) -> Result<SearchSessionMessagesResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }
        let results = self
            .session_manager
            .search_session_messages(session_id, req.query.trim(), req.limit.unwrap_or(20))
            .await
            .internal_err()?;
        Ok(SearchSessionMessagesResponse {
            matches: results
                .matches
                .into_iter()
                .map(|m| SessionMessageSearchMatch {
                    row_id: m.row_id,
                    message_id: m.message_id,
                    role: m.role,
                    snippet: m.snippet,
                    created: m.created,
                    before_cursor: m.before_cursor,
                })
                .collect(),
            total_matches: results.total_matches,
        })
    }

    pub(super) async fn on_get_session_summary(
        &self,
        req: GetSessionSummaryRequest,
    ) -> Result<GetSessionSummaryResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }
        let summary = self
            .session_manager
            .get_session_summary(session_id)
            .await
            .internal_err()?
            .map(|summary| SessionSummaryDto {
                summary: summary.summary,
                covered_through_row_id: summary.covered_through_row_id,
                covered_through_timestamp: summary.covered_through_timestamp,
                covered_message_count: summary.covered_message_count,
                status: summary.status.to_string(),
                error: summary.error,
                updated_at: summary.updated_at.to_rfc3339(),
            });
        let facts = self
            .session_manager
            .get_session_summary_facts(session_id)
            .await
            .internal_err()?
            .into_iter()
            .map(|fact| SessionSummaryFactDto {
                id: fact.id,
                scope: fact.scope,
                fact_type: fact.fact_type,
                content: fact.content,
                confidence: fact.confidence,
            })
            .collect();
        Ok(GetSessionSummaryResponse { summary, facts })
    }

    pub(super) async fn on_truncate_session_conversation(
        &self,
        req: TruncateSessionConversationRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        self.session_manager
            .truncate_conversation(session_id, req.truncate_from)
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_update_session_project(
        &self,
        req: UpdateSessionProjectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .project_id(req.project_id)
            .apply()
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_rename_session(
        &self,
        req: RenameSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .user_provided_name(req.title)
            .apply()
            .await
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_archive_session(
        &self,
        req: ArchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .archived_at(Some(chrono::Utc::now()))
            .apply()
            .await
            .internal_err()?;
        self.sessions.lock().await.remove(&req.session_id);
        self.agent_manager
            .remove_session_if_loaded(&req.session_id)
            .await
            .internal_err_ctx("Failed to remove in-memory agent")?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_unarchive_session(
        &self,
        req: UnarchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .archived_at(None)
            .apply()
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }
}

fn session_working_dirs_response(
    working_dir: &std::path::Path,
    additional_working_dirs: &[std::path::PathBuf],
    workspace_context: Option<&crate::workspace::WorkspaceSessionContext>,
) -> SessionWorkingDirsResponse {
    SessionWorkingDirsResponse {
        working_dir: working_dir.to_string_lossy().to_string(),
        additional_working_dirs: additional_working_dirs
            .iter()
            .map(|dir| dir.to_string_lossy().to_string())
            .collect(),
        workspace_folder_roots: workspace_context
            .map(|context| context.effective_folder_policy().roots)
            .unwrap_or_default(),
    }
}

fn workspace_context_with_added_root(
    session: &Session,
    path: &std::path::Path,
) -> Option<crate::workspace::WorkspaceSessionContext> {
    let mut context = session.workspace_context.clone()?;
    let mut policy = context.effective_folder_policy();
    if !policy
        .roots
        .iter()
        .any(|root| std::path::Path::new(&root.path) == path)
    {
        policy
            .roots
            .push(crate::workspace::WorkspaceFolderPolicyRoot {
                path: path.to_string_lossy().to_string(),
                access: crate::workspace::WorkspaceFolderAccess::ReadWrite,
            });
        policy
            .roots
            .sort_by(|left, right| left.path.cmp(&right.path));
    }
    context.folder_policy = policy;
    Some(context)
}

fn reject_workspace_folder_policy_mutation(
    session: &Session,
) -> Result<(), agent_client_protocol::Error> {
    if session.workspace_id.is_some() {
        return Err(agent_client_protocol::Error::invalid_params().data(
            "workspace session folder policy is pinned; edit the workspace and start a new session",
        ));
    }
    Ok(())
}

fn is_nostr_session_link(input: &str) -> bool {
    #[cfg(feature = "nostr")]
    {
        crate::session::nostr_share::is_session_share_deeplink(input)
    }
    #[cfg(not(feature = "nostr"))]
    {
        let _ = input;
        false
    }
}

#[cfg(feature = "nostr")]
async fn import_nostr_session_json(deeplink: &str) -> Result<String, agent_client_protocol::Error> {
    crate::session::nostr_share::import_session_json_from_deeplink(deeplink)
        .await
        .invalid_params_err()
}

#[cfg(not(feature = "nostr"))]
async fn import_nostr_session_json(
    _deeplink: &str,
) -> Result<String, agent_client_protocol::Error> {
    Err(agent_client_protocol::Error::invalid_params()
        .data("Nostr session import is not available in this build"))
}

#[cfg(feature = "nostr")]
async fn publish_session_to_nostr(
    data: &str,
    relays: Vec<String>,
) -> Result<NostrSessionShare, agent_client_protocol::Error> {
    let relays = crate::session::nostr_share::resolve_relays(relays, Config::global());
    let share = crate::session::nostr_share::publish_session_json(data, relays)
        .await
        .internal_err()?;
    Ok(NostrSessionShare {
        deeplink: share.deeplink,
        nevent: share.nevent,
        event_id: share.event_id,
        relays: share.relays,
    })
}

#[cfg(not(feature = "nostr"))]
async fn publish_session_to_nostr(
    _data: &str,
    _relays: Vec<String>,
) -> Result<NostrSessionShare, agent_client_protocol::Error> {
    Err(agent_client_protocol::Error::invalid_params()
        .data("Nostr session sharing is not available in this build"))
}

struct NostrSessionShare {
    deeplink: String,
    nevent: String,
    event_id: String,
    relays: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_session_folder_policy_replacement_remains_blocked() {
        let mut session = Session::default();
        assert!(reject_workspace_folder_policy_mutation(&session).is_ok());
        session.workspace_id = Some("workspace".to_string());
        assert!(reject_workspace_folder_policy_mutation(&session).is_err());
    }

    #[test]
    fn adding_a_root_changes_only_the_selected_workspace_session_context() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let private = root.path().join("private");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&private).unwrap();
        let context = crate::workspace::WorkspaceSessionContext {
            workspace_id: "workspace".into(),
            workspace_name: "Workspace".into(),
            primary_working_folder: project.to_string_lossy().to_string(),
            folders: Vec::new(),
            product_output_folders: Vec::new(),
            folder_policy: crate::workspace::WorkspaceFolderPolicy {
                roots: vec![crate::workspace::WorkspaceFolderPolicyRoot {
                    path: project.to_string_lossy().to_string(),
                    access: crate::workspace::WorkspaceFolderAccess::ReadWrite,
                }],
            },
        };
        let mut selected = Session {
            workspace_context: Some(context.clone()),
            ..Session::default()
        };
        let sibling = Session {
            workspace_context: Some(context),
            ..Session::default()
        };

        selected.workspace_context = workspace_context_with_added_root(&selected, &private);

        let response = session_working_dirs_response(
            &project,
            std::slice::from_ref(&private),
            selected.workspace_context.as_ref(),
        );
        assert!(response.workspace_folder_roots.iter().any(|root| {
            std::path::Path::new(&root.path) == private
                && root.access == crate::workspace::WorkspaceFolderAccess::ReadWrite
        }));
        let standalone = session_working_dirs_response(&project, &[], None);
        assert!(standalone.workspace_folder_roots.is_empty());
        assert!(serde_json::to_value(standalone)
            .unwrap()
            .get("workspaceFolderRoots")
            .is_none());

        assert!(selected
            .workspace_context
            .unwrap()
            .effective_folder_policy()
            .roots
            .iter()
            .any(|root| std::path::Path::new(&root.path) == private));
        assert!(!sibling
            .workspace_context
            .unwrap()
            .effective_folder_policy()
            .roots
            .iter()
            .any(|root| std::path::Path::new(&root.path) == private));
    }

    #[test]
    fn adding_an_existing_workspace_root_does_not_upgrade_its_access() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let reference = root.path().join("reference");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&reference).unwrap();
        let session = Session {
            workspace_context: Some(crate::workspace::WorkspaceSessionContext {
                workspace_id: "workspace".into(),
                workspace_name: "Workspace".into(),
                primary_working_folder: project.to_string_lossy().to_string(),
                folders: Vec::new(),
                product_output_folders: Vec::new(),
                folder_policy: crate::workspace::WorkspaceFolderPolicy {
                    roots: vec![
                        crate::workspace::WorkspaceFolderPolicyRoot {
                            path: project.to_string_lossy().to_string(),
                            access: crate::workspace::WorkspaceFolderAccess::ReadWrite,
                        },
                        crate::workspace::WorkspaceFolderPolicyRoot {
                            path: reference.to_string_lossy().to_string(),
                            access: crate::workspace::WorkspaceFolderAccess::Read,
                        },
                    ],
                },
            }),
            ..Session::default()
        };

        let updated = workspace_context_with_added_root(&session, &reference).unwrap();

        let response = session_working_dirs_response(&project, &[], Some(&updated));
        assert_eq!(
            response.workspace_folder_roots,
            updated.effective_folder_policy().roots
        );

        assert_eq!(
            updated
                .effective_folder_policy()
                .roots
                .iter()
                .find(|root| std::path::Path::new(&root.path) == reference)
                .unwrap()
                .access,
            crate::workspace::WorkspaceFolderAccess::Read
        );
    }
}
