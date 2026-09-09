//! Regression coverage for the agent compatibility facade.
//!
//! Maintainers: keep behavior tests at their original module path.
//! Clients: no production API or runtime behavior is defined here.

use super::*;
use crate::permission::permission_confirmation::PrincipalType;
use crate::plugins::discovery::{DiscoveredPlugin, PluginScope};
use crate::providers::base::{stream_from_single_message, MessageStream, PermissionRouting};
use crate::session::session_manager::SessionType;
use gosling_providers::conversation::token_usage::{ProviderUsage, Usage};
use rmcp::model::Tool;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

#[test]
fn resolve_use_login_shell_path_defaults_by_platform() {
    assert!(resolve_use_login_shell_path(
        None,
        &GoslingPlatform::GoslingDesktop
    ));
    assert!(!resolve_use_login_shell_path(
        None,
        &GoslingPlatform::GoslingCli
    ));
}

#[test]
fn resolve_use_login_shell_path_explicit_overrides_platform() {
    assert!(resolve_use_login_shell_path(
        Some(true),
        &GoslingPlatform::GoslingCli
    ));
    assert!(!resolve_use_login_shell_path(
        Some(false),
        &GoslingPlatform::GoslingDesktop
    ));
}

fn needs_approval_fixture() -> (PermissionCheckResult, HashMap<String, Message>) {
    let request = ToolRequest {
        id: "req-1".to_string(),
        tool_call: Ok(CallToolRequestParams::new("shell").with_arguments(rmcp::object!({}))),
        metadata: None,
        tool_meta: None,
    };
    let permission_check_result = PermissionCheckResult {
        approved: vec![],
        needs_approval: vec![request],
        denied: vec![],
    };
    let mut request_to_response_map = HashMap::new();
    request_to_response_map.insert("req-1".to_string(), Message::user().with_generated_id());
    (permission_check_result, request_to_response_map)
}

#[test]
fn redirect_unapprovable_subagent_requests_denies_in_auto_mode_subagent() {
    let (mut permission_check_result, mut request_to_response_map) = needs_approval_fixture();

    Agent::redirect_unapprovable_subagent_requests(
        GoslingMode::Auto,
        SessionType::SubAgent,
        &mut permission_check_result,
        &mut request_to_response_map,
    );

    assert!(
        permission_check_result.needs_approval.is_empty(),
        "the unanswerable approval request must be drained, not left to hang"
    );
    let response = request_to_response_map
        .get("req-1")
        .expect("response entry must still exist");
    let has_error_tool_response = response.content.iter().any(|c| match c {
        MessageContent::ToolResponse(r) => matches!(
            &r.tool_result,
            Ok(result) if r.id == "req-1" && result.is_error == Some(true)
        ),
        _ => false,
    });
    assert!(
        has_error_tool_response,
        "a synthesized error tool response must be written instead of hanging"
    );
}

#[test]
fn chat_mode_tool_skip_is_an_error_result() {
    let request = ToolRequest {
        id: "req-chat".to_string(),
        tool_call: Ok(CallToolRequestParams::new("shell")),
        metadata: None,
        tool_meta: None,
    };
    let mut responses = HashMap::from([(request.id.clone(), Message::user().with_generated_id())]);

    Agent::record_chat_mode_tool_skip(&request, &mut responses);

    assert!(responses[&request.id]
        .content
        .iter()
        .any(|content| match content {
            MessageContent::ToolResponse(response) => {
                response
                    .tool_result
                    .as_ref()
                    .is_ok_and(|result| result.is_error == Some(true))
            }
            _ => false,
        }));
}

#[test]
fn redirect_unapprovable_subagent_requests_leaves_top_level_auto_mode_untouched() {
    let (mut permission_check_result, mut request_to_response_map) = needs_approval_fixture();

    Agent::redirect_unapprovable_subagent_requests(
        GoslingMode::Auto,
        SessionType::User,
        &mut permission_check_result,
        &mut request_to_response_map,
    );

    assert_eq!(
        permission_check_result.needs_approval.len(),
        1,
        "a top-level (non-subagent) session can answer its own approval prompt"
    );
}

#[test]
fn redirect_unapprovable_subagent_requests_leaves_non_auto_subagent_untouched() {
    let (mut permission_check_result, mut request_to_response_map) = needs_approval_fixture();

    Agent::redirect_unapprovable_subagent_requests(
        GoslingMode::SmartApprove,
        SessionType::SubAgent,
        &mut permission_check_result,
        &mut request_to_response_map,
    );

    assert_eq!(permission_check_result.needs_approval.len(), 1);
}

struct ActionRequiredProvider {
    handled: tokio::sync::Mutex<Vec<(String, PermissionConfirmation)>>,
}

impl ActionRequiredProvider {
    fn new() -> Self {
        Self {
            handled: tokio::sync::Mutex::new(Vec::new()),
        }
    }
}

impl std::fmt::Debug for ActionRequiredProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionRequiredProvider").finish()
    }
}

#[async_trait::async_trait]
impl crate::providers::base::Provider for ActionRequiredProvider {
    fn get_name(&self) -> &str {
        "test-action-required"
    }
    async fn stream(
        &self,
        _: &gosling_providers::model::ModelConfig,
        _: &str,
        _: &[crate::conversation::message::Message],
        _: &[rmcp::model::Tool],
    ) -> Result<crate::providers::base::MessageStream, ProviderError> {
        unimplemented!()
    }
    fn permission_routing(&self) -> PermissionRouting {
        PermissionRouting::ActionRequired
    }
    async fn handle_permission_confirmation(
        &self,
        request_id: &str,
        confirmation: &PermissionConfirmation,
    ) -> bool {
        self.handled
            .lock()
            .await
            .push((request_id.to_string(), confirmation.clone()));
        request_id == "known"
    }
}

