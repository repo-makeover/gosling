use base64::Engine;
use gosling::config::GoslingMode;
use gosling::conversation::message::{InferenceMetadata, Message};
use gosling::session::{Session, SessionManager, SessionType};
use gosling_sdk_types::custom_requests::*;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Fixture {
    temp: TempDir,
    manager: SessionManager,
    session: Session,
    path: PathBuf,
}

impl Fixture {
    async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        fs::create_dir_all(root.join("Outputs")).unwrap();
        let root = root.canonicalize().unwrap();
        let manager = SessionManager::new(temp.path().join("state"));
        let session = manager
            .create_session(
                root.clone(),
                "Report chat".into(),
                SessionType::User,
                GoslingMode::Auto,
            )
            .await
            .unwrap();
        Self {
            temp,
            manager,
            session,
            path: root.join("Outputs/report.md"),
        }
    }

    async fn prepare(
        &self,
        session: &Session,
        request: &str,
        model: &str,
        call: &CallToolRequestParams,
    ) -> gosling::session::session_manager::OutputCapture {
        let message = Message::assistant()
            .with_generated_id()
            .with_inference(InferenceMetadata {
                provider: "test-provider".into(),
                requested_model: model.into(),
                resolved_model: Some(format!("{model}-resolved")),
            })
            .with_tool_request(request, Ok(call.clone()));
        self.manager
            .add_message(&session.id, &message)
            .await
            .unwrap();
        self.manager
            .prepare_output_capture(session, call, request)
            .await
            .unwrap()
            .unwrap()
    }

    async fn write(&self, session: &Session, request: &str, model: &str, content: &str) {
        let call = CallToolRequestParams::new("developer__write").with_arguments(
            rmcp::object!({"path": self.path.to_string_lossy(), "content": content}),
        );
        let capture = self.prepare(session, request, model, &call).await;
        fs::write(&self.path, content).unwrap();
        self.manager
            .finish_output_capture(capture, &CallToolResult::success(vec![]))
            .await
            .unwrap();
    }

    async fn history(&self) -> Vec<OutputRevisionDto> {
        self.manager
            .list_output_revisions(ListOutputRevisionsRequest {
                session_id: self.session.id.clone(),
                path: self.path.to_string_lossy().into(),
                before_version: None,
                limit: None,
            })
            .await
            .unwrap()
            .revisions
    }

    async fn get(&self, version: i64) -> GetOutputRevisionResponse {
        self.manager
            .get_output_revision(GetOutputRevisionRequest {
                session_id: self.session.id.clone(),
                path: self.path.to_string_lossy().into(),
                version,
            })
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn models_append_revisions_and_markdown_history_survives_restart() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "# First report")
        .await;
    fixture
        .write(&fixture.session, "second", "model-b", "# Revised report")
        .await;
    let history = fixture.history().await;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].version, 2);
    assert_eq!(
        history[0].contributor.selected_model.as_deref(),
        Some("model-b")
    );
    assert_eq!(
        history[1].contributor.selected_model.as_deref(),
        Some("model-a")
    );
    assert_eq!(history[1].attribution, OutputAttributionKind::Tool);
    let report = fs::read_to_string(&fixture.path).unwrap();
    assert!(report.starts_with("# Revised report"));
    assert!(report.contains("model-a-resolved") && report.contains("model-b-resolved"));
    assert_eq!(report.matches("gosling:output-history:start").count(), 1);
    let first = fixture.get(1).await;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(first.content_base64)
        .unwrap();
    assert!(String::from_utf8(bytes)
        .unwrap()
        .starts_with("# First report"));
    let reopened = SessionManager::new(fixture.temp.path().join("state"));
    let history = reopened
        .list_output_revisions(ListOutputRevisionsRequest {
            session_id: fixture.session.id.clone(),
            path: fixture.path.to_string_lossy().into(),
            before_version: None,
            limit: Some(1),
        })
        .await
        .unwrap();
    assert_eq!(history.revisions.len(), 1);
    assert_eq!(history.next_before_version, Some(2));
}

#[tokio::test]
async fn preexisting_content_is_saved_with_unknown_authorship() {
    let fixture = Fixture::new().await;
    fs::write(&fixture.path, "Human original").unwrap();
    fixture
        .write(&fixture.session, "edit", "model-a", "Revised")
        .await;
    let history = fixture.history().await;
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].action, OutputRevisionAction::Baseline);
    assert_eq!(history[1].contributor.agent, "Unknown");
    assert!(history[1].contributor.selected_model.is_none());
    let saved = fixture.get(1).await;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(saved.content_base64)
            .unwrap(),
        b"Human original"
    );
}

