//! Agent compatibility facade over hooks, extensions, tools, replies, and providers.
//!
//! Maintainers: keep public paths and shared state here while delegating cohesive behavior.
//! Clients: agent construction, events, streams, and public methods remain stable.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures::stream::BoxStream;
use futures::{stream, FutureExt, Stream, StreamExt, TryStreamExt};
use tracing_futures::Instrument;
use uuid::Uuid;

use super::container::Container;
use super::frontend_tool_result_router::{
    FrontendToolResultRegistration, FrontendToolResultRouter,
};
use super::mcp_client::GoslingMcpHostInfo;
use super::tool_confirmation_router::ToolConfirmationRouter;
use super::tool_execution::{
    ToolCallResult, CHAT_MODE_TOOL_SKIPPED_RESPONSE, SUBAGENT_APPROVAL_UNAVAILABLE_RESPONSE,
};
use crate::action_required_manager::ElicitationOutcome;
use crate::agents::extension::{ExtensionConfig, ExtensionResult, ToolInfo};
use crate::agents::extension_manager::{
    get_parameter_names, ExtensionManager, ExtensionManagerCapabilities,
};
use crate::agents::platform_extensions::MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE;
use crate::agents::prompt_manager::PromptManager;
use crate::agents::types::{FrontendTool, SessionConfig, SharedProvider};
use crate::config::extensions::name_to_key;
use crate::config::permission::PermissionManager;
use crate::config::{CodeExecutionRuntime, Config, GoslingMode};
use crate::context_mgmt::{
    check_if_compaction_needed, compact_messages, context_manager_mode, resolve_provider_input,
    summarizer, ContextBuildRequest, ContextManager, ContextManagerMode, FileMemorySource,
    MemoryQuery, MemorySource, SummarizerMode, DEFAULT_COMPACTION_THRESHOLD,
};
use crate::conversation::message::{
    ActionRequiredData, InferenceMetadata, Message, MessageContent, ProviderMetadata,
    SystemNotificationType, ToolRequest,
};
use crate::conversation::{debug_conversation_fix, fix_conversation, Conversation};
use crate::hints::SubdirectoryHintTracker;
use crate::mcp_utils::ToolResult;
use crate::permission::permission_confirmation::PrincipalType;
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::permission_judge::PermissionCheckResult;
use crate::permission::working_dir_scope_inspector::WorkingDirScopeInspector;
use crate::permission::{Permission, PermissionConfirmation};
use crate::providers::base::{PermissionRouting, Provider};
use crate::security::adversary_inspector::AdversaryInspector;
use crate::security::egress_inspector::EgressInspector;
use crate::security::security_inspector::SecurityInspector;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::{
    Session, SessionManager, SessionNameUpdate, SessionType, ToolOperationStart,
    DEFAULT_SESSION_TAIL_LIMIT,
};
use crate::tool_inspection::ToolInspectionManager;
use crate::tool_monitor::RepetitionInspector;
use crate::utils::is_token_cancelled;
use crate::workspace::WorkspaceService;
use gosling_providers::errors::ProviderError;
use gosling_providers::model::ModelConfig;
use gosling_providers::retry::{should_retry, RetryConfig};
use gosling_providers::thinking::ThinkingEffort;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ElicitationAction, ErrorCode, ErrorData,
    GetPromptResult, Prompt, ServerNotification, Tool,
};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, instrument, warn};

const DEFAULT_MAX_TURNS: u32 = 1000;
const DEFAULT_STOP_HOOK_BLOCK_CAP: u32 = 8;
// Bounds the "grind" nudge independently of `max_turns`: without its own cap, a
// grind goal that never completes re-injects "keep working" on every no-tool
// turn, run after run, relying solely on the shared 1000-turn ceiling to end it.
const DEFAULT_MAX_GRIND_NUDGES: u32 = 50;
const COMPACTION_THINKING_TEXT: &str = "gosling is compacting the conversation...";
const MAX_TURNS_MESSAGE: &str = "I've reached the maximum number of actions I can do without user input. Would you like me to continue?";
const MAX_GRIND_NUDGES_MESSAGE: &str = "I've kept working on the grind goal without completing it after many attempts. Stopping to avoid an unbounded loop — let me know if you'd like me to continue.";
const DEFAULT_FRONTEND_INSTRUCTIONS: &str = "The following tools are provided directly by the frontend and will be executed by the frontend when called.";
const STREAM_CHECKPOINT_INTERVAL: Duration = Duration::from_millis(250);
// A provider stream that dies partway through is only retryable while no tool
// from it has run: re-issuing the request replays the whole assistant message,
// which is fine for text nobody has acted on and wrong once a tool has taken
// effect in the world. `ProviderRetry` only covers establishing the stream, so
// without this a connection dropped mid-response ended the turn.
const MAX_MID_STREAM_RETRIES: usize = 3;

