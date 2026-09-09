use crate::agents::ExtensionLoadResult;
use crate::config::{Config, GoslingMode};
use crate::providers::inventory::{ProviderInventoryEntry, ProviderInventoryService};
use crate::session::import_formats::SessionImportProvenance;
use crate::session::{ExtensionState, Session};
use crate::slash_commands::types::{SlashCommandEntry, SlashCommandSource};
use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOption, SessionId, SessionInfo, SessionMode,
    SessionModeId, SessionModeState, SessionNotification, SessionUpdate, UnstructuredCommandInput,
};
use agent_client_protocol::{Client, ConnectionTo};
use gosling_providers::model::ModelConfig;
use gosling_providers::thinking::ThinkingEffort;
use serde::Serialize;
use strum::{EnumMessage, VariantNames};

use super::server::{build_usage_updates, DEFAULT_PROVIDER_ID, DEFAULT_PROVIDER_LABEL};

pub(super) fn session_provider_selection(session: &Session) -> &str {
    session
        .provider_name
        .as_deref()
        .unwrap_or(DEFAULT_PROVIDER_ID)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionMeta<'a> {
    message_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    user_set_name: bool,
    session_type: String,
    gosling_mode: GoslingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_message_snippet: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    additional_working_dirs: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    restrict_tools_to_working_dirs: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    workspace_folder_roots: Vec<crate::workspace::WorkspaceFolderPolicyRoot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    credential_profile_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    credential_profile_name: Option<&'a str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    imported_untrusted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    import_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    import_original_working_dir: Option<String>,
    /// Set for Deep Research sessions so a client can recognise one it did not
    /// create in this window (resume from history, restart, another device).
    #[serde(skip_serializing_if = "Option::is_none")]
    research_library_path: Option<String>,
}

impl<'a> From<&'a Session> for SessionMeta<'a> {
    fn from(session: &'a Session) -> Self {
        let provenance = SessionImportProvenance::from_extension_data(&session.extension_data);
        let research =
            crate::session::DeepResearchState::from_extension_data(&session.extension_data);
        Self {
            message_count: session.message_count,
            created_at: session.created_at,
            last_message_at: session.last_message_at,
            archived_at: session.archived_at,
            user_set_name: session.user_set_name,
            session_type: session.session_type.to_string(),
            gosling_mode: session.gosling_mode,
            project_id: session.project_id.as_deref(),
            provider_id: session.provider_name.as_deref(),
            model_id: session
                .model_config
                .as_ref()
                .map(|mc| mc.model_name.as_str()),
            last_message_snippet: session.last_message_snippet.as_deref(),
            additional_working_dirs: session
                .additional_working_dirs
                .iter()
                .map(|dir| dir.to_string_lossy().to_string())
                .collect(),
            restrict_tools_to_working_dirs: session.restrict_tools_to_working_dirs,
            workspace_id: session.workspace_id.as_deref(),
            workspace_name: session.workspace_name.as_deref(),
            workspace_folder_roots: session
                .workspace_context
                .as_ref()
                .map(|context| context.effective_folder_policy().roots)
                .unwrap_or_default(),
            credential_profile_id: session.credential_profile_id.as_deref(),
            credential_profile_name: session.credential_profile_name.as_deref(),
            imported_untrusted: provenance.is_some(),
            import_source: provenance
                .as_ref()
                .map(|provenance| provenance.source_format.clone()),
            import_original_working_dir: provenance
                .and_then(|provenance| provenance.original_working_dir),
            research_library_path: research.map(|state| state.library_path),
        }
    }
}

pub(super) fn session_meta(session: &Session) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::to_value(SessionMeta::from(session)) {
        Ok(serde_json::Value::Object(meta)) => meta,
        _ => serde_json::Map::new(),
    }
}