#[tokio::test]
async fn references_reads_and_unchanged_writes_do_not_add_authors() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "Report")
        .await;
    fixture
        .write(&fixture.session, "same", "model-b", "Report")
        .await;
    assert_eq!(fixture.history().await.len(), 1);
    assert!(fs::read_to_string(&fixture.path)
        .unwrap()
        .contains("model-a-resolved"));
    for name in ["developer__read", "developer__shell"] {
        let call = CallToolRequestParams::new(name).with_arguments(rmcp::object!({"path": fixture.path.to_string_lossy(), "command": "cat Outputs/report.md"}));
        assert!(fixture
            .manager
            .prepare_output_capture(&fixture.session, &call, "read")
            .await
            .unwrap()
            .is_none());
    }
    fixture
        .manager
        .register_completed_assistant_artifacts(
            &fixture.session.id,
            &Message::assistant()
                .with_generated_id()
                .with_text(format!("Read [report]({})", fixture.path.display())),
        )
        .await
        .unwrap();
    assert_eq!(fixture.history().await.len(), 1);
}

#[tokio::test]
async fn shell_generated_outputs_are_observed_and_failed_tools_get_no_credit() {
    let fixture = Fixture::new().await;
    let call = CallToolRequestParams::new("developer__shell")
        .with_arguments(rmcp::object!({"command": "python generate_report.py"}));
    let capture = fixture
        .prepare(&fixture.session, "shell", "model-a", &call)
        .await;
    fs::write(&fixture.path, "Shell report").unwrap();
    fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap();
    assert_eq!(
        fixture.history().await[0].attribution,
        OutputAttributionKind::Observed
    );
    let capture = fixture
        .prepare(&fixture.session, "failed", "model-b", &call)
        .await;
    fs::write(&fixture.path, "Partial failed work").unwrap();
    fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::error(vec![]))
        .await
        .unwrap();
    assert_eq!(fixture.history().await.len(), 1);
}

#[tokio::test]
async fn different_sessions_and_agents_share_file_history() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "Draft")
        .await;
    let delegate = fixture
        .manager
        .create_session(
            fixture.session.working_dir.clone(),
            "Reviewer".into(),
            SessionType::SubAgent,
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    fixture
        .write(&delegate, "review", "model-b", "Reviewed")
        .await;
    let history = fixture.history().await;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].contributor.agent, "Reviewer");
    assert_eq!(history[0].contributor.session_id, delegate.id);
    assert_eq!(history[1].contributor.session_id, fixture.session.id);
}

#[tokio::test]
async fn restore_appends_and_preserves_untracked_edits_and_rejects_stale_hash() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "Original")
        .await;
    fixture
        .write(&fixture.session, "second", "model-b", "Revised")
        .await;
    let saved = fixture.get(1).await;
    fs::write(&fixture.path, "External edit").unwrap();
    let request = RestoreOutputRevisionRequest {
        session_id: fixture.session.id.clone(),
        path: fixture.path.to_string_lossy().into(),
        version: 1,
        expected_current_hash: saved.current_hash.unwrap(),
    };
    assert!(fixture
        .manager
        .restore_output_revision(request.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("changed"));
    assert_eq!(fs::read_to_string(&fixture.path).unwrap(), "External edit");
    let refreshed = fixture.get(1).await;
    let restored = fixture
        .manager
        .restore_output_revision(RestoreOutputRevisionRequest {
            expected_current_hash: refreshed.current_hash.unwrap(),
            ..request
        })
        .await
        .unwrap();
    assert_eq!(restored.revision.version, 4);
    assert_eq!(restored.revision.restored_from, Some(1));
    assert_eq!(restored.revision.attribution, OutputAttributionKind::User);
    let external = fixture.get(3).await;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(external.content_base64)
            .unwrap(),
        b"External edit"
    );
    assert!(fs::read_to_string(&fixture.path)
        .unwrap()
        .starts_with("Original"));
}

