mod artifacts_storage;
mod legacy_import;
mod library_storage;
mod message_storage;
mod migrations;
mod output_revisions_storage;
mod pool_lifecycle;
mod schema;
mod session_crud;
mod session_leases;
mod session_listing;
mod session_transfer;
mod summary_storage;
mod tool_operations;

#[cfg(test)]
use summary_storage::summary_covers_history_before;

pub(crate) use tool_operations::ToolOperationStart;

use crate::config::paths::Paths;
use crate::config::GoslingMode;
use crate::conversation::message::{Message, SystemNotificationType, TokenState};
use crate::conversation::Conversation;
use crate::mcp_utils::ToolResult;
use crate::providers::base::Provider;
#[cfg(test)]
use crate::session::artifacts::SessionArtifactProvenance;
use crate::session::artifacts::{DiscoveredArtifact, SessionArtifact};
use crate::session::extension_data::ExtensionData;
#[cfg(test)]
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::library::{NewSessionLibraryContent, SessionLibraryItem, SessionLibraryScope};
use crate::session::session_naming::{
    generate_session_name, MSG_COUNT_FOR_SESSION_NAME_GENERATION,
};
use crate::utils::sanitize_unicode_tags;
use crate::workspace::WorkspaceSessionContext;
use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use gosling_providers::conversation::token_usage::Usage;
use gosling_providers::model::ModelConfig;
use rmcp::model::{CallToolRequestParams, CallToolResult, Role};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use utoipa::ToSchema;

pub const CURRENT_SCHEMA_VERSION: i32 = 32;

pub use output_revisions_storage::OutputCapture;
pub const SESSIONS_FOLDER: &str = "sessions";
pub const DB_NAME: &str = "sessions.db";
const MILLISECOND_TIMESTAMP_THRESHOLD: i64 = 10_000_000_000;
pub const DEFAULT_SESSION_TAIL_LIMIT: usize = 50;
pub const MAX_SESSION_MESSAGE_PAGE_LIMIT: usize = 200;

/// Result of importing a local transcript file. Identical content and a
/// changed source path are both prevented from creating a duplicate full
/// transcript; callers get the original session instead.
#[derive(Debug, Clone)]
pub enum SessionFileImportResult {
    Imported(Session),
    AlreadyImported(Session),
    /// The same canonical source file was imported before, but its content
    /// fingerprint has changed. Refuse to ingest the whole transcript again:
    /// that would duplicate all of the earlier messages. A future explicit
    /// refresh operation can merge new source records by their durable IDs.
    SourceChanged(Session),
}

fn validate_session_name(name: &str) -> Result<()> {
    anyhow::ensure!(!name.trim().is_empty(), "Session name must not be empty");
    Ok(())
}

#[cfg(test)]
mod session_name_validation_tests {
    use super::*;