#[tokio::test]
async fn test_handle_confirmation_routes_to_provider() {
    let agent = Agent::new();
    let provider = Arc::new(ActionRequiredProvider::new());
    *agent.provider.lock().await =
        Some(provider.clone() as Arc<dyn crate::providers::base::Provider>);

    // Known request_id → provider handles it, confirmation_router NOT called
    agent
        .handle_confirmation(
            "known".to_string(),
            PermissionConfirmation {
                principal_type: PrincipalType::Tool,
                permission: crate::permission::Permission::AllowOnce,
            },
        )
        .await;
    assert_eq!(provider.handled.lock().await.len(), 1);

    // Unknown request_id → provider returns false, falls through to confirmation_router
    // Register first so deliver() has somewhere to send
    let rx = agent
        .tool_confirmation_router
        .register("unknown".to_string())
        .await;
    agent
        .handle_confirmation(
            "unknown".to_string(),
            PermissionConfirmation {
                principal_type: PrincipalType::Tool,
                permission: crate::permission::Permission::DenyOnce,
            },
        )
        .await;
    assert_eq!(provider.handled.lock().await.len(), 2);
    // Verify the fallthrough went to confirmation_router
    let conf = rx.await.unwrap();
    assert_eq!(conf.permission, crate::permission::Permission::DenyOnce);
}

#[tokio::test]
async fn test_handle_confirmation_noop_provider() {
    let agent = Agent::new();
    // No provider set → Noop routing, goes straight to confirmation_router
    // Register first so deliver() has somewhere to send
    let rx = agent
        .tool_confirmation_router
        .register("any".to_string())
        .await;
    agent
        .handle_confirmation(
            "any".to_string(),
            PermissionConfirmation {
                principal_type: PrincipalType::Tool,
                permission: crate::permission::Permission::AllowOnce,
            },
        )
        .await;

    let conf = rx.await.unwrap();
    assert_eq!(conf.permission, crate::permission::Permission::AllowOnce);
}

const ALWAYS_BLOCK_SCRIPT: &str = r#"#!/bin/sh
echo blocked >> "$PLUGIN_ROOT/hook.log"
echo "always block" >&2
exit 2
"#;

const ALTERNATE_BLOCK_ALLOW_SCRIPT: &str = r#"#!/bin/sh
count_file="$PLUGIN_ROOT/count"
count=0
if [ -f "$count_file" ]; then
  count=$(cat "$count_file")
fi
count=$((count + 1))
echo "$count" > "$count_file"
echo "$count" >> "$PLUGIN_ROOT/hook.log"
if [ $((count % 2)) -eq 1 ]; then
  echo "block $count" >&2
  exit 2
fi
exit 0
"#;

const RECORD_PAYLOAD_SCRIPT: &str = r#"#!/bin/sh
cat > "$PLUGIN_ROOT/payload.json"
exit 0
"#;

const PRE_TOOL_BLOCK_SCRIPT: &str = r#"#!/bin/sh
echo "path denied" >&2
exit 2
"#;

struct PreToolHookTestEnv {
    temp_dir: TempDir,
    plugin_dir: PathBuf,
}

impl PreToolHookTestEnv {
    fn new(script: &str) -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let plugin_dir = temp_dir.path().join("pre-tool-blocker");
        std::fs::create_dir_all(plugin_dir.join("hooks"))?;
        std::fs::write(
            plugin_dir.join("hooks/hooks.json"),
            r#"{
  "hooks": {
    "PreToolUse": [
      {
        "hooks": [
          { "type": "command", "command": "sh ${PLUGIN_ROOT}/block.sh" }
        ]
      }
    ]
  }
}
"#,
        )?;
        std::fs::write(plugin_dir.join("block.sh"), script)?;

        Ok(Self {
            temp_dir,
            plugin_dir,
        })
    }

    fn hook_manager(&self) -> crate::hooks::HookManager {
        crate::hooks::HookManager::from_plugins_for_test(vec![DiscoveredPlugin {
            name: "pre-tool-blocker".into(),
            root: self.plugin_dir.clone(),
            scope: PluginScope::Project,
        }])
    }

    fn data_dir(&self) -> PathBuf {
        self.temp_dir.path().join("data")
    }

    fn work_dir(&self) -> PathBuf {
        self.temp_dir.path().join("work")
    }
}

struct StopHookTestEnv {
    temp_dir: TempDir,
    hook_log: PathBuf,
    payload_path: PathBuf,
}

impl StopHookTestEnv {
    fn new(script: &str) -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let plugin_dir = temp_dir.path().join("stop-blocker");
        std::fs::create_dir_all(plugin_dir.join("hooks"))?;
        std::fs::write(
            plugin_dir.join("hooks/hooks.json"),
            r#"{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          { "type": "command", "command": "sh ${PLUGIN_ROOT}/block.sh" }
        ]
      }
    ]
  }
}
"#,
        )?;
        std::fs::write(plugin_dir.join("block.sh"), script)?;

        Ok(Self {
            temp_dir,
            hook_log: plugin_dir.join("hook.log"),
            payload_path: plugin_dir.join("payload.json"),
        })
    }

    fn hook_manager(&self) -> crate::hooks::HookManager {
        crate::hooks::HookManager::from_plugins_for_test(vec![DiscoveredPlugin {
            name: "stop-blocker".into(),
            root: self.temp_dir.path().join("stop-blocker"),
            scope: PluginScope::Project,
        }])
    }

    fn data_dir(&self) -> PathBuf {
        self.temp_dir.path().join("data")
    }

    fn hook_invocations(&self) -> usize {
        std::fs::read_to_string(&self.hook_log)
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn stop_payload(&self) -> Result<Value> {
        let payload = std::fs::read_to_string(&self.payload_path)?;
        Ok(serde_json::from_str(&payload)?)
    }
}

struct SessionStartHookTestEnv {
    temp_dir: TempDir,
    hook_log: PathBuf,
}