#[tokio::test]
async fn history_access_requires_registered_output_and_session_folder() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "Private report")
        .await;
    let other_root = fixture.temp.path().join("other");
    fs::create_dir(&other_root).unwrap();
    let other = fixture
        .manager
        .create_session(
            other_root,
            "Other".into(),
            SessionType::User,
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    assert!(fixture
        .manager
        .get_output_revision(GetOutputRevisionRequest {
            session_id: other.id,
            path: fixture.path.to_string_lossy().into(),
            version: 1,
        })
        .await
        .is_err());
    let sibling = fixture
        .manager
        .create_session(
            fixture.session.working_dir.clone(),
            "Sibling".into(),
            SessionType::User,
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    assert!(fixture
        .manager
        .list_output_revisions(ListOutputRevisionsRequest {
            session_id: sibling.id,
            path: fixture.path.to_string_lossy().into(),
            before_version: None,
            limit: None,
        })
        .await
        .is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn symbolic_links_cannot_expose_or_overwrite_another_file() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "Report")
        .await;
    let outside = fixture.temp.path().join("outside.md");
    fs::write(&outside, "Outside").unwrap();
    fs::remove_file(&fixture.path).unwrap();
    std::os::unix::fs::symlink(&outside, &fixture.path).unwrap();
    assert!(fixture
        .manager
        .get_output_revision(GetOutputRevisionRequest {
            session_id: fixture.session.id.clone(),
            path: fixture.path.to_string_lossy().into(),
            version: 1
        })
        .await
        .is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "Outside");
}

#[tokio::test]
async fn binary_snapshots_export_exact_bytes_and_source_files_are_excluded() {
    let fixture = Fixture::new().await;
    let path = fixture.path.with_extension("pdf");
    let call = CallToolRequestParams::new("developer__write")
        .with_arguments(rmcp::object!({"path": path.to_string_lossy()}));
    let capture = fixture
        .prepare(&fixture.session, "pdf", "model-a", &call)
        .await;
    fs::write(&path, b"%PDF\0\x01\xff").unwrap();
    fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap();
    let saved = fixture
        .manager
        .get_output_revision(GetOutputRevisionRequest {
            session_id: fixture.session.id.clone(),
            path: path.to_string_lossy().into(),
            version: 1,
        })
        .await
        .unwrap();
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(saved.content_base64)
            .unwrap(),
        b"%PDF\0\x01\xff"
    );
    let source = Path::new("/tmp/main.rs");
    assert!(fixture
        .manager
        .list_output_revisions(ListOutputRevisionsRequest {
            session_id: fixture.session.id.clone(),
            path: source.to_string_lossy().into(),
            before_version: None,
            limit: None
        })
        .await
        .is_err());
}

#[tokio::test]
async fn external_changes_are_preserved_before_the_next_agent_edit() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "First")
        .await;
    fs::write(&fixture.path, "Outside edit").unwrap();
    fixture
        .write(&fixture.session, "second", "model-b", "Second")
        .await;
    let history = fixture.history().await;
    assert_eq!(history.len(), 3);
    assert_eq!(
        history[0].contributor.selected_model.as_deref(),
        Some("model-b")
    );
    assert_eq!(history[1].attribution, OutputAttributionKind::Unknown);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(fixture.get(2).await.content_base64)
            .unwrap(),
        b"Outside edit"
    );
}

#[tokio::test]
async fn deleted_outputs_can_be_exported_but_not_silently_recreated() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "Saved")
        .await;
    fs::remove_file(&fixture.path).unwrap();
    assert!(fixture.get(1).await.current_hash.is_none());
    assert!(fixture
        .manager
        .restore_output_revision(RestoreOutputRevisionRequest {
            session_id: fixture.session.id.clone(),
            path: fixture.path.to_string_lossy().into(),
            version: 1,
            expected_current_hash: "missing".into(),
        })
        .await
        .is_err());
    assert!(!fixture.path.exists());
}