pub(super) fn session_response_meta(
    session: &Session,
    extension_results: &[ExtensionLoadResult],
) -> serde_json::Map<String, serde_json::Value> {
    let mut meta = serde_json::Map::new();
    if let Ok(v) = serde_json::to_value(extension_results) {
        meta.insert("extensionResults".to_string(), v);
    }
    meta.insert(
        "workingDir".to_string(),
        serde_json::Value::String(session.working_dir.to_string_lossy().to_string()),
    );
    if let Some(workspace_id) = &session.workspace_id {
        meta.insert(
            "workspaceId".to_string(),
            serde_json::Value::String(workspace_id.clone()),
        );
    }
    if let Some(workspace_name) = &session.workspace_name {
        meta.insert(
            "workspaceName".to_string(),
            serde_json::Value::String(workspace_name.clone()),
        );
    }
    if let Some(provenance) = SessionImportProvenance::from_extension_data(&session.extension_data)
    {
        meta.insert(
            "importedUntrusted".to_string(),
            serde_json::Value::Bool(true),
        );
        meta.insert(
            "importSource".to_string(),
            serde_json::Value::String(provenance.source_format),
        );
    }
    meta
}

pub(super) fn build_session_info(session: Session) -> SessionInfo {
    let meta = session_meta(&session);
    let mut info = SessionInfo::new(SessionId::new(session.id), session.working_dir)
        .updated_at(session.updated_at.to_rfc3339())
        .meta(meta);
    if !session.name.is_empty() {
        info = info.title(session.name);
    }
    info
}

/// A model and its label, used to build the "model" session config option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModelOption {
    pub id: String,
    pub name: String,
}

/// The currently selected model and the set of available models for a session.
///
/// Replaces the removed `SessionModelState` ACP schema type; gosling now surfaces
/// model selection through the generic session config option API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModelSelection {
    pub current_model_id: String,
    pub available_models: Vec<ModelOption>,
}

pub(super) fn build_model_state(
    current_model: &str,
    inventory: &ProviderInventoryEntry,
) -> ModelSelection {
    let mut available_models = inventory
        .models
        .iter()
        .map(|model| ModelOption {
            id: model.id.clone(),
            name: model.name.clone(),
        })
        .collect::<Vec<_>>();
    if !available_models
        .iter()
        .any(|model| model.id == current_model)
    {
        available_models.insert(
            0,
            ModelOption {
                id: current_model.to_string(),
                name: current_model.to_string(),
            },
        );
    }
    ModelSelection {
        current_model_id: current_model.to_string(),
        available_models,
    }
}

struct ProviderOptionEntry {
    id: String,
    label: String,
}