mod extensions;
mod frontend_extensions;
mod hooks;
mod prompt_apis;
mod provider_transitions;
mod reply_context;
mod reply_entry;
mod reply_stream;
mod tool_dispatch;

pub(super) struct ToolOperationGuard {
    session_manager: Arc<SessionManager>,
    operation_id: Option<String>,
}

impl ToolOperationGuard {
    pub(super) fn new(session_manager: Arc<SessionManager>, operation_id: String) -> Self {
        Self {
            session_manager,
            operation_id: Some(operation_id),
        }
    }

    pub(super) fn disarm(&mut self) {
        self.operation_id = None;
    }
}

impl Drop for ToolOperationGuard {
    fn drop(&mut self) {
        let Some(operation_id) = self.operation_id.take() else {
            return;
        };
        self.session_manager.release_tool_operation(&operation_id);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let session_manager = self.session_manager.clone();
            runtime.spawn(async move {
                if let Err(error) = session_manager
                    .mark_tool_operation_in_doubt(&operation_id)
                    .await
                {
                    warn!(
                        "Failed to mark abandoned tool operation {} in doubt: {}",
                        operation_id, error
                    );
                }
            });
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCategory {
    Shell,
    Read,
    Write,
    Other,
}

fn categorize_tool(tool_name: &str) -> ToolCategory {
    let local = tool_name.rsplit("__").next().unwrap_or(tool_name);
    match local {
        "shell" | "bash" | "exec" | "run" => ToolCategory::Shell,
        "read" | "view" | "cat" | "read_file" => ToolCategory::Read,
        "write" | "edit" | "patch" | "write_file" | "edit_file" => ToolCategory::Write,
        _ => ToolCategory::Other,
    }
}

fn take_tool_confirmation_requests(message: &mut Message) -> Vec<String> {
    let mut request_ids = Vec::new();
    message.content.retain(|content| {
        let MessageContent::ActionRequired(action_required) = content else {
            return true;
        };
        let ActionRequiredData::ToolConfirmation { id, .. } = &action_required.data else {
            return true;
        };

        request_ids.push(id.clone());
        false
    });
    request_ids
}

fn extract_string_arg(input: &Value, keys: &[&str]) -> Option<String> {
    let obj = input.as_object()?;
    for k in keys {
        if let Some(s) = obj.get(*k).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn stop_hook_denial_context_message(plugin: &str, reason: &str) -> Message {
    let nudge = format!(
        "Stop hook `{plugin}` blocked ending this turn:

{reason}

Address this policy hook denial before trying to stop again."
    );
    Message::user()
        .with_text(nudge)
        .with_visibility(false, true)
}

fn stop_hook_denial_notification(plugin: &str) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!("Stop hook `{plugin}` blocked ending this turn."),
    )
}

fn stop_hook_block_cap_warning(plugin: &str, cap: u32) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!(
            "Stop hook `{plugin}` blocked the turn from ending more than {cap} consecutive times — overriding and ending turn to avoid an infinite loop. Set GOSLING_STOP_HOOK_BLOCK_CAP to raise this limit."
        ),
    )
}

/// Builds the message for a provider failure the mid-stream retry could not
/// absorb.
///
/// Whether the turn is actually over decides both halves. Once a tool has run
/// there is a result to carry forward, so the agent goes on by itself: telling
/// the user to resend describes something that isn't happening, and marking the
/// message `terminal_error` fails a non-interactive run that then went on to
/// finish its work.
fn provider_failure_message(
    provider_err: &ProviderError,
    ending_text: &str,
    turn_ends: bool,
) -> Message {
    if turn_ends {
        Message::assistant()
            .with_text(ending_text)
            .with_terminal_error(provider_err.to_string())
    } else {
        Message::assistant().with_text(format!(
            "{provider_err}\n\nContinuing with the tool results already collected."
        ))
    }
}

/// Context needed for the reply function
pub struct ReplyContext {
    pub conversation: Conversation,
    pub tools: Vec<Tool>,
    pub toolshim_tools: Vec<Tool>,
    pub system_prompt: String,
    pub gosling_mode: GoslingMode,
    pub tool_call_cut_off: usize,
    pub model_config: gosling_providers::model::ModelConfig,
}

pub struct ToolCategorizeResult {
    pub frontend_requests: Vec<ToolRequest>,
    pub remaining_requests: Vec<ToolRequest>,
    pub filtered_response: Message,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ExtensionLoadResult {
    pub name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum GoslingPlatform {
    GoslingDesktop,
    GoslingCli,
}

impl fmt::Display for GoslingPlatform {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GoslingPlatform::GoslingCli => write!(f, "gosling-cli"),
            GoslingPlatform::GoslingDesktop => write!(f, "gosling-desktop"),
        }
    }
}