#[tokio::test]
async fn read_only_roots_and_imported_requests_do_not_capture_content() {
    use gosling_sdk_types::workspace::{
        WorkspaceFolderAccess, WorkspaceFolderPolicy, WorkspaceFolderPolicyRoot,
        WorkspaceSessionContext,
    };
    let fixture = Fixture::new().await;
    let call = CallToolRequestParams::new("developer__write")
        .with_arguments(rmcp::object!({ "path": fixture.path.to_string_lossy() }));
    let mut scope = fixture.session.clone();
    scope.workspace_context = Some(WorkspaceSessionContext {
        folder_policy: WorkspaceFolderPolicy {
            roots: vec![WorkspaceFolderPolicyRoot {
                path: scope.working_dir.to_string_lossy().into(),
                access: WorkspaceFolderAccess::Read,
            }],
        },
        ..Default::default()
    });
    let capture = fixture.prepare(&scope, "read-only", "model-a", &call).await;
    fs::write(&fixture.path, "Outside writer").unwrap();
    fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap();
    assert_eq!(fs::read_to_string(&fixture.path).unwrap(), "Outside writer");
    assert!(fixture
        .manager
        .list_session_artifacts(&fixture.session.id, None, 20)
        .await
        .unwrap()
        .artifacts
        .is_empty());
    let mut imported = Message::assistant()
        .with_generated_id()
        .with_tool_request("imported", Ok(call.clone()));
    imported.metadata.imported_untrusted = true;
    fixture
        .manager
        .add_message(&fixture.session.id, &imported)
        .await
        .unwrap();
    assert!(fixture
        .manager
        .prepare_output_capture(&fixture.session, &call, "imported")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn capture_reports_size_limits_without_modifying_files() {
    let fixture = Fixture::new().await;
    let call = CallToolRequestParams::new("developer__write")
        .with_arguments(rmcp::object!({ "path": fixture.path.to_string_lossy() }));
    let capture = fixture
        .prepare(&fixture.session, "large", "model-a", &call)
        .await;
    let bytes = vec![b'x'; gosling::session::output_revisions::MAX_OUTPUT_REVISION_BYTES + 1];
    fs::write(&fixture.path, &bytes).unwrap();
    assert!(fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap_err()
        .to_string()
        .contains("8 MiB"));
    assert_eq!(fs::read(&fixture.path).unwrap(), bytes);
}

#[tokio::test]
async fn delegated_outputs_reach_parent_inventory_with_source_agent_identity() {
    let fixture = Fixture::new().await;
    let delegate = fixture
        .manager
        .create_session(
            fixture.session.working_dir.clone(),
            "Task description".into(),
            SessionType::SubAgent,
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    fixture.manager.merge_extension_state(&delegate.id, "output_agent.v1", serde_json::json!({ "name": "Research reviewer", "parentSessionId": fixture.session.id })).await.unwrap();
    let delegate = fixture
        .manager
        .get_session(&delegate.id, false)
        .await
        .unwrap();
    fixture
        .write(&delegate, "delegated", "model-b", "Delegate report")
        .await;
    assert_eq!(
        fixture.history().await[0].contributor.agent,
        "Research reviewer"
    );
    let delegate_call = CallToolRequestParams::new("summon__delegate");
    assert!(fixture
        .manager
        .prepare_output_capture(&fixture.session, &delegate_call, "parent")
        .await
        .unwrap()
        .is_none());
}

struct ReportProvider(std::sync::atomic::AtomicUsize);

#[async_trait::async_trait]
impl gosling::providers::base::Provider for ReportProvider {
    fn get_name(&self) -> &str {
        "output-history-test"
    }

    async fn stream(
        &self,
        _model: &gosling_providers::model::ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[rmcp::model::Tool],
    ) -> Result<gosling::providers::base::MessageStream, gosling_providers::errors::ProviderError>
    {
        let turn = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let message = if turn == 0 {
            Message::assistant().with_tool_request("real-write", Ok(CallToolRequestParams::new("developer__write").with_arguments(rmcp::object!({ "path": "Outputs/report.md", "content": "# Actual tool report" }))))
        } else {
            Message::assistant().with_text("Created [report](Outputs/report.md).")
        };
        Ok(gosling::providers::base::stream_from_single_message(
            message,
            gosling_providers::conversation::token_usage::ProviderUsage::new(
                "selected-model".into(),
                gosling_providers::conversation::token_usage::Usage::new(
                    Some(10),
                    Some(5),
                    Some(15),
                ),
            ),
        ))
    }
}

#[tokio::test]
#[serial_test::serial]
async fn actual_agent_tool_execution_records_selected_model_without_model_info() {
    use futures::StreamExt;
    use gosling::agents::{Agent, AgentConfig, GoslingPlatform, SessionConfig};
    use gosling::config::permission::{PermissionLevel, PermissionManager};
    use std::sync::Arc;
    let fixture = Fixture::new().await;
    let _env = env_lock::lock_env([(
        "GOSLING_PATH_ROOT",
        Some(fixture.temp.path().to_str().unwrap()),
    )]);
    let manager = Arc::new(SessionManager::new(fixture.temp.path().join("state")));
    let permissions = Arc::new(PermissionManager::new(
        fixture.temp.path().join("permissions"),
    ));
    permissions
        .update_user_permission("developer__write", PermissionLevel::AlwaysAllow)
        .unwrap();
    let agent = Agent::with_config(AgentConfig::new(
        manager,
        permissions,
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    ));
    agent
        .update_provider(
            Arc::new(ReportProvider(std::sync::atomic::AtomicUsize::new(0))),
            gosling_providers::model::ModelConfig::new("selected-model"),
            &fixture.session.id,
        )
        .await
        .unwrap();
    agent
        .add_extension(
            gosling::config::ExtensionConfig::Platform {
                name: "developer".into(),
                description: "Developer".into(),
                display_name: None,
                bundled: Some(true),
                available_tools: vec![],
            },
            &fixture.session.id,
        )
        .await
        .unwrap();
    let stream = agent
        .reply(
            Message::user().with_text("Write a report"),
            SessionConfig {
                id: fixture.session.id.clone(),
                max_turns: Some(3),
                compacted_context: false,
                tail_limit: None,
            },
            None,
        )
        .await
        .unwrap();
    tokio::pin!(stream);
    while let Some(event) = stream.next().await {
        event.unwrap();
    }
    let history = fixture.history().await;
    assert_eq!(
        history.len(),
        1,
        "{:?}",
        fixture
            .manager
            .get_session(&fixture.session.id, true)
            .await
            .unwrap()
            .conversation
    );
    assert_eq!(
        history[0].contributor.provider.as_deref(),
        Some("output-history-test")
    );
    assert_eq!(
        history[0].contributor.selected_model.as_deref(),
        Some("selected-model")
    );
    assert!(history[0].contributor.resolved_model.is_none());
    assert!(fs::read_to_string(&fixture.path)
        .unwrap()
        .contains("Actual tool report"));
}

#[tokio::test]
async fn schema_31_upgrade_preserves_existing_sessions() {
    let fixture = Fixture::new().await;
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        fixture
            .temp
            .path()
            .join("state/sessions/sessions.db")
            .display()
    ))
    .await
    .unwrap();
    sqlx::query("DROP TABLE output_revisions")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM schema_version WHERE version = 32")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO schema_version(version) VALUES (31)")
        .execute(&pool)
        .await
        .unwrap();
    let upgraded = SessionManager::new(fixture.temp.path().join("state"));
    assert_eq!(
        upgraded
            .get_session(&fixture.session.id, false)
            .await
            .unwrap()
            .name,
        fixture.session.name
    );
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM schema_version")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, 32);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM output_revisions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn editor_formatted_markdown_history_is_not_duplicated() {
    for (newline, trailing) in [("\n", "\n\n"), ("\r\n", " \r\n\t"), ("\n", "   ")] {
        let fixture = Fixture::new().await;
        fixture
            .write(&fixture.session, "first", "model-a", "Original")
            .await;
        let formatted = fs::read_to_string(&fixture.path)
            .unwrap()
            .replace('\n', newline)
            + trailing;
        fs::write(&fixture.path, &formatted).unwrap();
        fixture
            .write(&fixture.session, "same", "model-b", &formatted)
            .await;
        assert_eq!(
            fixture.history().await.len(),
            1,
            "footer-only changes are not revisions"
        );
        let edited = formatted.replacen("Original", "Edited", 1);
        fixture
            .write(&fixture.session, "edit", "model-b", &edited)
            .await;
        let report = fs::read_to_string(&fixture.path).unwrap();
        assert_eq!(report.matches("gosling:output-history:start").count(), 1);
        assert_eq!(fixture.history().await.len(), 2);
        assert!(report.starts_with("Edited\n\n<!--"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn failed_restore_does_not_commit_the_external_edit_baseline() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model-a", "Original")
        .await;
    fs::write(&fixture.path, "External edit").unwrap();
    let current = fixture.get(1).await;
    let directory = fixture.path.parent().unwrap();
    let permissions = fs::metadata(directory).unwrap().permissions();
    fs::set_permissions(directory, fs::Permissions::from_mode(0o555)).unwrap();
    let result = fixture
        .manager
        .restore_output_revision(RestoreOutputRevisionRequest {
            session_id: fixture.session.id.clone(),
            path: fixture.path.to_string_lossy().into(),
            version: 1,
            expected_current_hash: current.current_hash.unwrap(),
        })
        .await;
    fs::set_permissions(directory, permissions).unwrap();
    assert!(
        result.is_err(),
        "restore must fail in an unwritable directory"
    );
    assert_eq!(fs::read_to_string(&fixture.path).unwrap(), "External edit");
    assert_eq!(fixture.history().await.len(), 1);
    let reopened = SessionManager::new(fixture.temp.path().join("state"));
    let history = reopened
        .list_output_revisions(ListOutputRevisionsRequest {
            session_id: fixture.session.id.clone(),
            path: fixture.path.to_string_lossy().into(),
            before_version: None,
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(history.revisions.len(), 1);
}

#[tokio::test]
async fn oversized_sibling_does_not_abort_small_output_capture() {
    let fixture = Fixture::new().await;
    let huge = fixture.path.with_file_name("huge.pdf");
    fs::write(&huge, vec![b'x'; 8 * 1024 * 1024 + 1]).unwrap();
    let call = CallToolRequestParams::new("developer__write")
        .with_arguments(rmcp::object!({"path": fixture.path.to_string_lossy()}));
    let capture = fixture
        .prepare(&fixture.session, "small", "model", &call)
        .await;
    fs::write(&fixture.path, "Small document").unwrap();
    let error = fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("8 MiB"));
    assert_eq!(fixture.history().await.len(), 1);
    assert!(fs::read_to_string(&fixture.path)
        .unwrap()
        .starts_with("Small document"));
}

#[tokio::test]
async fn restore_commit_failure_leaves_live_bytes_unchanged() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model", "Original")
        .await;
    fs::write(&fixture.path, "Untracked precious edits").unwrap();
    let current = fixture.get(1).await;
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        fixture
            .temp
            .path()
            .join("state/sessions/sessions.db")
            .display()
    ))
    .await
    .unwrap();
    sqlx::query("CREATE TABLE restore_commit_parent (id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE restore_commit_failure (id INTEGER REFERENCES restore_commit_parent(id) DEFERRABLE INITIALLY DEFERRED)").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_restore_commit AFTER INSERT ON output_revisions WHEN json_extract(NEW.metadata_json, '$.action') = 'restored' BEGIN INSERT INTO restore_commit_failure VALUES (1); END").execute(&pool).await.unwrap();
    let result = fixture
        .manager
        .restore_output_revision(RestoreOutputRevisionRequest {
            session_id: fixture.session.id.clone(),
            path: fixture.path.to_string_lossy().into(),
            version: 1,
            expected_current_hash: current.current_hash.unwrap(),
        })
        .await;
    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(&fixture.path).unwrap(),
        "Untracked precious edits"
    );
    assert_eq!(fixture.history().await.len(), 1);
}

