use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::subagent_handler::{
    run_subagent_task, OnMessageCallback, SubagentRunParams, SubagentTask,
};
use crate::agents::subagent_task_config::{TaskConfig, DEFAULT_SUBAGENT_MAX_TURNS};
use crate::agents::tool_execution::ToolCallContext;
use crate::agents::AgentConfig;
use crate::config::paths::Paths;
use crate::config::{Config, GoslingMode};
use crate::providers;
use crate::session::extension_data::EnabledExtensionsState;
use crate::session::SessionType;
use crate::sources::parse_frontmatter;
use crate::utils::safe_truncate;
use anyhow::Result;
use async_trait::async_trait;
use gosling_sdk_types::custom_requests::{SourceEntry, SourceType};
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult, Meta,
    ServerCapabilities, ServerNotification, Tool,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex, OwnedSemaphorePermit, Semaphore};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

mod async_delegation;
mod delegate_config;
mod delegation;
mod loading;
mod mcp;
mod source_discovery;
mod task_tracking;

#[cfg(test)]
use delegate_config::{
    delegate_mode, delegate_mode_notice, resolve_working_dir, sync_delegate_timeout_from,
};
pub use source_discovery::discover_filesystem_sources;
use source_discovery::{
    build_instructions_with_context, build_subagent_instructions, delegate_authority_summary,
    kind_plural, resolve_delegate_extensions, validate_capability_policy, AgentMetadata,
    DelegateSpec,
};
#[cfg(test)]
use source_discovery::{parse_agent_content, scan_agents_from_dir, DelegateCapabilityPolicy};
use task_tracking::{current_epoch_millis, is_session_id, max_background_tasks, round_duration};
pub use task_tracking::{BackgroundTask, CompletedTask};

// This compatibility facade preserves the original `summon` module path and public names while
// cohesive implementation seams live in child modules.

pub static EXTENSION_NAME: &str = "summon";

const SUBAGENT_DESCRIPTION_BUDGET: usize = 160;

const TASK_LABEL_BUDGET: usize = 60;

#[derive(Debug, Default, Deserialize)]
pub struct DelegateParams {
    pub instructions: Option<String>,
    pub source: Option<String>,
    pub extensions: Option<Vec<String>>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_turns: Option<usize>,
    pub context: Option<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub r#async: bool,
}

impl DelegateParams {
    fn normalize(mut self) -> Self {
        self.instructions = non_blank(self.instructions);
        self.source = non_blank(self.source);
        self.context = non_blank(self.context);
        if self.instructions.is_some()
            && self.provider.is_some()
            && self.model.is_some()
            && self.source.as_deref() == Some("dummy")
        {
            self.source = None;
        }
        self
    }
}

fn non_blank(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub struct SummonClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
    source_cache: Mutex<Option<(Instant, PathBuf, Vec<SourceEntry>)>>,
    background_task_slots: Arc<Semaphore>,
    max_background_tasks: usize,
    background_tasks: Mutex<HashMap<String, BackgroundTask>>,
    completed_tasks: Mutex<HashMap<String, CompletedTask>>,
    notification_subscribers: Arc<Mutex<Vec<mpsc::Sender<ServerNotification>>>>,
}

