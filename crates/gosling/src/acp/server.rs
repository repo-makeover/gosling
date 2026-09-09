//! ACP server compatibility facade over protocol, session, tool, shell, and transport modules.
//!
//! Maintainers: keep public paths here while delegating cohesive behavior to sibling modules.
//! Clients: request schemas, notification ordering, errors, and response bounds remain stable.

use crate::acp::custom_notifications::*;
use crate::acp::custom_requests::*;
use crate::acp::fs::AcpTools;
pub(super) use crate::acp::response_builder::{
    build_config_options, build_mode_state, build_model_state, build_provider_options,
    build_session_info, build_session_setup_config, compatible_mode,
    send_session_setup_notifications, session_meta, session_provider_selection,
    session_response_meta, should_refresh_inventory_for_session_init,
};
use crate::acp::shell::ShellRuntime;
use crate::acp::tools::AcpAwareToolMeta;
use crate::acp::{PermissionDecision, ACP_CURRENT_MODEL};
use crate::agents::extension::{Envs, PLATFORM_EXTENSIONS};
use crate::agents::extension_manager::TRUSTED_TOOL_UPDATE_META_KEY;
use crate::agents::mcp_client::{GoslingMcpHostInfo, McpClientTrait};
use crate::agents::platform_extensions::developer::DeveloperClient;
use crate::agents::{
    Agent, AgentConfig, ExtensionConfig, ExtensionLoadResult, GoslingPlatform, SessionConfig,
};
use crate::config::base::CONFIG_YAML_NAME;
use crate::config::extensions::{
    get_enabled_extensions_with_config_for_cwd, is_builtin_disabled_by_user,
};
use crate::config::paths::Paths;
use crate::config::paths::RuntimePaths;
use crate::config::permission::PermissionManager;
use crate::config::{Config, GoslingMode};
use crate::conversation::message::{
    ActionRequiredData, Message, MessageContent, SystemNotificationContent, SystemNotificationType,
    ToolRequest,
};
use crate::execution::manager::{AgentManager, AgentManagerGetResult, RuntimeContext};
use crate::mcp_utils::ToolResult;
use crate::permission::permission_confirmation::PrincipalType;
use crate::permission::{Permission, PermissionConfirmation};
use crate::providers::base::Provider;
use crate::providers::inventory::{
    ProviderInventoryEntry, ProviderInventoryService, RefreshJobPlan, RefreshPlan,
    RefreshSkipReason,
};
use crate::session::{
    AcpPromptRunState, EnabledExtensionsState, ExtensionData, ExtensionState,
    NewSessionLibraryContent, Session, SessionArtifact, SessionArtifactProvenance,
    SessionArtifactRelation, SessionLibraryItem, SessionLibraryItemKind, SessionLibraryScope,
    SessionManager, SessionType, DEFAULT_SESSION_TAIL_LIMIT, MAX_SESSION_MESSAGE_PAGE_LIMIT,
};
use crate::source_roots::SourceRoot;
use crate::utils::sanitize_unicode_tags;
use crate::workspace::WorkspaceService;
use agent_client_protocol::schema::v1::{
    AgentCapabilities, Annotations, AuthMethod, AuthMethodAgent, AuthenticateRequest,
    AuthenticateResponse, BlobResourceContents, CancelNotification, CloseSessionRequest,
    CloseSessionResponse, ConfigOptionUpdate, Content, ContentBlock, ContentChunk, Cost,
    CurrentModeUpdate, EmbeddedResource, EmbeddedResourceResource, FileSystemCapabilities,
    ForkSessionRequest, ForkSessionResponse, ImageContent, Implementation, InitializeRequest,
    InitializeResponse, ListSessionsRequest, ListSessionsResponse, LoadSessionRequest,
    LoadSessionResponse, McpCapabilities, McpServer, Meta, NewSessionRequest, NewSessionResponse,
    PermissionOption, PermissionOptionKind, PromptCapabilities, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, ResourceLink, SessionCapabilities,
    SessionCloseCapabilities, SessionConfigOption, SessionId, SessionInfoUpdate,
    SessionListCapabilities, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse, StopReason,
    TextContent, TextResourceContents, ToolCall, ToolCallContent, ToolCallId, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind, Usage, UsageUpdate,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::util::MatchDispatchFrom;
use agent_client_protocol::{
    Agent as SacpAgent, ByteStreams, Client, ConnectionTo, Dispatch, HandleDispatchFrom, Handled,
    Responder,
};
use anyhow::Result;
use fs_err as fs;
use futures::channel::oneshot;
use futures::future::{select, BoxFuture, Either, FutureExt};
use futures::stream::{self, StreamExt};
use futures::AsyncRead;
use rmcp::model::{
    AnnotateAble, CallToolResult, RawContent, RawTextContent, ResourceContents, Role,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{Mutex, OnceCell};
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

mod agent_requests;
pub use agent_requests::agent_request_schemas;
mod active_runs;
mod agent_mentions;
mod config;
mod custom_dispatch;
mod diagnostics;
mod dictation;
mod dispatch;
mod elicitation;
mod extension_selection;
mod extensions;
mod fork_session;
mod initialization;
mod list_sessions;
mod load_session;
mod manage_sessions;
mod message_projection;
mod new_session;
mod onboarding;
mod presentation;
mod prompt_execution;
mod prompts;
mod providers;
mod research_completion;
mod resources;
mod session_activation;
mod session_configuration;
mod shell_handlers;
mod shell_library_formats;
mod slash_commands;

#[cfg(test)]
use prompt_execution::build_prompt_usage;
pub(super) use prompt_execution::build_usage_updates;
use session_configuration::{
    resolve_default_provider_model_config, resolve_provider_default_model_config,
};
mod sources;
mod tool_events;
mod tool_metadata;
mod tool_notifications;
mod tool_summaries;
mod tools;
mod transport;
mod workspace_handlers;

use active_runs::ActivePromptRun;
#[cfg(test)]
use active_runs::{register_active_prompt_run, unregister_active_prompt_run};
pub(crate) use extension_selection::{
    apply_shell_extension_selection, push_or_replace_extension, selected_builtin_extensions,
};
use extension_selection::{
    builtin_to_extension_config, mcp_server_to_extension_config, rehydrate_configured_envs,
};
#[cfg(test)]
use initialization::{
    custom_method_names, extract_client_capabilities_meta,
    extract_client_supports_gosling_custom_notifications,
};
use message_projection::{
    build_tool_call_content, extract_tool_call_update_meta, extract_tool_raw_output,
    merge_replay_message_meta, message_update_meta, outcome_to_confirmation,
    prompt_error_from_message, prompt_error_from_message_content, replay_message_meta,
    send_status_message_update, session_artifact_dto,
};
use tool_metadata::{
    extend_chain_membership, extract_locations_from_meta, extract_tool_locations, format_tool_name,
    pending_tool_call_from_request, read_resource_link, tool_call_identity_meta,
    with_tool_chain_summary_meta,
};
#[cfg(test)]
use tool_metadata::{get_requested_line, is_developer_file_tool, summarize_tool_call};
use transport::negotiate_protocol_version;
#[cfg(test)]
use transport::{finish_connection_on_eof, EofAwareReader};
pub use transport::{run, serve, GoslingAcpHandler, GoslingAgentConnection};

pub type AcpProviderFactory = Arc<
    dyn Fn(
            String,
            Vec<ExtensionConfig>,
            Option<PathBuf>,
        ) -> BoxFuture<'static, Result<Arc<dyn Provider>>>
        + Send
        + Sync,
>;

/// Convenience conversions from any `Display` error into an `agent_client_protocol::Error`.
///
/// Replaces the repetitive `.internal_err()`
/// pattern. Use `.internal_err()?` for server-side failures and `.invalid_params_err()?`
/// for bad client input. For custom messages use `.internal_err_ctx("context")?`.
#[allow(dead_code)]
trait ResultExt<T> {
    fn internal_err(self) -> Result<T, agent_client_protocol::Error>;
    fn invalid_params_err(self) -> Result<T, agent_client_protocol::Error>;
    fn internal_err_ctx(self, context: &str) -> Result<T, agent_client_protocol::Error>;
    fn invalid_params_err_ctx(self, context: &str) -> Result<T, agent_client_protocol::Error>;
}

impl<T, E: std::fmt::Display> ResultExt<T> for Result<T, E> {
    fn internal_err(self) -> Result<T, agent_client_protocol::Error> {
        self.map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))
    }
    fn invalid_params_err(self) -> Result<T, agent_client_protocol::Error> {
        self.map_err(|e| agent_client_protocol::Error::invalid_params().data(e.to_string()))
    }
    fn internal_err_ctx(self, context: &str) -> Result<T, agent_client_protocol::Error> {
        self.map_err(|e| {
            agent_client_protocol::Error::internal_error().data(format!("{context}: {e}"))
        })
    }
    fn invalid_params_err_ctx(self, context: &str) -> Result<T, agent_client_protocol::Error> {
        self.map_err(|e| {
            agent_client_protocol::Error::invalid_params().data(format!("{context}: {e}"))
        })
    }
}