impl SessionStartHookTestEnv {
    fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let plugin_dir = temp_dir.path().join("session-start");
        std::fs::create_dir_all(plugin_dir.join("hooks"))?;
        std::fs::write(
            plugin_dir.join("hooks/hooks.json"),
            r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          { "type": "command", "command": "sh ${PLUGIN_ROOT}/start.sh" }
        ]
      }
    ]
  }
}
"#,
        )?;
        std::fs::write(
            plugin_dir.join("start.sh"),
            r#"#!/bin/sh
echo start >> "$PLUGIN_ROOT/hook.log"
"#,
        )?;

        Ok(Self {
            temp_dir,
            hook_log: plugin_dir.join("hook.log"),
        })
    }

    fn hook_manager(&self) -> crate::hooks::HookManager {
        crate::hooks::HookManager::from_plugins_for_test(vec![DiscoveredPlugin {
            name: "session-start".into(),
            root: self.temp_dir.path().join("session-start"),
            scope: PluginScope::Project,
        }])
    }

    fn data_dir(&self) -> PathBuf {
        self.temp_dir.path().join("data")
    }

    fn hook_invocations(&self) -> usize {
        std::fs::read_to_string(&self.hook_log)
            .unwrap_or_default()
            .lines()
            .count()
    }
}

struct CountingTextProvider {
    call_count: AtomicUsize,
}

impl CountingTextProvider {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl crate::providers::base::Provider for CountingTextProvider {
    async fn stream(
        &self,
        _model_config: &gosling_providers::model::ModelConfig,
        _system_prompt: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst);
        let message = Message::assistant().with_text(format!("provider response {call}"));
        let usage = ProviderUsage::new("mock-model".to_string(), Usage::default());
        Ok(stream_from_single_message(message, usage))
    }

    fn get_name(&self) -> &str {
        "counting-text"
    }
}

struct ChunkedTextProvider;

#[async_trait::async_trait]
impl crate::providers::base::Provider for ChunkedTextProvider {
    async fn stream(
        &self,
        _model_config: &gosling_providers::model::ModelConfig,
        _system_prompt: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let usage = ProviderUsage::new("mock-model".to_string(), Usage::default());
        Ok(Box::pin(futures::stream::iter(vec![
            Ok((Some(Message::assistant().with_text("streamed ")), None)),
            Ok((
                Some(Message::assistant().with_text("assistant reply")),
                Some(usage),
            )),
        ])))
    }

    fn get_name(&self) -> &str {
        "chunked-text"
    }
}

struct RefusingProvider {
    call_count: AtomicUsize,
}

#[derive(Default)]
struct ModeRecordingProvider {
    updates: tokio::sync::Mutex<Vec<(String, GoslingMode)>>,
}

#[async_trait::async_trait]
impl crate::providers::base::Provider for ModeRecordingProvider {
    async fn stream(
        &self,
        _model_config: &gosling_providers::model::ModelConfig,
        _system_prompt: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        unimplemented!()
    }

    fn get_name(&self) -> &str {
        "mode-recording"
    }

    async fn update_mode(&self, session_id: &str, mode: GoslingMode) -> Result<(), ProviderError> {
        self.updates
            .lock()
            .await
            .push((session_id.to_string(), mode));
        Ok(())
    }
}

#[async_trait::async_trait]
impl crate::providers::base::Provider for RefusingProvider {
    async fn stream(
        &self,
        _model_config: &gosling_providers::model::ModelConfig,
        _system_prompt: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(Box::pin(futures::stream::once(async {
            Err(ProviderError::Refusal {
                details: "This request was declined.".to_string(),
                category: Some("cyber".to_string()),
            })
        })))
    }

    fn get_name(&self) -> &str {
        "refusing"
    }
}

#[tokio::test]
async fn update_provider_propagates_active_mode() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    let permission_manager = Arc::new(PermissionManager::new(temp_dir.path().to_path_buf()));
    let agent = Agent::with_config(AgentConfig::new(
        session_manager.clone(),
        permission_manager,
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    ));
    let session = session_manager
        .create_session(
            PathBuf::default(),
            "mode-propagation".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    let provider = Arc::new(ModeRecordingProvider::default());

    agent
        .update_provider(
            provider.clone(),
            gosling_providers::model::ModelConfig::new("mock-model"),
            &session.id,
        )
        .await?;

    assert_eq!(
        provider.updates.lock().await.as_slice(),
        &[(session.id, GoslingMode::Auto)]
    );
    Ok(())
}

#[tokio::test]
async fn provider_persistence_failure_preserves_live_provider() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    let agent = Agent::with_config(AgentConfig::new(
        session_manager.clone(),
        Arc::new(PermissionManager::new(temp_dir.path().to_path_buf())),
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    ));
    let session = session_manager
        .create_session(
            PathBuf::default(),
            "provider-persistence-failure".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    agent
        .update_provider(
            Arc::new(ChunkedTextProvider),
            gosling_providers::model::ModelConfig::new("old-model"),
            &session.id,
        )
        .await?;
    sqlx::query(
        "CREATE TRIGGER fail_session_updates BEFORE UPDATE ON sessions \
             BEGIN SELECT RAISE(FAIL, 'injected update failure'); END",
    )
    .execute(session_manager.storage().pool().await?)
    .await?;

    let result = agent
        .update_provider(
            Arc::new(ModeRecordingProvider::default()),
            gosling_providers::model::ModelConfig::new("new-model"),
            &session.id,
        )
        .await;

    assert!(result.is_err());
    assert_eq!(agent.provider().await?.get_name(), "chunked-text");
    Ok(())
}