#[derive(Clone)]
pub struct AgentConfig {
    pub session_manager: Arc<SessionManager>,
    pub permission_manager: Arc<PermissionManager>,
    pub gosling_mode: GoslingMode,
    pub code_execution_runtime: CodeExecutionRuntime,
    pub disable_session_naming: bool,
    pub gosling_platform: GoslingPlatform,
    pub mcp_host_info: Option<GoslingMcpHostInfo>,
    pub session_name_update_tx: Option<mpsc::UnboundedSender<SessionNameUpdate>>,
    pub use_login_shell_path: Option<bool>,
    pub workspace_service: Option<Arc<WorkspaceService>>,
    pub provider_failover: Option<ProviderFailoverConfig>,
}

#[derive(Clone)]
pub struct ProviderFailoverConfig {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
}

impl ProviderFailoverConfig {
    pub fn new(provider: Arc<dyn Provider>, model_config: ModelConfig) -> Self {
        Self {
            provider,
            model_config,
        }
    }
}

pub(super) enum ProviderFailoverTarget {
    Ready(ProviderFailoverConfig),
    Configured {
        provider_name: String,
        model_name: String,
    },
    Invalid(String),
}

impl AgentConfig {
    pub fn new(
        session_manager: Arc<SessionManager>,
        permission_manager: Arc<PermissionManager>,
        gosling_mode: GoslingMode,
        disable_session_naming: bool,
        gosling_platform: GoslingPlatform,
    ) -> Self {
        Self {
            session_manager,
            permission_manager,
            gosling_mode,
            code_execution_runtime: CodeExecutionRuntime::Disabled,
            disable_session_naming,
            gosling_platform,
            mcp_host_info: None,
            session_name_update_tx: None,
            use_login_shell_path: None,
            workspace_service: None,
            provider_failover: None,
        }
    }

    pub fn with_mcp_host_info(mut self, mcp_host_info: Option<GoslingMcpHostInfo>) -> Self {
        self.mcp_host_info = mcp_host_info;
        self
    }

    pub fn with_code_execution_runtime(mut self, runtime: CodeExecutionRuntime) -> Self {
        self.code_execution_runtime = runtime;
        self
    }

    pub fn with_session_name_update_tx(
        mut self,
        tx: Option<mpsc::UnboundedSender<SessionNameUpdate>>,
    ) -> Self {
        self.session_name_update_tx = tx;
        self
    }

    pub fn with_use_login_shell_path(mut self, use_login_shell_path: bool) -> Self {
        self.use_login_shell_path = Some(use_login_shell_path);
        self
    }

    pub fn with_workspace_service(mut self, service: Arc<WorkspaceService>) -> Self {
        self.workspace_service = Some(service);
        self
    }

    pub fn with_provider_failover(mut self, failover: ProviderFailoverConfig) -> Self {
        self.provider_failover = Some(failover);
        self
    }

    fn resolve_use_login_shell_path(&self) -> bool {
        resolve_use_login_shell_path(self.use_login_shell_path, &self.gosling_platform)
    }
}

fn resolve_use_login_shell_path(explicit: Option<bool>, platform: &GoslingPlatform) -> bool {
    explicit.unwrap_or(matches!(platform, GoslingPlatform::GoslingDesktop))
}

/// The main gosling Agent
pub struct Agent {
    pub(super) provider: SharedProvider,
    pub config: AgentConfig,
    pub(super) current_gosling_mode: Mutex<GoslingMode>,
    pub(super) gosling_mode_changes: tokio::sync::watch::Sender<GoslingMode>,
    state_transition: Mutex<()>,

    pub extension_manager: Arc<ExtensionManager>,
    pub(super) frontend_extensions: Mutex<HashMap<String, ExtensionConfig>>,
    pub(super) frontend_tools: Mutex<HashMap<String, FrontendTool>>,
    pub(super) frontend_instructions: Mutex<Option<String>>,
    pub(super) prompt_manager: Mutex<PromptManager>,
    pub(super) subdirectory_hint_tracker: Mutex<SubdirectoryHintTracker>,
    pub tool_confirmation_router: ToolConfirmationRouter,
    pub(super) frontend_tool_result_router: FrontendToolResultRouter,

    pub(super) tool_inspection_manager: ToolInspectionManager,
    pub(super) hook_manager: crate::hooks::HookManager,
    #[cfg(test)]
    stop_hook_block_cap_override: Option<u32>,
    container: Mutex<Option<Container>>,
    goal: Mutex<Option<String>>,
    grind: Mutex<Option<String>>,
    pending_steers: Mutex<HashMap<String, VecDeque<Message>>>,
}