#[tokio::test]
async fn restore_uses_full_file_hash_not_body_hash() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model", "Original")
        .await;
    let current = fixture.get(1).await;
    assert_ne!(
        current.current_hash.as_deref(),
        Some(current.revision.content_hash.as_str())
    );
    let mut request = RestoreOutputRevisionRequest {
        session_id: fixture.session.id.clone(),
        path: fixture.path.to_string_lossy().into(),
        version: 1,
        expected_current_hash: current.revision.content_hash,
    };
    assert!(fixture
        .manager
        .restore_output_revision(request.clone())
        .await
        .is_err());
    request.expected_current_hash = current.current_hash.unwrap();
    assert!(fixture
        .manager
        .restore_output_revision(request)
        .await
        .is_ok());
}

#[tokio::test]
async fn per_file_storage_failure_does_not_abort_other_revisions() {
    let fixture = Fixture::new().await;
    let broken = fixture.path.with_file_name("broken.md");
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        fixture
            .temp
            .path()
            .join("state/sessions/sessions.db")
            .display()
    ))
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER reject_one_output BEFORE INSERT ON output_revisions WHEN NEW.path LIKE '%/broken.md' BEGIN SELECT RAISE(FAIL, 'per-file storage failure'); END")
        .execute(&pool).await.unwrap();
    let call = CallToolRequestParams::new("developer__write")
        .with_arguments(rmcp::object!({"path": fixture.path.to_string_lossy()}));
    let capture = fixture
        .prepare(&fixture.session, "siblings", "model", &call)
        .await;
    fs::write(&broken, "Cannot record this file").unwrap();
    fs::write(&fixture.path, "Record this file").unwrap();
    let error = fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("per-file storage failure"));
    assert_eq!(fixture.history().await.len(), 1);
}