#[tokio::test]
async fn provider_switch_rejected_when_it_disagrees_with_pinned_credential_profile() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let workspace_data_dir = tempfile::tempdir()?;
    let workspace_root = tempfile::tempdir()?;
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    let workspace_service = Arc::new(
        crate::workspace::WorkspaceService::initialize(
            workspace_data_dir.path(),
            workspace_root.path(),
        )
        .await?,
    );
    // "ollama" has no required secret fields, so the profile is
    // immediately Configured without touching global secret storage.
    let profile = workspace_service
        .create_profile(
            "Local Ollama".to_string(),
            "ollama".to_string(),
            crate::workspace::CredentialAuthKind::Local,
            std::collections::BTreeMap::new(),
            Vec::new(),
        )
        .await?;
    assert_eq!(
        profile.status,
        crate::workspace::CredentialProfileStatus::Configured
    );

    let agent = Agent::with_config(
        AgentConfig::new(
            session_manager.clone(),
            Arc::new(PermissionManager::new(temp_dir.path().to_path_buf())),
            GoslingMode::Auto,
            true,
            GoslingPlatform::GoslingCli,
        )
        .with_workspace_service(workspace_service),
    );
    let session = session_manager
        .create_session(
            PathBuf::default(),
            "pinned-credential-mismatch".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    session_manager
        .update(&session.id)
        .workspace_snapshot(
            "workspace-id".to_string(),
            "Workspace".to_string(),
            Some(profile.id.clone()),
            Some(profile.name.clone()),
            None,
            crate::workspace::WorkspaceSessionContext::default(),
        )
        .apply()
        .await?;

    let result = agent
        .recreate_provider_for_session(
            &session.id,
            "definitely-not-ollama",
            gosling_providers::model::ModelConfig::new("mock-model"),
        )
        .await;

    let error = result.expect_err(
        "switching a pinned session to a provider outside its credential profile must fail",
    );
    assert!(
        error.to_string().contains("pinned to provider 'ollama'"),
        "unexpected error: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn mode_persistence_failure_preserves_live_mode() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    let agent = Agent::with_config(AgentConfig::new(
        session_manager.clone(),
        Arc::new(PermissionManager::new(temp_dir.path().to_path_buf())),
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    ));
    let session = session_manager
        .create_session(
            PathBuf::default(),
            "mode-persistence-failure".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    let provider = Arc::new(ModeRecordingProvider::default());
    agent
        .update_provider(
            provider.clone(),
            gosling_providers::model::ModelConfig::new("model"),
            &session.id,
        )
        .await?;
    sqlx::query(
        "CREATE TRIGGER fail_session_updates BEFORE UPDATE ON sessions \
             BEGIN SELECT RAISE(FAIL, 'injected update failure'); END",
    )
    .execute(session_manager.storage().pool().await?)
    .await?;

    let result = agent
        .update_gosling_mode(GoslingMode::SmartApprove, &session.id)
        .await;

    assert!(result.is_err());
    assert_eq!(agent.gosling_mode().await, GoslingMode::Auto);
    assert_eq!(
        provider.updates.lock().await.as_slice(),
        &[(session.id, GoslingMode::Auto)]
    );
    Ok(())
}

#[tokio::test]
async fn denied_pre_tool_use_does_not_inject_subdirectory_hints() -> Result<()> {
    let env = PreToolHookTestEnv::new(PRE_TOOL_BLOCK_SCRIPT)?;
    let work_dir = env.work_dir();
    let sub_dir = work_dir.join("sub");
    std::fs::create_dir_all(&sub_dir)?;
    std::fs::write(
        sub_dir.join(crate::hints::GOSLING_HINTS_FILENAME),
        "denied hint",
    )?;

    let provider = Arc::new(CountingTextProvider::new());
    let session_manager = Arc::new(SessionManager::new(env.data_dir()));
    let permission_manager = PermissionManager::instance();
    let config = AgentConfig::new(
        session_manager.clone(),
        permission_manager,
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    );
    let mut agent = Agent::with_config(config);
    agent.set_hook_manager_for_test(env.hook_manager());
    let session = session_manager
        .create_session(
            work_dir.clone(),
            "pre-tool-deny".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    agent
        .update_provider(
            provider,
            gosling_providers::model::ModelConfig::new("mock-model"),
            &session.id,
        )
        .await?;

    let tool_call = CallToolRequestParams::new("inspect")
        .with_arguments(rmcp::object!({ "path": "sub/secret.txt" }));
    let (_request_id, result) = agent
        .dispatch_tool_call(
            tool_call,
            "request-1".to_string(),
            None,
            &session_manager.get_session(&session.id, false).await?,
        )
        .await;

    assert!(result.is_err(), "policy hook should deny the tool call");
    let hints = agent
        .subdirectory_hint_tracker
        .lock()
        .await
        .collect_new_hints(&work_dir);
    assert!(
        hints.is_none(),
        "a denied tool path must not be converted into hidden agent-visible hints"
    );
    Ok(())
}

#[tokio::test]
async fn refusal_exits_turn() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let provider = Arc::new(RefusingProvider {
        call_count: AtomicUsize::new(0),
    });
    let hook_manager = crate::hooks::HookManager::from_plugins_for_test(vec![]);
    let (agent, session_id) =
        create_test_agent(temp_dir.path().join("data"), hook_manager, provider.clone()).await?;

    let session_config = SessionConfig {
        id: session_id,
        max_turns: Some(10),
        compacted_context: false,
        tail_limit: None,
    };

    let reply_stream = agent
        .reply(Message::user().with_text("hi"), session_config, None)
        .await?;
    tokio::pin!(reply_stream);
    while let Some(event) = reply_stream.next().await {
        event?;
    }

    assert_eq!(
        provider.call_count.load(Ordering::SeqCst),
        1,
        "a refused request must not be resent"
    );
    Ok(())
}