#[derive(Clone, Debug)]
pub enum AgentEvent {
    Message(Message),
    Usage(crate::providers::base::ProviderUsage),
    McpNotification((String, ServerNotification)),
    HistoryReplaced(Conversation),
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

pub enum ToolStreamItem<T> {
    ActionRequired(Message),
    Message(ServerNotification),
    Result(T),
}

pub type ToolStream =
    Pin<Box<dyn Stream<Item = ToolStreamItem<ToolResult<CallToolResult>>> + Send>>;

// tool_stream combines a stream of ServerNotifications with a future representing the
// final result of the tool call. MCP notifications are not request-scoped, but
// this lets us capture all notifications emitted during the tool call for
// simpler consumption
pub fn tool_stream<S, A, F>(rx: S, action_required_rx: A, done: F) -> ToolStream
where
    S: Stream<Item = ServerNotification> + Send + Unpin + 'static,
    A: Stream<Item = Message> + Send + Unpin + 'static,
    F: Future<Output = ToolResult<CallToolResult>> + Send + 'static,
{
    Box::pin(async_stream::stream! {
        tokio::pin!(done);
        let mut rx = rx;
        let mut action_required_rx = action_required_rx;

        loop {
            tokio::select! {
                Some(msg) = action_required_rx.next() => {
                    yield ToolStreamItem::ActionRequired(msg);
                }
                Some(msg) = rx.next() => {
                    yield ToolStreamItem::Message(msg);
                }
                r = &mut done => {
                    yield ToolStreamItem::Result(r);
                    break;
                }
            }
        }
    })
}

impl Agent {
    pub fn new() -> Self {
        let config = Config::global();
        let agent_config = AgentConfig::new(
            Arc::new(SessionManager::instance()),
            PermissionManager::instance(),
            config.get_gosling_mode().unwrap_or_default(),
            config.get_gosling_disable_session_naming().unwrap_or(false),
            GoslingPlatform::GoslingCli,
        )
        .with_code_execution_runtime(config.resolve_gosling_code_execution_runtime());
        Self::with_config(agent_config)
    }

    pub fn with_config(config: AgentConfig) -> Self {
        let provider = Arc::new(Mutex::new(None));

        let gosling_platform = config.gosling_platform.clone();
        let initial_mode = config.gosling_mode;
        let (gosling_mode_changes, _) = tokio::sync::watch::channel(initial_mode);
        let explicit_mcp_host_info = config.mcp_host_info.clone();
        let mcpui = explicit_mcp_host_info
            .as_ref()
            .filter(|host_info| host_info.explicit_extensions)
            .map(GoslingMcpHostInfo::mcpui_enabled)
            .unwrap_or_else(|| match config.gosling_platform {
                GoslingPlatform::GoslingDesktop => true,
                GoslingPlatform::GoslingCli => false,
            });
        let capabilities = ExtensionManagerCapabilities {
            mcpui,
            host_info: explicit_mcp_host_info.clone(),
        };
        let client_name = explicit_mcp_host_info
            .as_ref()
            .and_then(|host_info| host_info.client_name.clone())
            .unwrap_or_else(|| gosling_platform.to_string());
        let session_manager = Arc::clone(&config.session_manager);
        let inspection_session_manager = Arc::clone(&config.session_manager);
        let permission_manager = Arc::clone(&config.permission_manager);
        let use_login_shell_path = config.resolve_use_login_shell_path();
        let code_execution_runtime = config.code_execution_runtime;
        Self {
            provider: provider.clone(),
            config,
            current_gosling_mode: Mutex::new(initial_mode),
            gosling_mode_changes,
            state_transition: Mutex::new(()),
            extension_manager: Arc::new(ExtensionManager::new(
                provider.clone(),
                session_manager,
                client_name,
                capabilities,
                use_login_shell_path,
                code_execution_runtime,
            )),
            frontend_extensions: Mutex::new(HashMap::new()),
            frontend_tools: Mutex::new(HashMap::new()),
            frontend_instructions: Mutex::new(None),
            prompt_manager: Mutex::new(PromptManager::new()),
            subdirectory_hint_tracker: Mutex::new(SubdirectoryHintTracker::new()),
            tool_confirmation_router: ToolConfirmationRouter::new(),
            frontend_tool_result_router: FrontendToolResultRouter::new(),
            tool_inspection_manager: Self::create_tool_inspection_manager(
                permission_manager,
                provider.clone(),
                inspection_session_manager,
            ),
            hook_manager: crate::hooks::HookManager::load(
                std::env::current_dir().ok().as_deref(),
                use_login_shell_path,
            ),
            #[cfg(test)]
            stop_hook_block_cap_override: None,
            container: Mutex::new(None),
            goal: Mutex::new(None),
            grind: Mutex::new(None),
            pending_steers: Mutex::new(HashMap::new()),
        }
    }

    pub async fn shutdown(&self) {
        self.extension_manager.shutdown().await;
    }
}

#[cfg(test)]
mod tests;