pub(super) const DEFAULT_PROVIDER_ID: &str = "gosling";
pub(super) const DEFAULT_PROVIDER_LABEL: &str = "Gosling (Default)";
const PROVIDER_CONFIG_STATUS_CHECK_CONCURRENCY: usize = 16;

/// In-memory state for an active ACP session.
///
/// ## Terminology (temporary, until all clients migrate to ACP)
///
/// The ACP protocol uses "session" to mean the conversation as the human sees it —
/// a durable, append-only exchange of messages. Internally, gosling also has a concept
/// called "Session" (the `sessions` DB table) which represents the agent's working
/// state: the message list the LLM sees, compaction state, provider binding, etc.
///
/// The ACP session ID maps directly to a `sessions` row. The `sessions` HashMap
/// below is keyed by session ID.
struct GoslingAcpSession {
    agent: Arc<Agent>,
    tool_requests: HashMap<String, crate::conversation::message::ToolRequest>,
    compacted_context: bool,
    tail_limit: usize,
    /// For each tool_call_id that belongs to a multi-tool chain (run of
    /// consecutive ToolRequest blocks within one assistant message), the chain
    /// it belongs to. Populated when the assistant message is processed.
    /// Used by `handle_tool_response` to detect when a chain has fully
    /// completed and fire a single LLM summary covering the run.
    chain_membership: HashMap<String, Arc<ToolChain>>,
    /// Set of tool_call_ids whose ToolResponse has already been processed.
    /// Drives the "all responses present" check for chain completion.
    responded_tool_ids: HashSet<String>,
    /// Tool_call_ids of chains that have already had a summary task fired.
    /// Idempotence guard so we summarize each chain at most once.
    summarized_chains: HashSet<String>,
}