impl SummonClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        Self::with_background_task_limit(context, max_background_tasks())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_context() -> PlatformExtensionContext {
        PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(crate::session::SessionManager::instance()),
            session: None,
            use_login_shell_path: false,
            code_execution_runtime: crate::config::CodeExecutionRuntime::Enabled,
        }
    }

    fn test_extension(name: &str) -> crate::agents::ExtensionConfig {
        crate::agents::ExtensionConfig::Builtin {
            name: name.to_string(),
            description: name.to_string(),
            display_name: None,
            timeout: None,
            bundled: None,
            available_tools: Vec::new(),
        }
    }

    #[test]
    fn test_original_module_path_compatibility_facade() {
        let _discover: fn(&Path) -> Vec<SourceEntry> =
            crate::agents::platform_extensions::summon::discover_filesystem_sources;
        let _params = crate::agents::platform_extensions::summon::DelegateParams::default();
        let _background_task_size =
            std::mem::size_of::<crate::agents::platform_extensions::summon::BackgroundTask>();
        let _completed_task_size =
            std::mem::size_of::<crate::agents::platform_extensions::summon::CompletedTask>();
        let _client =
            crate::agents::platform_extensions::summon::SummonClient::new(create_test_context())
                .unwrap();

        assert_eq!(
            crate::agents::platform_extensions::summon::EXTENSION_NAME,
            "summon"
        );
    }

    #[test]
    fn test_agent_frontmatter_parsing() {
        let agent = r#"---
name: reviewer
model: sonnet
---
You review code."#;
        let source = parse_agent_content(agent, Path::new(""), false).unwrap();
        assert_eq!(source.name, "reviewer");
        assert!(source.description.contains("sonnet"));
    }

    #[test]
    fn test_delegate_capability_policy_is_versioned_and_deduplicated() {
        let extensions = validate_capability_policy(Some(DelegateCapabilityPolicy {
            version: 1,
            extensions: vec![
                "developer".to_string(),
                "summarize".to_string(),
                "developer".to_string(),
            ],
        }))
        .unwrap();
        assert_eq!(extensions, vec!["developer", "summarize"]);

        let error = validate_capability_policy(Some(DelegateCapabilityPolicy {
            version: 2,
            extensions: Vec::new(),
        }))
        .unwrap_err();
        assert!(error.contains("version 2"));
    }

    #[test]
    fn test_adhoc_delegate_defaults_to_no_extensions() {
        let parent = vec![test_extension("developer"), test_extension("summarize")];
        let resolved = resolve_delegate_extensions(parent, &DelegateSpec::default(), None).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_source_delegate_is_bounded_by_role_and_explicit_request() {
        let parent = vec![
            test_extension("developer"),
            test_extension("summarize"),
            test_extension("summon"),
        ];
        let spec = DelegateSpec {
            role_extensions: Some(vec!["developer".to_string(), "summarize".to_string()]),
            ..Default::default()
        };

        let role_default = resolve_delegate_extensions(parent.clone(), &spec, None).unwrap();
        assert_eq!(
            role_default
                .iter()
                .map(|ext| ext.name())
                .collect::<Vec<_>>(),
            vec!["developer", "summarize"]
        );

        let narrowed =
            resolve_delegate_extensions(parent.clone(), &spec, Some(&["summarize".to_string()]))
                .unwrap();
        assert_eq!(narrowed[0].name(), "summarize");

        let error =
            resolve_delegate_extensions(parent, &spec, Some(&["summon".to_string()])).unwrap_err();
        assert!(error.contains("outside the role capability policy"));
    }

    #[test]
    fn test_delegate_extension_must_exist_in_parent_session() {
        let error = resolve_delegate_extensions(
            vec![test_extension("developer")],
            &DelegateSpec::default(),
            Some(&["summarize".to_string()]),
        )
        .unwrap_err();
        assert!(error.contains("unavailable in the parent session"));
    }

    #[tokio::test]
    async fn test_legacy_source_without_capability_policy_gets_no_extensions() {
        let temp_dir = TempDir::new().unwrap();
        let agents = temp_dir.path().join(".gosling/agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("reviewer.md"),
            "---\nname: reviewer\n---\nReview without tools.",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let params = DelegateParams {
            source: Some("reviewer".to_string()),
            ..Default::default()
        };
        let spec = client
            .build_delegate_spec(&params, temp_dir.path())
            .await
            .unwrap();
        assert_eq!(spec.role_extensions, Some(Vec::new()));
    }

    #[tokio::test]
    async fn test_repo_committed_agent_capability_policy_is_ignored() {
        // AOC-GOS-004: a repo-committed agent file cannot grant itself
        // extensions by declaring a `capabilities` policy, even one the
        // parent session happens to have enabled.
        let temp_dir = TempDir::new().unwrap();
        let agents = temp_dir.path().join(".gosling/agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("helper.md"),
            "---\nname: helper\ncapabilities:\n  version: 1\n  extensions: [developer]\n---\nHelp.",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let params = DelegateParams {
            source: Some("helper".to_string()),
            ..Default::default()
        };
        let spec = client
            .build_delegate_spec(&params, temp_dir.path())
            .await
            .unwrap();
        assert_eq!(spec.role_extensions, Some(Vec::new()));

        let resolved =
            resolve_delegate_extensions(vec![test_extension("developer")], &spec, None).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_global_agent_capability_policy_is_honored() {
        // Companion to the untrusted-source test above: an operator-authored
        // global agent file (`source.global == true`) is still trusted to
        // declare a capability policy, so `build_spec_from_agent` itself
        // must honor it rather than only the discovery-layer flag.
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("helper.md");
        fs::write(
            &path,
            "---\nname: helper\ncapabilities:\n  version: 1\n  extensions: [developer]\n---\nHelp.",
        )
        .unwrap();

        let source = SourceEntry {
            source_type: SourceType::Agent,
            name: "helper".to_string(),
            description: "Global helper".to_string(),
            content: "Help.".to_string(),
            path: path.to_string_lossy().into_owned(),
            global: true,
            writable: true,
            supporting_files: Vec::new(),
            properties: std::collections::HashMap::new(),
        };

        let client = SummonClient::new(create_test_context()).unwrap();
        let spec = client
            .build_spec_from_agent(&source, &DelegateParams::default())
            .unwrap();
        assert_eq!(spec.role_extensions, Some(vec!["developer".to_string()]));
    }

    /// REL-GSL-004. Resolved against an isolated config rather than
    /// `Config::global()`: the global singleton reads this machine's real
    /// settings file, so asserting a default against it would fail on any
    /// operator who has set the key (the REL-CI-002 defect class).
    fn isolated_config() -> Config {
        let config_file = tempfile::NamedTempFile::new().unwrap();
        let secrets_file = tempfile::NamedTempFile::new().unwrap();
        Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
    }

    #[test]
    #[serial]
    fn sync_delegate_timeout_defaults_to_thirty_minutes() {
        let _guard = env_lock::lock_env([("GOSLING_SYNC_DELEGATE_TIMEOUT_SECS", None::<&str>)]);
        assert_eq!(
            sync_delegate_timeout_from(&isolated_config()),
            Some(Duration::from_secs(1800))
        );
    }

    #[test]
    #[serial]
    fn sync_delegate_timeout_is_configurable_and_zero_opts_out() {
        let config = isolated_config();
        {
            let _guard = env_lock::lock_env([("GOSLING_SYNC_DELEGATE_TIMEOUT_SECS", Some("45"))]);
            assert_eq!(
                sync_delegate_timeout_from(&config),
                Some(Duration::from_secs(45))
            );
        }
        let _guard = env_lock::lock_env([("GOSLING_SYNC_DELEGATE_TIMEOUT_SECS", Some("0"))]);
        assert_eq!(
            sync_delegate_timeout_from(&config),
            None,
            "0 means the operator opted out of the bound entirely"
        );
    }

    #[test]
    fn test_resolve_working_dir_relative_subdir() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().canonicalize().unwrap();
        let subdir = parent.join("sub");
        fs::create_dir(&subdir).unwrap();

        let resolved = resolve_working_dir(&parent, "sub").unwrap();
        assert_eq!(resolved, subdir.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_working_dir_rejects_traversal_outside_parent() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().join("parent");
        let sibling = temp_dir.path().join("sibling");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&sibling).unwrap();

        let err = resolve_working_dir(&parent, "../sibling").unwrap_err();
        assert!(
            err.to_string()
                .contains("outside the parent session directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_resolve_working_dir_rejects_file_path() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().canonicalize().unwrap();
        let file = parent.join("a.txt");
        fs::write(&file, "hello").unwrap();

        let err = resolve_working_dir(&parent, "a.txt").unwrap_err();
        assert!(
            err.to_string().contains("is not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_resolve_working_dir_rejects_nonexistent_path() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().canonicalize().unwrap();

        let err = resolve_working_dir(&parent, "does-not-exist").unwrap_err();
        assert!(
            err.to_string().contains("could not be resolved"),
            "unexpected error: {err}"
        );
    }
    #[test]
    fn test_agent_scan_skips_non_agent_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let agents_dir = temp_dir.path().join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(
            agents_dir.join("README.md"),
            "---\ntitle: Notes\n---\nThis is not an agent.",
        )
        .unwrap();
        fs::write(
            agents_dir.join("notes.md"),
            "---\nauthor: someone\ntags: [docs]\n---\nJust documentation.",
        )
        .unwrap();
        fs::write(
            agents_dir.join("reviewer.md"),
            "---\nname: reviewer\nmodel: sonnet\n---\nYou review code.",
        )
        .unwrap();
        fs::write(agents_dir.join("plain.md"), "No frontmatter at all.").unwrap();
        fs::write(
            agents_dir.join("broken.md"),
            "---\nname: [unterminated\n---\nBroken YAML.",
        )
        .unwrap();

        let mut sources = Vec::new();
        let mut seen = HashSet::new();
        scan_agents_from_dir(&agents_dir, &mut sources, &mut seen, false);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "reviewer");
    }

    #[tokio::test]
    async fn test_discover_agents() {
        let temp_dir = TempDir::new().unwrap();

        let agents = temp_dir.path().join(".gosling/agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("reviewer.md"),
            "---\nname: reviewer\nmodel: sonnet\ndescription: Code reviewer\n---\nYou review code.",
        )
        .unwrap();

        let sources = discover_filesystem_sources(temp_dir.path());

        let agent = sources
            .iter()
            .find(|s| s.name == "reviewer" && s.source_type == SourceType::Agent)
            .unwrap();
        assert_eq!(agent.description, "Code reviewer");
        assert!(agent.content.contains("You review code"));
    }

    #[tokio::test]
    async fn test_agent_deduplication_local_wins() {
        let temp_dir = TempDir::new().unwrap();

        let local = temp_dir.path().join(".gosling/agents");
        fs::create_dir_all(&local).unwrap();
        fs::write(
            local.join("reviewer.md"),
            "---\nname: reviewer\ndescription: Local reviewer\n---\nlocal steps",
        )
        .unwrap();

        let also_local = temp_dir.path().join(".agents/agents");
        fs::create_dir_all(&also_local).unwrap();
        fs::write(
            also_local.join("reviewer.md"),
            "---\nname: reviewer\ndescription: Agents reviewer\n---\nagents steps",
        )
        .unwrap();

        let sources = discover_filesystem_sources(temp_dir.path());

        let reviewers: Vec<_> = sources.iter().filter(|s| s.name == "reviewer").collect();
        assert_eq!(reviewers.len(), 1);
    }

    #[tokio::test]
    async fn test_load_agent_source() {
        let temp_dir = TempDir::new().unwrap();

        let agents = temp_dir.path().join(".gosling/agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("reviewer.md"),
            "---\nname: reviewer\nmodel: sonnet\ndescription: Code reviewer\n---\nYou review code carefully.",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let result = client
            .handle_load_source("reviewer", temp_dir.path())
            .await
            .unwrap();

        let text = &result[0].as_text().expect("expected text content").text;
        assert!(text.contains("reviewer"));
        assert!(text.contains("You review code carefully"));
        assert!(text.contains("now available in your context"));
    }

    #[tokio::test]
    async fn test_load_nonexistent_source_suggests_similar() {
        let temp_dir = TempDir::new().unwrap();

        let agents = temp_dir.path().join(".gosling/agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("deploy.md"),
            "---\nname: deploy\ndescription: Deploy to production\n---\nsteps",
        )
        .unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let err = client
            .handle_load_source("deploy-prod", temp_dir.path())
            .await
            .unwrap_err();

        assert!(err.contains("not found"));
        assert!(err.contains("deploy"), "should suggest 'deploy': {}", err);
    }

    #[tokio::test]
    async fn test_load_completely_unknown_source() {
        let temp_dir = TempDir::new().unwrap();

        let client = SummonClient::new(create_test_context()).unwrap();
        let err = client
            .handle_load_source("zzz-nonexistent", temp_dir.path())
            .await
            .unwrap_err();

        assert!(err.contains("not found"));
        assert!(err.contains("Use load()"));
    }

    #[tokio::test]
    async fn test_client_tools_and_unknown_tool() {
        let client = SummonClient::new(create_test_context()).unwrap();

        let result = client
            .list_tools("test", None, CancellationToken::new())
            .await
            .unwrap();
        let names: Vec<_> = result.tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&"load") && names.contains(&"delegate"));

        let ctx = ToolCallContext::new("test".to_string(), None, None);
        let result = client
            .call_tool(&ctx, "unknown", None, CancellationToken::new())
            .await
            .unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn test_duration_rounding_for_moim() {
        assert_eq!(round_duration(Duration::from_secs(5)), "0s");
        assert_eq!(round_duration(Duration::from_secs(15)), "10s");
        assert_eq!(round_duration(Duration::from_secs(59)), "50s");

        assert_eq!(round_duration(Duration::from_secs(60)), "1m");
        assert_eq!(round_duration(Duration::from_secs(90)), "1m");
        assert_eq!(round_duration(Duration::from_secs(120)), "2m");
    }

    #[test]
    fn test_task_description_formatting() {
        let make_params = |source: Option<&str>, instructions: Option<&str>| DelegateParams {
            source: source.map(String::from),
            instructions: instructions.map(String::from),
            ..Default::default()
        };

        assert_eq!(
            SummonClient::get_task_description(&make_params(Some("reviewer"), None)),
            "reviewer"
        );
        assert_eq!(
            SummonClient::get_task_description(&make_params(None, Some("do stuff"))),
            "do stuff"
        );
        assert_eq!(
            SummonClient::get_task_description(&make_params(Some("r"), Some("task"))),
            "r: task"
        );
        assert_eq!(
            SummonClient::get_task_description(&make_params(None, None)),
            "Unknown task"
        );
    }

    #[tokio::test]
    async fn test_context_injected_into_adhoc_spec() {
        let temp_dir = TempDir::new().unwrap();
        let client = SummonClient::new(create_test_context()).unwrap();

        let params = DelegateParams {
            instructions: Some("do the task".to_string()),
            context: Some("background info".to_string()),
            ..Default::default()
        };

        let spec = client
            .build_delegate_spec(&params, temp_dir.path())
            .await
            .unwrap();

        assert_eq!(
            spec.instructions.as_deref(),
            Some("# Reference Context\n\nbackground info")
        );
        assert_eq!(spec.prompt.as_deref(), Some("do the task"));
    }

    #[test]
    fn test_build_instructions_with_context_wraps_existing_instructions() {
        assert_eq!(
            build_instructions_with_context("background info", "Run deploy steps"),
            "# Reference Context\n\nbackground info\n\n# Task Instructions\n\nRun deploy steps"
        );
        assert_eq!(
            build_instructions_with_context("background info", ""),
            "# Reference Context\n\nbackground info"
        );
    }

    #[test]
    fn test_validate_delegate_params_rejects_zero_max_turns() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        let params = DelegateParams {
            instructions: Some("do something".to_string()),
            max_turns: Some(0),
            ..Default::default()
        };
        let result = client.validate_delegate_params(&params);
        assert_eq!(result, Err("'max_turns' must be at least 1".to_string()));
    }

    #[test]
    fn test_validate_delegate_params_accepts_positive_max_turns() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        let params = DelegateParams {
            instructions: Some("do something".to_string()),
            max_turns: Some(5),
            ..Default::default()
        };
        assert!(client.validate_delegate_params(&params).is_ok());
    }

    #[test]
    fn test_delegate_params_normalize_blank_optional_strings() {
        let params = DelegateParams {
            instructions: Some("  research this  ".to_string()),
            source: Some("   ".to_string()),
            context: Some("  supporting context  ".to_string()),
            ..Default::default()
        }
        .normalize();

        assert_eq!(params.instructions.as_deref(), Some("research this"));
        assert_eq!(params.source, None);
        assert_eq!(params.context.as_deref(), Some("supporting context"));
    }

    #[test]
    fn test_delegate_params_normalize_dummy_source_for_explicit_adhoc_model() {
        let params = DelegateParams {
            instructions: Some("research independently".to_string()),
            source: Some("dummy".to_string()),
            provider: Some("claude-code".to_string()),
            model: Some("claude-opus-5".to_string()),
            ..Default::default()
        }
        .normalize();

        assert_eq!(params.source, None);
    }

    #[test]
    fn test_delegate_params_preserve_dummy_named_source_without_explicit_adhoc_model() {
        let params = DelegateParams {
            instructions: Some("run the named agent".to_string()),
            source: Some("dummy".to_string()),
            ..Default::default()
        }
        .normalize();

        assert_eq!(params.source.as_deref(), Some("dummy"));
    }

    #[test]
    fn test_delegate_mode_disables_tools_for_external_tool_providers() {
        assert_eq!(delegate_mode(false), GoslingMode::Auto);
        assert_eq!(delegate_mode(true), GoslingMode::Chat);
        assert!(delegate_mode_notice(GoslingMode::Auto).is_empty());
        assert!(delegate_mode_notice(GoslingMode::Chat).contains("tool calls are disabled"));
    }

    #[tokio::test]
    async fn test_delegate_schema_constrains_source_and_documents_adhoc_shape() {
        let client = SummonClient::new(create_test_context()).unwrap();
        let tool = client.create_delegate_tool();
        let schema = serde_json::Value::Object(tool.input_schema.as_ref().clone());

        assert_eq!(schema["properties"]["source"]["minLength"], 1);
        assert_eq!(schema["properties"]["instructions"]["minLength"], 1);
        assert!(schema["properties"]["source"]["description"]
            .as_str()
            .unwrap()
            .contains("omit this argument entirely"));
        assert!(schema["properties"]["source"]["description"]
            .as_str()
            .unwrap()
            .contains("dummy"));
    }

    #[test]
    #[serial]
    fn test_resolve_max_turns_falls_back_to_env_var() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        std::env::set_var("GOSLING_SUBAGENT_MAX_TURNS", "7");
        let result = client.resolve_max_turns();
        std::env::remove_var("GOSLING_SUBAGENT_MAX_TURNS");

        assert_eq!(
            result, 7,
            "should fall back to GOSLING_SUBAGENT_MAX_TURNS env var"
        );
    }

    #[test]
    #[serial]
    fn test_resolve_max_turns_falls_back_to_default() {
        let context = create_test_context();
        let client = SummonClient::new(context).unwrap();

        std::env::remove_var("GOSLING_SUBAGENT_MAX_TURNS");
        let result = client.resolve_max_turns();

        assert_eq!(
            result,
            crate::agents::subagent_task_config::DEFAULT_SUBAGENT_MAX_TURNS,
            "should fall back to DEFAULT_SUBAGENT_MAX_TURNS"
        );
    }

    fn empty_spec() -> DelegateSpec {
        DelegateSpec::default()
    }

    const PARENT_MODEL: &str = "claude-3-5-sonnet-20241022";
    const OVERRIDE_MODEL: &str = "claude-opus-4-6";
    const PROVIDER: &str = "anthropic";

    fn session_with(parent: gosling_providers::model::ModelConfig) -> crate::session::Session {
        crate::session::Session {
            provider_name: Some(PROVIDER.to_string()),
            model_config: Some(parent),
            ..Default::default()
        }
    }

    fn resolve_with_override(
        model: Option<&str>,
        parent: gosling_providers::model::ModelConfig,
    ) -> gosling_providers::model::ModelConfig {
        let client = SummonClient::new(create_test_context()).unwrap();
        let params = DelegateParams {
            model: model.map(String::from),
            ..Default::default()
        };
        client
            .resolve_model_config(&params, &empty_spec(), &session_with(parent), PROVIDER)
            .expect("resolve_model_config")
    }

    fn parent_config() -> gosling_providers::model::ModelConfig {
        gosling_providers::model::ModelConfig::new(PARENT_MODEL).with_canonical_limits(PROVIDER)
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_applies_canonical_limits_to_overridden_model() {
        let _env = env_lock::lock_env([
            ("GOSLING_CONTEXT_LIMIT", None::<&str>),
            ("GOSLING_MAX_TOKENS", None::<&str>),
            ("GOSLING_SUBAGENT_MODEL", None::<&str>),
        ]);

        let parent = parent_config();
        let overridden = gosling_providers::model::ModelConfig::new(OVERRIDE_MODEL)
            .with_canonical_limits(PROVIDER);
        assert_ne!(parent.context_limit, overridden.context_limit);
        assert_ne!(parent.reasoning, overridden.reasoning);

        let resolved = resolve_with_override(Some(OVERRIDE_MODEL), parent);

        assert_eq!(resolved.model_name, OVERRIDE_MODEL);
        assert_eq!(resolved.context_limit, overridden.context_limit);
        assert_eq!(resolved.max_tokens, overridden.max_tokens);
        assert_eq!(resolved.reasoning, overridden.reasoning);
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_model_config_preserves_parent_request_params_on_override() {
        let _env = env_lock::lock_env([
            ("GOSLING_CONTEXT_LIMIT", None::<&str>),
            ("GOSLING_MAX_TOKENS", None::<&str>),
            ("GOSLING_SUBAGENT_MODEL", None::<&str>),
        ]);

        let mut parent = parent_config();
        parent.request_params = Some(HashMap::from([(
            "anthropic_beta".to_string(),
            serde_json::json!("custom-beta-header"),
        )]));

        let resolved = resolve_with_override(Some(OVERRIDE_MODEL), parent);

        assert_eq!(
            resolved
                .request_params
                .as_ref()
                .and_then(|p| p.get("anthropic_beta")),
            Some(&serde_json::json!("custom-beta-header")),
        );
    }

    fn extract_text(content: &Content) -> &str {
        use rmcp::model::RawContent;
        match &content.raw {
            RawContent::Text(t) => t.text.as_str(),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_is_session_id() {
        assert!(is_session_id("20260204_1"));
        assert!(is_session_id("20260204_42"));
        assert!(is_session_id("20260204_999"));
        assert!(!is_session_id("task_12345_0001"));
        assert!(!is_session_id("my-agent"));
        assert!(!is_session_id("2026020_1"));
        assert!(!is_session_id("20260204"));
    }

    #[tokio::test]
    async fn background_task_slot_reservation_is_atomic() {
        async fn attempt(
            client: &SummonClient,
            start: Arc<tokio::sync::Barrier>,
            winner_ready: Arc<tokio::sync::Barrier>,
            mut release: tokio::sync::watch::Receiver<bool>,
        ) -> bool {
            start.wait().await;
            let Ok(_slot) = client.try_reserve_background_task_slot() else {
                return false;
            };
            winner_ready.wait().await;
            release.changed().await.unwrap();
            true
        }

        let client = SummonClient::with_background_task_limit(create_test_context(), 1).unwrap();
        let start = Arc::new(tokio::sync::Barrier::new(3));
        let winner_ready = Arc::new(tokio::sync::Barrier::new(2));
        let (release_tx, release_rx) = tokio::sync::watch::channel(false);

        let first = attempt(
            &client,
            Arc::clone(&start),
            Arc::clone(&winner_ready),
            release_rx.clone(),
        );
        let second = attempt(
            &client,
            Arc::clone(&start),
            Arc::clone(&winner_ready),
            release_rx,
        );
        let driver = async {
            start.wait().await;
            winner_ready.wait().await;
            release_tx.send(true).unwrap();
        };

        let (first_reserved, second_reserved, ()) = tokio::join!(first, second, driver);
        assert_ne!(first_reserved, second_reserved);
    }

    #[tokio::test]
    async fn test_async_task_result_lifecycle() {
        let client = SummonClient::new(create_test_context()).unwrap();
        let temp_dir = TempDir::new().unwrap();

        let result = client
            .handle_load_task_result("20260204_999", false, false)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));

        {
            use crate::agents::subagent_handler::create_tool_notification;
            use crate::conversation::message::MessageContent;
            use rmcp::model::CallToolRequestParams;

            let tool_call = CallToolRequestParams::new("developer__shell").with_arguments(
                serde_json::json!({"command": "ls"})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
            let content = MessageContent::tool_request("req1", Ok(tool_call));
            let notif = create_tool_notification(&content, "20260204_1").unwrap();

            let buffer = Arc::new(Mutex::new(vec![notif]));

            let mut running = client.background_tasks.lock().await;
            running.insert(
                "20260204_1".to_string(),
                BackgroundTask {
                    id: "20260204_1".to_string(),
                    description: "Running task".to_string(),
                    started_at: Instant::now(),
                    turns: Arc::new(AtomicU32::new(2)),
                    last_activity: Arc::new(AtomicU64::new(current_epoch_millis())),
                    handle: tokio::spawn(async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok("done".to_string())
                    }),
                    cancellation_token: CancellationToken::new(),
                    notification_buffer: buffer,
                    _slot: client.try_reserve_background_task_slot().unwrap(),
                },
            );
        }

        let mut subscriber = client.subscribe().await;

        let result = client
            .handle_load_task_result("20260204_1", false, false)
            .await
            .expect("load should wait and return result");
        let text = extract_text(&result.content[0]);
        assert!(text.contains("Completed"));
        assert!(text.contains("done"));

        let notif = subscriber
            .try_recv()
            .expect("subscriber should receive buffered notification");
        if let ServerNotification::LoggingMessageNotification(log) = notif {
            let data = log.params.data.as_object().unwrap();
            assert_eq!(
                data.get("subagent_id").and_then(|v| v.as_str()),
                Some("20260204_1")
            );
        } else {
            panic!("expected logging notification");
        }

        {
            let mut completed = client.completed_tasks.lock().await;
            completed.insert(
                "20260204_2".to_string(),
                CompletedTask {
                    id: "20260204_2".to_string(),
                    description: "Successful task".to_string(),
                    result: Ok("Task completed successfully with output".to_string()),
                    turns_taken: 5,
                    duration: Duration::from_secs(60),
                    completed_at: Instant::now(),
                },
            );
            completed.insert(
                "20260204_3".to_string(),
                CompletedTask {
                    id: "20260204_3".to_string(),
                    description: "Failed task".to_string(),
                    result: Err("Something went wrong".to_string()),
                    turns_taken: 3,
                    duration: Duration::from_secs(30),
                    completed_at: Instant::now(),
                },
            );
        }

        let moim = client.get_moim("test").await.unwrap();
        assert!(moim.contains("20260204_2"));
        assert!(moim.contains("20260204_3"));
        assert!(moim.contains(r#"use load("20260204_2") to get result"#));
        assert!(moim.contains(r#"use load("20260204_3") to get result"#));

        let discovery = client.handle_load_discovery(temp_dir.path()).await.unwrap();
        let discovery_text = extract_text(&discovery[0]);
        assert!(discovery_text.contains("Completed Tasks (awaiting retrieval)"));
        assert!(discovery_text.contains("20260204_2"));
        assert!(discovery_text.contains("20260204_3"));

        let result = client
            .handle_load_task_result("20260204_2", false, false)
            .await
            .unwrap();
        let text = extract_text(&result.content[0]);
        assert!(text.contains("20260204_2"));
        assert!(text.contains("Successful task"));
        assert!(text.contains("✓ Completed"));
        assert!(text.contains("1m"));
        assert!(text.contains("5 turns"));
        assert!(text.contains("Task completed successfully with output"));
        assert_eq!(result.status, "completed");
        assert_eq!(result.turns, Some(5));

        assert!(!client
            .completed_tasks
            .lock()
            .await
            .contains_key("20260204_2"));

        let result = client
            .handle_load_task_result("20260204_3", false, false)
            .await
            .unwrap();
        let text = extract_text(&result.content[0]);
        assert!(text.contains("✗ Failed"));
        assert!(text.contains("Error: Something went wrong"));
        assert_eq!(result.status, "failed");

        let result = client
            .handle_load_task_result("20260204_3", false, false)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));

        // All tasks consumed -- moim should be empty
        assert!(client.get_moim("test").await.is_none());
    }

    #[tokio::test]
    async fn test_cancel_running_task() {
        let client = SummonClient::new(create_test_context()).unwrap();
        let token = CancellationToken::new();

        {
            let mut running = client.background_tasks.lock().await;
            running.insert(
                "20260204_1".to_string(),
                BackgroundTask {
                    id: "20260204_1".to_string(),
                    description: "Cancellable task".to_string(),
                    started_at: Instant::now(),
                    turns: Arc::new(AtomicU32::new(3)),
                    last_activity: Arc::new(AtomicU64::new(current_epoch_millis())),
                    handle: tokio::spawn(async {
                        tokio::time::sleep(Duration::from_secs(1000)).await;
                        Ok("should not see this".to_string())
                    }),
                    cancellation_token: token.clone(),
                    notification_buffer: Arc::new(Mutex::new(Vec::new())),
                    _slot: client.try_reserve_background_task_slot().unwrap(),
                },
            );
        }

        let result = client
            .handle_load_task_result("20260204_1", true, false)
            .await
            .unwrap();
        let text = extract_text(&result.content[0]);
        assert!(text.contains("Cancelled"));
        assert!(text.contains("20260204_1"));
        assert!(text.contains("Cancellable task"));
        assert_eq!(result.status, "cancelled");
        assert_eq!(result.turns, Some(3));
        assert!(token.is_cancelled());
        assert!(!client
            .background_tasks
            .lock()
            .await
            .contains_key("20260204_1"));
    }

    #[tokio::test]
    async fn test_peek_running_task() {
        let client = SummonClient::new(create_test_context()).unwrap();

        {
            let mut running = client.background_tasks.lock().await;
            running.insert(
                "20260204_1".to_string(),
                BackgroundTask {
                    id: "20260204_1".to_string(),
                    description: "Long running analysis".to_string(),
                    started_at: Instant::now(),
                    turns: Arc::new(AtomicU32::new(7)),
                    last_activity: Arc::new(AtomicU64::new(current_epoch_millis())),
                    handle: tokio::spawn(async {
                        tokio::time::sleep(Duration::from_secs(1000)).await;
                        Ok("eventual result".to_string())
                    }),
                    cancellation_token: CancellationToken::new(),
                    notification_buffer: Arc::new(Mutex::new(Vec::new())),
                    _slot: client.try_reserve_background_task_slot().unwrap(),
                },
            );
        }

        // Peek should return status without removing the task
        let result = client
            .handle_load_task_result("20260204_1", false, true)
            .await
            .unwrap();
        let text = extract_text(&result.content[0]);
        assert!(text.contains("Running"));
        assert!(text.contains("Long running analysis"));
        assert!(text.contains("7")); // turns taken

        // Task should still be in background_tasks (not consumed)
        assert!(client
            .background_tasks
            .lock()
            .await
            .contains_key("20260204_1"));
    }

    #[tokio::test]
    async fn test_peek_nonexistent_task() {
        let client = SummonClient::new(create_test_context()).unwrap();

        let result = client
            .handle_load_task_result("20260204_999", false, true)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_peek_completed_task_returns_result() {
        let client = SummonClient::new(create_test_context()).unwrap();

        {
            let mut completed = client.completed_tasks.lock().await;
            completed.insert(
                "20260204_1".to_string(),
                CompletedTask {
                    id: "20260204_1".to_string(),
                    description: "Finished task".to_string(),
                    result: Ok("final output".to_string()),
                    turns_taken: 4,
                    duration: Duration::from_secs(30),
                    completed_at: Instant::now(),
                },
            );
        }

        // Peek on a completed task should return the full result (same as non-peek)
        let result = client
            .handle_load_task_result("20260204_1", false, true)
            .await
            .unwrap();
        let text = extract_text(&result.content[0]);
        assert!(text.contains("Completed"));
        assert!(text.contains("final output"));

        // Peek must be non-destructive: the result is still retrievable afterwards.
        assert!(client
            .completed_tasks
            .lock()
            .await
            .contains_key("20260204_1"));
        let result = client
            .handle_load_task_result("20260204_1", false, false)
            .await
            .unwrap();
        assert!(extract_text(&result.content[0]).contains("final output"));
    }
}