#[tokio::test]
async fn saved_revision_remains_available_when_live_file_exceeds_capture_limit() {
    let fixture = Fixture::new().await;
    fixture
        .write(&fixture.session, "first", "model", "Original")
        .await;
    let saved = fixture.get(1).await;
    let expected_hash = saved.current_hash.clone().unwrap();
    let oversized = vec![b'x'; 8 * 1024 * 1024 + 1];
    fs::write(&fixture.path, &oversized).unwrap();

    let fetched = fixture.get(1).await;
    assert_eq!(fetched.content_base64, saved.content_base64);
    assert_eq!(fetched.revision, saved.revision);
    assert!(fetched.current_hash.is_none());
    assert!(fixture
        .manager
        .restore_output_revision(RestoreOutputRevisionRequest {
            session_id: fixture.session.id.clone(),
            path: fixture.path.to_string_lossy().into(),
            version: 1,
            expected_current_hash: expected_hash,
        })
        .await
        .is_err());
    assert_eq!(fs::read(&fixture.path).unwrap(), oversized);
    assert_eq!(fixture.history().await.len(), 1);
}

#[tokio::test]
async fn skipped_preimage_does_not_acquire_authorship_when_capture_budget_frees_up() {
    let fixture = Fixture::new().await;
    let root = fixture.path.parent().unwrap();
    let target = root.join("a.txt");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        fs::write(root.join(name), vec![b'x'; 8 * 1024 * 1024]).unwrap();
    }
    let untouched = root.join("z.md");
    fs::write(&untouched, "Human document").unwrap();
    let call = CallToolRequestParams::new("developer__write")
        .with_arguments(rmcp::object!({"path": target.to_string_lossy()}));
    let capture = fixture
        .prepare(&fixture.session, "shrink", "model", &call)
        .await;
    fs::write(&target, "Small replacement").unwrap();
    let error = fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("32 MiB"));
    assert_eq!(fs::read_to_string(&untouched).unwrap(), "Human document");
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        fixture
            .temp
            .path()
            .join("state/sessions/sessions.db")
            .display()
    ))
    .await
    .unwrap();
    for table in ["output_revisions", "session_artifacts"] {
        let path_column = if table == "output_revisions" {
            "path"
        } else {
            "resolved_path"
        };
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE {path_column} = ?"
        ))
        .bind(untouched.to_string_lossy().as_ref())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count, 0,
            "unchanged unobserved files must not enter {table}"
        );
    }
    let history = fixture
        .manager
        .list_output_revisions(ListOutputRevisionsRequest {
            session_id: fixture.session.id.clone(),
            path: target.to_string_lossy().into(),
            before_version: None,
            limit: None,
        })
        .await
        .unwrap()
        .revisions;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].action, OutputRevisionAction::Modified);
    assert_eq!(history[1].action, OutputRevisionAction::Baseline);
}