async fn list_provider_entries(current_provider: Option<&str>) -> Vec<ProviderOptionEntry> {
    let mut providers = crate::providers::providers()
        .await
        .into_iter()
        .filter(|(metadata, _)| {
            !crate::providers::catalog::hide_from_automatic_provider_setup(&metadata.name)
        })
        .map(|(metadata, _)| ProviderOptionEntry {
            id: metadata.name,
            label: metadata.display_name,
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left.id.cmp(&right.id));
    providers.dedup_by(|left, right| left.id == right.id);

    if let Some(current_provider) = current_provider {
        if current_provider != DEFAULT_PROVIDER_ID
            && !crate::providers::catalog::hide_from_automatic_provider_setup(current_provider)
            && !providers
                .iter()
                .any(|provider| provider.id == current_provider)
        {
            providers.push(ProviderOptionEntry {
                id: current_provider.to_string(),
                label: current_provider.to_string(),
            });
            providers.sort_by(|left, right| left.id.cmp(&right.id));
        }
    }

    let mut entries = Vec::with_capacity(providers.len() + 1);
    entries.push(ProviderOptionEntry {
        id: DEFAULT_PROVIDER_ID.to_string(),
        label: DEFAULT_PROVIDER_LABEL.to_string(),
    });
    entries.extend(providers);
    entries
}

pub(super) async fn build_provider_options(
    current_provider: Option<&str>,
) -> Vec<SessionConfigSelectOption> {
    list_provider_entries(current_provider)
        .await
        .into_iter()
        .map(|provider| SessionConfigSelectOption::new(provider.id, provider.label))
        .collect()
}

pub(super) fn should_refresh_inventory_for_session_init(entry: &ProviderInventoryEntry) -> bool {
    entry.configured
        && entry.supports_refresh
        && (entry.last_updated_at.is_none() || ProviderInventoryService::is_stale(entry))
}

pub(super) fn build_mode_state(
    current_mode: GoslingMode,
    executes_tools_outside_gosling: bool,
) -> Result<SessionModeState, agent_client_protocol::Error> {
    let current_mode = compatible_mode(current_mode, executes_tools_outside_gosling);
    let mut available = Vec::with_capacity(GoslingMode::VARIANTS.len());
    for &name in GoslingMode::VARIANTS {
        let gosling_mode: GoslingMode = name.parse().map_err(|_| {
            agent_client_protocol::Error::internal_error() // impossible but satisfy linters
                .data(format!("Failed to parse GoslingMode variant: {}", name))
        })?;
        let mut mode = SessionMode::new(SessionModeId::new(name), name);
        mode.description = gosling_mode.get_message().map(Into::into);
        available.push(mode);
    }
    Ok(SessionModeState::new(
        SessionModeId::new(current_mode.to_string()),
        available,
    ))
}

pub(super) fn compatible_mode(
    mode: GoslingMode,
    _executes_tools_outside_gosling: bool,
) -> GoslingMode {
    mode
}

pub(super) async fn build_session_setup_config(
    provider_inventory: &ProviderInventoryService,
    session: &Session,
) -> Result<(SessionModeState, Option<Vec<SessionConfigOption>>), agent_client_protocol::Error> {
    let (Some(provider_name), Some(model_config)) = (
        session.provider_name.as_deref(),
        session.model_config.as_ref(),
    ) else {
        let mode_state = build_mode_state(session.gosling_mode, false)?;
        return Ok((mode_state, None));
    };
    let executes_tools_outside_gosling = crate::providers::get_from_registry(provider_name)
        .await
        .map(|entry| entry.executes_tools_outside_gosling())
        .unwrap_or(false);
    let mode_state = build_mode_state(session.gosling_mode, executes_tools_outside_gosling)?;
    let Some(inventory) = provider_inventory
        .find_entry_for_provider(provider_name)
        .await
    else {
        return Ok((mode_state, None));
    };
    let model_state = build_model_state(model_config.model_name.as_str(), &inventory);
    let provider_selection = session_provider_selection(session);
    let provider_options = build_provider_options(Some(provider_name)).await;
    let config_options = build_config_options(
        &mode_state,
        &model_state,
        model_config,
        provider_selection,
        provider_options,
    );
    Ok((mode_state, Some(config_options)))
}

pub(super) fn build_config_options(
    mode_state: &SessionModeState,
    model_state: &ModelSelection,
    model_config: &ModelConfig,
    provider_selection: &str,
    provider_options: Vec<SessionConfigSelectOption>,
) -> Vec<SessionConfigOption> {
    let mode_options: Vec<SessionConfigSelectOption> = mode_state
        .available_modes
        .iter()
        .map(|m| {
            SessionConfigSelectOption::new(m.id.0.clone(), m.name.clone())
                .description(m.description.clone())
        })
        .collect();
    let model_options: Vec<SessionConfigSelectOption> = model_state
        .available_models
        .iter()
        .map(|m| SessionConfigSelectOption::new(m.id.clone(), m.name.clone()))
        .collect();
    let thinking_effort_options = thinking_effort_values(model_config)
        .iter()
        .map(|effort| {
            let effort = effort.to_string();
            SessionConfigSelectOption::new(effort.clone(), effort)
        })
        .collect::<Vec<_>>();
    let current_thinking_effort = current_thinking_effort_value(model_config);
    vec![
        SessionConfigOption::select(
            "provider",
            "Provider",
            provider_selection.to_string(),
            provider_options,
        ),
        SessionConfigOption::select(
            "mode",
            "Mode",
            mode_state.current_mode_id.0.clone(),
            mode_options,
        )
        .category(SessionConfigOptionCategory::Mode),
        SessionConfigOption::select(
            "model",
            "Model",
            model_state.current_model_id.clone(),
            model_options,
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::select(
            "thinking_effort",
            "Thinking effort",
            current_thinking_effort,
            thinking_effort_options,
        )
        .description("Controls reasoning effort for models that support extended thinking.")
        .category(SessionConfigOptionCategory::ThoughtLevel),
    ]
}

fn thinking_effort_values(model_config: &ModelConfig) -> &'static [ThinkingEffort] {
    if model_config.is_reasoning_model() {
        &[
            ThinkingEffort::Off,
            ThinkingEffort::Low,
            ThinkingEffort::Medium,
            ThinkingEffort::High,
            ThinkingEffort::Max,
            ThinkingEffort::Ultra,
        ]
    } else {
        &[ThinkingEffort::Off]
    }
}

fn current_thinking_effort_value(model_config: &ModelConfig) -> String {
    if model_config.is_reasoning_model() {
        model_config
            .thinking_effort()
            .or_else(|| Config::global().get_gosling_thinking_effort())
            .map(|effort| effort.to_string())
            .unwrap_or_else(|| "off".to_string())
    } else {
        "off".to_string()
    }
}

fn slash_command_meta(entry: &SlashCommandEntry) -> serde_json::Map<String, serde_json::Value> {
    let mut meta = serde_json::Map::new();
    let command_type = match entry.source {
        SlashCommandSource::Builtin => "Builtin",
        SlashCommandSource::Skill => "Skill",
    };
    meta.insert(
        "commandType".to_string(),
        serde_json::Value::String(command_type.to_string()),
    );
    if let Some(source_path) = &entry.source_path {
        meta.insert(
            "sourcePath".to_string(),
            serde_json::Value::String(source_path.clone()),
        );
    }
    meta
}

fn slash_command_to_available_command(entry: SlashCommandEntry) -> AvailableCommand {
    let meta = slash_command_meta(&entry);
    let mut command = AvailableCommand::new(entry.name, entry.description);
    if let Some(input_hint) = entry.input_hint {
        command = command.input(AvailableCommandInput::Unstructured(
            UnstructuredCommandInput::new(input_hint),
        ));
    }
    command.meta(meta)
}

pub(super) fn available_commands_for_working_dir(
    working_dir: &std::path::Path,
) -> Vec<AvailableCommand> {
    available_commands_for_optional_working_dir(Some(working_dir))
}

pub(super) fn available_commands_for_optional_working_dir(
    working_dir: Option<&std::path::Path>,
) -> Vec<AvailableCommand> {
    crate::slash_commands::slash_command::list_acp_commands(working_dir)
        .into_iter()
        .map(slash_command_to_available_command)
        .collect()
}

fn available_commands_update(working_dir: &std::path::Path) -> AvailableCommandsUpdate {
    AvailableCommandsUpdate::new(available_commands_for_working_dir(working_dir))
}

pub(super) fn send_session_setup_notifications(
    cx: &ConnectionTo<Client>,
    session: &Session,
    supports_gosling_custom_notifications: bool,
) -> Result<(), agent_client_protocol::Error> {
    let session_id = SessionId::new(session.id.clone());
    if let Some(updates) = build_usage_updates(session) {
        if supports_gosling_custom_notifications {
            cx.send_notification(updates.custom)?;
        }
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::UsageUpdate(updates.standard),
        ))?;
    }
    cx.send_notification(SessionNotification::new(
        session_id,
        SessionUpdate::AvailableCommandsUpdate(available_commands_update(&session.working_dir)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::SessionConfigKind;
    use test_case::test_case;

    #[test]
    fn session_meta_exposes_pinned_folder_access() {
        let session = Session {
            workspace_context: Some(crate::workspace::WorkspaceSessionContext {
                folder_policy: crate::workspace::WorkspaceFolderPolicy {
                    roots: vec![crate::workspace::WorkspaceFolderPolicyRoot {
                        path: "/reference".into(),
                        access: crate::workspace::WorkspaceFolderAccess::Read,
                    }],
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let meta = session_meta(&session);
        assert_eq!(meta["workspaceFolderRoots"][0]["access"], "read");
    }

    #[test]
    fn session_meta_marks_deep_research_sessions() {
        let mut session = Session::default();
        assert!(session_meta(&session).get("researchLibraryPath").is_none());

        crate::session::DeepResearchState {
            library_path: "/library".into(),
            output_paths: vec!["/outputs".into()],
        }
        .to_extension_data(&mut session.extension_data)
        .unwrap();
        assert_eq!(
            session_meta(&session)
                .get("researchLibraryPath")
                .and_then(|value| value.as_str()),
            Some("/library")
        );
    }

    fn model_selection(current: &str, models: &[&str]) -> ModelSelection {
        ModelSelection {
            current_model_id: current.to_string(),
            available_models: models
                .iter()
                .map(|m| ModelOption {
                    id: m.to_string(),
                    name: m.to_string(),
                })
                .collect(),
        }
    }

    #[test_case(
        vec!["model-a".into(), "model-b".into()]
        => model_selection("unused", &["unused", "model-a", "model-b"])
        ; "returns current and available models"
    )]
    #[test_case(
        vec![]
        => model_selection("unused", &["unused"])
        ; "empty model list"
    )]
    fn test_build_model_state(models: Vec<String>) -> ModelSelection {
        let inventory = ProviderInventoryEntry {
            provider_id: "mock".to_string(),
            provider_name: "Mock".to_string(),
            description: "Mock".to_string(),
            default_model: "unused".to_string(),
            configured: true,
            provider_type: crate::providers::base::ProviderType::Builtin,
            category: crate::providers::catalog::ProviderSetupCategory::Model,
            config_keys: vec![],
            setup_steps: vec![],
            supports_refresh: true,
            refreshing: false,
            models: models
                .into_iter()
                .map(|id| crate::providers::inventory::InventoryModel {
                    name: id.clone(),
                    id,
                    family: None,
                    context_limit: None,
                    reasoning: None,
                    recommended: false,
                })
                .collect(),
            last_updated_at: None,
            last_refresh_attempt_at: None,
            last_refresh_error: None,
            model_selection_hint: None,
            manages_own_context: false,
        };
        build_model_state("unused", &inventory)
    }

    #[test_case(GoslingMode::Auto, "auto"; "auto mode")]
    #[test_case(GoslingMode::Approve, "approve"; "approve mode")]
    fn test_build_mode_state(current_mode: GoslingMode, expected_current_mode: &str) {
        let mode_state = build_mode_state(current_mode, false).unwrap();

        assert_eq!(mode_state.current_mode_id.0.as_ref(), expected_current_mode);
        assert_eq!(
            mode_state
                .available_modes
                .iter()
                .map(|mode| mode.id.0.as_ref())
                .collect::<Vec<_>>(),
            vec!["auto", "smart_approve", "approve", "chat"]
        );
        assert_eq!(
            mode_state
                .available_modes
                .iter()
                .map(|mode| mode.description.as_deref())
                .collect::<Vec<_>>(),
            vec![
                Some("Automatically approve tool calls"),
                Some("Ask only for sensitive tool calls"),
                Some("Ask before every tool call"),
                Some("Chat only, no tool calls"),
            ]
        );
    }

    #[test]
    fn test_build_mode_state_keeps_auto_for_external_tool_provider() {
        let mode_state = build_mode_state(GoslingMode::Auto, true).unwrap();

        assert_eq!(mode_state.current_mode_id.0.as_ref(), "auto");
        assert_eq!(
            mode_state
                .available_modes
                .iter()
                .map(|mode| mode.id.0.as_ref())
                .collect::<Vec<_>>(),
            vec!["auto", "smart_approve", "approve", "chat"]
        );
    }

    #[test]
    fn test_slash_command_to_available_command_maps_core_fields_to_acp() {
        let cases = [
            (SlashCommandSource::Builtin, "Builtin", None),
            (
                SlashCommandSource::Skill,
                "Skill",
                Some("/tmp/release.md".to_string()),
            ),
        ];

        for (source, expected_command_type, expected_source_path) in cases {
            let command = slash_command_to_available_command(SlashCommandEntry {
                name: "release".to_string(),
                description: "Run release workflow".to_string(),
                source,
                source_path: expected_source_path.clone(),
                input_hint: Some("[task]".to_string()),
            });

            assert_eq!(command.name, "release");
            assert_eq!(command.description, "Run release workflow");

            match command.input.as_ref() {
                Some(AvailableCommandInput::Unstructured(input)) => {
                    assert_eq!(input.hint, "[task]");
                }
                other => panic!("unexpected command input: {other:?}"),
            }

            let meta = command.meta.as_ref().expect("command _meta");
            let expected_command_type = serde_json::json!(expected_command_type);
            assert_eq!(meta.get("commandType"), Some(&expected_command_type));
            if let Some(source_path) = expected_source_path {
                let expected_source_path = serde_json::json!(source_path);
                assert_eq!(meta.get("sourcePath"), Some(&expected_source_path));
            } else {
                assert!(meta.get("sourcePath").is_none());
            }
        }
    }

    #[test_case(
        build_mode_state(GoslingMode::Auto, false).unwrap(),
        "openai",
        vec![
            SessionConfigSelectOption::new("anthropic", "anthropic"),
            SessionConfigSelectOption::new("openai", "openai"),
        ],
        model_selection("gpt-4", &["gpt-4", "gpt-3.5"]),
        "auto",
        vec!["anthropic", "openai"],
        vec!["gpt-4", "gpt-3.5"]
        ; "auto mode with multiple models"
    )]
    #[test_case(
        build_mode_state(GoslingMode::Approve, false).unwrap(),
        "openai",
        vec![SessionConfigSelectOption::new("openai", "openai")],
        model_selection("only-model", &["only-model"]),
        "approve",
        vec!["openai"],
        vec!["only-model"]
        ; "approve mode with single model"
    )]
    fn test_build_config_options(
        mode_state: SessionModeState,
        provider_name: &'static str,
        provider_options: Vec<SessionConfigSelectOption>,
        model_state: ModelSelection,
        expected_mode: &str,
        expected_provider_values: Vec<&str>,
        expected_model_values: Vec<&str>,
    ) {
        let model_config = ModelConfig::new(model_state.current_model_id.as_str())
            .with_merged_request_params(std::collections::HashMap::from([(
                "thinking_effort".to_string(),
                serde_json::json!("off"),
            )]));

        let options = build_config_options(
            &mode_state,
            &model_state,
            &model_config,
            provider_name,
            provider_options,
        );

        assert_eq!(
            options
                .iter()
                .map(|option| option.id.0.as_ref())
                .collect::<Vec<_>>(),
            vec!["provider", "mode", "model", "thinking_effort"]
        );

        let provider = match &options[0].kind {
            SessionConfigKind::Select(select) => select,
            _ => panic!("provider should be a select option"),
        };
        assert_eq!(provider.current_value.0.as_ref(), provider_name);
        assert_eq!(
            select_option_values(&provider.options),
            expected_provider_values
        );

        let mode = match &options[1].kind {
            SessionConfigKind::Select(select) => select,
            _ => panic!("mode should be a select option"),
        };
        assert_eq!(mode.current_value.0.as_ref(), expected_mode);
        assert_eq!(
            select_option_values(&mode.options),
            vec!["auto", "smart_approve", "approve", "chat"]
        );

        let model = match &options[2].kind {
            SessionConfigKind::Select(select) => select,
            _ => panic!("model should be a select option"),
        };
        assert_eq!(model.current_value.0.as_ref(), model_state.current_model_id);
        assert_eq!(select_option_values(&model.options), expected_model_values);

        let thinking_effort = match &options[3].kind {
            SessionConfigKind::Select(select) => select,
            _ => panic!("thinking_effort should be a select option"),
        };
        assert_eq!(thinking_effort.current_value.0.as_ref(), "off");
        assert_eq!(select_option_values(&thinking_effort.options), vec!["off"]);
    }

    fn select_option_values(
        options: &agent_client_protocol::schema::v1::SessionConfigSelectOptions,
    ) -> Vec<&str> {
        match options {
            agent_client_protocol::schema::v1::SessionConfigSelectOptions::Ungrouped(options) => {
                options
                    .iter()
                    .map(|option| option.value.0.as_ref())
                    .collect()
            }
            agent_client_protocol::schema::v1::SessionConfigSelectOptions::Grouped(_) => {
                panic!("expected ungrouped select options")
            }
            _ => panic!("unexpected select option shape"),
        }
    }

    #[test]
    fn test_build_config_options_uses_current_thinking_effort() {
        let mode_state = build_mode_state(GoslingMode::Auto, false).unwrap();
        let model_state = model_selection("claude-sonnet-4", &["claude-sonnet-4"]);
        let model_config = ModelConfig::new("claude-sonnet-4").with_merged_request_params(
            std::collections::HashMap::from([(
                "thinking_effort".to_string(),
                serde_json::json!("high"),
            )]),
        );

        let options = build_config_options(
            &mode_state,
            &model_state,
            &model_config,
            "openai",
            vec![SessionConfigSelectOption::new("openai", "openai")],
        );
        let option = options
            .iter()
            .find(|option| option.id.0.as_ref() == "thinking_effort")
            .expect("thinking_effort option");
        let select = match &option.kind {
            SessionConfigKind::Select(select) => select,
            _ => panic!("thinking_effort should be a select option"),
        };

        assert_eq!(select.current_value.0.as_ref(), "high");
        assert_eq!(
            select_option_values(&select.options),
            vec!["off", "low", "medium", "high", "max", "ultra"]
        );
    }

    #[test]
    fn test_build_config_options_masks_non_reasoning_thinking_effort() {
        let mode_state = build_mode_state(GoslingMode::Auto, false).unwrap();
        let model_state = model_selection("gpt-4", &["gpt-4"]);
        let mut model_config =
            ModelConfig::new("gpt-4").with_merged_request_params(std::collections::HashMap::from(
                [("thinking_effort".to_string(), serde_json::json!("high"))],
            ));
        model_config.reasoning = Some(false);

        let options = build_config_options(
            &mode_state,
            &model_state,
            &model_config,
            "openai",
            vec![SessionConfigSelectOption::new("openai", "openai")],
        );
        let option = options
            .iter()
            .find(|option| option.id.0.as_ref() == "thinking_effort")
            .expect("thinking_effort option");
        let select = match &option.kind {
            SessionConfigKind::Select(select) => select,
            _ => panic!("thinking_effort should be a select option"),
        };

        assert_eq!(select.current_value.0.as_ref(), "off");
        assert_eq!(
            select.options,
            agent_client_protocol::schema::v1::SessionConfigSelectOptions::Ungrouped(vec![
                SessionConfigSelectOption::new("off", "off")
            ])
        );
    }
}