async fn create_test_agent(
    data_dir: PathBuf,
    hook_manager: crate::hooks::HookManager,
    provider: Arc<dyn crate::providers::base::Provider>,
) -> Result<(Agent, String)> {
    let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
    let permission_manager = Arc::new(PermissionManager::new(data_dir));
    let config = AgentConfig::new(
        session_manager.clone(),
        permission_manager,
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    );
    let mut agent = Agent::with_config(config);
    agent.set_hook_manager_for_test(hook_manager);
    let session = session_manager
        .create_session(
            PathBuf::default(),
            "test".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    agent
        .update_provider(
            provider,
            gosling_providers::model::ModelConfig::new("mock-model"),
            &session.id,
        )
        .await?;
    Ok((agent, session.id))
}

#[cfg(feature = "code-mode")]
#[tokio::test]
async fn disabled_code_execution_runtime_omits_code_mode_prompt_behavior() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("data");
    let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
    let permission_manager = Arc::new(PermissionManager::new(data_dir));
    let config = AgentConfig::new(
        session_manager.clone(),
        permission_manager,
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    )
    .with_code_execution_runtime(CodeExecutionRuntime::Disabled);
    let agent = Agent::with_config(config);
    let session = session_manager
        .create_session(
            PathBuf::default(),
            "code-runtime-disabled".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    agent
        .update_provider(
            Arc::new(CountingTextProvider::new()),
            gosling_providers::model::ModelConfig::new("mock-model"),
            &session.id,
        )
        .await?;

    let code_execution_config = ExtensionConfig::Platform {
        name: "code_execution".to_string(),
        description: "Code Mode".to_string(),
        display_name: Some("Code Mode".to_string()),
        bundled: Some(true),
        available_tools: vec![],
    };
    let error = agent
        .add_extension(code_execution_config.clone(), &session.id)
        .await
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("GOSLING_CODE_EXECUTION_RUNTIME=disabled"));
    let (tools, toolshim_tools, system_prompt, _model_config) = agent
        .prepare_tools_and_prompt(&session.id, &session.working_dir)
        .await?;

    assert!(tools.is_empty());
    assert!(toolshim_tools.is_empty());
    assert!(system_prompt.contains("# Extensions"));
    assert!(system_prompt.contains("No extensions are defined"));
    assert!(!system_prompt.contains("execute_typescript"));
    assert_eq!(
        agent.extension_configs_for_persistence().await,
        vec![code_execution_config]
    );
    Ok(())
}

#[cfg(feature = "code-mode")]
#[tokio::test]
async fn default_code_execution_runtime_is_disabled_and_omits_code_mode() -> Result<()> {
    // PROVING TEST for CER-GSL-002: with GOSLING_CODE_EXECUTION_RUNTIME unset,
    // the default AgentConfig (no explicit .with_code_execution_runtime call)
    // must be fail-closed — no code_execution extension registered, no
    // execute_typescript tool offered, no code-mode prompt disclosure.
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("data");
    let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
    let permission_manager = Arc::new(PermissionManager::new(data_dir));
    // Note: no .with_code_execution_runtime(...) — this exercises the default.
    let config = AgentConfig::new(
        session_manager.clone(),
        permission_manager,
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    );
    assert_eq!(
        config.code_execution_runtime,
        CodeExecutionRuntime::Disabled,
        "unset code execution runtime must default to Disabled (opt-in)"
    );
    let agent = Agent::with_config(config);
    let session = session_manager
        .create_session(
            PathBuf::default(),
            "code-runtime-default".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    agent
        .update_provider(
            Arc::new(CountingTextProvider::new()),
            gosling_providers::model::ModelConfig::new("mock-model"),
            &session.id,
        )
        .await?;

    let code_execution_config = ExtensionConfig::Platform {
        name: "code_execution".to_string(),
        description: "Code Mode".to_string(),
        display_name: Some("Code Mode".to_string()),
        bundled: Some(true),
        available_tools: vec![],
    };
    let error = agent
        .add_extension(code_execution_config, &session.id)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("GOSLING_CODE_EXECUTION_RUNTIME=disabled"));

    let (tools, toolshim_tools, system_prompt, _model_config) = agent
        .prepare_tools_and_prompt(&session.id, &session.working_dir)
        .await?;

    assert!(tools.is_empty());
    assert!(toolshim_tools.is_empty());
    assert!(!system_prompt.contains("execute_typescript"));
    Ok(())
}

#[cfg(feature = "code-mode")]
#[tokio::test]
async fn disabled_code_execution_runtime_does_not_resurrect_persisted_extension_on_resume(
) -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("data");
    let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
    let permission_manager = Arc::new(PermissionManager::new(data_dir));

    let enabled_config = AgentConfig::new(
        session_manager.clone(),
        permission_manager.clone(),
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    )
    .with_code_execution_runtime(CodeExecutionRuntime::Enabled);
    let agent = Agent::with_config(enabled_config);
    let session = session_manager
        .create_session(
            PathBuf::default(),
            "code-runtime-resume".to_string(),
            SessionType::Hidden,
            GoslingMode::Auto,
        )
        .await?;
    agent
        .update_provider(
            Arc::new(CountingTextProvider::new()),
            gosling_providers::model::ModelConfig::new("mock-model"),
            &session.id,
        )
        .await?;

    let code_execution_config = ExtensionConfig::Platform {
        name: "code_execution".to_string(),
        description: "Code Mode".to_string(),
        display_name: Some("Code Mode".to_string()),
        bundled: Some(true),
        available_tools: vec![],
    };
    let developer_config = ExtensionConfig::Platform {
        name: "developer".to_string(),
        description: "Developer tools".to_string(),
        display_name: Some("Developer".to_string()),
        bundled: Some(true),
        available_tools: vec![],
    };
    agent
        .add_extension(developer_config.clone(), &session.id)
        .await?;
    agent
        .add_extension(code_execution_config.clone(), &session.id)
        .await?;
    let persisted_before_resume = session_manager.get_session(&session.id, false).await?;
    let configured_before_resume =
        EnabledExtensionsState::from_extension_data(&persisted_before_resume.extension_data)
            .expect("enabled extension state should be present")
            .extensions;
    assert_eq!(configured_before_resume.len(), 2);

    let disabled_config = AgentConfig::new(
        session_manager.clone(),
        permission_manager,
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    )
    .with_code_execution_runtime(CodeExecutionRuntime::Disabled);
    let resumed_agent = Arc::new(Agent::with_config(disabled_config));
    resumed_agent
        .update_provider(
            Arc::new(CountingTextProvider::new()),
            gosling_providers::model::ModelConfig::new("mock-model"),
            &session.id,
        )
        .await?;

    let persisted_session = session_manager.get_session(&session.id, false).await?;
    let load_results = resumed_agent
        .load_extensions_from_session(&persisted_session)
        .await;
    assert!(
        load_results.iter().any(|result| result.success)
            && load_results.iter().any(|result| !result.success),
        "developer should load while code_execution remains unavailable: {load_results:?}"
    );

    let persisted_after_resume = session_manager.get_session(&session.id, false).await?;
    let configured_after_resume =
        EnabledExtensionsState::from_extension_data(&persisted_after_resume.extension_data)
            .expect("enabled extension state should remain present")
            .extensions;
    assert_eq!(configured_after_resume.len(), 2);
    assert!(configured_after_resume.contains(&developer_config));
    assert!(configured_after_resume.contains(&code_execution_config));

    let (tools, toolshim_tools, system_prompt, _model_config) = resumed_agent
        .prepare_tools_and_prompt(&session.id, &session.working_dir)
        .await?;

    assert!(!tools
        .iter()
        .chain(toolshim_tools.iter())
        .any(|tool| tool.name == "execute_typescript"));
    assert!(!system_prompt.contains("execute_typescript"));
    Ok(())
}