/// A run of consecutive ToolRequest blocks within one assistant message,
/// tracked by [`GoslingAcpSession::chain_membership`]. Used to drive a single
/// LLM summary for the whole run once every step has a recorded ToolResponse.
#[derive(Debug, Clone)]
struct ToolChain {
    /// Tool call ids in document order. Always `len() >= 2`.
    ids: Vec<String>,
    /// The message_id of the assistant message containing these tool calls.
    /// Used to persist chain summaries back to the messages table.
    message_id: String,
}

pub struct GoslingAcpAgentOptions {
    pub state_dir: PathBuf,
    pub provider_factory: AcpProviderFactory,
    pub builtins: Vec<String>,
    pub data_dir: std::path::PathBuf,
    pub platform_data_dir: std::path::PathBuf,
    pub config_dir: std::path::PathBuf,
    pub disable_session_naming: bool,
    pub gosling_platform: GoslingPlatform,
    pub additional_source_roots: Vec<SourceRoot>,
    pub shell_runtime: ShellRuntime,
}

pub struct GoslingAcpAgent {
    runtime_paths: RuntimePaths,
    sessions: Arc<Mutex<HashMap<String, GoslingAcpSession>>>,
    active_prompt_runs: Arc<Mutex<HashMap<String, ActivePromptRun>>>,
    closed_session_ids: Arc<Mutex<HashSet<String>>>,
    agent_manager: Arc<AgentManager>,
    provider_factory: AcpProviderFactory,
    builtins: Vec<String>,
    client_fs_capabilities: OnceCell<FileSystemCapabilities>,
    client_terminal: OnceCell<bool>,
    client_mcp_host_info: OnceCell<GoslingMcpHostInfo>,
    client_supports_acp_elicitation: OnceCell<bool>,
    client_supports_gosling_custom_notifications: OnceCell<bool>,
    use_login_shell_path: OnceCell<bool>,
    client_cx: OnceCell<ConnectionTo<Client>>,
    config_dir: std::path::PathBuf,
    session_manager: Arc<SessionManager>,
    permission_manager: Arc<PermissionManager>,
    disable_session_naming: bool,
    provider_inventory: ProviderInventoryService,
    additional_source_roots: Vec<SourceRoot>,
    workspace_service: Arc<WorkspaceService>,
    default_working_folder: PathBuf,
    shell_runtime: ShellRuntime,
    shell_credential_lookup_cooldown_until: std::sync::Mutex<Option<std::time::Instant>>,
}