#[tokio::test]
async fn incomplete_scan_preserves_known_creation_without_crediting_unknown_preimages() {
    let fixture = Fixture::new().await;
    fs::create_dir_all(
        fixture
            .path
            .parent()
            .unwrap()
            .join("one/two/three/four/five"),
    )
    .unwrap();
    let call = CallToolRequestParams::new("developer__write")
        .with_arguments(rmcp::object!({"path": fixture.path.to_string_lossy()}));
    let capture = fixture
        .prepare(&fixture.session, "known-create", "model", &call)
        .await;
    let unknown = fixture.path.with_file_name("unknown.md");
    fs::write(&fixture.path, "Explicitly created").unwrap();
    fs::write(&unknown, "Uncertain origin").unwrap();
    let error = fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("four directory levels"));
    assert_eq!(fs::read_to_string(&unknown).unwrap(), "Uncertain origin");
    let history = fixture.history().await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action, OutputRevisionAction::Created);
    assert_eq!(history[0].attribution, OutputAttributionKind::Tool);
}

#[cfg(unix)]
#[tokio::test]
async fn recovered_preimage_permissions_do_not_invent_output_authorship() {
    use std::os::unix::fs::PermissionsExt;

    struct RestorePermissions(PathBuf, fs::Permissions);
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            fs::set_permissions(&self.0, self.1.clone()).unwrap();
        }
    }

    for blocked_workspace in [true, false] {
        let fixture = Fixture::new().await;
        let (target, blocked_directory, call) = if blocked_workspace {
            (
                fixture.path.clone(),
                fixture.session.working_dir.clone(),
                CallToolRequestParams::new("developer__shell")
                    .with_arguments(rmcp::object!({"command": "python generate_report.py"})),
            )
        } else {
            let directory = fixture.session.working_dir.join("documents");
            fs::create_dir(&directory).unwrap();
            let target = directory.join("untouched.md");
            let call = CallToolRequestParams::new("developer__write")
                .with_arguments(rmcp::object!({"path": target.to_string_lossy()}));
            (target, directory, call)
        };
        fs::write(&target, "Original human document").unwrap();
        let restore = RestorePermissions(
            blocked_directory.clone(),
            fs::metadata(&blocked_directory).unwrap().permissions(),
        );
        fs::set_permissions(&blocked_directory, fs::Permissions::from_mode(0o000)).unwrap();
        let capture = fixture
            .prepare(&fixture.session, "permission-recovery", "model", &call)
            .await;
        drop(restore);
        let result = fixture
            .manager
            .finish_output_capture(capture, &CallToolResult::success(vec![]))
            .await;
        assert!(
            result.is_err(),
            "incomplete pre-observation must be reported"
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "Original human document"
        );
        let pool = sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            fixture
                .temp
                .path()
                .join("state/sessions/sessions.db")
                .display()
        ))
        .await
        .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM output_revisions WHERE path = ?")
            .bind(target.to_string_lossy().as_ref())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn newly_created_output_parent_remains_captureable() {
    let fixture = Fixture::new().await;
    let target = fixture.path.parent().unwrap().join("new/report.md");
    let call = CallToolRequestParams::new("developer__write")
        .with_arguments(rmcp::object!({"path": target.to_string_lossy()}));
    let capture = fixture
        .prepare(&fixture.session, "new-parent", "model", &call)
        .await;
    fs::create_dir(target.parent().unwrap()).unwrap();
    fs::write(&target, "New output").unwrap();
    fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap();
    let history = fixture
        .manager
        .list_output_revisions(ListOutputRevisionsRequest {
            session_id: fixture.session.id.clone(),
            path: target.to_string_lossy().into(),
            before_version: None,
            limit: None,
        })
        .await
        .unwrap()
        .revisions;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action, OutputRevisionAction::Created);
    assert_eq!(history[0].attribution, OutputAttributionKind::Tool);
}