async fn create_stop_hook_test_agent(
    env: &StopHookTestEnv,
    stop_hook_block_cap: u32,
) -> Result<(Agent, String, Arc<CountingTextProvider>)> {
    let provider = Arc::new(CountingTextProvider::new());
    let (mut agent, session_id) =
        create_test_agent(env.data_dir(), env.hook_manager(), provider.clone()).await?;
    agent.set_stop_hook_block_cap_for_test(stop_hook_block_cap);
    Ok((agent, session_id, provider))
}

async fn run_stop_hook_test_turn(
    agent: &Agent,
    session_id: &str,
    text: &str,
) -> Result<Vec<Message>> {
    let session_config = SessionConfig {
        id: session_id.to_string(),
        max_turns: Some(10),
        compacted_context: false,
        tail_limit: None,
    };
    let reply_stream = agent
        .reply(Message::user().with_text(text), session_config, None)
        .await?;
    tokio::pin!(reply_stream);

    let mut messages = Vec::new();
    while let Some(event) = reply_stream.next().await {
        match event? {
            AgentEvent::Message(message) => messages.push(message),
            AgentEvent::McpNotification(_)
            | AgentEvent::HistoryReplaced(_)
            | AgentEvent::Usage(_) => {}
        }
    }
    Ok(messages)
}

fn visible_texts(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .map(Message::as_concat_text)
        .filter(|text| !text.is_empty())
        .collect()
}

#[tokio::test]
async fn session_start_hook_emits_once_for_first_reply_turn() -> Result<()> {
    let env = SessionStartHookTestEnv::new()?;
    let provider = Arc::new(CountingTextProvider::new());
    let (agent, session_id) =
        create_test_agent(env.data_dir(), env.hook_manager(), provider.clone()).await?;

    run_stop_hook_test_turn(&agent, &session_id, "first").await?;
    run_stop_hook_test_turn(&agent, &session_id, "second").await?;

    assert_eq!(env.hook_invocations(), 1);
    assert_eq!(provider.call_count(), 2);
    Ok(())
}

#[tokio::test]
async fn stop_hook_block_cap_allows_configured_consecutive_blocks_then_overrides() -> Result<()> {
    let env = StopHookTestEnv::new(ALWAYS_BLOCK_SCRIPT)?;
    let (agent, session_id, provider) = create_stop_hook_test_agent(&env, 2).await?;

    let messages = run_stop_hook_test_turn(&agent, &session_id, "hello").await?;
    let texts = visible_texts(&messages);

    assert_eq!(
        provider.call_count(),
        3,
        "cap=2 should allow two blocked retries, then override on the third block"
    );
    assert_eq!(
        env.hook_invocations(),
        3,
        "Stop hook should run for the initial response plus the two honored retries"
    );
    assert!(texts.iter().any(|text| text == "provider response 0"));
    assert!(texts.iter().any(|text| text == "provider response 1"));
    assert!(texts.iter().any(|text| text == "provider response 2"));
    assert!(messages.iter().any(|message| {
        message.content.iter().any(|content| {
            matches!(
                content,
                MessageContent::SystemNotification(notification)
                    if notification.msg.contains("more than 2 consecutive times")
                        && notification.msg.contains("GOSLING_STOP_HOOK_BLOCK_CAP")
            )
        })
    }));

    Ok(())
}

#[tokio::test]
async fn stop_hook_block_cap_counts_only_consecutive_blocks() -> Result<()> {
    let env = StopHookTestEnv::new(ALTERNATE_BLOCK_ALLOW_SCRIPT)?;
    let (agent, session_id, provider) = create_stop_hook_test_agent(&env, 1).await?;

    let first_turn = run_stop_hook_test_turn(&agent, &session_id, "first").await?;
    let second_turn = run_stop_hook_test_turn(&agent, &session_id, "second").await?;
    let mut texts = visible_texts(&first_turn);
    texts.extend(visible_texts(&second_turn));

    assert_eq!(
        provider.call_count(),
        4,
        "each turn should honor one block, retry, then stop when the next Stop hook allows"
    );
    assert_eq!(env.hook_invocations(), 4);
    assert!(texts.iter().any(|text| text == "provider response 0"));
    assert!(texts.iter().any(|text| text == "provider response 1"));
    assert!(texts.iter().any(|text| text == "provider response 2"));
    assert!(texts.iter().any(|text| text == "provider response 3"));
    assert!(
        !texts
            .iter()
            .any(|text| text.contains("overriding and ending turn")),
        "non-consecutive Stop hook blocks should not trip the cap warning"
    );

    Ok(())
}