/// Shorten a session/thread id for perf log correlation.
/// All `perf:` logs use `sid=<8-char-prefix>` so a single session's activity
/// can be extracted with `grep 'perf:' <log> | grep 'sid=abc12345'`.
pub(super) fn sid_short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn meta_string(
    meta: Option<&Meta>,
    key: &str,
) -> Result<Option<String>, agent_client_protocol::Error> {
    let Some(value) = meta.and_then(|m| m.get(key)) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_str() else {
        return Err(
            agent_client_protocol::Error::invalid_params().data(format!("{key} must be a string"))
        );
    };
    Ok(Some(value.to_string()))
}

#[derive(Debug, Clone, Copy)]
struct SessionLoadOptions {
    compacted: bool,
    tail_limit: usize,
}

fn compacted_load_options_from_meta(
    meta: Option<&Meta>,
) -> Result<SessionLoadOptions, agent_client_protocol::Error> {
    let Some(gosling) = meta
        .and_then(|m| m.get("gosling"))
        .and_then(|value| value.as_object())
    else {
        return Ok(SessionLoadOptions {
            compacted: false,
            tail_limit: DEFAULT_SESSION_TAIL_LIMIT,
        });
    };

    let load_mode = gosling
        .get("loadMode")
        .and_then(|value| value.as_str())
        .unwrap_or("full");
    let compacted = match load_mode {
        "compacted" => true,
        "full" => false,
        other => {
            return Err(agent_client_protocol::Error::invalid_params().data(format!(
                "gosling.loadMode must be 'full' or 'compacted', got {other}"
            )));
        }
    };

    let tail_limit = match gosling.get("tailLimit") {
        Some(value) if value.is_null() => DEFAULT_SESSION_TAIL_LIMIT,
        Some(value) => {
            let Some(raw_limit) = value.as_u64() else {
                return Err(agent_client_protocol::Error::invalid_params()
                    .data("gosling.tailLimit must be a number"));
            };
            raw_limit
                .clamp(1, MAX_SESSION_MESSAGE_PAGE_LIMIT as u64)
                .try_into()
                .unwrap_or(DEFAULT_SESSION_TAIL_LIMIT)
        }
        None => DEFAULT_SESSION_TAIL_LIMIT,
    };

    Ok(SessionLoadOptions {
        compacted,
        tail_limit,
    })
}