#[tokio::test]
async fn excluded_read_only_output_does_not_suppress_new_shell_output() {
    use gosling_sdk_types::workspace::{
        ProductOutputFolder, WorkspaceFolderAccess, WorkspaceFolderPolicy,
        WorkspaceFolderPolicyRoot, WorkspaceSessionContext,
    };

    let fixture = Fixture::new().await;
    let read_only = fixture.session.working_dir.join("ReferenceOutputs");
    fs::create_dir(&read_only).unwrap();
    let reference = read_only.join("reference.md");
    fs::write(&reference, "Read-only reference").unwrap();
    let mut scope = fixture.session.clone();
    scope.workspace_context = Some(WorkspaceSessionContext {
        product_output_folders: vec![ProductOutputFolder {
            path: read_only.to_string_lossy().into(),
            ..Default::default()
        }],
        folder_policy: WorkspaceFolderPolicy {
            roots: vec![
                WorkspaceFolderPolicyRoot {
                    path: scope.working_dir.to_string_lossy().into(),
                    access: WorkspaceFolderAccess::ReadWrite,
                },
                WorkspaceFolderPolicyRoot {
                    path: read_only.to_string_lossy().into(),
                    access: WorkspaceFolderAccess::Read,
                },
            ],
        },
        ..Default::default()
    });
    let call = CallToolRequestParams::new("developer__shell")
        .with_arguments(rmcp::object!({"command": "python generate_report.py"}));
    let capture = fixture.prepare(&scope, "shell", "model", &call).await;
    fs::write(&fixture.path, "New shell output").unwrap();
    fixture
        .manager
        .finish_output_capture(capture, &CallToolResult::success(vec![]))
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(&reference).unwrap(),
        "Read-only reference"
    );
    let history = fixture.history().await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action, OutputRevisionAction::Created);
    assert_eq!(history[0].attribution, OutputAttributionKind::Observed);
    let artifacts = fixture
        .manager
        .list_session_artifacts(&fixture.session.id, None, 20)
        .await
        .unwrap()
        .artifacts;
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].resolved_path, fixture.path.to_string_lossy());
}