#[tokio::test]
async fn stop_hook_payload_includes_streamed_assistant_reply_text() -> Result<()> {
    let env = StopHookTestEnv::new(RECORD_PAYLOAD_SCRIPT)?;
    let provider = Arc::new(ChunkedTextProvider);
    let (agent, session_id) =
        create_test_agent(env.data_dir(), env.hook_manager(), provider).await?;

    let messages = run_stop_hook_test_turn(&agent, &session_id, "hello").await?;
    let texts = visible_texts(&messages);
    assert_eq!(texts.join(""), "streamed assistant reply");

    let payload = env.stop_payload()?;
    assert_eq!(payload.get("event").and_then(Value::as_str), Some("Stop"));
    assert_eq!(
        payload.get("session_id").and_then(Value::as_str),
        Some(session_id.as_str())
    );
    assert_eq!(
        payload
            .get("last_assistant_message")
            .and_then(Value::as_str),
        Some("streamed assistant reply")
    );
    assert!(payload.get("message").is_none());

    Ok(())
}

#[tokio::test]
async fn reply_persists_user_input_and_streamed_assistant_checkpoints() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let hook_manager = crate::hooks::HookManager::from_plugins_for_test(vec![]);
    let (agent, session_id) = create_test_agent(
        temp_dir.path().join("data"),
        hook_manager,
        Arc::new(ChunkedTextProvider),
    )
    .await?;
    let session_config = SessionConfig {
        id: session_id.clone(),
        max_turns: Some(10),
        compacted_context: false,
        tail_limit: None,
    };

    let reply_stream = agent
        .reply(Message::user().with_text("hello"), session_config, None)
        .await?;

    let submitted = agent
        .config
        .session_manager
        .get_session(&session_id, true)
        .await?;
    let submitted_messages = submitted.conversation.unwrap();
    assert_eq!(submitted_messages.messages().len(), 1);
    assert_eq!(submitted_messages.messages()[0].as_concat_text(), "hello");

    tokio::pin!(reply_stream);
    let first_event = reply_stream.next().await.transpose()?;
    let Some(AgentEvent::Message(first_chunk)) = first_event else {
        panic!("expected the first streamed assistant chunk");
    };
    assert_eq!(first_chunk.as_concat_text(), "streamed ");

    let checkpoint = agent
        .config
        .session_manager
        .get_session(&session_id, true)
        .await?;
    let checkpoint_messages = checkpoint.conversation.unwrap();
    assert_eq!(checkpoint_messages.messages().len(), 2);
    assert_eq!(
        checkpoint_messages.messages()[1].as_concat_text(),
        "streamed "
    );

    while let Some(event) = reply_stream.next().await {
        event?;
    }

    let completed = agent
        .config
        .session_manager
        .get_session(&session_id, true)
        .await?;
    let completed_messages = completed.conversation.unwrap();
    assert_eq!(completed_messages.messages().len(), 2);
    assert_eq!(
        completed_messages.messages()[1].as_concat_text(),
        "streamed assistant reply"
    );
    assert!(completed_messages.messages()[1].id.is_some());

    Ok(())
}

#[tokio::test]
async fn already_cancelled_reply_does_not_persist_a_user_message() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let hook_manager = crate::hooks::HookManager::from_plugins_for_test(vec![]);
    let (agent, session_id) = create_test_agent(
        temp_dir.path().join("data"),
        hook_manager,
        Arc::new(ChunkedTextProvider),
    )
    .await?;
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let reply_stream = agent
        .reply(
            Message::user().with_text("do not store this"),
            SessionConfig {
                id: session_id.clone(),
                max_turns: Some(10),
                compacted_context: false,
                tail_limit: None,
            },
            Some(cancellation),
        )
        .await?;
    assert_eq!(reply_stream.count().await, 0);

    let session = agent
        .config
        .session_manager
        .get_session(&session_id, true)
        .await?;
    assert!(session.conversation.unwrap().messages().is_empty());
    Ok(())
}

#[tokio::test]
async fn frontend_tool_execution_uses_the_durable_operation_ledger() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().join("data")));
    let permission_manager = Arc::new(PermissionManager::new(temp_dir.path().join("config")));
    let agent = Agent::with_config(AgentConfig::new(
        session_manager.clone(),
        permission_manager,
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    ));
    let session = session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "Frontend ledger".to_string(),
            SessionType::User,
            GoslingMode::Auto,
        )
        .await?;
    let tool_call = CallToolRequestParams::new("frontend__save_artifact")
        .with_arguments(rmcp::object!({ "name": "report.md" }));
    let request = ToolRequest {
        id: "frontend-request-1".to_string(),
        tool_call: Ok(tool_call.clone()),
        metadata: None,
        tool_meta: None,
    };
    session_manager
        .add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request(request.id.clone(), Ok(tool_call)),
        )
        .await?;
    agent.frontend_tools.lock().await.insert(
        "frontend__save_artifact".to_string(),
        FrontendTool {
            name: "frontend__save_artifact".to_string(),
            tool: Tool::new(
                "frontend__save_artifact".to_string(),
                "Save an artifact".to_string(),
                rmcp::object!({ "type": "object" }),
            ),
        },
    );
    let terminal_result = Ok(CallToolResult::success(vec![Content::text("saved")]));
    agent
        .handle_tool_result(request.id.clone(), terminal_result.clone())
        .await;

    let mut response = Message::user().with_generated_id();
    let events = agent
        .handle_frontend_tool_request(&request, &mut response, &session)
        .try_collect::<Vec<_>>()
        .await?;
    assert_eq!(events.len(), 1);

    let reloaded = session_manager.get_session(&session.id, true).await?;
    let conversation = reloaded.conversation.unwrap();
    let responses = conversation
        .messages()
        .iter()
        .flat_map(|message| message.content.iter())
        .filter_map(MessageContent::as_tool_response)
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].tool_result, terminal_result);
    assert_eq!(
        session_manager.recover_tool_operations(&session.id).await?,
        0
    );

    Ok(())
}

#[tokio::test]
async fn test_tool_inspection_manager_has_all_inspectors() -> Result<()> {
    let agent = Agent::new();

    // Verify that the tool inspection manager has all expected inspectors
    let inspector_names = agent.tool_inspection_manager.inspector_names();

    assert!(
        inspector_names.contains(&"repetition"),
        "Tool inspection manager should contain repetition inspector"
    );
    assert!(
        inspector_names.contains(&"permission"),
        "Tool inspection manager should contain permission inspector"
    );
    assert!(
        inspector_names.contains(&"security"),
        "Tool inspection manager should contain security inspector"
    );
    assert!(
        inspector_names.contains(&"adversary"),
        "Tool inspection manager should contain adversary inspector"
    );

    Ok(())
}