fn spawn_session_name_update_notifier(
    cx: ConnectionTo<Client>,
) -> tokio::sync::mpsc::UnboundedSender<crate::session::SessionNameUpdate> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::session::SessionNameUpdate>();
    tokio::spawn(async move {
        while let Some(update) = rx.recv().await {
            let mut meta = serde_json::Map::new();
            meta.insert(
                "messageCount".to_string(),
                serde_json::Value::Number(update.message_count.into()),
            );
            meta.insert(
                "userSetName".to_string(),
                serde_json::Value::Bool(update.user_set_name),
            );
            let notification = SessionNotification::new(
                SessionId::new(update.session_id.clone()),
                SessionUpdate::SessionInfoUpdate(
                    SessionInfoUpdate::new()
                        .title(update.name)
                        .updated_at(update.updated_at.to_rfc3339())
                        .meta(meta),
                ),
            );
            if let Err(error) = cx.send_notification(notification) {
                warn!(
                    session_id = %update.session_id,
                    error = %error,
                    "Failed to send generated session name update"
                );
            }
        }
    });
    tx
}

pub(super) fn validate_absolute_cwd(cwd: &Path) -> Result<(), agent_client_protocol::Error> {
    if !cwd.is_absolute() {
        return Err(
            agent_client_protocol::Error::invalid_params().data("cwd must be an absolute path")
        );
    }

    if !cwd.exists() || !cwd.is_dir() {
        return Err(agent_client_protocol::Error::invalid_params().data("invalid directory path"));
    }

    Ok(())
}

impl GoslingAcpAgent {
    pub fn permission_manager(&self) -> Arc<PermissionManager> {
        Arc::clone(&self.permission_manager)
    }

    pub(super) fn supports_gosling_custom_notifications(&self) -> bool {
        self.client_supports_gosling_custom_notifications
            .get()
            .copied()
            .unwrap_or(false)
    }

    fn supports_acp_elicitation(&self) -> bool {
        self.client_supports_acp_elicitation
            .get()
            .copied()
            .unwrap_or(false)
    }