    #[test]
    fn rejects_empty_and_whitespace_only_session_names() {
        assert!(validate_session_name("").is_err());
        assert!(validate_session_name("  \n\t").is_err());
        assert!(validate_session_name("CLI Session").is_ok());
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    ToSchema,
    PartialEq,
    Eq,
    Default,
    strum::Display,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SessionType {
    #[default]
    User,
    Scheduled,
    SubAgent,
    Hidden,
    Terminal,
    Acp,
}

static SESSION_STORAGE: LazyLock<Arc<SessionStorage>> =
    LazyLock::new(|| Arc::new(SessionStorage::new(Paths::data_dir())));

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Session {
    pub id: String,
    #[schema(value_type = String)]
    pub working_dir: PathBuf,
    /// Extra directories the agent has full tool access to, beyond `working_dir`.
    #[serde(default)]
    #[schema(value_type = Vec<String>)]
    pub additional_working_dirs: Vec<PathBuf>,
    /// Opt-in, off by default. When true, tool calls that touch a path outside
    /// every working directory require approval with a message explaining why,
    /// instead of following the session's normal approval mode.
    #[serde(default)]
    pub restrict_tools_to_working_dirs: bool,
    #[serde(alias = "description")]
    pub name: String,
    #[serde(default)]
    pub user_set_name: bool,
    #[serde(default)]
    pub session_type: SessionType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub extension_data: ExtensionData,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub accumulated_usage: Usage,
    pub accumulated_cost: Option<f64>,
    pub conversation: Option<Conversation>,
    pub message_count: usize,
    #[serde(default)]
    pub last_message_at: Option<DateTime<Utc>>,
    pub provider_name: Option<String>,
    pub model_config: Option<ModelConfig>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub workspace_name: Option<String>,
    #[serde(default)]
    pub credential_profile_id: Option<String>,
    #[serde(default)]
    pub credential_profile_name: Option<String>,
    #[serde(default)]
    pub credential_binding_id: Option<String>,
    #[serde(default)]
    pub workspace_context: Option<WorkspaceSessionContext>,
    #[serde(default)]
    pub gosling_mode: GoslingMode,
    #[serde(default)]
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub last_message_snippet: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionSummaryStatus {
    Current,
    #[default]
    Stale,
    Failed,
}

impl std::fmt::Display for SessionSummaryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Current => write!(f, "current"),
            Self::Stale => write!(f, "stale"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for SessionSummaryStatus {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "current" => Ok(Self::Current),
            "stale" => Ok(Self::Stale),
            "failed" => Ok(Self::Failed),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionSummary {
    pub session_id: String,
    pub summary: String,
    pub covered_through_row_id: i64,
    pub covered_through_timestamp: i64,
    pub covered_message_count: usize,
    pub source_hash: String,
    pub summarizer_model: Option<String>,
    pub status: SessionSummaryStatus,
    pub error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionSummaryFact {
    pub id: i64,
    pub session_id: String,
    pub project_id: Option<String>,
    pub working_dir: String,
    pub scope: String,
    pub fact_type: String,
    pub content: String,
    pub confidence: f32,
    pub source_start_row_id: Option<i64>,
    pub source_end_row_id: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionMessagePage {
    pub messages: Vec<Message>,
    pub next_before_cursor: Option<String>,
    pub total_count: usize,
    pub oldest_row_id: Option<i64>,
    pub newest_row_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionMessageSearchMatch {
    pub row_id: i64,
    pub message_id: Option<String>,
    pub role: String,
    pub snippet: String,
    pub created: i64,
    pub before_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionMessageSearchResults {
    pub matches: Vec<SessionMessageSearchMatch>,
    pub total_matches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionArtifactPage {
    pub artifacts: Vec<SessionArtifact>,
    pub next_cursor: Option<String>,
    pub total_count: usize,
}

impl From<&Session> for TokenState {
    fn from(session: &Session) -> Self {
        Self {
            input_tokens: session.usage.input_tokens.unwrap_or(0),
            output_tokens: session.usage.output_tokens.unwrap_or(0),
            total_tokens: session.usage.total_tokens.unwrap_or(0),
            cache_read_tokens: session.usage.cache_read_input_tokens.unwrap_or(0),
            cache_write_tokens: session.usage.cache_write_input_tokens.unwrap_or(0),
            accumulated_input_tokens: session.accumulated_usage.input_tokens.unwrap_or(0),
            accumulated_output_tokens: session.accumulated_usage.output_tokens.unwrap_or(0),
            accumulated_total_tokens: session.accumulated_usage.total_tokens.unwrap_or(0),
            accumulated_cache_read_tokens: session
                .accumulated_usage
                .cache_read_input_tokens
                .unwrap_or(0),
            accumulated_cache_write_tokens: session
                .accumulated_usage
                .cache_write_input_tokens
                .unwrap_or(0),
            accumulated_cost: session.accumulated_cost,
        }
    }
}

pub struct SessionUpdateBuilder<'a> {
    session_manager: &'a SessionManager,
    session_id: String,
    name: Option<String>,
    user_set_name: Option<bool>,
    /// Set by `system_generated_name`: only apply this update if the
    /// session's `user_set_name` is still false at write time, so a
    /// background auto-naming task can't clobber a rename the user made
    /// while it was running. See `apply_update`.
    only_if_not_user_named: bool,
    session_type: Option<SessionType>,
    working_dir: Option<PathBuf>,
    additional_working_dirs: Option<Vec<PathBuf>>,
    restrict_tools_to_working_dirs: Option<bool>,
    extension_data: Option<ExtensionData>,
    usage: Option<Usage>,
    accumulated_usage: Option<Usage>,
    accumulated_cost: Option<Option<f64>>,
    provider_name: Option<Option<String>>,
    model_config: Option<Option<ModelConfig>>,
    workspace_id: Option<Option<String>>,
    workspace_name: Option<Option<String>>,
    credential_profile_id: Option<Option<String>>,
    credential_profile_name: Option<Option<String>>,
    credential_binding_id: Option<Option<String>>,
    workspace_context: Option<Option<WorkspaceSessionContext>>,
    gosling_mode: Option<GoslingMode>,
    archived_at: Option<Option<DateTime<Utc>>>,

    project_id: Option<Option<String>>,
}

#[derive(Serialize, ToSchema, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionInsights {
    pub total_sessions: usize,
    pub total_tokens: i64,
}

impl<'a> SessionUpdateBuilder<'a> {
    fn new(session_manager: &'a SessionManager, session_id: String) -> Self {
        Self {
            session_manager,
            session_id,
            name: None,
            user_set_name: None,
            only_if_not_user_named: false,
            session_type: None,
            working_dir: None,
            additional_working_dirs: None,
            restrict_tools_to_working_dirs: None,
            extension_data: None,
            usage: None,
            accumulated_usage: None,
            accumulated_cost: None,
            provider_name: None,
            model_config: None,
            workspace_id: None,
            workspace_name: None,
            credential_profile_id: None,
            credential_profile_name: None,
            credential_binding_id: None,
            workspace_context: None,
            gosling_mode: None,
            archived_at: None,
            project_id: None,
        }
    }

    pub async fn apply(self) -> Result<()> {
        self.session_manager.apply_update_inner(self).await
    }

    pub fn user_provided_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into().trim().to_string();
        if !name.is_empty() {
            self.name = Some(name);
            self.user_set_name = Some(true);
        }
        self
    }

    pub fn system_generated_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into().trim().to_string();
        if !name.is_empty() {
            self.name = Some(name);
            self.user_set_name = Some(false);
            self.only_if_not_user_named = true;
        }
        self
    }

    pub fn session_type(mut self, session_type: SessionType) -> Self {
        self.session_type = Some(session_type);
        self
    }

    pub fn working_dir(mut self, working_dir: PathBuf) -> Self {
        self.working_dir = Some(working_dir);
        self
    }

    pub fn additional_working_dirs(mut self, additional_working_dirs: Vec<PathBuf>) -> Self {
        self.additional_working_dirs = Some(additional_working_dirs);
        self
    }

    pub(crate) fn workspace_context(
        mut self,
        workspace_context: Option<WorkspaceSessionContext>,
    ) -> Self {
        self.workspace_context = Some(workspace_context);
        self
    }

    pub fn restrict_tools_to_working_dirs(mut self, restrict: bool) -> Self {
        self.restrict_tools_to_working_dirs = Some(restrict);
        self
    }

    pub fn extension_data(mut self, data: ExtensionData) -> Self {
        self.extension_data = Some(data);
        self
    }

    pub fn usage(mut self, usage: Usage) -> Self {
        self.usage = Some(usage);
        self
    }

    pub fn accumulated_usage(mut self, usage: Usage) -> Self {
        self.accumulated_usage = Some(usage);
        self
    }

    pub fn accumulated_cost(mut self, cost: Option<f64>) -> Self {
        self.accumulated_cost = Some(cost);
        self
    }

    pub fn provider_name(mut self, provider_name: impl Into<String>) -> Self {
        self.provider_name = Some(Some(provider_name.into()));
        self
    }

    pub fn model_config(mut self, model_config: ModelConfig) -> Self {
        self.model_config = Some(Some(model_config));
        self
    }

    pub fn clear_model_config(mut self) -> Self {
        self.model_config = Some(None);
        self
    }

    pub fn gosling_mode(mut self, mode: GoslingMode) -> Self {
        self.gosling_mode = Some(mode);
        self
    }

    pub fn archived_at(mut self, archived_at: Option<DateTime<Utc>>) -> Self {
        self.archived_at = Some(archived_at);
        self
    }

    pub fn project_id(mut self, project_id: Option<String>) -> Self {
        self.project_id = Some(project_id);
        self
    }

    pub fn credential_profile_snapshot(
        mut self,
        credential_profile_id: String,
        credential_profile_name: String,
    ) -> Self {
        self.credential_profile_id = Some(Some(credential_profile_id));
        self.credential_profile_name = Some(Some(credential_profile_name));
        self.credential_binding_id = Some(None);
        self
    }

    pub fn workspace_snapshot(
        mut self,
        workspace_id: String,
        workspace_name: String,
        credential_profile_id: Option<String>,
        credential_profile_name: Option<String>,
        credential_binding_id: Option<String>,
        mut context: WorkspaceSessionContext,
    ) -> Self {
        let folder_policy = context.effective_folder_policy();
        let primary = PathBuf::from(&context.primary_working_folder);
        self.additional_working_dirs = Some(
            folder_policy
                .roots
                .iter()
                .map(|root| PathBuf::from(&root.path))
                .filter(|path| path != &primary)
                .collect(),
        );
        // The restriction flag is left to the stored column default (off / opt-in):
        // the WorkingDirScopeInspector still enforces this workspace's folder policy
        // because workspace_context is set, so scoping holds without pre-blocking
        // providers that run their own tools (Claude Code CLI, Codex CLI, …).
        context.folder_policy = folder_policy;
        self.workspace_id = Some(Some(workspace_id));
        self.workspace_name = Some(Some(workspace_name));
        self.credential_profile_id = Some(credential_profile_id);
        self.credential_profile_name = Some(credential_profile_name);
        self.credential_binding_id = Some(credential_binding_id);
        self.workspace_context = Some(Some(context));
        self
    }
}

pub struct SessionManager {
    storage: Arc<SessionStorage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionListCursor {
    pub(crate) sort_at: DateTime<Utc>,
    pub(crate) session_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionListPage {
    pub(crate) sessions: Vec<Session>,
    pub(crate) next_cursor: Option<SessionListCursor>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SessionArchiveState {
    #[default]
    Active,
    Archived,
    All,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct SessionListFilters<'a> {
    pub(crate) types: Option<&'a [SessionType]>,
    pub(crate) working_dir: Option<&'a Path>,
    pub(crate) keyword: Option<&'a str>,
    pub(crate) only_sessions_with_messages: bool,
    pub(crate) archive_state: SessionArchiveState,
    pub(crate) workspace_id: Option<&'a str>,
    pub(crate) include_unassigned: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionListPageQuery<'a> {
    pub(crate) filters: SessionListFilters<'a>,
    pub(crate) cursor: Option<&'a SessionListCursor>,
    pub(crate) page_size: usize,
    pub(crate) include_last_message_snippet: bool,
}

#[derive(Debug, Clone)]
pub struct SessionNameUpdate {
    pub session_id: String,
    pub name: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
    pub user_set_name: bool,
}

impl SessionManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            storage: Arc::new(SessionStorage::new(data_dir)),
        }
    }

    pub fn instance() -> Self {
        Self {
            storage: Arc::clone(&SESSION_STORAGE),
        }
    }

    pub fn storage(&self) -> &Arc<SessionStorage> {
        &self.storage
    }

    /// Cheap liveness probe for the session store: acquires the connection
    /// pool and runs a trivial query. Intended for health/readiness
    /// endpoints that need to distinguish "the process is up" from "the
    /// session database is actually reachable", not for hot paths.
    pub async fn healthy(&self) -> Result<()> {
        let pool = self.storage.pool().await?;
        sqlx::query("SELECT 1").fetch_one(pool).await?;
        Ok(())
    }

    pub(crate) async fn acquire_session_turn_lease(
        &self,
        session_id: &str,
        parent_cancel: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<session_leases::SessionTurnLease> {
        self.storage
            .clone()
            .acquire_session_turn_lease(session_id, parent_cancel)
            .await
    }

    pub async fn create_session(
        &self,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
        gosling_mode: GoslingMode,
    ) -> Result<Session> {
        validate_session_name(&name)?;
        self.storage
            .create_session(working_dir, name, session_type, gosling_mode)
            .await
    }

    pub async fn get_session(&self, id: &str, include_messages: bool) -> Result<Session> {
        self.storage.get_session(id, include_messages).await
    }

    pub async fn get_session_for_compacted_resume(
        &self,
        id: &str,
        tail_limit: usize,
    ) -> Result<Session> {
        self.storage
            .get_session_for_compacted_resume(id, tail_limit)
            .await
    }

    pub async fn get_session_message_page(
        &self,
        id: &str,
        before_cursor: Option<&str>,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        self.storage
            .get_session_message_page(id, before_cursor, limit)
            .await
    }

    pub async fn get_session_tail_page(
        &self,
        id: &str,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        self.storage.get_session_tail_page(id, limit).await
    }

    pub async fn get_session_message_rows_between(
        &self,
        id: &str,
        after_row_id: i64,
        before_row_id: i64,
        limit: usize,
    ) -> Result<Vec<(i64, Message)>> {
        self.storage
            .get_session_message_rows_between(id, after_row_id, before_row_id, limit)
            .await
    }

    /// Return a bounded chronological window around a specific durable
    /// message. Recall uses this to hydrate one relevant hit without loading
    /// the whole session transcript.
    pub async fn get_session_message_window(
        &self,
        id: &str,
        message_id: &str,
        before: usize,
        after: usize,
    ) -> Result<Vec<Message>> {
        self.storage
            .get_session_message_window(id, message_id, before, after)
            .await
    }

    pub async fn search_session_messages(
        &self,
        id: &str,
        query: &str,
        limit: usize,
    ) -> Result<SessionMessageSearchResults> {
        self.storage.search_session_messages(id, query, limit).await
    }

    pub async fn list_session_artifacts(
        &self,
        id: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<SessionArtifactPage> {
        self.storage.list_session_artifacts(id, cursor, limit).await
    }

    pub async fn upsert_session_artifacts(
        &self,
        session_id: &str,
        artifacts: &[DiscoveredArtifact],
    ) -> Result<Vec<SessionArtifact>> {
        self.storage
            .upsert_session_artifacts(session_id, artifacts)
            .await
    }

    pub async fn list_session_library_items(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionLibraryItem>> {
        self.storage.list_session_library_items(session_id).await
    }

    pub async fn add_session_library_item(
        &self,
        session_id: &str,
        scope: SessionLibraryScope,
        name: String,
        content: NewSessionLibraryContent,
    ) -> Result<SessionLibraryItem> {
        self.storage
            .add_session_library_item(session_id, scope, name, content)
            .await
    }

    pub async fn remove_session_library_item(
        &self,
        session_id: &str,
        item_id: &str,
    ) -> Result<bool> {
        self.storage
            .remove_session_library_item(session_id, item_id)
            .await
    }

    pub async fn get_session_library_items(
        &self,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<Vec<SessionLibraryItem>> {
        self.storage
            .get_session_library_items(session_id, item_ids)
            .await
    }

    pub async fn get_session_summary(&self, id: &str) -> Result<Option<SessionSummary>> {
        self.storage.get_session_summary(id).await
    }

    pub async fn get_session_summary_facts(&self, id: &str) -> Result<Vec<SessionSummaryFact>> {
        self.storage.get_session_summary_facts(id).await
    }

    pub async fn upsert_session_summary(&self, summary: &SessionSummary) -> Result<()> {
        self.storage.upsert_session_summary(summary).await
    }

    pub async fn replace_session_summary_facts(
        &self,
        session_id: &str,
        facts: &[SessionSummaryFact],
    ) -> Result<()> {
        self.storage
            .replace_session_summary_facts(session_id, facts)
            .await
    }

    pub fn update(&self, id: &str) -> SessionUpdateBuilder<'_> {
        SessionUpdateBuilder::new(self, id.to_string())
    }

    async fn apply_update_inner(&self, builder: SessionUpdateBuilder<'_>) -> Result<()> {
        self.storage.apply_update(builder).await
    }

    pub async fn add_message(&self, id: &str, message: &Message) -> Result<()> {
        self.storage.add_message(id, message).await
    }

    pub async fn upsert_message(&self, id: &str, message: &Message) -> Result<()> {
        self.storage.upsert_message(id, message).await
    }

    pub async fn register_completed_assistant_artifacts(
        &self,
        id: &str,
        message: &Message,
    ) -> Result<()> {
        self.storage
            .register_completed_assistant_artifacts(id, message)
            .await
    }

    pub(crate) async fn begin_tool_operation(
        &self,
        session_id: &str,
        tool_request_id: &str,
        tool_call: &CallToolRequestParams,
        conversation_bound: bool,
    ) -> Result<ToolOperationStart> {
        self.storage
            .begin_tool_operation(session_id, tool_request_id, tool_call, conversation_bound)
            .await
    }

    pub(crate) async fn complete_tool_operation(
        &self,
        operation_id: &str,
        result: &ToolResult<CallToolResult>,
    ) -> Result<()> {
        let completion = self
            .storage
            .complete_tool_operation(operation_id, result)
            .await;
        self.storage.release_tool_operation(operation_id);
        completion
    }

    pub(crate) fn release_tool_operation(&self, operation_id: &str) {
        self.storage.release_tool_operation(operation_id);
    }

    pub(crate) async fn mark_tool_operation_in_doubt(&self, operation_id: &str) -> Result<()> {
        self.storage
            .mark_tool_operation_in_doubt(operation_id)
            .await
    }

    pub(crate) async fn persist_tool_operation_response(
        &self,
        session_id: &str,
        tool_request_id: &str,
        message: &Message,
    ) -> Result<()> {
        self.storage
            .persist_tool_operation_response(session_id, tool_request_id, message)
            .await
    }

    pub async fn recover_tool_operations(&self, session_id: &str) -> Result<usize> {
        self.storage.recover_tool_operations(session_id).await
    }

    pub async fn cancel_undispatched_tool_requests(
        &self,
        session_id: &str,
        cancelled_request_id: &str,
    ) -> Result<usize> {
        self.storage
            .cancel_undispatched_tool_requests(session_id, cancelled_request_id)
            .await
    }

    pub async fn add_model_switch_record(
        &self,
        id: &str,
        msg: impl Into<String>,
    ) -> Result<Message> {
        let message = Message::assistant()
            .with_generated_id()
            .with_system_notification_with_data(
                SystemNotificationType::InlineMessage,
                sanitize_unicode_tags(&msg.into()),
                serde_json::json!({ "kind": "modelSwitch" }),
            );
        self.add_message(id, &message).await?;
        Ok(message)
    }

    pub async fn replace_conversation(&self, id: &str, conversation: &Conversation) -> Result<()> {
        self.storage.replace_conversation(id, conversation).await
    }

    /// Atomic pairing of `replace_conversation` and `record_usage`, for
    /// compaction call sites that must not let the two commit separately.
    pub async fn replace_conversation_and_record_usage(
        &self,
        id: &str,
        conversation: &Conversation,
        current_usage: Usage,
        accumulated_delta: Usage,
        cost_delta: Option<f64>,
    ) -> Result<()> {
        self.storage
            .replace_conversation_and_record_usage(
                id,
                conversation,
                current_usage,
                accumulated_delta,
                cost_delta,
            )
            .await
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.storage.list_sessions().await
    }

    pub async fn list_sessions_by_types(&self, types: &[SessionType]) -> Result<Vec<Session>> {
        self.storage
            .list_sessions_by_types(Some(types), SessionArchiveState::Active)
            .await
    }

    pub async fn record_usage(
        &self,
        session_id: &str,
        current_usage: Usage,
        accumulated_delta: Usage,
        cost_delta: Option<f64>,
    ) -> Result<()> {
        self.storage
            .record_usage(session_id, current_usage, accumulated_delta, cost_delta)
            .await
    }

    pub(crate) async fn list_sessions_paged(
        &self,
        query: SessionListPageQuery<'_>,
    ) -> Result<SessionListPage> {
        self.storage.list_sessions_paged(query).await
    }

    pub async fn list_all_sessions(&self) -> Result<Vec<Session>> {
        self.storage
            .list_sessions_by_types(None, SessionArchiveState::All)
            .await
    }

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        self.storage.delete_session(id).await
    }

    pub async fn get_insights(&self) -> Result<SessionInsights> {
        self.storage
            .get_insights(&[SessionType::User, SessionType::Scheduled])
            .await
    }

    pub async fn export_session(&self, id: &str) -> Result<String> {
        self.storage.export_session(id).await
    }

    pub async fn import_session(
        &self,
        json: &str,
        session_type_override: Option<SessionType>,
        working_dir: PathBuf,
        transport: super::import_formats::SessionImportTransport,
    ) -> Result<Session> {
        let source_sha256 = Sha256::digest(json.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        for session in self.list_all_sessions().await? {
            let Some(provenance) =
                super::import_formats::SessionImportProvenance::from_extension_data(
                    &session.extension_data,
                )
            else {
                continue;
            };
            if provenance.source_sha256.as_deref() == Some(&source_sha256) {
                return Ok(session);
            }
        }

        self.storage
            .import_session(
                self,
                json,
                session_type_override,
                working_dir,
                transport,
                Some((None, source_sha256)),
            )
            .await
    }

    /// Import a transcript file once, retaining an untrusted source label and
    /// content fingerprint in the session provenance. JSON and Nostr imports
    /// share the same content-fingerprint replay guard but do not acquire a
    /// misleading local source path.
    pub async fn import_session_file(
        &self,
        path: &Path,
        session_type_override: Option<SessionType>,
        working_dir: PathBuf,
    ) -> Result<SessionFileImportResult> {
        let source_path = fs::canonicalize(path)?;
        let source_path_string = source_path.to_string_lossy().to_string();
        let json = super::import_formats::read_session_import_file(&source_path)?;
        let source_sha256 = Sha256::digest(json.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();

        for session in self.list_all_sessions().await? {
            let Some(provenance) =
                super::import_formats::SessionImportProvenance::from_extension_data(
                    &session.extension_data,
                )
            else {
                continue;
            };

            if provenance.source_sha256.as_deref() == Some(&source_sha256) {
                return Ok(SessionFileImportResult::AlreadyImported(session));
            }

            if provenance.source_path.as_deref() == Some(source_path_string.as_str()) {
                return Ok(SessionFileImportResult::SourceChanged(session));
            }
        }

        let session = self
            .storage
            .import_session(
                self,
                &json,
                session_type_override,
                working_dir,
                super::import_formats::SessionImportTransport::CliFile,
                Some((Some(&source_path), source_sha256)),
            )
            .await?;
        Ok(SessionFileImportResult::Imported(session))
    }

    pub async fn copy_session(&self, session_id: &str, new_name: String) -> Result<Session> {
        self.storage.copy_session(self, session_id, new_name).await
    }

    pub async fn truncate_conversation(&self, session_id: &str, timestamp: i64) -> Result<()> {
        self.storage
            .truncate_conversation(session_id, timestamp)
            .await
    }

    pub async fn truncate_conversation_from_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<()> {
        self.storage
            .truncate_conversation_from_message(session_id, message_id)
            .await
    }

    async fn system_generated_name_update(
        &self,
        id: &str,
        name: String,
    ) -> Result<SessionNameUpdate> {
        self.update(id)
            .system_generated_name(name.clone())
            .apply()
            .await?;

        let session = self.get_session(id, false).await?;
        Ok(SessionNameUpdate {
            session_id: id.to_string(),
            name,
            updated_at: session.updated_at,
            message_count: session.message_count,
            user_set_name: session.user_set_name,
        })
    }

    pub async fn maybe_update_name(
        &self,
        id: &str,
        provider: Arc<dyn Provider>,
    ) -> Result<Option<SessionNameUpdate>> {
        let session = self.get_session(id, true).await?;

        if session.user_set_name {
            return Ok(None);
        }

        if session.session_type == SessionType::Scheduled {
            return Ok(None);
        }

        let model_config = match session.model_config.clone() {
            Some(model_config) => model_config,
            None => {
                let model_name = crate::config::Config::global()
                    .get_gosling_model()
                    .map_err(|_| {
                        anyhow::anyhow!("Could not resolve model config: missing model")
                    })?;
                crate::model_config::model_config_from_user_config(
                    provider.get_name(),
                    &model_name,
                )?
            }
        };
        let conversation = session
            .conversation
            .ok_or_else(|| anyhow::anyhow!("No messages found"))?;

        let user_message_count = conversation
            .messages()
            .iter()
            .filter(|m| matches!(m.role, Role::User))
            .count();

        if user_message_count <= MSG_COUNT_FOR_SESSION_NAME_GENERATION {
            let name =
                generate_session_name(provider.as_ref(), &model_config, id, &conversation).await?;
            return Ok(Some(self.system_generated_name_update(id, name).await?));
        }
        Ok(None)
    }

    pub async fn search_chat_history(
        &self,
        query: &str,
        limit: Option<usize>,
        after_date: Option<chrono::DateTime<chrono::Utc>>,
        before_date: Option<chrono::DateTime<chrono::Utc>>,
        exclude_session_id: Option<String>,
        session_types: Vec<SessionType>,
    ) -> Result<crate::session::chat_history_search::ChatRecallResults> {
        self.storage
            .search_chat_history(
                query,
                limit,
                after_date,
                before_date,
                exclude_session_id,
                session_types,
            )
            .await
    }

    pub async fn update_message_metadata<F>(id: &str, message_id: &str, f: F) -> Result<()>
    where
        F: FnOnce(
            crate::conversation::message::MessageMetadata,
        ) -> crate::conversation::message::MessageMetadata,
    {
        Self::instance()
            .storage
            .update_message_metadata(id, message_id, f)
            .await
    }

    /// Patch `tool_meta` on a specific `ToolRequest` within a stored message.
    /// Used to persist LLM-generated tool titles and chain summaries so they
    /// survive session reload. Merge-based: existing keys not in `patch` are
    /// preserved. No-op if the message or tool_call_id is not found.
    pub async fn update_tool_request_meta(
        &self,
        session_id: &str,
        message_id: &str,
        tool_call_id: &str,
        patch: serde_json::Value,
    ) -> Result<()> {
        self.storage
            .update_tool_request_meta(session_id, message_id, tool_call_id, patch)
            .await
    }

    /// Atomically merge a single extension's state (keyed as
    /// `"{extension_name}.{version}"`, see `ExtensionState`/`ExtensionData`)
    /// into the session's `extension_data`, leaving every other key
    /// untouched. Unlike `update(...).extension_data(...)`, which replaces
    /// the whole column from a snapshot the caller read earlier, this reads
    /// and writes inside a single `BEGIN IMMEDIATE` transaction so a
    /// concurrent writer touching a *different* key can never be silently
    /// clobbered (CON-001) — see `SessionStorage::merge_extension_state`.
    pub async fn merge_extension_state(
        &self,
        session_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        self.storage
            .merge_extension_state(session_id, key, value)
            .await
    }
}

pub struct SessionStorage {
    pool: Pool<Sqlite>,
    initialized: tokio::sync::OnceCell<()>,
    /// Queue SQLite writers before they acquire pooled connections. Otherwise
    /// a burst of `BEGIN IMMEDIATE` waiters can occupy the entire pool and
    /// starve unrelated ACP prompt-state reads and writes.
    write_gate: Arc<tokio::sync::Mutex<()>>,
    session_dir: PathBuf,
    owner_id: String,
    active_tool_operations: std::sync::Mutex<HashSet<String>>,
}

pub(crate) fn role_to_string(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

fn message_timestamp_to_datetime(timestamp: i64) -> Option<DateTime<Utc>> {
    let timestamp = if timestamp > MILLISECOND_TIMESTAMP_THRESHOLD {
        timestamp / 1000
    } else {
        timestamp
    };
    Utc.timestamp_opt(timestamp, 0).single()
}

fn normalized_message_timestamp_sql(column: &str) -> String {
    format!(
        "CASE WHEN {column} > {MILLISECOND_TIMESTAMP_THRESHOLD} THEN {column} / 1000 ELSE {column} END"
    )
}

fn session_sort_at(session: &Session) -> DateTime<Utc> {
    session.last_message_at.unwrap_or(session.updated_at)
}

impl Default for Session {
    fn default() -> Self {
        Self {
            id: String::new(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            additional_working_dirs: Vec::new(),
            restrict_tools_to_working_dirs: false,
            name: String::new(),
            user_set_name: false,
            session_type: SessionType::default(),
            created_at: Default::default(),
            updated_at: Default::default(),
            extension_data: ExtensionData::default(),
            usage: Usage::default(),
            accumulated_usage: Usage::default(),
            accumulated_cost: None,
            conversation: None,
            message_count: 0,
            last_message_at: None,
            provider_name: None,
            model_config: None,
            workspace_id: None,
            workspace_name: None,
            credential_profile_id: None,
            credential_profile_name: None,
            credential_binding_id: None,
            workspace_context: None,
            gosling_mode: GoslingMode::default(),
            archived_at: None,
            project_id: None,
            last_message_snippet: None,
        }
    }
}

impl Session {
    pub fn without_messages(mut self) -> Self {
        self.conversation = None;
        self
    }
}

impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for Session {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        let model_config_json: Option<String> = row.try_get("model_config_json").ok().flatten();
        let model_config = model_config_json.and_then(|json| serde_json::from_str(&json).ok());

        let name: String = {
            let name_val: String = row.try_get("name").unwrap_or_default();
            if !name_val.is_empty() {
                name_val
            } else {
                row.try_get("description").unwrap_or_default()
            }
        };

        let user_set_name = row.try_get("user_set_name").unwrap_or(false);

        let session_type_str: String = row
            .try_get("session_type")
            .unwrap_or_else(|_| "user".to_string());
        let session_type = session_type_str.parse().unwrap_or_default();

        let last_message_at = row
            .try_get::<Option<i64>, _>("last_message_timestamp")
            .ok()
            .flatten()
            .and_then(message_timestamp_to_datetime);

        let mut additional_working_dirs = row
            .try_get::<String, _>("additional_working_dirs_json")
            .ok()
            .and_then(|json| serde_json::from_str::<Vec<PathBuf>>(&json).ok())
            .unwrap_or_default();

        let restrict_tools_to_working_dirs = row
            .try_get("restrict_tools_to_working_dirs")
            .unwrap_or(false);
        let mut workspace_context: Option<WorkspaceSessionContext> = row
            .try_get::<Option<String>, _>("workspace_context_json")
            .ok()
            .flatten()
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;

        if let Some(context) = workspace_context.as_mut() {
            let policy = context.effective_folder_policy();
            let primary = PathBuf::from(&context.primary_working_folder);
            additional_working_dirs = policy
                .roots
                .iter()
                .map(|root| PathBuf::from(&root.path))
                .filter(|path| path != &primary)
                .collect();
            // `restrict_tools_to_working_dirs` comes from the stored column, which
            // defaults off (opt-in). Respecting it here (rather than forcing a value)
            // lets a user opt in per-chat while providers that manage their own tools
            // stay usable by default; the workspace folder-policy checks in
            // WorkingDirScopeInspector still apply because workspace_context is set.
            context.folder_policy = policy;
        }

        Ok(Session {
            id: row.try_get("id")?,
            working_dir: PathBuf::from(row.try_get::<String, _>("working_dir")?),
            additional_working_dirs,
            restrict_tools_to_working_dirs,
            name,
            user_set_name,
            session_type,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            extension_data: serde_json::from_str(&row.try_get::<String, _>("extension_data")?)
                .unwrap_or_default(),
            usage: Usage {
                input_tokens: row.try_get("input_tokens")?,
                output_tokens: row.try_get("output_tokens")?,
                total_tokens: row.try_get("total_tokens")?,
                cache_read_input_tokens: row.try_get("cache_read_tokens").ok().flatten(),
                cache_write_input_tokens: row.try_get("cache_write_tokens").ok().flatten(),
            },
            accumulated_usage: Usage {
                input_tokens: row.try_get("accumulated_input_tokens")?,
                output_tokens: row.try_get("accumulated_output_tokens")?,
                total_tokens: row.try_get("accumulated_total_tokens")?,
                cache_read_input_tokens: row
                    .try_get("accumulated_cache_read_tokens")
                    .ok()
                    .flatten(),
                cache_write_input_tokens: row
                    .try_get("accumulated_cache_write_tokens")
                    .ok()
                    .flatten(),
            },
            accumulated_cost: row.try_get("accumulated_cost").ok().flatten(),
            conversation: None,
            message_count: row.try_get("message_count").unwrap_or(0) as usize,
            last_message_at,
            provider_name: row.try_get("provider_name").ok().flatten(),
            model_config,
            workspace_id: row.try_get("workspace_id").ok().flatten(),
            workspace_name: row.try_get("workspace_name").ok().flatten(),
            credential_profile_id: row.try_get("credential_profile_id").ok().flatten(),
            credential_profile_name: row.try_get("credential_profile_name").ok().flatten(),
            credential_binding_id: row.try_get("credential_binding_id").ok().flatten(),
            workspace_context,
            gosling_mode: row
                .try_get::<String, _>("gosling_mode")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default(),
            archived_at: row.try_get("archived_at").ok(),
            project_id: row.try_get("project_id").ok().flatten(),
            last_message_snippet: None,
        })
    }
}

impl SessionStorage {
    async fn search_chat_history(
        &self,
        query: &str,
        limit: Option<usize>,
        after_date: Option<chrono::DateTime<chrono::Utc>>,
        before_date: Option<chrono::DateTime<chrono::Utc>>,
        exclude_session_id: Option<String>,
        session_types: Vec<SessionType>,
    ) -> Result<crate::session::chat_history_search::ChatRecallResults> {
        use crate::session::chat_history_search::ChatHistorySearch;

        let pool = self.pool().await?;
        ChatHistorySearch::new(
            pool,
            query,
            limit,
            after_date,
            before_date,
            exclude_session_id,
            session_types,
        )
        .execute()
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{Message, MessageContent};
    use crate::providers::base::MessageStream;
    use gosling_providers::conversation::token_usage::ProviderUsage;
    use gosling_providers::errors::ProviderError;
    use rmcp::model::Tool;
    use tempfile::TempDir;
    use test_case::test_case;

    const NUM_CONCURRENT_SESSIONS: i32 = 10;
    const GENERATED_SESSION_NAME: &str = "Generated session name";

    struct NamingTestProvider;

    #[async_trait::async_trait]
    impl Provider for NamingTestProvider {
        fn get_name(&self) -> &str {
            "naming-test"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[rmcp::model::Tool],
        ) -> std::result::Result<MessageStream, gosling_providers::errors::ProviderError> {
            unimplemented!("session naming calls complete")
        }

        async fn complete(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            Ok((
                Message::assistant().with_text(GENERATED_SESSION_NAME),
                ProviderUsage::new("test".to_string(), Default::default()),
            ))
        }
    }

    fn naming_test_provider() -> Arc<dyn Provider> {
        Arc::new(NamingTestProvider)
    }

    async fn create_session_for_list(
        sm: &SessionManager,
        working_dir: &str,
        has_message: bool,
    ) -> String {
        let session = sm
            .create_session(
                PathBuf::from(working_dir),
                format!("Session in {working_dir}"),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        if has_message {
            sm.add_message(&session.id, &Message::user().with_text("message"))
                .await
                .unwrap();
        }

        session.id
    }

    async fn create_session_for_list_with_message(
        sm: &SessionManager,
        working_dir: &str,
        message: &str,
    ) -> String {
        let session_id = create_session_for_list(sm, working_dir, false).await;
        sm.add_message(&session_id, &Message::user().with_text(message))
            .await
            .unwrap();
        session_id
    }

    async fn set_sessions_updated_at(
        sm: &SessionManager,
        session_ids: &[String],
        updated_at: &str,
    ) {
        let pool = sm.storage().pool().await.unwrap();
        let updated_at = chrono::DateTime::parse_from_rfc3339(updated_at).unwrap();
        let timestamp = updated_at.format("%Y-%m-%d %H:%M:%S").to_string();

        for session_id in session_ids {
            sqlx::query("UPDATE sessions SET updated_at = ? WHERE id = ?")
                .bind(&timestamp)
                .bind(session_id)
                .execute(pool)
                .await
                .unwrap();
        }
    }

    async fn add_message_at(sm: &SessionManager, session_id: &str, text: &str, timestamp: &str) {
        sm.add_message(session_id, &Message::user().with_text(text))
            .await
            .unwrap();

        let pool = sm.storage().pool().await.unwrap();
        let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp).unwrap();
        let timestamp_string = timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            "UPDATE messages SET timestamp = ?, created_timestamp = ? WHERE id = (SELECT MAX(id) FROM messages WHERE session_id = ?)",
        )
        .bind(&timestamp_string)
        .bind(timestamp.timestamp())
        .bind(session_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn add_message_at_millis(
        sm: &SessionManager,
        session_id: &str,
        text: &str,
        timestamp: &str,
    ) {
        sm.add_message(session_id, &Message::user().with_text(text))
            .await
            .unwrap();

        let pool = sm.storage().pool().await.unwrap();
        let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp).unwrap();
        let timestamp_string = timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            "UPDATE messages SET timestamp = ?, created_timestamp = ? WHERE id = (SELECT MAX(id) FROM messages WHERE session_id = ?)",
        )
        .bind(&timestamp_string)
        .bind(timestamp.timestamp_millis())
        .bind(session_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_message_timestamp(
        sm: &SessionManager,
        session_id: &str,
        message_id: &str,
        timestamp: &str,
    ) {
        let pool = sm.storage().pool().await.unwrap();
        let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp).unwrap();
        let timestamp_string = timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            "UPDATE messages SET timestamp = ?, created_timestamp = ? WHERE session_id = ? AND message_id = ?",
        )
        .bind(&timestamp_string)
        .bind(timestamp.timestamp())
        .bind(session_id)
        .bind(message_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn add_user_message(sm: &SessionManager, session_id: &str) {
        sm.add_message(session_id, &Message::user().with_text("hello world"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn healthy_succeeds_against_a_reachable_session_store() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        sm.healthy()
            .await
            .expect("a freshly created session store must report healthy");
    }

    #[tokio::test]
    async fn upsert_message_replaces_a_stream_checkpoint_in_place() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Streaming checkpoint".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        sm.upsert_message(
            &session.id,
            &Message::assistant()
                .with_id("stream-reply")
                .with_text("partial"),
        )
        .await
        .unwrap();
        sm.upsert_message(
            &session.id,
            &Message::assistant()
                .with_id("stream-reply")
                .with_text("partial response"),
        )
        .await
        .unwrap();

        let reloaded = sm.get_session(&session.id, true).await.unwrap();
        let messages = reloaded.conversation.unwrap();
        assert_eq!(reloaded.message_count, 1);
        assert_eq!(messages.messages().len(), 1);
        assert_eq!(messages.messages()[0].id.as_deref(), Some("stream-reply"));
        assert_eq!(messages.messages()[0].as_concat_text(), "partial response");
    }

    #[tokio::test]
    async fn tool_operation_ledger_prevents_redispatch_and_replays_terminal_result() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Tool ledger".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        const SECRET_SENTINEL: &str = "AUD031_LEDGER_SECRET_SENTINEL";
        let tool_call = rmcp::model::CallToolRequestParams::new("write_file")
            .with_arguments(rmcp::object!({ "path": "report.md", "content": SECRET_SENTINEL }));
        let missing_checkpoint = sm
            .begin_tool_operation(&session.id, "tool-request-1", &tool_call, true)
            .await
            .unwrap_err();
        assert!(missing_checkpoint
            .to_string()
            .contains("must be durably checkpointed"));
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("tool-request-1", Ok(tool_call.clone())),
        )
        .await
        .unwrap();

        let operation_id = match sm
            .begin_tool_operation(&session.id, "tool-request-1", &tool_call, true)
            .await
            .unwrap()
        {
            ToolOperationStart::Execute { operation_id } => operation_id,
            other => panic!("new operation should execute, got {other:?}"),
        };
        let persisted_identity = sqlx::query_as::<_, (String, String)>(
            "SELECT tool_name, request_digest FROM tool_operations WHERE operation_id = ?",
        )
        .bind(&operation_id)
        .fetch_one(sm.storage().pool().await.unwrap())
        .await
        .unwrap();
        assert!(!format!("{persisted_identity:?}").contains(SECRET_SENTINEL));

        assert!(matches!(
            sm.begin_tool_operation(&session.id, "tool-request-1", &tool_call, true)
                .await
                .unwrap(),
            ToolOperationStart::InDoubt { .. }
        ));

        let terminal_result = Ok(rmcp::model::CallToolResult::success(vec![
            rmcp::model::Content::text("written"),
        ]));
        sm.complete_tool_operation(&operation_id, &terminal_result)
            .await
            .unwrap();

        match sm
            .begin_tool_operation(&session.id, "tool-request-1", &tool_call, true)
            .await
            .unwrap()
        {
            ToolOperationStart::Replay { result, .. } => assert_eq!(result, terminal_result),
            other => panic!("completed operation should replay, got {other:?}"),
        }

        let changed_call = rmcp::model::CallToolRequestParams::new("write_file")
            .with_arguments(rmcp::object!({ "path": "other.md", "content": SECRET_SENTINEL }));
        let collision = sm
            .begin_tool_operation(&session.id, "tool-request-1", &changed_call, true)
            .await
            .unwrap_err();
        assert!(collision.to_string().contains("different tool payload"));
    }

    fn summary_fixture(
        status: SessionSummaryStatus,
        covered_through_row_id: i64,
    ) -> SessionSummary {
        SessionSummary {
            session_id: "s".to_string(),
            summary: "earlier work".to_string(),
            covered_through_row_id,
            covered_through_timestamp: 0,
            covered_message_count: 10,
            source_hash: "hash".to_string(),
            summarizer_model: None,
            status,
            error: None,
            updated_at: Utc::now(),
        }
    }

    fn page_fixture(oldest_row_id: Option<i64>) -> SessionMessagePage {
        SessionMessagePage {
            messages: Vec::new(),
            next_before_cursor: None,
            total_count: 100,
            oldest_row_id,
            newest_row_id: Some(100),
        }
    }

    // Resume injected any non-empty summary regardless of status or reach
    // (DAT-GSL-001).
    #[test]
    fn a_stale_or_failed_summary_is_not_presented_as_coverage() {
        let page = page_fixture(Some(51));
        for status in [SessionSummaryStatus::Stale, SessionSummaryStatus::Failed] {
            assert!(!summary_covers_history_before(
                &summary_fixture(status, 50),
                &page
            ));
        }
    }

    #[test]
    fn a_current_summary_that_reaches_the_tail_is_accepted() {
        assert!(summary_covers_history_before(
            &summary_fixture(SessionSummaryStatus::Current, 50),
            &page_fixture(Some(51))
        ));
    }

    // The damaging case: the session grew after the summary was written, so
    // rows between the summary and the tail are in neither.
    #[test]
    fn a_current_summary_that_leaves_a_gap_before_the_tail_is_rejected() {
        assert!(!summary_covers_history_before(
            &summary_fixture(SessionSummaryStatus::Current, 50),
            &page_fixture(Some(400))
        ));
    }

    #[test]
    fn an_empty_tail_cannot_have_a_gap() {
        assert!(summary_covers_history_before(
            &summary_fixture(SessionSummaryStatus::Current, 50),
            &page_fixture(None)
        ));
    }

    #[tokio::test]
    async fn interrupted_tool_operation_recovers_as_visible_in_doubt_response() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Interrupted tool".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let tool_call = rmcp::model::CallToolRequestParams::new("send_email")
            .with_arguments(rmcp::object!({ "recipient": "person@example.com" }));
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("tool-request-2", Ok(tool_call.clone())),
        )
        .await
        .unwrap();
        assert!(matches!(
            sm.begin_tool_operation(&session.id, "tool-request-2", &tool_call, true)
                .await
                .unwrap(),
            ToolOperationStart::Execute { .. }
        ));
        assert_eq!(sm.recover_tool_operations(&session.id).await.unwrap(), 0);

        // Simulate the owning process actually having crashed: an in-process
        // `SessionManager` restart alone would still carry this test's own,
        // very-much-alive PID (CON-GSL-001) and must NOT be recoverable —
        // see `live_peer_tool_operation_survives_a_concurrent_recover` below.
        // Overwrite the dispatch PID with one guaranteed not to exist so this
        // test exercises the crash path it is named for.
        sqlx::query("UPDATE tool_operations SET owner_pid = ? WHERE session_id = ?")
            .bind(i32::MAX as i64)
            .bind(&session.id)
            .execute(sm.storage.pool().await.unwrap())
            .await
            .unwrap();

        let restarted = SessionManager::new(temp_dir.path().to_path_buf());
        assert_eq!(
            restarted
                .recover_tool_operations(&session.id)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            restarted
                .recover_tool_operations(&session.id)
                .await
                .unwrap(),
            0
        );

        let reloaded = restarted.get_session(&session.id, true).await.unwrap();
        let messages = reloaded.conversation.unwrap();
        let responses = messages
            .messages()
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(MessageContent::as_tool_response)
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 1);
        let error = responses[0]
            .tool_result
            .as_ref()
            .expect_err("interrupted operation should recover as an error");
        assert!(error.message.contains("execution status is in doubt"));
        assert!(error.message.contains("must not be retried automatically"));

        assert!(matches!(
            restarted
                .begin_tool_operation(&session.id, "tool-request-2", &tool_call, true)
                .await
                .unwrap(),
            ToolOperationStart::InDoubt { .. }
        ));
    }

    #[tokio::test]
    async fn live_peer_tool_operation_survives_a_concurrent_recover() {
        // CON-GSL-001: a second `SessionManager` on the same DB (the
        // CLI+desktop topology, since both share the default session dirs)
        // used to be indistinguishable from a crashed owner -- `owner_id` is
        // only a per-instance UUID. Recovering here shares this test's own
        // OS process, so it must be treated as a live owner and left alone,
        // not stomped into `in_doubt` mid-execution.
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Live peer".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let tool_call = rmcp::model::CallToolRequestParams::new("send_email")
            .with_arguments(rmcp::object!({ "recipient": "person@example.com" }));
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("tool-request-live", Ok(tool_call.clone())),
        )
        .await
        .unwrap();
        let operation_id = match sm
            .begin_tool_operation(&session.id, "tool-request-live", &tool_call, true)
            .await
            .unwrap()
        {
            ToolOperationStart::Execute { operation_id } => operation_id,
            other => panic!("new operation should execute, got {other:?}"),
        };

        // A tool call only ever runs inside a turn, and a turn always holds the
        // session's turn lease. Hold one here so the fixture matches what a
        // genuinely in-flight operation looks like on disk: since REL-GSL-006 a
        // live PID alone is not enough to protect a `started` row, because a
        // process can outlive the turn that dispatched the tool.
        let owner_lease = sm
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap();

        let peer = SessionManager::new(temp_dir.path().to_path_buf());
        assert_eq!(
            peer.recover_tool_operations(&session.id).await.unwrap(),
            0,
            "a live owner's in-flight tool must not be recovered by a peer"
        );

        let reloaded = peer.get_session(&session.id, true).await.unwrap();
        let conversation = reloaded.conversation.unwrap();
        assert!(
            conversation
                .messages()
                .iter()
                .flat_map(|message| message.content.iter())
                .all(|content| !matches!(content, MessageContent::ToolResponse(_))),
            "no synthetic in_doubt response should have been written"
        );

        // The real owner can still complete the call normally afterward.
        owner_lease.release().await.unwrap();
        let terminal_result = Ok(rmcp::model::CallToolResult::success(vec![
            rmcp::model::Content::text("sent"),
        ]));
        sm.complete_tool_operation(&operation_id, &terminal_result)
            .await
            .unwrap();
        let mut response = Message::user().with_generated_id();
        response.add_tool_response_with_metadata("tool-request-live", terminal_result, None);
        sm.persist_tool_operation_response(&session.id, "tool-request-live", &response)
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, true).await.unwrap();
        let conversation = reloaded.conversation.unwrap();
        let responses = conversation
            .messages()
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(MessageContent::as_tool_response)
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 1);
        assert!(responses[0].tool_result.as_ref().is_ok());
    }

    #[tokio::test]
    async fn cancelling_tool_request_cancels_undispatched_siblings_once() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Partially dispatched tools".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request(
                    "tool-request-completed",
                    Ok(rmcp::model::CallToolRequestParams::new("read_file")),
                )
                .with_tool_request(
                    "tool-request-undispatched",
                    Ok(rmcp::model::CallToolRequestParams::new("write_file")),
                ),
        )
        .await
        .unwrap();
        let mut completed_response = Message::user().with_generated_id();
        completed_response.add_tool_response_with_metadata(
            "tool-request-completed",
            Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::Content::text("read"),
            ])),
            None,
        );
        sm.add_message(&session.id, &completed_response)
            .await
            .unwrap();