struct DenyAutoToolInspector;

#[async_trait::async_trait]
impl crate::tool_inspection::ToolInspector for DenyAutoToolInspector {
    fn name(&self) -> &'static str {
        "deny_auto_tool"
    }

    async fn inspect(
        &self,
        _session_id: &str,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        _gosling_mode: GoslingMode,
    ) -> Result<Vec<crate::tool_inspection::InspectionResult>> {
        Ok(tool_requests
            .iter()
            .map(|request| crate::tool_inspection::InspectionResult {
                tool_request_id: request.id.clone(),
                action: crate::tool_inspection::InspectionAction::Deny,
                reason: "test denial".to_string(),
                confidence: 1.0,
                inspector_name: self.name().to_string(),
                finding_id: None,
                metadata: None,
            })
            .collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[tokio::test]
async fn dispatch_app_tool_call_runs_inspectors_in_auto_mode() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    let permission_manager = Arc::new(PermissionManager::new(temp_dir.path().to_path_buf()));
    let provider = Arc::new(Mutex::new(None));
    let mut agent = Agent::with_config(AgentConfig::new(
        session_manager.clone(),
        permission_manager.clone(),
        GoslingMode::Auto,
        true,
        GoslingPlatform::GoslingCli,
    ));
    let mut inspection_manager = ToolInspectionManager::new();
    inspection_manager.add_inspector(Box::new(PermissionInspector::new(
        permission_manager,
        provider,
        session_manager,
    )));
    inspection_manager.add_inspector(Box::new(DenyAutoToolInspector));
    agent.tool_inspection_manager = inspection_manager;

    let result = agent
        .dispatch_app_tool_call(
            "session",
            CallToolRequestParams::new("test_tool"),
            CancellationToken::new(),
        )
        .await;
    let Err(error) = result else {
        panic!("Auto mode bypassed the denying inspector");
    };
    assert_eq!(error.code, ErrorCode::INVALID_REQUEST);

    Ok(())
}

#[tokio::test]
async fn discard_pending_steers_clears_queued_messages() {
    let agent = Agent::new();
    let session_id = "session-discard";

    agent
        .steer(session_id, Message::user().with_text("queued steer"))
        .await;
    assert!(agent.has_pending_steers(session_id).await);

    agent.discard_pending_steers(session_id).await;

    assert!(
            !agent.has_pending_steers(session_id).await,
            "discarding must drop steers orphaned by a cancelled run so they cannot leak into a later prompt"
        );
    assert!(agent.drain_pending_steers(session_id).await.is_empty());
}

#[test]
fn categorize_tool_recognizes_conventional_names() {
    assert_eq!(categorize_tool("developer__shell"), ToolCategory::Shell);
    assert_eq!(categorize_tool("filesystem__write"), ToolCategory::Write);
    assert_eq!(categorize_tool("filesystem__edit"), ToolCategory::Write);
    assert_eq!(categorize_tool("filesystem__read"), ToolCategory::Read);
    assert_eq!(categorize_tool("filesystem__view"), ToolCategory::Read);
    assert_eq!(categorize_tool("filesystem__cat"), ToolCategory::Read);
    assert_eq!(categorize_tool("scheduler__list"), ToolCategory::Other);
    assert_eq!(categorize_tool("shell"), ToolCategory::Shell);
}

#[test]
fn extract_string_arg_picks_first_present_key() {
    let input = serde_json::json!({ "file_path": "/tmp/a.txt", "path": "/tmp/b.txt" });
    assert_eq!(
        extract_string_arg(&input, &["path", "file", "file_path"]).as_deref(),
        Some("/tmp/b.txt")
    );
    let input = serde_json::json!({ "file_path": "/tmp/a.txt" });
    assert_eq!(
        extract_string_arg(&input, &["path", "file", "file_path"]).as_deref(),
        Some("/tmp/a.txt")
    );
    let input = serde_json::json!({ "other": 1 });
    assert!(extract_string_arg(&input, &["path"]).is_none());
    let input = serde_json::json!({ "path": "" });
    assert!(extract_string_arg(&input, &["path"]).is_none());
}

#[test]
fn auto_permission_filter_removes_tool_confirmation_and_keeps_other_content() {
    let mut message = Message::assistant()
        .with_text("working")
        .with_action_required(
            "permission-1",
            "Write".to_string(),
            serde_json::Map::new(),
            None,
            None,
        );

    let request_ids = take_tool_confirmation_requests(&mut message);

    assert_eq!(request_ids, vec!["permission-1"]);
    assert_eq!(message.as_concat_text(), "working");
    assert!(message
        .content
        .iter()
        .all(|content| !matches!(content, MessageContent::ActionRequired(_))));
}

#[tokio::test]
async fn policy_denial_preserves_inspector_reason() {
    let agent = Agent::new();
    let (mut permissions, mut responses) = needs_approval_fixture();
    permissions.denied = std::mem::take(&mut permissions.needs_approval);
    let inspections = vec![crate::tool_inspection::InspectionResult {
        tool_request_id: "req-1".into(),
        action: crate::tool_inspection::InspectionAction::Deny,
        reason: "Workspace policy forbids mutation of read-only folders".into(),
        confidence: 1.0,
        inspector_name: "working_dir_scope".into(),
        finding_id: None,
        metadata: None,
    }];
    agent
        .handle_approved_and_denied_tools(
            &permissions,
            &inspections,
            &mut responses,
            None,
            &Session::default(),
        )
        .await
        .unwrap();
    let text = serde_json::to_string(&responses).unwrap();
    assert!(text.contains("forbids mutation"));
    assert!(!text.contains("user has declined"));
}