    // TODO[POLISH-20260827-006]: gosling reads Paths::in_state_dir globally (e.g. RequestLog), ignoring this data_dir.
    pub async fn new(options: GoslingAcpAgentOptions) -> Result<Self> {
        let runtime_paths = RuntimePaths::new(
            options.config_dir.clone(),
            options.data_dir.clone(),
            options.state_dir.clone(),
        );
        let agent_runtime_paths = runtime_paths.clone();

        Paths::scope(runtime_paths, async move {
            let default_working_folder = std::env::var_os("GOSLING_WORKING_DIR")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("/"));
            let workspace_service = Arc::new(
                WorkspaceService::initialize(&options.platform_data_dir, &default_working_folder)
                    .await?,
            );
            let session_manager = Arc::new(SessionManager::new(options.data_dir));

            session_manager.storage().pool().await?;

            let permission_manager = PermissionManager::for_config_dir(options.config_dir.clone());
            let provider_inventory =
                ProviderInventoryService::new(session_manager.storage().clone());
            let config = Config::global();
            let agent_config = AgentConfig::new(
                Arc::clone(&session_manager),
                Arc::clone(&permission_manager),
                config.get_gosling_mode().unwrap_or_default(),
                options.disable_session_naming,
                options.gosling_platform.clone(),
            )
            .with_code_execution_runtime(config.resolve_gosling_code_execution_runtime())
            .with_workspace_service(Arc::clone(&workspace_service));
            let agent_manager = Arc::new(AgentManager::new(agent_config, None).await?);

            Ok(Self {
                runtime_paths: agent_runtime_paths,
                sessions: Arc::new(Mutex::new(HashMap::new())),
                active_prompt_runs: Arc::new(Mutex::new(HashMap::new())),
                closed_session_ids: Arc::new(Mutex::new(HashSet::new())),
                agent_manager,
                provider_factory: options.provider_factory,
                builtins: options.builtins,
                client_fs_capabilities: OnceCell::new(),
                client_terminal: OnceCell::new(),
                client_mcp_host_info: OnceCell::new(),
                client_supports_acp_elicitation: OnceCell::new(),
                client_supports_gosling_custom_notifications: OnceCell::new(),
                use_login_shell_path: OnceCell::new(),
                client_cx: OnceCell::new(),
                config_dir: options.config_dir,
                session_manager,
                permission_manager,
                disable_session_naming: options.disable_session_naming,
                provider_inventory,
                additional_source_roots: options.additional_source_roots,
                workspace_service,
                default_working_folder,
                shell_runtime: options.shell_runtime,
                shell_credential_lookup_cooldown_until: std::sync::Mutex::new(None),
            })
        })
        .await
    }

    fn config(&self) -> Result<&'static Config, agent_client_protocol::Error> {
        Ok(Config::global())
    }

    async fn create_provider(
        &self,
        provider_name: &str,
        extensions: Vec<ExtensionConfig>,
        working_dir: Option<PathBuf>,
    ) -> Result<Arc<dyn Provider>> {
        (self.provider_factory)(provider_name.to_string(), extensions, working_dir).await
    }

    async fn maybe_refresh_provider_inventory_with_agent(
        &self,
        gosling_session: &Session,
        agent: &Arc<Agent>,
    ) {
        let Some(provider_name) = gosling_session.provider_name.as_deref() else {
            return;
        };
        let Some(mut inventory) = self
            .provider_inventory
            .find_entry_for_provider(provider_name)
            .await
        else {
            return;
        };
        if !should_refresh_inventory_for_session_init(&inventory) {
            return;
        }
        let provider = match agent.provider().await {
            Ok(provider) => provider,
            Err(error) => {
                warn!(
                    provider = %provider_name,
                    session = %gosling_session.id,
                    error = %error,
                    "agent has no provider available for inventory refresh"
                );
                return;
            }
        };
        self.provider_inventory
            .refresh_with_provider(provider_name, &provider, &mut inventory, "session init")
            .await;
    }

    async fn get_or_create_session_agent_with_results(
        &self,
        cx: &ConnectionTo<Client>,
        session_id: String,
    ) -> Result<AgentManagerGetResult, agent_client_protocol::Error> {
        self.agent_manager
            .get_or_create_agent_with_runtime_context(
                session_id,
                RuntimeContext {
                    mcp_host_info: self.client_mcp_host_info.get().cloned(),
                    use_login_shell_path: self.use_login_shell_path.get().copied(),
                    session_name_update_tx: (!self.disable_session_naming)
                        .then(|| spawn_session_name_update_notifier(cx.clone())),
                },
            )
            .await
            .internal_err_ctx("Failed to create agent")
    }
}

impl GoslingAcpAgent {
    async fn on_new_session(
        &self,
        cx: &ConnectionTo<Client>,
        args: NewSessionRequest,
    ) -> Result<NewSessionResponse, agent_client_protocol::Error> {
        self.handle_new_session(cx, args).await
    }

    /// Look up the session's agent.
    async fn get_session_agent(
        &self,
        session_id: &str,
    ) -> Result<Arc<Agent>, agent_client_protocol::Error> {
        if self.closed_session_ids.lock().await.contains(session_id) {
            return Err(agent_client_protocol::Error::resource_not_found(Some(
                session_id.to_string(),
            ))
            .data(format!("Session not found: {}", session_id)));
        }

        {
            let sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get(session_id) {
                return Ok(session.agent.clone());
            }
        }

        let cx = self.client_cx.get().ok_or_else(|| {
            agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                .data(format!("Session not found: {}", session_id))
        })?;
        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;
        let (agent, _) = self
            .activate_acp_session(cx, &session, HashMap::new())
            .await?;
        Ok(agent)
    }

    async fn on_load_session(
        &self,
        cx: &ConnectionTo<Client>,
        args: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, agent_client_protocol::Error> {
        self.handle_load_session(cx, args).await
    }

    async fn on_fork_session(
        &self,
        cx: &ConnectionTo<Client>,
        args: ForkSessionRequest,
    ) -> Result<ForkSessionResponse, agent_client_protocol::Error> {
        self.handle_fork_session(cx, args).await
    }
}

#[cfg(test)]
mod tests;