        assert_eq!(
            sm.cancel_undispatched_tool_requests(&session.id, "tool-request-completed")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sm.cancel_undispatched_tool_requests(&session.id, "tool-request-completed")
                .await
                .unwrap(),
            0
        );
        assert_eq!(sm.recover_tool_operations(&session.id).await.unwrap(), 0);

        let reloaded = sm.get_session(&session.id, true).await.unwrap();
        let conversation = reloaded.conversation.unwrap();
        let responses = conversation
            .messages()
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(MessageContent::as_tool_response)
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 2);
        let undispatched = responses
            .iter()
            .find(|response| response.id == "tool-request-undispatched")
            .expect("undispatched request should receive a terminal response");
        let error = undispatched
            .tool_result
            .as_ref()
            .expect_err("undispatched request should be cancelled");
        assert!(error.message.contains("before it started"));
        assert!(error.message.contains("will not be retried automatically"));
    }

    #[tokio::test]
    async fn generic_recovery_does_not_cancel_a_pending_approval() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Pending approval".to_string(),
                SessionType::User,
                GoslingMode::Approve,
            )
            .await
            .unwrap();
        sm.add_message(
            &session.id,
            &Message::assistant().with_generated_id().with_tool_request(
                "tool-request-pending",
                Ok(rmcp::model::CallToolRequestParams::new("write_file")),
            ),
        )
        .await
        .unwrap();

        assert_eq!(sm.recover_tool_operations(&session.id).await.unwrap(), 0);
        let conversation = sm
            .get_session(&session.id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert!(conversation
            .messages()
            .iter()
            .flat_map(|message| message.content.iter())
            .all(|content| !matches!(content, MessageContent::ToolResponse(response) if response.id == "tool-request-pending")));
    }

    #[tokio::test]
    async fn abandoned_in_process_tool_operation_is_recoverable_without_redispatch() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Cancelled tool".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let tool_call = rmcp::model::CallToolRequestParams::new("publish_report");
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("tool-request-cancelled", Ok(tool_call.clone())),
        )
        .await
        .unwrap();
        let operation_id = match sm
            .begin_tool_operation(&session.id, "tool-request-cancelled", &tool_call, true)
            .await
            .unwrap()
        {
            ToolOperationStart::Execute { operation_id } => operation_id,
            other => panic!("new operation should execute, got {other:?}"),
        };

        sm.release_tool_operation(&operation_id);
        assert_eq!(sm.recover_tool_operations(&session.id).await.unwrap(), 1);
        assert!(matches!(
            sm.begin_tool_operation(&session.id, "tool-request-cancelled", &tool_call, true,)
                .await
                .unwrap(),
            ToolOperationStart::InDoubt { .. }
        ));
    }

    #[tokio::test]
    async fn completed_tool_operation_recovers_response_without_redispatch() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Completed tool".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let tool_call = rmcp::model::CallToolRequestParams::new("create_document");
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("tool-request-3", Ok(tool_call.clone())),
        )
        .await
        .unwrap();
        let operation_id = match sm
            .begin_tool_operation(&session.id, "tool-request-3", &tool_call, true)
            .await
            .unwrap()
        {
            ToolOperationStart::Execute { operation_id } => operation_id,
            other => panic!("new operation should execute, got {other:?}"),
        };
        let terminal_result = Ok(rmcp::model::CallToolResult::success(vec![
            rmcp::model::Content::text("created"),
        ]));
        sm.complete_tool_operation(&operation_id, &terminal_result)
            .await
            .unwrap();

        let restarted = SessionManager::new(temp_dir.path().to_path_buf());
        assert_eq!(
            restarted
                .recover_tool_operations(&session.id)
                .await
                .unwrap(),
            1
        );
        let reloaded = restarted.get_session(&session.id, true).await.unwrap();
        let conversation = reloaded.conversation.unwrap();
        let response = conversation
            .messages()
            .iter()
            .flat_map(|message| message.content.iter())
            .find_map(MessageContent::as_tool_response)
            .expect("completed result should be restored to conversation");
        assert_eq!(response.tool_result, terminal_result);
    }

    #[tokio::test]
    async fn terminal_tool_result_and_conversation_response_are_persisted_idempotently() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Terminal tool response".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let tool_call = rmcp::model::CallToolRequestParams::new("export_file");
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("tool-request-4", Ok(tool_call.clone())),
        )
        .await
        .unwrap();
        let operation_id = match sm
            .begin_tool_operation(&session.id, "tool-request-4", &tool_call, true)
            .await
            .unwrap()
        {
            ToolOperationStart::Execute { operation_id } => operation_id,
            other => panic!("new operation should execute, got {other:?}"),
        };
        let terminal_result = Ok(rmcp::model::CallToolResult::success(vec![
            rmcp::model::Content::text("exported"),
        ]));
        sm.complete_tool_operation(&operation_id, &terminal_result)
            .await
            .unwrap();
        let mut response = Message::user().with_id("tool-response-4");
        response.add_tool_response_with_metadata("tool-request-4", terminal_result, None);

        sm.persist_tool_operation_response(&session.id, "tool-request-4", &response)
            .await
            .unwrap();
        sm.persist_tool_operation_response(&session.id, "tool-request-4", &response)
            .await
            .unwrap();

        let restarted = SessionManager::new(temp_dir.path().to_path_buf());
        assert_eq!(
            restarted
                .recover_tool_operations(&session.id)
                .await
                .unwrap(),
            0
        );
        let reloaded = restarted.get_session(&session.id, true).await.unwrap();
        let response_count = reloaded
            .conversation
            .unwrap()
            .messages()
            .iter()
            .flat_map(|message| message.content.iter())
            .filter(|content| {
                matches!(content, MessageContent::ToolResponse(response) if response.id == "tool-request-4")
            })
            .count();
        assert_eq!(response_count, 1);
    }

    #[tokio::test]
    async fn test_last_message_at_is_derived_from_messages() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Session recency".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let empty = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(empty.message_count, 0);
        assert_eq!(empty.last_message_at, None);

        add_message_at_millis(&sm, &session.id, "older", "2026-01-01T00:00:00Z").await;
        add_message_at(&sm, &session.id, "newer", "2026-01-02T03:04:05Z").await;

        let expected = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
            .unwrap()
            .with_timezone(&Utc);

        let without_messages = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(without_messages.message_count, 2);
        assert_eq!(without_messages.last_message_at, Some(expected));

        let with_messages = sm.get_session(&session.id, true).await.unwrap();
        assert_eq!(with_messages.message_count, 2);
        assert_eq!(with_messages.last_message_at, Some(expected));
    }

    #[tokio::test]
    async fn test_truncate_conversation_from_message_keeps_same_second_previous_rows() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "Same second truncation".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let timestamp = "2026-06-23T12:00:00Z";
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_text("assistant reply")
                .with_id("assistant"),
        )
        .await
        .unwrap();
        set_message_timestamp(&sm, &session.id, "assistant", timestamp).await;

        sm.add_message(
            &session.id,
            &Message::user()
                .with_text("terminal history")
                .with_id("terminal-history"),
        )
        .await
        .unwrap();
        set_message_timestamp(&sm, &session.id, "terminal-history", timestamp).await;

        sm.add_message(
            &session.id,
            &Message::user()
                .with_text("next prompt")
                .with_id("next-prompt"),
        )
        .await
        .unwrap();
        set_message_timestamp(&sm, &session.id, "next-prompt", timestamp).await;

        sm.truncate_conversation_from_message(&session.id, "terminal-history")
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, true).await.unwrap();
        let messages = reloaded.conversation.unwrap().messages().to_vec();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id.as_deref(), Some("assistant"));
        assert_eq!(messages[0].as_concat_text(), "assistant reply");
    }

    #[tokio::test]
    async fn test_maybe_update_name_updates_eligible_session() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "New Chat".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        // Persist a model config so naming doesn't fall back to the
        // globally-configured model, which doesn't exist in test environments.
        sm.update(&session.id)
            .model_config(ModelConfig::new("test-model"))
            .apply()
            .await
            .unwrap();

        add_user_message(&sm, &session.id).await;

        let update = sm
            .maybe_update_name(&session.id, naming_test_provider())
            .await
            .unwrap();
        assert_eq!(
            update.as_ref().map(|update| update.name.as_str()),
            Some(GENERATED_SESSION_NAME)
        );

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.name, GENERATED_SESSION_NAME);
        assert!(!reloaded.user_set_name);
    }

    #[tokio::test]
    async fn test_maybe_update_name_preserves_user_renamed_session() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "New Chat".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        sm.update(&session.id)
            .user_provided_name("Manual title".to_string())
            .apply()
            .await
            .unwrap();
        add_user_message(&sm, &session.id).await;

        let update = sm
            .maybe_update_name(&session.id, naming_test_provider())
            .await
            .unwrap();
        assert!(update.is_none());

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.name, "Manual title");
        assert!(reloaded.user_set_name);
    }

    #[tokio::test]
    async fn test_system_generated_name_does_not_clobber_concurrent_user_rename() {
        // Simulates the actual race: a background auto-naming write that
        // started before the user renamed the session, but whose apply()
        // lands after the user's rename already committed. Regression test
        // for the TOCTOU in apply_update's system_generated_name path -
        // previously this unconditionally overwrote the name and reset
        // user_set_name back to false.
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "New Chat".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        // User renames first...
        sm.update(&session.id)
            .user_provided_name("Manual title".to_string())
            .apply()
            .await
            .unwrap();

        // ...then the stale background auto-name write lands.
        sm.update(&session.id)
            .system_generated_name("Auto-generated title".to_string())
            .apply()
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.name, "Manual title");
        assert!(reloaded.user_set_name);
    }

    #[tokio::test]
    async fn test_merge_extension_state_concurrent_writers_do_not_clobber_each_other() {
        // CON-001 regression: two different extensions (or, concretely, an
        // LRU-evicted-but-still-running agent and the freshly re-created
        // agent that replaced it) writing *different* extension_data keys
        // for the same session at the same time must not lose either
        // write. The old `get_session` -> mutate -> `update().extension_data()`
        // pattern raced two independent DB round trips; `merge_extension_state`
        // does the read and write inside one `BEGIN IMMEDIATE` transaction so
        // SQLite serializes the two callers instead.
        let temp_dir = TempDir::new().unwrap();
        let sm = std::sync::Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "New Chat".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let mut handles = Vec::new();
        for i in 0..20 {
            let sm = std::sync::Arc::clone(&sm);
            let session_id = session.id.clone();
            handles.push(tokio::spawn(async move {
                sm.merge_extension_state(
                    &session_id,
                    &format!("ext_{i}.v0"),
                    serde_json::json!({ "value": i }),
                )
                .await
            }));
        }

        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        for i in 0..20 {
            let key = format!("ext_{i}.v0");
            assert_eq!(
                reloaded.extension_data.extension_states.get(&key),
                Some(&serde_json::json!({ "value": i })),
                "key {key} must survive concurrent merges to other keys"
            );
        }
    }

    #[tokio::test]
    async fn test_queued_writers_do_not_exhaust_the_connection_pool() {
        let temp_dir = TempDir::new().unwrap();
        let sm = std::sync::Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "New Chat".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let write_guard = sm.storage.acquire_write_guard().await;
        let pool = sm.storage.pool().await.unwrap();
        let blocking_transaction = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(21));
        let mut handles = Vec::new();

        for i in 0..20 {
            let sm = std::sync::Arc::clone(&sm);
            let session_id = session.id.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                sm.merge_extension_state(
                    &session_id,
                    &format!("pool_{i}.v0"),
                    serde_json::json!({ "value": i }),
                )
                .await
            }));
        }

        barrier.wait().await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tokio::time::timeout(std::time::Duration::from_secs(1), sm.healthy())
            .await
            .expect("queued writers must not consume every pool connection")
            .unwrap();

        blocking_transaction.rollback().await.unwrap();
        drop(write_guard);
        for handle in handles {
            handle.await.unwrap().unwrap();
        }
    }

    #[tokio::test]
    async fn test_merge_extension_state_missing_session_errors() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let result = sm
            .merge_extension_state("does-not-exist", "todo.v0", serde_json::json!({}))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Session not found"));
    }

    #[tokio::test]
    async fn test_maybe_update_name_preserves_scheduled_session() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let original_name = "Scheduled job: test-job";

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                original_name.to_string(),
                SessionType::Scheduled,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        add_user_message(&sm, &session.id).await;

        let update = sm
            .maybe_update_name(&session.id, naming_test_provider())
            .await
            .unwrap();
        assert!(update.is_none());

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.name, original_name);
        assert!(!reloaded.user_set_name);
    }

    async fn create_search_session(
        sm: &SessionManager,
        name: &str,
        session_type: SessionType,
        updated_at: &str,
        messages: &[(&str, &str)],
    ) -> String {
        let session = sm
            .create_session(
                PathBuf::from("/tmp/search-test"),
                name.to_string(),
                session_type,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        for (text, timestamp) in messages {
            add_message_at(sm, &session.id, text, timestamp).await;
        }
        set_sessions_updated_at(sm, std::slice::from_ref(&session.id), updated_at).await;

        session.id
    }

    #[tokio::test]
    async fn test_search_chat_history_uses_relevance_ranked_text_projection() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let _older_target = create_search_session(
            &sm,
            "Older target",
            SessionType::User,
            "2026-05-01T00:00:00Z",
            &[
                (
                    "does Acme have an email address for John Doe",
                    "2026-05-01T00:00:00Z",
                ),
                ("follow-up without search terms", "2026-05-01T00:01:00Z"),
            ],
        )
        .await;

        let _newer_noise = create_search_session(
            &sm,
            "Newer noise",
            SessionType::User,
            "2026-05-22T00:00:00Z",
            &[
                ("Acme person name looking for Acme", "2026-05-22T00:00:00Z"),
                (
                    "another Acme person name looking for Acme",
                    "2026-05-22T00:01:00Z",
                ),
            ],
        )
        .await;

        let results = sm
            .search_chat_history(
                "Acme John Doe",
                Some(2),
                None,
                None,
                None,
                vec![SessionType::User],
            )
            .await
            .unwrap();

        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].session_id, _older_target);
        assert_eq!(results.results[0].messages.len(), 1);
        assert_eq!(results.results[0].total_messages_in_session, 2);
        assert_eq!(results.results[0].messages[0].role, "user");
        assert!(results.results[0].messages[0].message_id.is_some());
        assert!(results.results[0].messages[0].content.contains("Acme"));
    }

    #[tokio::test]
    async fn chat_recall_message_window_hydrates_only_neighboring_messages() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/search-test"),
                "Window target".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        for text in [
            "before one",
            "before two",
            "recall needle",
            "after one",
            "after two",
        ] {
            sm.add_message(&session.id, &Message::user().with_text(text))
                .await
                .unwrap();
        }

        let hit = sm
            .search_chat_history("needle", Some(1), None, None, None, vec![SessionType::User])
            .await
            .unwrap()
            .results
            .pop()
            .unwrap()
            .messages
            .pop()
            .unwrap();
        let window = sm
            .get_session_message_window(&session.id, hit.message_id.as_deref().unwrap(), 1, 1)
            .await
            .unwrap();
        let texts: Vec<&str> = window
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(MessageContent::as_text)
            .collect();
        assert_eq!(texts, ["before two", "recall needle", "after one"]);
    }

    #[tokio::test]
    async fn migration_25_backfills_existing_messages_into_chat_recall_index() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/search-test"),
                "Migration target".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        sm.add_message(
            &session.id,
            &Message::user().with_text("unique migration recall needle"),
        )
        .await
        .unwrap();
        drop(sm);

        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        let pool = SqlitePoolOptions::new()
            .connect_with(SqliteConnectOptions::new().filename(&db_path))
            .await
            .unwrap();
        sqlx::query("DROP TRIGGER messages_search_after_insert")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TRIGGER messages_search_after_delete")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TRIGGER messages_search_after_update")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE message_search")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE schema_version SET version = 24")
            .execute(&pool)
            .await
            .unwrap();
        drop(pool);

        let upgraded = SessionManager::new(temp_dir.path().to_path_buf());
        let results = upgraded
            .search_chat_history(
                "unique migration needle",
                Some(1),
                None,
                None,
                None,
                vec![SessionType::User],
            )
            .await
            .unwrap();
        assert_eq!(results.total_matches, 1);
        assert_eq!(results.results[0].session_id, session.id);
    }

    async fn expected_session_list_ids(sm: &SessionManager, session_ids: &[String]) -> Vec<String> {
        let mut sessions = Vec::new();
        for session_id in session_ids {
            sessions.push(sm.get_session(session_id, false).await.unwrap());
        }
        sessions.sort_by(|a, b| {
            session_sort_at(b)
                .cmp(&session_sort_at(a))
                .then_with(|| b.id.cmp(&a.id))
        });
        sessions.into_iter().map(|session| session.id).collect()
    }

    async fn assert_session_list_page(
        sm: &SessionManager,
        cursor: Option<&SessionListCursor>,
        working_dir: Option<&str>,
        page_size: usize,
        expected_ids: &[String],
        expected_next_cursor: bool,
    ) -> Option<SessionListCursor> {
        let types = [SessionType::User];
        let page = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    working_dir: working_dir.map(Path::new),
                    only_sessions_with_messages: true,
                    ..Default::default()
                },
                cursor,
                page_size,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        let ids = page
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ids.as_slice(), expected_ids);
        assert_eq!(page.next_cursor.is_some(), expected_next_cursor);
        page.next_cursor
    }

    async fn run_lock_upgrade_attempt(
        pool: Pool<Sqlite>,
        session_id: String,
        begin_statement: &'static str,
        worker_id: i32,
        barrier: Option<Arc<tokio::sync::Barrier>>,
    ) -> anyhow::Result<()> {
        let mut tx = pool.begin_with(begin_statement).await?;

        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE id = ?")
            .bind(&session_id)
            .fetch_one(&mut *tx)
            .await?;

        if let Some(barrier) = barrier {
            barrier.wait().await;
        }

        sqlx::query("UPDATE sessions SET total_tokens = ? WHERE id = ?")
            .bind(worker_id)
            .bind(&session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn run_lock_upgrade_race(
        pool: Pool<Sqlite>,
        session_id: String,
        begin_statement: &'static str,
        use_barrier: bool,
    ) -> Vec<anyhow::Result<()>> {
        let barrier = if use_barrier {
            Some(Arc::new(tokio::sync::Barrier::new(2)))
        } else {
            None
        };
        let mut handles = Vec::new();

        for worker_id in 0..2 {
            let pool = pool.clone();
            let session_id = session_id.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                run_lock_upgrade_attempt(pool, session_id, begin_statement, worker_id, barrier)
                    .await
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.expect("lock-upgrade task panicked"));
        }
        results
    }

    #[tokio::test]
    async fn test_begin_immediate_prevents_lock_upgrade_deadlock() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = SessionManager::new(temp_dir.path().to_path_buf());

        let session = session_manager
            .create_session(
                PathBuf::from("/tmp/lock-upgrade-test"),
                "Lock Upgrade Session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let pool = session_manager.storage().pool.clone();

        let results = run_lock_upgrade_race(pool.clone(), session.id.clone(), "BEGIN", true).await;
        assert!(
            results.iter().any(Result::is_err),
            "BEGIN (DEFERRED) should cause SQLITE_BUSY when two tasks try to upgrade SHARED → RESERVED"
        );

        let results = run_lock_upgrade_race(pool, session.id, "BEGIN IMMEDIATE", false).await;
        assert!(
            results.iter().all(Result::is_ok),
            "BEGIN IMMEDIATE should serialize contention without SQLITE_BUSY: {:?}",
            results
                .iter()
                .filter_map(|r| r.as_ref().err().map(ToString::to_string))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_session_list_paged_first_second_and_final_page() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let mut expected_ids = Vec::new();
        for _ in 0..5 {
            expected_ids.push(create_session_for_list(&sm, "/tmp/session-list", true).await);
        }
        let expected_ids = expected_session_list_ids(&sm, &expected_ids).await;

        let cursor = assert_session_list_page(&sm, None, None, 2, &expected_ids[0..2], true).await;
        let cursor =
            assert_session_list_page(&sm, cursor.as_ref(), None, 2, &expected_ids[2..4], true)
                .await;
        assert_session_list_page(&sm, cursor.as_ref(), None, 2, &expected_ids[4..5], false).await;
    }

    #[tokio::test]
    async fn test_session_list_paged_sorts_by_last_message_at() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let stale_but_modified = create_session_for_list(&sm, "/tmp/session-list", false).await;
        add_message_at(
            &sm,
            &stale_but_modified,
            "older message",
            "2026-01-01T00:00:00Z",
        )
        .await;
        set_sessions_updated_at(
            &sm,
            std::slice::from_ref(&stale_but_modified),
            "2026-02-01T00:00:00Z",
        )
        .await;

        let active_but_not_modified =
            create_session_for_list(&sm, "/tmp/session-list", false).await;
        add_message_at(
            &sm,
            &active_but_not_modified,
            "newer message",
            "2026-01-02T00:00:00Z",
        )
        .await;
        set_sessions_updated_at(
            &sm,
            std::slice::from_ref(&active_but_not_modified),
            "2026-01-15T00:00:00Z",
        )
        .await;

        assert_session_list_page(
            &sm,
            None,
            None,
            2,
            &[active_but_not_modified, stale_but_modified],
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn test_session_list_paged_uses_id_tiebreaker_for_duplicate_activity_time() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let mut expected_ids = Vec::new();
        for _ in 0..3 {
            expected_ids.push(create_session_for_list(&sm, "/tmp/session-list", true).await);
        }
        set_sessions_updated_at(&sm, &expected_ids, "2024-01-01T00:00:00Z").await;
        let expected_ids = expected_session_list_ids(&sm, &expected_ids).await;

        let cursor = assert_session_list_page(&sm, None, None, 2, &expected_ids[0..2], true).await;
        assert_session_list_page(&sm, cursor.as_ref(), None, 2, &expected_ids[2..3], false).await;
    }

    #[tokio::test]
    async fn test_session_list_paged_filters_empty_and_cwd_before_pagination() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let expected_ids = vec![
            create_session_for_list(&sm, "/tmp/session-list/a", true).await,
            create_session_for_list(&sm, "/tmp/session-list/a", true).await,
        ];
        create_session_for_list(&sm, "/tmp/session-list/a", false).await;
        create_session_for_list(&sm, "/tmp/session-list/b", true).await;
        let expected_ids = expected_session_list_ids(&sm, &expected_ids).await;

        let cursor = assert_session_list_page(
            &sm,
            None,
            Some("/tmp/session-list/a"),
            1,
            &expected_ids[0..1],
            true,
        )
        .await;
        assert_session_list_page(
            &sm,
            cursor.as_ref(),
            Some("/tmp/session-list/a"),
            1,
            &expected_ids[1..2],
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn test_session_list_paged_filters_by_keyword() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let target = create_session_for_list_with_message(
            &sm,
            "/tmp/session-list",
            "Discuss Postgres migrations",
        )
        .await;
        create_session_for_list_with_message(&sm, "/tmp/session-list", "Plan the mobile release")
            .await;

        let types = [SessionType::User];
        let page = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    keyword: Some("postgres"),
                    only_sessions_with_messages: true,
                    ..Default::default()
                },
                cursor: None,
                page_size: 10,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        let ids = page
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![target]);
        assert!(page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn test_session_list_paged_keyword_uses_or_terms() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let postgres = create_session_for_list_with_message(
            &sm,
            "/tmp/session-list",
            "Postgres migration plan",
        )
        .await;
        let sqlite =
            create_session_for_list_with_message(&sm, "/tmp/session-list", "SQLite backup notes")
                .await;
        create_session_for_list_with_message(&sm, "/tmp/session-list", "Mobile release notes")
            .await;
        let expected_ids = expected_session_list_ids(&sm, &[postgres, sqlite]).await;

        let types = [SessionType::User];
        let page = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    keyword: Some("postgres sqlite"),
                    only_sessions_with_messages: true,
                    ..Default::default()
                },
                cursor: None,
                page_size: 10,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        let ids = page
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();

        assert_eq!(ids, expected_ids);
        assert!(page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn test_session_list_paged_empty_keyword_matches_plain_list() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let expected_ids = vec![
            create_session_for_list_with_message(&sm, "/tmp/session-list", "first message").await,
            create_session_for_list_with_message(&sm, "/tmp/session-list", "second message").await,
        ];
        let expected_ids = expected_session_list_ids(&sm, &expected_ids).await;

        let types = [SessionType::User];
        let page = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    keyword: Some("   "),
                    only_sessions_with_messages: true,
                    ..Default::default()
                },
                cursor: None,
                page_size: 10,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        let ids = page
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();

        assert_eq!(ids, expected_ids);
    }

    #[tokio::test]
    async fn test_session_list_paged_keyword_treats_like_wildcards_as_literals() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let percent_id =
            create_session_for_list_with_message(&sm, "/tmp/session-list", "Deploy is 100% done")
                .await;
        let underscore_id = create_session_for_list_with_message(
            &sm,
            "/tmp/session-list",
            "feature_flag is enabled",
        )
        .await;
        create_session_for_list_with_message(&sm, "/tmp/session-list", "plain message").await;

        let types = [SessionType::User];
        let percent_page = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    keyword: Some("%"),
                    only_sessions_with_messages: true,
                    ..Default::default()
                },
                cursor: None,
                page_size: 10,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        let percent_ids = percent_page
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(percent_ids, vec![percent_id]);

        let underscore_page = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    keyword: Some("_"),
                    only_sessions_with_messages: true,
                    ..Default::default()
                },
                cursor: None,
                page_size: 10,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        let underscore_ids = underscore_page
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(underscore_ids, vec![underscore_id]);
    }

    #[tokio::test]
    async fn test_session_list_paged_keyword_combines_with_cwd_and_pagination() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let expected_ids = vec![
            create_session_for_list_with_message(&sm, "/tmp/session-list/a", "Postgres plan one")
                .await,
            create_session_for_list_with_message(&sm, "/tmp/session-list/a", "Postgres plan two")
                .await,
        ];
        create_session_for_list_with_message(&sm, "/tmp/session-list/a", "Mobile release").await;
        create_session_for_list_with_message(&sm, "/tmp/session-list/b", "Postgres plan other")
            .await;
        let expected_ids = expected_session_list_ids(&sm, &expected_ids).await;

        let types = [SessionType::User];
        let filters = SessionListFilters {
            types: Some(&types),
            working_dir: Some(Path::new("/tmp/session-list/a")),
            keyword: Some("postgres"),
            only_sessions_with_messages: true,
            archive_state: SessionArchiveState::Active,
            ..Default::default()
        };
        let cursor = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: filters.clone(),
                cursor: None,
                page_size: 1,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        let ids = cursor
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ids, expected_ids[0..1]);
        assert!(cursor.next_cursor.is_some());

        let page = sm
            .list_sessions_paged(SessionListPageQuery {
                filters,
                cursor: cursor.next_cursor.as_ref(),
                page_size: 1,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        let ids = page
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ids, expected_ids[1..2]);
        assert!(page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_session_creation() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));

        let mut handles = vec![];

        for i in 0..NUM_CONCURRENT_SESSIONS {
            let sm = Arc::clone(&session_manager);
            let handle = tokio::spawn(async move {
                let working_dir = PathBuf::from(format!("/tmp/test_{}", i));
                let description = format!("Test session {}", i);

                let session = sm
                    .create_session(
                        working_dir.clone(),
                        description,
                        SessionType::User,
                        GoslingMode::default(),
                    )
                    .await
                    .unwrap();

                sm.add_message(
                    &session.id,
                    &Message {
                        id: None,
                        role: Role::User,
                        created: chrono::Utc::now().timestamp_millis(),
                        content: vec![MessageContent::text("hello world")],
                        metadata: Default::default(),
                    },
                )
                .await
                .unwrap();

                sm.add_message(
                    &session.id,
                    &Message {
                        id: None,
                        role: Role::Assistant,
                        created: chrono::Utc::now().timestamp_millis(),
                        content: vec![MessageContent::text("sup world?")],
                        metadata: Default::default(),
                    },
                )
                .await
                .unwrap();

                sm.update(&session.id)
                    .user_provided_name(format!("Updated session {}", i))
                    .usage(Usage::new(None, None, Some(100 * i)))
                    .apply()
                    .await
                    .unwrap();

                let updated = sm.get_session(&session.id, true).await.unwrap();
                assert_eq!(updated.message_count, 2);
                assert_eq!(updated.usage.total_tokens, Some(100 * i));

                session.id
            });
            handles.push(handle);
        }

        let mut results = vec![];
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        assert_eq!(results.len(), NUM_CONCURRENT_SESSIONS as usize);

        let unique_ids: std::collections::HashSet<_> = results.iter().collect();
        assert_eq!(unique_ids.len(), NUM_CONCURRENT_SESSIONS as usize);

        let sessions = session_manager.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), NUM_CONCURRENT_SESSIONS as usize);

        for session in &sessions {
            assert_eq!(session.message_count, 2);
            assert!(session.name.starts_with("Updated session"));
        }

        let insights = session_manager.get_insights().await.unwrap();
        assert_eq!(insights.total_sessions, NUM_CONCURRENT_SESSIONS as usize);
        let expected_tokens = 100 * NUM_CONCURRENT_SESSIONS * (NUM_CONCURRENT_SESSIONS - 1) / 2;
        assert_eq!(insights.total_tokens, expected_tokens as i64);
    }

    #[tokio::test]
    async fn test_export_import_roundtrip() {
        const DESCRIPTION: &str = "Original session";
        const USER_MESSAGE: &str = "test message";
        const ASSISTANT_MESSAGE: &str = "test response";

        let usage =
            Usage::new(Some(300), Some(200), Some(500)).with_cache_tokens(Some(120), Some(80));
        let accumulated_usage =
            Usage::new(Some(600), Some(400), Some(1000)).with_cache_tokens(Some(400), Some(150));

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                DESCRIPTION.to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let mut extension_data = ExtensionData::new();
        extension_data.set_extension_state(
            EnabledExtensionsState::EXTENSION_NAME,
            EnabledExtensionsState::VERSION,
            serde_json::json!({
                "extensions": [{
                    "type": "stdio",
                    "name": "imported-executable",
                    "description": "must be quarantined",
                    "cmd": "sh",
                    "args": ["-c", "touch /tmp/imported-session-executed"],
                    "envs": {},
                    "env_keys": [],
                    "timeout": null,
                    "cwd": null,
                    "bundled": false,
                    "available_tools": []
                }]
            }),
        );
        extension_data.set_extension_state(
            "todo",
            "v0",
            serde_json::json!({"content": "safe imported state"}),
        );

        sm.update(&original.id)
            .usage(usage)
            .accumulated_usage(accumulated_usage)
            .restrict_tools_to_working_dirs(true)
            .extension_data(extension_data)
            .provider_name("untrusted-provider")
            .model_config(ModelConfig::new("untrusted-model"))
            .workspace_snapshot(
                "untrusted-workspace".into(),
                "Untrusted workspace".into(),
                Some("untrusted-profile".into()),
                Some("Untrusted profile".into()),
                Some("untrusted-binding".into()),
                WorkspaceSessionContext {
                    workspace_id: "untrusted-workspace".into(),
                    workspace_name: "Untrusted workspace".into(),
                    primary_working_folder: "/tmp/test".into(),
                    folders: Vec::new(),
                    product_output_folders: Vec::new(),
                    folder_policy: Default::default(),
                },
            )
            .apply()
            .await
            .unwrap();

        sm.add_message(
            &original.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text(USER_MESSAGE)],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        sm.add_message(
            &original.id,
            &Message {
                id: None,
                role: Role::Assistant,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text(ASSISTANT_MESSAGE)],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        let exported = sm.export_session(&original.id).await.unwrap();
        let imported = sm
            .import_session(
                &exported,
                None,
                temp_dir.path().to_path_buf(),
                crate::session::import_formats::SessionImportTransport::Json,
            )
            .await
            .unwrap();

        assert_ne!(imported.id, original.id);
        assert_eq!(imported.name, DESCRIPTION);
        assert_eq!(
            imported.working_dir,
            temp_dir.path().canonicalize().unwrap()
        );
        assert_eq!(imported.usage, usage);
        assert_eq!(imported.accumulated_usage, accumulated_usage);
        assert!(imported.provider_name.is_none());
        assert!(imported.model_config.is_none());
        assert!(imported.workspace_id.is_none());
        assert!(imported.workspace_context.is_none());
        assert!(imported.credential_profile_id.is_none());
        assert!(imported.credential_binding_id.is_none());
        assert!(imported.restrict_tools_to_working_dirs);
        assert!(imported.additional_working_dirs.is_empty());
        assert_eq!(imported.gosling_mode, GoslingMode::Approve);
        let provenance =
            crate::session::import_formats::SessionImportProvenance::from_extension_data(
                &imported.extension_data,
            )
            .unwrap();
        assert_eq!(
            provenance.original_working_dir.as_deref(),
            Some("/tmp/test")
        );
        assert!(!provenance.history_trusted);
        assert!(imported
            .extension_data
            .get_extension_state(
                EnabledExtensionsState::EXTENSION_NAME,
                EnabledExtensionsState::VERSION,
            )
            .is_none());
        assert_eq!(
            imported.extension_data.get_extension_state("todo", "v0"),
            Some(&serde_json::json!({"content": "safe imported state"}))
        );
        assert_eq!(imported.message_count, 2);
        assert!(imported
            .conversation
            .as_ref()
            .unwrap()
            .messages()
            .iter()
            .all(|message| message.metadata.imported_untrusted));

        let conversation = imported.conversation.unwrap();
        assert_eq!(conversation.messages().len(), 2);
        assert_eq!(conversation.messages()[0].role, Role::User);
        assert_eq!(conversation.messages()[1].role, Role::Assistant);
    }

    #[tokio::test]
    async fn test_copy_session_rolls_back_atomically_when_post_create_step_fails() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let original = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "original".into(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        // copy_session only reaches replace_conversation (the step broken
        // below) when the source session has a conversation to copy.
        sm.add_message(&original.id, &Message::user().with_text("hello"))
            .await
            .unwrap();

        let pool = sm.storage().pool().await.unwrap();
        let before_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(pool)
            .await
            .unwrap();

        // Breaking inserts into `messages` (not `sessions`) isolates the
        // failure to copy_session's final replace_conversation step,
        // *after* create_session and the extension_data/metadata update
        // have already run — simulating an interruption partway through the
        // copy rather than one before it even starts. A trigger is used
        // instead of dropping a column because the message_search triggers
        // reference content_json, which makes SQLite reject the drop.
        sqlx::query(
            r#"
            CREATE TRIGGER break_messages_insert BEFORE INSERT ON messages
            BEGIN SELECT RAISE(ABORT, 'forced test failure'); END
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        let result = sm.copy_session(&original.id, "copy".into()).await;
        assert!(
            result.is_err(),
            "the forced schema mismatch must surface as an error"
        );

        let after_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(
            after_count, before_count,
            "create_session, the metadata update, and the conversation replace share one \
             transaction, so a failure partway through must roll the whole copy back rather \
             than leaving an empty stray session"
        );
    }

    #[tokio::test]
    async fn test_import_session_rolls_back_atomically_when_post_create_step_fails() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let original = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "original".into(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        // import_session only reaches replace_conversation (the step broken
        // below) when the imported document has a conversation to restore.
        sm.add_message(&original.id, &Message::user().with_text("hello"))
            .await
            .unwrap();
        let exported = sm.export_session(&original.id).await.unwrap();

        let pool = sm.storage().pool().await.unwrap();
        let before_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(pool)
            .await
            .unwrap();

        // Same technique as the analogous copy_session test above: break
        // inserts into `messages` to isolate the failure to import_session's
        // final replace_conversation step, after create_session and the
        // metadata update have already run in the same shared transaction.
        sqlx::query(
            r#"
            CREATE TRIGGER break_messages_insert_on_import BEFORE INSERT ON messages
            BEGIN SELECT RAISE(ABORT, 'forced test failure'); END
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        let result = sm
            .import_session(
                &exported,
                None,
                temp_dir.path().to_path_buf(),
                crate::session::import_formats::SessionImportTransport::Json,
            )
            .await;
        assert!(
            result.is_err(),
            "the forced trigger failure must surface as an error"
        );

        let after_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(
            after_count, before_count,
            "create_session, the metadata update, and the conversation replace share one \
             transaction, so a failure partway through must roll the whole import back \
             rather than leaving an empty stray session — including on a hard interrupt, \
             not just a handled error"
        );
    }

    #[tokio::test]
    async fn test_list_sessions_filters_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let user_session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "User session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        sm.add_message(
            &user_session.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text("hello world")],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        let acp_session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "ACP session".to_string(),
                SessionType::Acp,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        sm.add_message(
            &acp_session.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text("hello acp")],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        let default_sessions = sm.list_sessions().await.unwrap();
        assert_eq!(default_sessions.len(), 1);
        assert_eq!(default_sessions[0].name, "User session");

        let acp_sessions = sm
            .list_sessions_by_types(&[SessionType::Acp])
            .await
            .unwrap();
        assert_eq!(acp_sessions.len(), 1);
        assert_eq!(acp_sessions[0].name, "ACP session");
    }

    #[tokio::test]
    async fn test_list_sessions_filters_by_archive_state() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let active_session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Active session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        sm.add_message(
            &active_session.id,
            &Message::user().with_text("hello active"),
        )
        .await
        .unwrap();

        let archived_session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Archived session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        sm.add_message(
            &archived_session.id,
            &Message::user().with_text("hello archived"),
        )
        .await
        .unwrap();
        sm.update(&archived_session.id)
            .archived_at(Some(chrono::Utc::now()))
            .apply()
            .await
            .unwrap();

        let types = [SessionType::User];
        let active = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    archive_state: SessionArchiveState::Active,
                    ..Default::default()
                },
                cursor: None,
                page_size: 100,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        assert_eq!(active.sessions.len(), 1);
        assert_eq!(active.sessions[0].name, "Active session");

        let archived = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    archive_state: SessionArchiveState::Archived,
                    ..Default::default()
                },
                cursor: None,
                page_size: 100,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        assert_eq!(archived.sessions.len(), 1);
        assert_eq!(archived.sessions[0].name, "Archived session");

        let all = sm
            .list_sessions_paged(SessionListPageQuery {
                filters: SessionListFilters {
                    types: Some(&types),
                    archive_state: SessionArchiveState::All,
                    ..Default::default()
                },
                cursor: None,
                page_size: 100,
                include_last_message_snippet: false,
            })
            .await
            .unwrap();
        assert_eq!(all.sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_import_session_with_legacy_flat_fields() {
        const OLD_FORMAT_JSON: &str = r#"{
            "id": "20240101_1",
            "description": "Old format session",
            "user_set_name": true,
            "working_dir": "/tmp/test",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "extension_data": {},
            "message_count": 0,
            "total_tokens": 500,
            "input_tokens": 300,
            "output_tokens": 200,
            "cache_read_tokens": 120,
            "accumulated_total_tokens": 1000,
            "accumulated_input_tokens": 600,
            "accumulated_output_tokens": 400
        }"#;

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let imported = sm
            .import_session(
                OLD_FORMAT_JSON,
                None,
                temp_dir.path().to_path_buf(),
                crate::session::import_formats::SessionImportTransport::Json,
            )
            .await
            .unwrap();

        assert_eq!(imported.name, "Old format session");
        assert!(imported.user_set_name);
        assert_eq!(
            imported.working_dir,
            temp_dir.path().canonicalize().unwrap()
        );
        assert_eq!(
            imported.usage,
            Usage::new(Some(300), Some(200), Some(500)).with_cache_tokens(Some(120), None)
        );
        assert_eq!(
            imported.accumulated_usage,
            Usage::new(Some(600), Some(400), Some(1000))
        );
    }

    #[tokio::test]
    async fn file_import_is_idempotent_and_records_untrusted_source_provenance() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("history.json");
        let original = r#"{
                "id": "20240101_1",
                "name": "Imported history",
                "working_dir": "/tmp/test",
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z",
                "extension_data": {},
                "message_count": 0
            }"#;
        fs::write(&source, original).unwrap();

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let imported = sm
            .import_session_file(&source, None, temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let SessionFileImportResult::Imported(imported) = imported else {
            panic!("first file import must create a session");
        };
        let provenance =
            crate::session::import_formats::SessionImportProvenance::from_extension_data(
                &imported.extension_data,
            )
            .unwrap();
        let canonical_source = source.canonicalize().unwrap().to_string_lossy().to_string();
        assert_eq!(
            provenance.source_path.as_deref(),
            Some(canonical_source.as_str())
        );
        assert!(provenance.source_sha256.is_some());
        assert!(!provenance.history_trusted);

        let repeated = sm
            .import_session_file(&source, None, temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let SessionFileImportResult::AlreadyImported(repeated) = repeated else {
            panic!("replaying the same file must not duplicate the session");
        };
        assert_eq!(repeated.id, imported.id);

        let repeated_over_json = sm
            .import_session(
                original,
                None,
                temp_dir.path().to_path_buf(),
                crate::session::import_formats::SessionImportTransport::Json,
            )
            .await
            .unwrap();
        assert_eq!(repeated_over_json.id, imported.id);
        assert_eq!(sm.list_all_sessions().await.unwrap().len(), 1);

        fs::write(
            &source,
            r#"{
                "id": "20240101_1",
                "name": "Imported history with a later write",
                "working_dir": "/tmp/test",
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:01Z",
                "extension_data": {},
                "message_count": 0
            }"#,
        )
        .unwrap();
        let changed = sm
            .import_session_file(&source, None, temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let SessionFileImportResult::SourceChanged(changed) = changed else {
            panic!("a changed source must not duplicate the earlier transcript");
        };
        assert_eq!(changed.id, imported.id);
        assert_eq!(sm.list_all_sessions().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_legacy_import_retries_after_interrupted_first_run() {
        let temp_dir = TempDir::new().unwrap();
        let session_dir = temp_dir.path().join(SESSIONS_FOLDER);
        fs::create_dir_all(&session_dir).unwrap();

        let legacy_content = r#"{"description":"Legacy session","id":"20240101_120000","created_at":"2024-01-01T12:00:00Z","updated_at":"2024-01-01T12:00:00Z","extension_data":{},"message_count":0}
{"id":"msg1","role":"user","created":1704110400,"content":[{"type":"text","text":"Hello"}]}
{"id":"msg2","role":"assistant","created":1704110401,"content":[{"type":"text","text":"Hi there"}]}"#;
        fs::write(session_dir.join("20240101_120000.jsonl"), legacy_content).unwrap();

        // Simulate a process that was killed right after the schema was
        // created but before `import_legacy` ran: create the schema
        // directly (bypassing `pool()`'s init sequence) and leave
        // `legacy_import_status` unmarked, exactly as `create_schema`
        // itself leaves it.
        let db_path = session_dir.join(DB_NAME);
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        SessionStorage::create_schema(&pool).await.unwrap();
        let completed_before: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM legacy_import_status WHERE id = 1)")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!completed_before);
        pool.close().await;

        // Starting a `SessionManager` against this half-initialized
        // database must retry the legacy import instead of treating the
        // already-committed schema as proof the import also finished.
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let pool = sm.storage().pool().await.unwrap();

        let imported = sm
            .get_session("20240101_120000", true)
            .await
            .expect("interrupted legacy import must be retried, not silently skipped");
        assert_eq!(imported.name, "Legacy session");
        let messages = imported.conversation.unwrap().messages().len();
        assert_eq!(messages, 2);

        let completed_after: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM legacy_import_status WHERE id = 1)")
                .fetch_one(pool)
                .await
                .unwrap();
        assert!(completed_after);
    }

    #[tokio::test]
    async fn test_legacy_import_not_replayed_for_pre_existing_database() {
        let temp_dir = TempDir::new().unwrap();
        let session_dir = temp_dir.path().join(SESSIONS_FOLDER);
        fs::create_dir_all(&session_dir).unwrap();

        // A legacy `.jsonl` file that, if (re-)imported, would clobber the
        // session below with stale content.
        let legacy_content = r#"{"description":"Stale legacy content","id":"20240101_120000","created_at":"2024-01-01T12:00:00Z","updated_at":"2024-01-01T12:00:00Z","extension_data":{},"message_count":0}
{"id":"msg1","role":"user","created":1704110400,"content":[{"type":"text","text":"stale legacy message"}]}"#;
        fs::write(session_dir.join("20240101_120000.jsonl"), legacy_content).unwrap();

        let db_path = session_dir.join(DB_NAME);
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        SessionStorage::create_schema(&pool).await.unwrap();

        // Simulate a database that was already fully set up and used
        // *before* the `legacy_import_status` marker existed (schema
        // version predates migration 21), with the same session id already
        // present and since diverged from whatever the legacy file holds.
        sqlx::query("UPDATE schema_version SET version = 20")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, name, working_dir, extension_data, gosling_mode) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("20240101_120000")
        .bind("Live session name")
        .bind("/tmp")
        .bind("{}")
        .bind("auto")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("msg_live")
        .bind("20240101_120000")
        .bind("user")
        .bind(r#"[{"type":"text","text":"live message"}]"#)
        .bind(1_704_200_000_i64)
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        // Upgrading this pre-existing database must not silently replay
        // the legacy import and overwrite session data accumulated since
        // the original (pre-marker) import.
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let pool = sm.storage().pool().await.unwrap();

        let session = sm.get_session("20240101_120000", true).await.unwrap();
        assert_eq!(session.name, "Live session name");
        let messages = session.conversation.unwrap();
        assert_eq!(messages.messages().len(), 1);
        assert_eq!(
            messages.messages()[0].content,
            vec![MessageContent::text("live message")]
        );

        let completed: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM legacy_import_status WHERE id = 1)")
                .fetch_one(pool)
                .await
                .unwrap();
        assert!(completed);
    }

    #[test_case(GoslingMode::Approve)]
    #[test_case(GoslingMode::SmartApprove)]
    #[test_case(GoslingMode::Chat)]
    #[tokio::test]
    async fn test_gosling_mode_persists(mode: GoslingMode) {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "test".into(),
                SessionType::User,
                mode,
            )
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.gosling_mode, mode);
    }

    #[tokio::test]
    async fn test_additional_working_dirs_default_empty_and_updates() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "test".into(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        assert!(session.additional_working_dirs.is_empty());

        let extra_dirs = vec![
            PathBuf::from("/tmp/extra-one"),
            PathBuf::from("/tmp/extra-two"),
        ];
        sm.update(&session.id)
            .additional_working_dirs(extra_dirs.clone())
            .apply()
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.additional_working_dirs, extra_dirs);

        sm.update(&session.id)
            .additional_working_dirs(Vec::new())
            .apply()
            .await
            .unwrap();
        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert!(reloaded.additional_working_dirs.is_empty());
    }

    #[tokio::test]
    async fn test_gosling_mode_update() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "test".into(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        sm.update(&session.id)
            .gosling_mode(GoslingMode::Approve)
            .apply()
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.gosling_mode, GoslingMode::Approve);
    }

    #[tokio::test]
    async fn workspace_snapshot_round_trips_and_is_copied() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "workspace session".into(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let context = WorkspaceSessionContext {
            workspace_id: "workspace-id".into(),
            workspace_name: "Project".into(),
            primary_working_folder: temp_dir.path().to_string_lossy().to_string(),
            folders: Vec::new(),
            product_output_folders: Vec::new(),
            folder_policy: Default::default(),
        };
        sm.update(&session.id)
            .workspace_snapshot(
                "workspace-id".into(),
                "Project".into(),
                Some("profile-id".into()),
                Some("Provider profile".into()),
                Some("binding-id".into()),
                context.clone(),
            )
            .apply()
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        let mut pinned_context = context.clone();
        pinned_context.folder_policy = context.effective_folder_policy();
        assert_eq!(reloaded.workspace_id.as_deref(), Some("workspace-id"));
        assert_eq!(reloaded.workspace_context, Some(pinned_context.clone()));
        assert!(!reloaded.restrict_tools_to_working_dirs);
        let copied = sm.copy_session(&session.id, "copy".into()).await.unwrap();
        assert_eq!(
            copied.restrict_tools_to_working_dirs,
            reloaded.restrict_tools_to_working_dirs
        );
        assert_eq!(copied.workspace_id, reloaded.workspace_id);
        assert_eq!(copied.credential_profile_id, reloaded.credential_profile_id);
        assert_eq!(copied.workspace_context, Some(pinned_context));

        let database = std::fs::read(temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME)).unwrap();
        assert!(!String::from_utf8_lossy(&database).contains("GOSLING_SENTINEL_SECRET"));
    }

    #[tokio::test]
    async fn workspace_session_restriction_defaults_off_and_opt_in_survives_reload() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "workspace session".into(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let context = WorkspaceSessionContext {
            workspace_id: "workspace-id".into(),
            workspace_name: "Project".into(),
            primary_working_folder: temp_dir.path().to_string_lossy().to_string(),
            folders: Vec::new(),
            product_output_folders: Vec::new(),
            folder_policy: Default::default(),
        };
        sm.update(&session.id)
            .workspace_snapshot(
                "workspace-id".into(),
                "Project".into(),
                None,
                None,
                None,
                context.clone(),
            )
            .apply()
            .await
            .unwrap();

        // Workspaces are unrestricted by default (opt-in), so providers that run
        // their own tools stay usable without a per-chat toggle.
        let unrestricted = sm.get_session(&session.id, false).await.unwrap();
        assert!(!unrestricted.restrict_tools_to_working_dirs);

        // Opting in per-chat must persist through reload rather than being forced
        // back off for workspace sessions (the loader must respect the stored column).
        sm.update(&session.id)
            .restrict_tools_to_working_dirs(true)
            .apply()
            .await
            .unwrap();
        let opted_in = sm.get_session(&session.id, false).await.unwrap();
        assert!(opted_in.restrict_tools_to_working_dirs);
        // The workspace binding and folder policy remain intact after opting in.
        assert_eq!(opted_in.workspace_id.as_deref(), Some("workspace-id"));
        assert!(opted_in.workspace_context.is_some());
    }

    #[tokio::test]
    async fn migration_24_clears_force_seeded_workspace_restriction_only() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let workspace_session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "workspace session".into(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let context = WorkspaceSessionContext {
            workspace_id: "workspace-id".into(),
            workspace_name: "Project".into(),
            primary_working_folder: temp_dir.path().to_string_lossy().to_string(),
            folders: Vec::new(),
            product_output_folders: Vec::new(),
            folder_policy: Default::default(),
        };
        sm.update(&workspace_session.id)
            .workspace_snapshot(
                "workspace-id".into(),
                "Project".into(),
                None,
                None,
                None,
                context,
            )
            .restrict_tools_to_working_dirs(true)
            .apply()
            .await
            .unwrap();
        let plain_session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "plain session".into(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        sm.update(&plain_session.id)
            .restrict_tools_to_working_dirs(true)
            .apply()
            .await
            .unwrap();
        drop(sm);

        // Rewind the schema version so reopening replays migration 24 against
        // data shaped like a pre-24 install (workspace restriction forced on).
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        let pool = SqlitePoolOptions::new()
            .connect_with(SqliteConnectOptions::new().filename(&db_path))
            .await
            .unwrap();
        sqlx::query("UPDATE schema_version SET version = 23 WHERE version >= 24")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let workspace_reloaded = sm.get_session(&workspace_session.id, false).await.unwrap();
        assert!(!workspace_reloaded.restrict_tools_to_working_dirs);
        assert!(workspace_reloaded.workspace_context.is_some());
        // A deliberate opt-in on a non-workspace chat survives the migration.
        let plain_reloaded = sm.get_session(&plain_session.id, false).await.unwrap();
        assert!(plain_reloaded.restrict_tools_to_working_dirs);
    }

    #[tokio::test]
    async fn workspace_columns_migrate_from_schema_21_without_assigning_legacy_sessions() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        SessionStorage::create_schema(&pool).await.unwrap();
        sqlx::query("DROP INDEX idx_sessions_workspace")
            .execute(&pool)
            .await
            .unwrap();
        for column in [
            "workspace_context_json",
            "credential_binding_id",
            "credential_profile_name",
            "credential_profile_id",
            "workspace_name",
            "workspace_id",
        ] {
            sqlx::query(&format!("ALTER TABLE sessions DROP COLUMN {column}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE schema_version SET version = 21")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, name, working_dir, extension_data, gosling_mode) VALUES ('legacy-workspace', 'Legacy', '/tmp', '{}', 'auto')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let legacy = sm.get_session("legacy-workspace", false).await.unwrap();
        assert!(legacy.workspace_id.is_none());
        assert!(legacy.credential_profile_id.is_none());
        assert!(legacy.workspace_context.is_none());
    }

    #[tokio::test]
    async fn tool_operation_ledger_migrates_from_schema_22() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        SessionStorage::create_schema(&pool).await.unwrap();
        sqlx::query("DROP TABLE tool_operations")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE schema_version SET version = 22")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let pool = sm.storage().pool().await.unwrap();
        let table_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tool_operations')",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(table_exists);
        let schema_version: i32 = sqlx::query_scalar("SELECT MAX(version) FROM schema_version")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_removed_tagteam_schema_is_cleaned_up() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        SessionStorage::create_schema(&pool).await.unwrap();
        let mut migration = pool.begin().await.unwrap();
        SessionStorage::apply_migration(&mut migration, 19)
            .await
            .unwrap();
        migration.commit().await.unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, name, working_dir, extension_data, gosling_mode, workflow_kind) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("legacy-tagteam")
        .bind("Legacy")
        .bind("/tmp")
        .bind("{}")
        .bind("auto")
        .bind("tagteam")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO tagteam_run_bindings(
                session_id, launch_generation, schema_version, launch_spec_json,
                action_digest, producer_run_id, last_sequence, snapshot_json
            ) VALUES ('legacy-tagteam', 1, 1, '{}', 'digest', 'run-1', 0, '{}')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE schema_version SET version = 29")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let pool = sm.storage().pool().await.unwrap();
        let migrated = sm.get_session("legacy-tagteam", false).await.unwrap();
        assert_eq!(migrated.name, "Legacy");
        let workflow_column_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'workflow_kind')",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(!workflow_column_exists);
        let table_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tagteam_run_bindings')",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(!table_exists);
        let counter_table_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tagteam_launch_counters')",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(!counter_table_exists);
        let schema_version: i32 = sqlx::query_scalar("SELECT MAX(version) FROM schema_version")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn migration_31_adds_conversation_order_index() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        sm.create_session(
            temp_dir.path().to_path_buf(),
            "Index migration".to_string(),
            SessionType::User,
            GoslingMode::default(),
        )
        .await
        .unwrap();
        drop(sm);

        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        let pool = SqlitePoolOptions::new()
            .connect_with(SqliteConnectOptions::new().filename(&db_path))
            .await
            .unwrap();
        sqlx::query("DROP INDEX idx_messages_session_time_asc")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE schema_version SET version = 30")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let upgraded = SessionManager::new(temp_dir.path().to_path_buf());
        let pool = upgraded.storage().pool().await.unwrap();
        let index_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_index_list('messages') WHERE name = 'idx_messages_session_time_asc'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(index_count, 1);

        let plan = sqlx::query_as::<_, (i64, i64, i64, String)>(
            "EXPLAIN QUERY PLAN SELECT role, content_json, created_timestamp, metadata_json, message_id FROM messages WHERE session_id = ? ORDER BY created_timestamp, id",
        )
        .bind("session")
        .fetch_all(pool)
        .await
        .unwrap();
        let plan_details = plan
            .into_iter()
            .map(|(_, _, _, detail)| detail)
            .collect::<Vec<_>>();
        assert!(plan_details
            .iter()
            .any(|detail| detail.contains("idx_messages_session_time_asc")));
        assert!(!plan_details
            .iter()
            .any(|detail| detail.contains("USE TEMP B-TREE FOR ORDER BY")));
    }

    #[tokio::test]
    async fn test_gosling_mode_malformed_uses_safe_default() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "test".into(),
                SessionType::User,
                GoslingMode::Approve,
            )
            .await
            .unwrap();

        let pool = &sm.storage().pool;
        sqlx::query("UPDATE sessions SET gosling_mode = 'garbage' WHERE id = ?")
            .bind(&session.id)
            .execute(pool)
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.gosling_mode, GoslingMode::default());
    }

    #[tokio::test]
    async fn session_schema_default_matches_runtime_mode_default() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        sm.create_session(
            temp_dir.path().to_path_buf(),
            "test".into(),
            SessionType::User,
            GoslingMode::default(),
        )
        .await
        .unwrap();
        let row: (String,) = sqlx::query_as(
            "SELECT dflt_value FROM pragma_table_info('sessions') WHERE name = 'gosling_mode'",
        )
        .fetch_one(&sm.storage().pool)
        .await
        .unwrap();

        assert_eq!(row.0.trim_matches('\''), GoslingMode::default().to_string());
    }

    #[tokio::test]
    async fn test_acp_session_migration() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }

        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();

        SessionStorage::create_schema(&pool).await.unwrap();

        // Demote the schema back to v8 to simulate a database
        // that has never seen migration 9.
        sqlx::query("UPDATE schema_version SET version = 8")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data, gosling_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("user_id")
        .bind("User Session")
        .bind(false)
        .bind("user")
        .bind("/tmp")
        .bind("{}")
        .bind("auto")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data, gosling_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("acp_id")
        .bind("ACP Session")
        .bind(false)
        .bind("user")
        .bind("/tmp")
        .bind("{}")
        .bind("auto")
        .execute(&pool)
        .await
        .unwrap();

        pool.close().await;

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        sm.storage().pool().await.unwrap(); // Triggers migration

        let user_session = sm.storage().get_session("user_id", false).await.unwrap();
        assert_eq!(user_session.session_type, SessionType::User);

        let acp_session = sm.storage().get_session("acp_id", false).await.unwrap();
        assert_eq!(acp_session.session_type, SessionType::Acp);
    }

    #[tokio::test]
    async fn test_cache_token_columns_migration_and_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }

        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();

        SessionStorage::create_schema(&pool).await.unwrap();

        // Recreate a v13-shaped database without cache token columns.
        for column in [
            "cache_read_tokens",
            "cache_write_tokens",
            "accumulated_cache_read_tokens",
            "accumulated_cache_write_tokens",
        ] {
            sqlx::query(&format!("ALTER TABLE sessions DROP COLUMN {column}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE schema_version SET version = 13")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data, gosling_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("cache_id")
        .bind("Cache Session")
        .bind(false)
        .bind("user")
        .bind("/tmp")
        .bind("{}")
        .bind("auto")
        .execute(&pool)
        .await
        .unwrap();

        pool.close().await;

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        sm.storage().pool().await.unwrap(); // Triggers migration

        let usage =
            Usage::new(Some(8000), Some(500), None).with_cache_tokens(Some(5000), Some(1000));
        let accumulated_usage =
            Usage::new(Some(24000), Some(1500), None).with_cache_tokens(Some(15000), Some(3000));

        sm.update("cache_id")
            .usage(usage)
            .accumulated_usage(accumulated_usage)
            .apply()
            .await
            .unwrap();

        let loaded = sm.get_session("cache_id", false).await.unwrap();
        assert_eq!(loaded.usage, usage);
        assert_eq!(loaded.accumulated_usage, accumulated_usage);
    }

    #[tokio::test]
    async fn record_usage_accumulates_concurrent_updates() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let session = manager
            .create_session(
                PathBuf::from("/tmp/test"),
                "Usage accounting".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let first_usage = Usage::new(Some(10), Some(5), None).with_cache_tokens(Some(2), None);
        let second_usage = Usage::new(Some(20), Some(10), None).with_cache_tokens(None, Some(3));
        let first = manager.record_usage(&session.id, first_usage, first_usage, Some(0.25));
        let second = manager.record_usage(&session.id, second_usage, second_usage, Some(0.75));

        let (first_result, second_result) = tokio::join!(first, second);
        first_result.unwrap();
        second_result.unwrap();

        let reloaded = manager.get_session(&session.id, false).await.unwrap();
        assert_eq!(
            reloaded.accumulated_usage,
            Usage::new(Some(30), Some(15), None).with_cache_tokens(Some(2), Some(3))
        );
        assert_eq!(reloaded.accumulated_cost, Some(1.0));
    }

    // FSR-CROSS-001: compaction used to call `replace_conversation` and
    // `record_usage` as two separately-committed writes; a crash between the
    // two could leave `sessions.total_tokens` stale-high relative to the
    // already-compacted conversation, spuriously re-triggering auto-compaction
    // on the next turn. `replace_conversation_and_record_usage` commits both
    // in one transaction; this asserts both effects are observable together
    // after a single successful call, and that a failing call leaves neither
    // applied (not a torn state where messages replaced but usage stale).
    #[tokio::test]
    async fn replace_conversation_and_record_usage_applies_both_writes_together() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let session = manager
            .create_session(
                PathBuf::from("/tmp/test"),
                "Atomic compaction write".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let original = Conversation::new_unvalidated(vec![
            Message::user().with_text("original 1"),
            Message::assistant().with_text("original 2"),
        ]);
        manager
            .replace_conversation(&session.id, &original)
            .await
            .unwrap();
        manager
            .record_usage(
                &session.id,
                Usage::new(Some(9_900), Some(100), Some(10_000)),
                Usage::new(Some(9_900), Some(100), Some(10_000)),
                None,
            )
            .await
            .unwrap();

        let compacted =
            Conversation::new_unvalidated(vec![Message::assistant().with_text("<summary>")]);
        let current_usage = Usage::new(Some(200), None, Some(200));
        manager
            .replace_conversation_and_record_usage(
                &session.id,
                &compacted,
                current_usage,
                Usage::new(Some(6_000), Some(200), Some(6_200)),
                Some(0.5),
            )
            .await
            .unwrap();

        let reloaded = manager.get_session(&session.id, true).await.unwrap();
        let stored_conversation = reloaded.conversation.expect("conversation should load");
        assert_eq!(stored_conversation.messages().len(), 1);
        assert_eq!(
            stored_conversation.messages()[0].as_concat_text(),
            "<summary>"
        );
        assert_eq!(reloaded.usage, current_usage);
        assert_eq!(reloaded.accumulated_usage.total_tokens, Some(16_200));
        assert_eq!(reloaded.accumulated_cost, Some(0.5));

        // A failing call (nonexistent session) must not leave a torn state:
        // no messages committed for an id `sessions` never accepted.
        let orphan_id = "does-not-exist";
        let err = manager
            .replace_conversation_and_record_usage(
                orphan_id,
                &compacted,
                current_usage,
                Usage::default(),
                None,
            )
            .await
            .unwrap_err();
        assert!(!err.to_string().is_empty());
        let orphan_messages = manager.storage().get_conversation(orphan_id).await.unwrap();
        assert!(
            orphan_messages.messages().is_empty(),
            "a rolled-back write must not leave stray messages for a session that was never created"
        );
    }

    #[tokio::test]
    async fn session_artifact_inventory_persists_deduplicates_and_copies_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let working_dir = temp_dir.path().join("workspace");
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let session = manager
            .create_session(
                working_dir.clone(),
                "Artifact inventory".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let message = Message::assistant()
            .with_id("artifact-message")
            .with_text("Created `src/main.rs` and [binary](output/result.bin).");

        manager.upsert_message(&session.id, &message).await.unwrap();
        assert!(manager
            .list_session_artifacts(&session.id, None, 20)
            .await
            .unwrap()
            .artifacts
            .is_empty());

        manager
            .register_completed_assistant_artifacts(&session.id, &message)
            .await
            .unwrap();
        manager
            .register_completed_assistant_artifacts(&session.id, &message)
            .await
            .unwrap();
        manager
            .upsert_session_artifacts(
                &session.id,
                &[DiscoveredArtifact::from_path(
                    "src/main.rs",
                    &working_dir,
                    None,
                    None,
                    crate::session::SessionArtifactRelation::Created,
                    SessionArtifactProvenance::BuiltInTool,
                    Some("tool-call"),
                )
                .unwrap()],
            )
            .await
            .unwrap();
        manager
            .register_completed_assistant_artifacts(&session.id, &message)
            .await
            .unwrap();
        let page = manager
            .list_session_artifacts(&session.id, None, 20)
            .await
            .unwrap();
        assert_eq!(page.total_count, 2);
        assert!(page
            .artifacts
            .iter()
            .any(|artifact| artifact.display_path == "src/main.rs"));
        assert!(page
            .artifacts
            .iter()
            .any(|artifact| artifact.display_path == "output/result.bin"));
        assert_eq!(
            page.artifacts
                .iter()
                .find(|artifact| artifact.display_path == "src/main.rs")
                .unwrap()
                .provenance,
            SessionArtifactProvenance::BuiltInTool
        );

        drop(manager);
        let reloaded = SessionManager::new(temp_dir.path().to_path_buf());
        assert_eq!(
            reloaded
                .list_session_artifacts(&session.id, None, 20)
                .await
                .unwrap()
                .total_count,
            2
        );
        let copied = reloaded
            .copy_session(&session.id, "Artifact inventory copy".to_string())
            .await
            .unwrap();
        let copied_page = reloaded
            .list_session_artifacts(&copied.id, None, 20)
            .await
            .unwrap();
        assert_eq!(copied_page.total_count, 2);
        assert!(copied_page
            .artifacts
            .iter()
            .all(|artifact| artifact.session_id == copied.id));

        reloaded.delete_session(&session.id).await.unwrap();
        assert_eq!(
            reloaded
                .list_session_artifacts(&session.id, None, 20)
                .await
                .unwrap()
                .total_count,
            0
        );
    }

    #[tokio::test]
    async fn session_artifact_legacy_backfill_uses_messages_and_skips_untrusted_history() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let session = manager
            .create_session(
                temp_dir.path().join("workspace"),
                "Legacy artifacts".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        manager
            .add_message(
                &session.id,
                &Message::assistant()
                    .with_id("trusted")
                    .with_text("See `output/trusted.py`."),
            )
            .await
            .unwrap();
        let mut untrusted = Message::assistant()
            .with_id("untrusted")
            .with_text("See `/outside/untrusted.rs`.");
        untrusted.metadata = untrusted.metadata.with_imported_untrusted();
        manager.add_message(&session.id, &untrusted).await.unwrap();

        let pool = manager.storage().pool().await.unwrap();
        sqlx::query("UPDATE schema_version SET version = 25")
            .execute(pool)
            .await
            .unwrap();
        drop(manager);

        let migrated = SessionManager::new(temp_dir.path().to_path_buf());
        let page = migrated
            .list_session_artifacts(&session.id, None, 20)
            .await
            .unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.artifacts[0].display_path, "output/trusted.py");
        assert_eq!(
            page.artifacts[0].provenance,
            SessionArtifactProvenance::CompatibilityInference
        );
    }

    #[tokio::test]
    async fn session_artifact_inventory_paginates_large_result_sets() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let working_dir = temp_dir.path().join("workspace");
        let session = manager
            .create_session(
                working_dir.clone(),
                "Many artifacts".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let artifacts = (0..205)
            .map(|index| {
                DiscoveredArtifact::from_path(
                    &format!("output/{index}.txt"),
                    &working_dir,
                    None,
                    Some("text/plain".to_string()),
                    crate::session::SessionArtifactRelation::Created,
                    SessionArtifactProvenance::BuiltInTool,
                    Some("bulk-tool"),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        manager
            .upsert_session_artifacts(&session.id, &artifacts)
            .await
            .unwrap();

        let first = manager
            .list_session_artifacts(&session.id, None, 200)
            .await
            .unwrap();
        assert_eq!(first.total_count, 205);
        assert_eq!(first.artifacts.len(), 200);
        let second = manager
            .list_session_artifacts(&session.id, first.next_cursor.as_deref(), 200)
            .await
            .unwrap();
        assert_eq!(second.artifacts.len(), 5);
        assert!(second.next_cursor.is_none());
    }

    #[tokio::test]
    async fn session_library_separates_session_items_and_shares_project_items() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let working_dir = temp_dir.path().join("workspace");
        let first = manager
            .create_session(
                working_dir.clone(),
                "First library session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let second = manager
            .create_session(
                working_dir,
                "Second library session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let private = manager
            .add_session_library_item(
                &first.id,
                SessionLibraryScope::Session,
                "Private notes".to_string(),
                NewSessionLibraryContent::Text("only the first session".to_string()),
            )
            .await
            .unwrap();
        let shared = manager
            .add_session_library_item(
                &first.id,
                SessionLibraryScope::Project,
                "Project notes".to_string(),
                NewSessionLibraryContent::Text("shared with this project".to_string()),
            )
            .await
            .unwrap();

        let first_items = manager.list_session_library_items(&first.id).await.unwrap();
        assert_eq!(first_items.len(), 2);
        let second_items = manager
            .list_session_library_items(&second.id)
            .await
            .unwrap();
        assert_eq!(second_items, vec![shared.clone()]);
        assert!(manager
            .get_session_library_items(&second.id, std::slice::from_ref(&private.id))
            .await
            .is_err());
        assert!(manager
            .remove_session_library_item(&second.id, &private.id)
            .await
            .is_ok_and(|removed| !removed));
        assert!(manager
            .remove_session_library_item(&second.id, &shared.id)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn deleting_session_removes_private_library_items_only() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let working_dir = temp_dir.path().join("workspace");
        let deleted = manager
            .create_session(
                working_dir.clone(),
                "Deleted session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let survivor = manager
            .create_session(
                working_dir,
                "Surviving session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let private = manager
            .add_session_library_item(
                &deleted.id,
                SessionLibraryScope::Session,
                "Private".to_string(),
                NewSessionLibraryContent::Text("remove me".to_string()),
            )
            .await
            .unwrap();
        let shared = manager
            .add_session_library_item(
                &deleted.id,
                SessionLibraryScope::Project,
                "Shared".to_string(),
                NewSessionLibraryContent::Text("keep me".to_string()),
            )
            .await
            .unwrap();

        manager.delete_session(&deleted.id).await.unwrap();

        let private_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_library_items WHERE id = ?")
                .bind(&private.id)
                .fetch_one(manager.storage().pool().await.unwrap())
                .await
                .unwrap();
        assert_eq!(private_count, 0);
        assert_eq!(
            manager
                .list_session_library_items(&survivor.id)
                .await
                .unwrap(),
            vec![shared]
        );
    }

    #[tokio::test]
    async fn session_turn_lease_excludes_other_managers_until_release() {
        let temp_dir = TempDir::new().unwrap();
        let first_manager = SessionManager::new(temp_dir.path().to_path_buf());
        let session = first_manager
            .create_session(
                temp_dir.path().join("workspace"),
                "Leased session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let second_manager = SessionManager::new(temp_dir.path().to_path_buf());

        let first_lease = first_manager
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap();
        let error = second_manager
            .acquire_session_turn_lease(&session.id, None)
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("already has an active turn"));

        first_lease.release().await.unwrap();
        second_manager
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap()
            .release()
            .await
            .unwrap();
    }

    /// REL-GSL-006: a `started` row whose owner process is alive but whose
    /// session has no running turn is safe to surface as `in_doubt`. Before
    /// this rule such a row stayed `started` until the owning process exited,
    /// which is what stranded three tool operations for hours on 2026-09-05.
    #[tokio::test]
    async fn a_started_tool_operation_without_a_live_turn_recovers_as_in_doubt() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "Orphaned tool".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let tool_call = rmcp::model::CallToolRequestParams::new("send_email")
            .with_arguments(rmcp::object!({ "recipient": "person@example.com" }));
        sm.add_message(
            &session.id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("tool-request-orphan", Ok(tool_call.clone())),
        )
        .await
        .unwrap();
        assert!(matches!(
            sm.begin_tool_operation(&session.id, "tool-request-orphan", &tool_call, true)
                .await
                .unwrap(),
            ToolOperationStart::Execute { .. }
        ));

        // The owner PID stays this very-much-alive test process; only the turn
        // is gone. A peer must still finalize the row.
        let peer = SessionManager::new(temp_dir.path().to_path_buf());
        assert_eq!(peer.recover_tool_operations(&session.id).await.unwrap(), 1);

        let reloaded = peer.get_session(&session.id, true).await.unwrap();
        let conversation = reloaded.conversation.unwrap();
        let responses = conversation
            .messages()
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(MessageContent::as_tool_response)
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 1);
        let error = responses[0]
            .tool_result
            .as_ref()
            .expect_err("an orphaned operation recovers as an error");
        assert!(error.message.contains("must not be retried automatically"));
    }

    /// REL-GSL-006: the lease has to belong to the operation's own owner. A
    /// lease held by someone else means this operation's turn is over, so its
    /// `started` row is stale even though its process is still running.
    #[tokio::test]
    async fn a_started_operation_is_recovered_when_another_owner_holds_the_turn() {
        let temp_dir = TempDir::new().unwrap();
        let dispatcher = SessionManager::new(temp_dir.path().to_path_buf());
        let session = dispatcher
            .create_session(
                PathBuf::from("/tmp/test"),
                "Handed-over session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let tool_call = rmcp::model::CallToolRequestParams::new("send_email")
            .with_arguments(rmcp::object!({ "recipient": "person@example.com" }));
        dispatcher
            .add_message(
                &session.id,
                &Message::assistant()
                    .with_generated_id()
                    .with_tool_request("tool-request-handover", Ok(tool_call.clone())),
            )
            .await
            .unwrap();
        assert!(matches!(
            dispatcher
                .begin_tool_operation(&session.id, "tool-request-handover", &tool_call, true)
                .await
                .unwrap(),
            ToolOperationStart::Execute { .. }
        ));

        // A second manager now owns the session's turn. Both `owner_id`s are
        // this live process, so only the lease ownership distinguishes them.
        let successor = SessionManager::new(temp_dir.path().to_path_buf());
        let successor_lease = successor
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap();

        assert_eq!(
            successor
                .recover_tool_operations(&session.id)
                .await
                .unwrap(),
            1,
            "an operation whose owner no longer holds the turn is stale"
        );
        successor_lease.release().await.unwrap();
    }

    /// REL-GSL-005: a live owner that stops heartbeating loses the session.
    /// Requiring the owner to be dead first would mean the only way to recover
    /// a wedged turn is killing the app, which is what the 2026-09-05 write-gate
    /// deadlock actually required.
    #[tokio::test]
    async fn session_turn_lease_expires_for_a_live_owner_that_stops_heartbeating() {
        let temp_dir = TempDir::new().unwrap();
        let owner = SessionManager::new(temp_dir.path().to_path_buf());
        let session = owner
            .create_session(
                temp_dir.path().join("workspace"),
                "Wedged turn".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let lease = owner
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap();
        // The stored PID is this test process, so the owner probes as alive;
        // only the heartbeat has gone stale.
        set_lease_age(&owner, &session.id, 200).await;

        SessionManager::new(temp_dir.path().to_path_buf())
            .acquire_session_turn_lease(&session.id, None)
            .await
            .expect("a live owner that stopped heartbeating can be taken over")
            .release()
            .await
            .unwrap();

        // Fencing: the evicted owner learns it lost the session on its next
        // heartbeat and cancels its own turn, so the session never has two
        // writers. This is what makes taking a live owner's lease safe.
        assert!(!lease.heartbeat_once().await);
        assert!(lease.turn_cancel_token().is_cancelled());
        lease.abandon();
    }

    /// REL-GSL-005: a lease is still held while its owner keeps heartbeating,
    /// however long the turn runs.
    #[tokio::test]
    async fn session_turn_lease_is_held_while_its_owner_keeps_heartbeating() {
        let temp_dir = TempDir::new().unwrap();
        let owner = SessionManager::new(temp_dir.path().to_path_buf());
        let session = owner
            .create_session(
                temp_dir.path().join("workspace"),
                "Long turn".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let lease = owner
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap();
        set_lease_age(&owner, &session.id, 80).await;
        assert!(lease.heartbeat_once().await, "the renewal keeps the lease");

        let error = SessionManager::new(temp_dir.path().to_path_buf())
            .acquire_session_turn_lease(&session.id, None)
            .await
            .err()
            .expect("a heartbeating owner keeps its lease");
        assert!(error.to_string().contains("already has an active turn"));
        assert!(!lease.turn_cancel_token().is_cancelled());
        lease.abandon();
    }

    /// REL-GSL-005: a crashed owner's lease is free at once. Waiting out the
    /// TTL after a crash would leave a relaunched app unable to touch its own
    /// session for a minute and a half.
    #[tokio::test]
    async fn a_dead_owners_lease_is_free_even_with_a_fresh_heartbeat() {
        let temp_dir = TempDir::new().unwrap();
        let owner = SessionManager::new(temp_dir.path().to_path_buf());
        let session = owner
            .create_session(
                temp_dir.path().join("workspace"),
                "Crashed owner".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        owner
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap()
            .abandon();
        // The heartbeat is current; only the owning process is gone.
        sqlx::query("UPDATE session_turn_leases SET owner_pid = ? WHERE session_id = ?")
            .bind(i32::MAX as i64)
            .bind(&session.id)
            .execute(owner.storage().pool().await.unwrap())
            .await
            .unwrap();

        SessionManager::new(temp_dir.path().to_path_buf())
            .acquire_session_turn_lease(&session.id, None)
            .await
            .expect("a dead owner's lease is free immediately")
            .release()
            .await
            .unwrap();
    }

    /// REL-GSL-005: cancelling the caller's token still cancels the turn, since
    /// the lease's token is that token's child.
    #[tokio::test]
    async fn a_lease_turn_token_follows_the_callers_cancellation() {
        let temp_dir = TempDir::new().unwrap();
        let owner = SessionManager::new(temp_dir.path().to_path_buf());
        let session = owner
            .create_session(
                temp_dir.path().join("workspace"),
                "Cancelled turn".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let caller = tokio_util::sync::CancellationToken::new();
        let lease = owner
            .acquire_session_turn_lease(&session.id, Some(&caller))
            .await
            .unwrap();
        let turn = lease.turn_cancel_token();
        assert!(!turn.is_cancelled());
        caller.cancel();
        assert!(turn.is_cancelled());
        lease.abandon();
    }

    async fn set_lease_age(manager: &SessionManager, session_id: &str, age_secs: i64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        sqlx::query("UPDATE session_turn_leases SET updated_at = ? WHERE session_id = ?")
            .bind(now - age_secs)
            .bind(session_id)
            .execute(manager.storage().pool().await.unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn session_turn_lease_recovers_stale_owner() {
        let temp_dir = TempDir::new().unwrap();
        let first_manager = SessionManager::new(temp_dir.path().to_path_buf());
        let session = first_manager
            .create_session(
                temp_dir.path().join("workspace"),
                "Stale lease session".to_string(),
                SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        first_manager
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap()
            .abandon();
        sqlx::query("UPDATE session_turn_leases SET updated_at = 0 WHERE session_id = ?")
            .bind(&session.id)
            .execute(first_manager.storage().pool().await.unwrap())
            .await
            .unwrap();

        SessionManager::new(temp_dir.path().to_path_buf())
            .acquire_session_turn_lease(&session.id, None)
            .await
            .unwrap()
            .release()
            .await
            .unwrap();
    }
}
