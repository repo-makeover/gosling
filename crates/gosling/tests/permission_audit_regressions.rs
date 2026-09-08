use gosling::config::permission::PermissionLevel;
use gosling::config::{GoslingMode, PermissionManager};
use gosling::conversation::message::ToolRequest;
use gosling::permission::{PermissionInspector, WorkingDirScopeInspector};
use gosling::security::egress_inspector::EgressInspector;
use gosling::session::{Session, SessionManager, SessionType};
use gosling::tool_inspection::{InspectionAction, ToolInspectionManager, ToolInspector};
use gosling::workspace::{
    WorkspaceFolderAccess, WorkspaceFolderPolicy, WorkspaceFolderPolicyRoot,
    WorkspaceSessionContext,
};
use rmcp::model::CallToolRequestParams;
use std::sync::Arc;
use tempfile::TempDir;

fn shell_request(id: &str, command: &str) -> ToolRequest {
    ToolRequest {
        id: id.into(),
        tool_call: Ok(
            CallToolRequestParams::new("developer__shell".to_string()).with_arguments(
                serde_json::json!({"command": command})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ),
        metadata: None,
        tool_meta: None,
    }
}

async fn workspace(
    root: &TempDir,
    access: WorkspaceFolderAccess,
) -> (Arc<SessionManager>, Session) {
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let manager = Arc::new(SessionManager::new(root.path().join("sessions")));
    let session = manager
        .create_session(
            project.clone(),
            "audit fixture".into(),
            SessionType::User,
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    let context = WorkspaceSessionContext {
        workspace_id: "audit".into(),
        workspace_name: "Audit".into(),
        primary_working_folder: project.to_string_lossy().into(),
        folders: Vec::new(),
        product_output_folders: Vec::new(),
        folder_policy: WorkspaceFolderPolicy {
            roots: vec![WorkspaceFolderPolicyRoot {
                path: project.to_string_lossy().into(),
                access,
            }],
        },
    };
    manager
        .update(&session.id)
        .workspace_snapshot("audit".into(), "Audit".into(), None, None, None, context)
        .apply()
        .await
        .unwrap();
    let session = manager.get_session(&session.id, false).await.unwrap();
    (manager, session)
}

#[test]
fn independent_permission_writer_preserves_existing_denial() {
    let root = tempfile::tempdir().unwrap();
    let first = PermissionManager::new(root.path().into());
    let second = PermissionManager::new(root.path().into());
    first
        .update_user_permission("dangerous_tool", PermissionLevel::NeverAllow)
        .unwrap();
    second
        .update_user_permission("other_tool", PermissionLevel::AlwaysAllow)
        .unwrap();
    let reloaded = PermissionManager::new(root.path().into());
    assert_eq!(
        reloaded.get_user_permission("dangerous_tool"),
        Some(PermissionLevel::NeverAllow),
        "a successful unrelated write must preserve another writer's denial"
    );
}

#[test]
fn existing_permission_reader_observes_revocation() {
    let root = tempfile::tempdir().unwrap();
    let first = PermissionManager::new(root.path().into());
    first
        .update_user_permission("dangerous_tool", PermissionLevel::AlwaysAllow)
        .unwrap();
    let second = PermissionManager::new(root.path().into());
    second
        .update_user_permission("dangerous_tool", PermissionLevel::NeverAllow)
        .unwrap();
    assert_eq!(
        first.get_user_permission("dangerous_tool"),
        Some(PermissionLevel::NeverAllow),
        "a long-lived manager must not retain revoked authority"
    );
}

#[test]
fn concurrent_permission_writers_preserve_both_decisions() {
    let root = tempfile::tempdir().unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    std::thread::scope(|scope| {
        for (principal, level) in [
            ("dangerous_tool", PermissionLevel::NeverAllow),
            ("other_tool", PermissionLevel::AlwaysAllow),
        ] {
            let directory = root.path().to_path_buf();
            let barrier = barrier.clone();
            scope.spawn(move || {
                let manager = PermissionManager::new(directory);
                barrier.wait();
                manager.update_user_permission(principal, level).unwrap();
            });
        }
    });
    let reloaded = PermissionManager::new(root.path().into());
    assert_eq!(
        [
            reloaded.get_user_permission("dangerous_tool"),
            reloaded.get_user_permission("other_tool"),
        ],
        [
            Some(PermissionLevel::NeverAllow),
            Some(PermissionLevel::AlwaysAllow)
        ],
        "both independent concurrent writes returned success and must survive"
    );
}

#[test]
fn permission_writer_child() {
    let Ok(directory) = std::env::var("GOSLING_PERMISSION_TEST_DIRECTORY") else {
        return;
    };
    let worker = std::env::var("GOSLING_PERMISSION_TEST_WORKER").unwrap();
    let directory = std::path::PathBuf::from(directory);
    let manager = PermissionManager::new(directory.clone());
    std::fs::write(directory.join(format!("ready-{worker}")), "ready").unwrap();
    wait_for_fixture_file(&directory.join("start"));
    for index in 0..30 {
        manager
            .update_user_permission(
                &format!("worker-{worker}-{index}"),
                PermissionLevel::NeverAllow,
            )
            .unwrap();
    }
}

fn wait_for_fixture_file(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {path:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn permission_updates_are_atomic_across_processes_and_visible_to_existing_readers() {
    let root = tempfile::tempdir().unwrap();
    let reader = PermissionManager::new(root.path().into());
    let children: Vec<_> = (0..2)
        .map(|worker| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "permission_writer_child", "--nocapture"])
                .env("GOSLING_PERMISSION_TEST_DIRECTORY", root.path())
                .env("GOSLING_PERMISSION_TEST_WORKER", worker.to_string())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for worker in 0..2 {
        wait_for_fixture_file(&root.path().join(format!("ready-{worker}")));
    }
    std::fs::write(root.path().join("start"), "start").unwrap();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    for worker in 0..2 {
        for index in 0..30 {
            assert_eq!(
                reader.get_user_permission(&format!("worker-{worker}-{index}")),
                Some(PermissionLevel::NeverAllow)
            );
        }
    }
}

#[test]
fn permission_removal_preserves_other_writers_and_revokes_existing_readers() {
    let root = tempfile::tempdir().unwrap();
    let first = PermissionManager::new(root.path().into());
    let second = PermissionManager::new(root.path().into());
    first
        .update_user_permission("extension__tool", PermissionLevel::AlwaysAllow)
        .unwrap();
    second
        .update_user_permission("other__tool", PermissionLevel::NeverAllow)
        .unwrap();
    first.remove_extension("extension").unwrap();
    assert_eq!(second.get_user_permission("extension__tool"), None);
    assert_eq!(
        first.get_user_permission("other__tool"),
        Some(PermissionLevel::NeverAllow)
    );
}

#[test]
fn unreadable_permission_state_never_reuses_a_cached_grant_or_overwrites_corruption() {
    let root = tempfile::tempdir().unwrap();
    let manager = PermissionManager::new(root.path().into());
    manager
        .update_user_permission("tool", PermissionLevel::AlwaysAllow)
        .unwrap();
    std::fs::write(manager.get_config_path(), "{{invalid yaml: [broken").unwrap();
    assert_eq!(
        manager.get_user_permission("tool"),
        Some(PermissionLevel::NeverAllow)
    );
    assert!(manager
        .update_user_permission("other", PermissionLevel::AlwaysAllow)
        .is_err());
    assert_eq!(
        std::fs::read_to_string(manager.get_config_path()).unwrap(),
        "{{invalid yaml: [broken"
    );
    std::fs::remove_file(manager.get_config_path()).unwrap();
    assert_eq!(manager.get_user_permission("tool"), None);
}

#[tokio::test]
async fn hosted_save_failure_is_reported_without_a_grant() {
    let root = tempfile::tempdir().unwrap();
    let permissions = Arc::new(PermissionManager::new(root.path().join("config")));
    std::fs::create_dir(permissions.get_config_path()).unwrap();
    let sessions = Arc::new(SessionManager::new(root.path().join("sessions")));
    let inspector = PermissionInspector::new(
        permissions.clone(),
        Arc::new(tokio::sync::Mutex::new(None)),
        sessions,
    );
    let mut manager = ToolInspectionManager::new();
    manager.add_inspector(Box::new(inspector));
    assert!(manager
        .update_permission_manager("developer__shell", PermissionLevel::AlwaysAllow)
        .await
        .is_err());
    assert_eq!(
        permissions.get_user_permission("developer__shell"),
        Some(PermissionLevel::NeverAllow)
    );
    assert!(permissions.get_config_path().is_dir());
}

#[tokio::test]
async fn readonly_workspace_rejects_env_and_background_mutations() {
    let root = tempfile::tempdir().unwrap();
    let (sessions, session) = workspace(&root, WorkspaceFolderAccess::Read).await;
    let inspector = WorkingDirScopeInspector::new(sessions);
    let target = session.working_dir.join("must-not-write");
    let requests = vec![
        shell_request("env", &format!("env touch '{}'", target.display())),
        shell_request(
            "background",
            &format!("true & touch '{}'", target.display()),
        ),
    ];
    let results = inspector
        .inspect(&session.id, &requests, &[], GoslingMode::Auto)
        .await
        .unwrap();
    let missing: Vec<_> = requests
        .iter()
        .filter(|request| {
            !results.iter().any(|r| {
                r.tool_request_id == request.id && matches!(r.action, InspectionAction::Deny)
            })
        })
        .map(|r| r.id.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "mutations missing read-only denial: {missing:?}"
    );
    assert!(!target.exists(), "audit strings must never be executed");
}

#[tokio::test]
async fn literal_heredoc_does_not_hide_a_later_outside_write() {
    let root = tempfile::tempdir().unwrap();
    let (sessions, session) = workspace(&root, WorkspaceFolderAccess::ReadWrite).await;
    let inspector = WorkingDirScopeInspector::new(sessions);
    let target = root.path().join("outside");
    let command = format!(
        "cat <<'EOF'\nDon't panic\nEOF\nprintf x > '{}'",
        target.display()
    );
    let results = inspector
        .inspect(
            &session.id,
            &[shell_request("heredoc", &command)],
            &[],
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    assert!(
        results
            .iter()
            .any(|r| matches!(r.action, InspectionAction::RequireApproval(_))),
        "the real write after literal heredoc data must require approval: {results:?}"
    );
    assert!(!target.exists(), "audit strings must never be executed");
}

#[tokio::test]
async fn readonly_workspace_accepts_a_comment_with_an_apostrophe() {
    let root = tempfile::tempdir().unwrap();
    let (sessions, session) = workspace(&root, WorkspaceFolderAccess::Read).await;
    let results = WorkingDirScopeInspector::new(sessions)
        .inspect(
            &session.id,
            &[shell_request("comment", "ls # don't change anything\nls")],
            &[],
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    assert!(
        results.is_empty(),
        "benign comment must not become a mutation: {results:?}"
    );
}

#[tokio::test]
async fn egress_checks_later_upload_to_the_same_destination() {
    let root = tempfile::tempdir().unwrap();
    let inspector = EgressInspector::new(Arc::new(PermissionManager::new(root.path().into())));
    let mut missed = Vec::new();
    for first in [
        "curl https://audit.invalid/endpoint",
        "printf '%s' 'https://audit.invalid/endpoint'",
        "curl -X POST --data first https://audit.invalid/endpoint",
    ] {
        let results = inspector
            .inspect(
                "audit",
                &[
                    shell_request("first", first),
                    shell_request(
                        "upload",
                        "curl -X POST --data payload https://audit.invalid/endpoint",
                    ),
                ],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();
        if first.starts_with("printf") {
            assert!(!results.iter().any(|r| r.tool_request_id == "first"));
        } else if !first.contains("POST") {
            assert!(results.iter().any(|r| {
                r.tool_request_id == "first" && matches!(r.action, InspectionAction::Allow)
            }));
        } else {
            assert!(results.iter().any(|r| {
                r.tool_request_id == "first"
                    && matches!(r.action, InspectionAction::RequireApproval(_))
            }));
        }
        if !results.iter().any(|r| {
            r.tool_request_id == "upload"
                && matches!(r.action, InspectionAction::RequireApproval(_))
        }) {
            missed.push((first, results));
        }
    }
    assert!(
        missed.is_empty(),
        "earlier requests suppressed upload checks: {missed:?}"
    );
}

#[tokio::test]
async fn shell_syntax_preserves_executable_mutations_and_literal_data() {
    let root = tempfile::tempdir().unwrap();
    let (sessions, session) = workspace(&root, WorkspaceFolderAccess::ReadWrite).await;
    let inspector = WorkingDirScopeInspector::new(sessions);
    let outside = root.path().join("outside");
    let target = outside.to_str().unwrap();
    let mutations = [
        format!("env -i FOO=x touch '{target}'"),
        format!("/usr/bin/env -u FOO command -- touch '{target}'"),
        format!("true & touch '{target}' & true"),
        format!("printf '%s' \"$(touch '{target}')\""),
        format!("cat <(touch '{target}')"),
        format!("x=$(touch '{target}'); printf done"),
        format!("{{ printf x; }} > '{target}'"),
        format!("> '{target}' printf x"),
        format!("env bash -lc 'touch {target}'"),
        format!("sh <<'EOS'\ntouch '{target}'\nEOS"),
        format!("env sh <<'EOS'\ntouch '{target}'\nEOS"),
        format!("cat <<EOS\n$(touch '{target}')\nEOS"),
        format!("cat <<-'EOS'\n\tDon't panic\n\tEOS\nprintf x > '{target}'"),
    ];
    for command in mutations {
        let results = inspector
            .inspect(
                &session.id,
                &[shell_request("mutation", &command)],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();
        assert!(
            results
                .iter()
                .any(|result| matches!(result.action, InspectionAction::RequireApproval(_))),
            "missed mutation: {command:?}"
        );
    }
    for command in [
        format!("cat <<'EOS'\nDon't panic; touch {target}\n$(touch {target})\nEOS"),
        format!("printf '%s' '>{target}'"),
        format!("env FOO=x cat '{target}'"),
        "printf ok >/dev/null 2>&1 # don't merge this\nls".into(),
    ] {
        let results = inspector
            .inspect(
                &session.id,
                &[shell_request("literal", &command)],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "literal or read-only diagnostic invented an effect: {command:?}: {results:?}"
        );
    }
    assert!(
        !outside.exists(),
        "inspection must never execute these strings"
    );
}

#[tokio::test]
async fn unsupported_shell_syntax_is_not_silently_dropped() {
    let root = tempfile::tempdir().unwrap();
    let (sessions, session) = workspace(&root, WorkspaceFolderAccess::ReadWrite).await;
    let results = WorkingDirScopeInspector::new(sessions)
        .inspect(
            &session.id,
            &[shell_request("unparsed", "printf 'unterminated")],
            &[],
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    assert!(matches!(
        results[0].action,
        InspectionAction::RequireApproval(_)
    ));
}

#[tokio::test]
async fn egress_revocation_and_unreadable_policy_deny_previously_allowed_requests() {
    let root = tempfile::tempdir().unwrap();
    let permissions = Arc::new(PermissionManager::new(root.path().into()));
    permissions
        .update_egress_domain_permission("audit.invalid", PermissionLevel::AlwaysAllow)
        .unwrap();
    let inspector = EgressInspector::new(permissions.clone());
    let request = [shell_request(
        "upload",
        "curl -d payload https://audit.invalid/endpoint",
    )];
    let results = inspector
        .inspect("test", &request, &[], GoslingMode::Auto)
        .await
        .unwrap();
    assert!(matches!(results[0].action, InspectionAction::Allow));
    PermissionManager::new(root.path().into())
        .update_egress_domain_permission("audit.invalid", PermissionLevel::NeverAllow)
        .unwrap();
    let results = inspector
        .inspect("test", &request, &[], GoslingMode::Auto)
        .await
        .unwrap();
    assert!(matches!(results[0].action, InspectionAction::Deny));
    std::fs::write(permissions.get_config_path(), "{{invalid yaml: [broken").unwrap();
    let results = inspector
        .inspect("test", &request, &[], GoslingMode::Auto)
        .await
        .unwrap();
    assert!(matches!(results[0].action, InspectionAction::Deny));
}

#[tokio::test]
async fn multiple_urls_on_one_domain_offer_one_persistent_domain_scope() {
    let root = tempfile::tempdir().unwrap();
    let inspector = EgressInspector::new(Arc::new(PermissionManager::new(root.path().into())));
    let results = inspector
        .inspect(
            "test",
            &[shell_request(
                "upload",
                "curl -d payload https://audit.invalid/first https://audit.invalid/second",
            )],
            &[],
            GoslingMode::Auto,
        )
        .await
        .unwrap();
    assert_eq!(
        gosling::tool_inspection::single_flagged_domain_for_request("upload", &results),
        Some("audit.invalid".into())
    );
}

#[tokio::test]
async fn a_missing_session_does_not_skip_its_workspace_inspection() {
    let root = tempfile::tempdir().unwrap();
    let inspector =
        WorkingDirScopeInspector::new(Arc::new(SessionManager::new(root.path().into())));
    assert!(inspector
        .inspect(
            "missing",
            &[shell_request("write", "touch /outside/file")],
            &[],
            GoslingMode::Auto
        )
        .await
        .is_err());
}
