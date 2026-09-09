//! Regression coverage for the ACP server compatibility facade.
//!
//! Maintainers: keep behavior-focused tests beside the responsibility modules they cover.
//! Clients: this test-only module does not alter the public ACP surface.

use super::*;
use crate::conversation::message::{ToolRequest, ToolResponse};
use crate::session::session_manager::SessionType;
use agent_client_protocol::schema::v1::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
    PermissionOptionId, ResourceLink, SelectedPermissionOutcome,
};
use gosling_providers::conversation::token_usage::Usage as TokenUsage;
use rmcp::model::{CallToolRequestParams, Content as RmcpContent};
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use test_case::test_case;

fn config_with_yaml(yaml: &str) -> (Config, NamedTempFile, NamedTempFile) {
    let config_file = NamedTempFile::new().unwrap();
    let secrets_file = NamedTempFile::new().unwrap();
    std::fs::write(config_file.path(), yaml).unwrap();
    let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();
    (config, config_file, secrets_file)
}

fn has_developer(extensions: &[ExtensionConfig]) -> bool {
    extensions.iter().any(|ext| ext.name() == "developer")
}

#[tokio::test]
async fn acp_active_run_pins_the_agent_manager_lru_entry() {
    let temp = tempfile::tempdir().unwrap();
    let session_manager = Arc::new(SessionManager::new(temp.path().join("data")));
    let agent_config = AgentConfig::new(
        session_manager,
        Arc::new(PermissionManager::new(temp.path().join("config"))),
        GoslingMode::default(),
        false,
        GoslingPlatform::GoslingDesktop,
    );
    let manager = AgentManager::new(agent_config, Some(2)).await.unwrap();
    manager.get_or_create_agent("active".into()).await.unwrap();
    manager.get_or_create_agent("idle".into()).await.unwrap();
    let active_prompt_runs = Mutex::new(HashMap::new());

    register_active_prompt_run(
        &active_prompt_runs,
        &manager,
        "active",
        "run-1".into(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    manager.get_or_create_agent("new".into()).await.unwrap();

    assert!(manager.has_session("active").await);
    assert!(!manager.has_session("idle").await);
    assert!(manager.is_session_busy("active").await);
    assert_eq!(
        unregister_active_prompt_run(&active_prompt_runs, &manager, "active", "run-1").await,
        Some(false)
    );
    assert!(!manager.is_session_busy("active").await);
}

#[test]
fn builtin_developer_loads_when_config_is_empty() {
    let (config, _c, _s) = config_with_yaml("");
    let selected = selected_builtin_extensions(&config, &["developer".to_string()]);
    assert!(
        has_developer(&selected),
        "developer should load by default on a fresh config"
    );
}

#[test]
fn builtin_developer_loads_when_explicitly_enabled() {
    let (config, _c, _s) = config_with_yaml(
        r#"
extensions:
  developer:
    enabled: true
    type: builtin
    name: developer
"#,
    );
    let selected = selected_builtin_extensions(&config, &["developer".to_string()]);
    assert!(has_developer(&selected));
}

#[test]
fn builtin_developer_skipped_when_explicitly_disabled() {
    let (config, _c, _s) = config_with_yaml(
        r#"
extensions:
  developer:
    enabled: false
    type: builtin
    name: developer
"#,
    );
    let selected = selected_builtin_extensions(&config, &["developer".to_string()]);
    assert!(
        !has_developer(&selected),
        "developer must NOT load when the user disabled it (issue #10221)"
    );
}

#[test]
fn default_off_builtin_loads_when_explicitly_requested() {
    // summarize is default_enabled: false, so read-migration writes
    // `enabled: false` into config. An explicit builtins request must still
    // load it (mirrors code mode requesting code_execution).
    let (config, _c, _s) = config_with_yaml("");
    let selected = selected_builtin_extensions(&config, &["summarize".to_string()]);
    assert!(
        selected.iter().any(|ext| ext.name() == "summarize"),
        "default-off builtins must load when explicitly requested via builtins"
    );
}

#[test_case(
        McpServer::Stdio(
            McpServerStdio::new("github", "/path/to/github-mcp-server")
                .args(vec!["stdio".into()])
                .env(vec![EnvVariable::new("GITHUB_PERSONAL_ACCESS_TOKEN", "ghp_xxxxxxxxxxxx")])
        ),
        Ok(ExtensionConfig::Stdio {
            name: "github".into(),
            description: String::new(),
            cmd: "/path/to/github-mcp-server".into(),
            args: vec!["stdio".into()],
            envs: Envs::new(
                [(
                    "GITHUB_PERSONAL_ACCESS_TOKEN".into(),
                    "ghp_xxxxxxxxxxxx".into()
                )]
                .into()
            ),
            env_keys: vec![],
            timeout: None,
            cwd: None,
            bundled: Some(false),
            available_tools: vec![],
        })
    )]
#[test_case(
        McpServer::Http(
            McpServerHttp::new("github", "https://api.githubcopilot.com/mcp/")
                .headers(vec![HttpHeader::new("Authorization", "Bearer ghp_xxxxxxxxxxxx")])
        ),
        Ok(ExtensionConfig::StreamableHttp {
            name: "github".into(),
            description: String::new(),
            uri: "https://api.githubcopilot.com/mcp/".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::from([(
                "Authorization".into(),
                "Bearer ghp_xxxxxxxxxxxx".into()
            )]),
            timeout: None,
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: Some(false),
            available_tools: vec![],
        })
    )]
#[test_case(
        McpServer::Sse(McpServerSse::new("test-sse", "https://agent-fin.biodnd.com/sse")),
        Err("SSE is unsupported, migrate to streamable_http".to_string())
    )]
fn test_mcp_server_to_extension_config(
    input: McpServer,
    expected: Result<ExtensionConfig, String>,
) {
    assert_eq!(mcp_server_to_extension_config(input), expected);
}

fn stdio_extension(name: &str, envs: Envs, env_keys: Vec<String>) -> ExtensionConfig {
    ExtensionConfig::Stdio {
        name: name.into(),
        description: String::new(),
        cmd: "server-bin".into(),
        args: vec!["mcp".into()],
        envs,
        env_keys,
        timeout: Some(300),
        cwd: None,
        bundled: Some(false),
        available_tools: vec![],
    }
}

fn http_extension(
    uri: &str,
    headers: HashMap<String, String>,
    socket: Option<String>,
    envs: Envs,
    env_keys: Vec<String>,
) -> ExtensionConfig {
    ExtensionConfig::StreamableHttp {
        name: "muninn".into(),
        description: String::new(),
        uri: uri.into(),
        envs,
        env_keys,
        headers,
        timeout: Some(300),
        socket,
        client_id: None,
        client_secret_key: None,
        scopes: vec![],
        bundled: Some(false),
        available_tools: vec![],
    }
}

#[test]
fn rehydrate_configured_envs_restores_stripped_stdio_envs() {
    // The client-facing DTO strips plain `envs` (config_to_gosling_extension),
    // so a client echoing an extension back at session creation loses them.
    // The merge must restore stored envs, let client-sent values win, and
    // adopt stored env_keys when the client sent none.
    let configured = vec![stdio_extension(
        "muninn",
        Envs::new(
            [
                ("MUNINN_EMBED_PROVIDER".to_string(), "ollama".to_string()),
                (
                    "MUNINN_VECTOR_BACKEND".to_string(),
                    "sqlite-vec".to_string(),
                ),
            ]
            .into(),
        ),
        vec!["MUNINN_SECRET".to_string()],
    )];

    let mut echoed = stdio_extension(
        "muninn",
        Envs::new([("MUNINN_VECTOR_BACKEND".to_string(), "brute".to_string())].into()),
        vec![],
    );
    rehydrate_configured_envs(&mut echoed, &configured);

    let ExtensionConfig::Stdio { envs, env_keys, .. } = echoed else {
        panic!("expected stdio extension");
    };
    let env = envs.get_env();
    assert_eq!(
        env.get("MUNINN_EMBED_PROVIDER").map(String::as_str),
        Some("ollama"),
        "stored envs must be restored"
    );
    assert_eq!(
        env.get("MUNINN_VECTOR_BACKEND").map(String::as_str),
        Some("brute"),
        "client-supplied values must win on collision"
    );
    assert_eq!(env_keys, vec!["MUNINN_SECRET".to_string()]);

    // A server with no configured counterpart is left untouched.
    let mut unknown = stdio_extension("not-configured", Envs::default(), vec![]);
    rehydrate_configured_envs(&mut unknown, &configured);
    let ExtensionConfig::Stdio { envs, .. } = unknown else {
        panic!("expected stdio extension");
    };
    assert!(envs.get_env().is_empty());

    for redirected in [
        ExtensionConfig::Stdio {
            name: "muninn".into(),
            description: String::new(),
            cmd: "different-server".into(),
            args: vec!["mcp".into()],
            envs: Envs::default(),
            env_keys: vec![],
            timeout: Some(300),
            cwd: None,
            bundled: Some(false),
            available_tools: vec![],
        },
        ExtensionConfig::Stdio {
            name: "muninn".into(),
            description: String::new(),
            cmd: "server-bin".into(),
            args: vec!["serve".into()],
            envs: Envs::default(),
            env_keys: vec![],
            timeout: Some(300),
            cwd: None,
            bundled: Some(false),
            available_tools: vec![],
        },
    ] {
        let mut redirected = redirected;
        rehydrate_configured_envs(&mut redirected, &configured);
        let ExtensionConfig::Stdio { envs, env_keys, .. } = redirected else {
            panic!("expected stdio extension");
        };
        assert!(envs.get_env().is_empty());
        assert!(env_keys.is_empty());
    }
}

#[test]
fn rehydrate_configured_envs_requires_exact_http_destination() {
    let configured_headers = HashMap::from([("Authorization".into(), "Bearer token".into())]);
    let configured = vec![http_extension(
        "https://mcp.example.test/api",
        configured_headers.clone(),
        Some("socket-a".into()),
        Envs::new([("MUNINN_SECRET".into(), "stored".into())].into()),
        vec!["MUNINN_SECRET_KEY".into()],
    )];

    let mut echoed = http_extension(
        "https://mcp.example.test/api",
        configured_headers.clone(),
        Some("socket-a".into()),
        Envs::default(),
        vec![],
    );
    rehydrate_configured_envs(&mut echoed, &configured);
    let ExtensionConfig::StreamableHttp { envs, env_keys, .. } = echoed else {
        panic!("expected HTTP extension");
    };
    assert_eq!(
        envs.get_env().get("MUNINN_SECRET").map(String::as_str),
        Some("stored")
    );
    assert_eq!(env_keys, vec!["MUNINN_SECRET_KEY"]);

    for redirected in [
        http_extension(
            "https://attacker.example.test/api",
            configured_headers.clone(),
            Some("socket-a".into()),
            Envs::default(),
            vec![],
        ),
        http_extension(
            "https://mcp.example.test/api",
            HashMap::from([("Authorization".into(), "Bearer different".into())]),
            Some("socket-a".into()),
            Envs::default(),
            vec![],
        ),
        http_extension(
            "https://mcp.example.test/api",
            configured_headers.clone(),
            Some("socket-b".into()),
            Envs::default(),
            vec![],
        ),
    ] {
        let mut redirected = redirected;
        rehydrate_configured_envs(&mut redirected, &configured);
        let ExtensionConfig::StreamableHttp { envs, env_keys, .. } = redirected else {
            panic!("expected HTTP extension");
        };
        assert!(envs.get_env().is_empty());
        assert!(env_keys.is_empty());
    }
}

fn new_resource_link(content: &str) -> anyhow::Result<(ResourceLink, NamedTempFile)> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;

    let name = file
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let uri = format!("file://{}", file.path().to_str().unwrap());
    let link = ResourceLink::new(name, uri);
    Ok((link, file))
}

#[test]
fn test_read_resource_link_non_file_scheme() {
    let (link, file) = new_resource_link("print(\"hello, world\")").unwrap();

    let result = read_resource_link(link).unwrap();
    let expected = format!(
        "

# {}
```
print(\"hello, world\")
```",
        file.path().to_str().unwrap(),
    );

    assert_eq!(result, expected,)
}

#[test]
fn test_format_tool_name_with_extension() {
    assert_eq!(format_tool_name("developer__edit"), "developer: edit");
    assert_eq!(
        format_tool_name("platform__manage_extensions"),
        "platform: manage extensions"
    );
    assert_eq!(format_tool_name("todo__write"), "todo: write");
}

#[test]
fn test_format_tool_name_without_extension() {
    assert_eq!(format_tool_name("simple_tool"), "simple tool");
    assert_eq!(format_tool_name("another_name"), "another name");
    assert_eq!(format_tool_name("single"), "single");
}

#[test]
fn test_summarize_tool_call_no_args() {
    assert_eq!(
        summarize_tool_call("developer__shell", None),
        "developer: shell"
    );
}

#[test]
fn test_summarize_tool_call_with_path() {
    let args = serde_json::json!({"path": "/src/main.rs", "content": "fn main() {}"});
    assert_eq!(
        summarize_tool_call("developer__edit", Some(&args)),
        "developer: edit · /src/main.rs"
    );
}

#[test]
fn test_summarize_tool_call_with_command() {
    let args = serde_json::json!({"command": "cargo build"});
    assert_eq!(
        summarize_tool_call("developer__shell", Some(&args)),
        "developer: shell · cargo build"
    );
}

#[test]
fn test_tool_call_identity_meta_uses_gosling_extension_metadata() {
    let request = ToolRequest {
        id: "req_1".to_string(),
        tool_call: Ok(CallToolRequestParams::new("context7__query-docs")),
        metadata: None,
        tool_meta: Some(serde_json::json!({"gosling_extension": "context7"})),
    };

    let meta = tool_call_identity_meta(&request).expect("expected metadata");

    assert_eq!(
        meta.get("gosling"),
        Some(&serde_json::json!({
            "toolCall": {
                "toolName": "context7__query-docs",
                "extensionName": "context7",
            },
        })),
    );
}

fn buf_entry(tool_id: &str, msg_id: &str) -> (String, String) {
    (tool_id.to_string(), msg_id.to_string())
}

#[test]
fn extend_chain_membership_skips_singleton_and_leaves_buffer() {
    let mut membership: HashMap<String, Arc<ToolChain>> = HashMap::new();
    let buffer = vec![buf_entry("a", "row_1")];

    extend_chain_membership(&buffer, &mut membership);

    assert_eq!(buffer.len(), 1, "buffer is left intact for caller");
    assert!(
        membership.is_empty(),
        "single-tool runs should not register a chain",
    );
}

#[test]
fn extend_chain_membership_registers_each_id_against_shared_chain() {
    let mut membership: HashMap<String, Arc<ToolChain>> = HashMap::new();
    let buffer = vec![
        buf_entry("a", "row_first"),
        buf_entry("b", "row_second"),
        buf_entry("c", "row_third"),
    ];

    extend_chain_membership(&buffer, &mut membership);

    assert_eq!(membership.len(), 3);
    let chain_a = membership.get("a").expect("a registered");
    let chain_b = membership.get("b").expect("b registered");
    let chain_c = membership.get("c").expect("c registered");
    assert!(
        Arc::ptr_eq(chain_a, chain_b) && Arc::ptr_eq(chain_b, chain_c),
        "every id in the run must point at the same ToolChain Arc",
    );
    assert_eq!(
        chain_a.ids,
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
    );
}

#[test]
fn extend_chain_membership_anchors_on_first_row_for_split_messages() {
    // Sequential tool use (Bedrock/Anthropic) emits each tool request as
    // its own assistant message, with the tool response interleaved in
    // between. The chain should still form, anchored on the *first*
    // tool's row id so `update_tool_request_meta` can find that
    // ToolRequest when persisting the summary.
    let mut membership: HashMap<String, Arc<ToolChain>> = HashMap::new();
    let buffer = vec![
        buf_entry("toolu_bdrk_1", "row_for_tool_1"),
        buf_entry("toolu_bdrk_2", "row_for_tool_2"),
    ];

    extend_chain_membership(&buffer, &mut membership);

    let chain = membership
        .get("toolu_bdrk_1")
        .expect("first tool registered");
    assert_eq!(
        chain.ids,
        vec!["toolu_bdrk_1".to_string(), "toolu_bdrk_2".to_string()],
    );
    let chain_via_second = membership
        .get("toolu_bdrk_2")
        .expect("second tool registered");
    assert!(Arc::ptr_eq(chain, chain_via_second));
}

#[test]
fn extend_chain_membership_grows_chain_as_more_requests_arrive() {
    // The streaming loop re-registers eagerly each time a new request
    // arrives, so a chain that started at length 2 must grow to include
    // a third tool whose response is yet to come. Both the original
    // members and the new member must point at the new (extended) chain.
    let mut membership: HashMap<String, Arc<ToolChain>> = HashMap::new();
    let mut buffer = vec![buf_entry("a", "row_1"), buf_entry("b", "row_2")];
    extend_chain_membership(&buffer, &mut membership);

    buffer.push(buf_entry("c", "row_3"));
    extend_chain_membership(&buffer, &mut membership);

    let chain_a = membership.get("a").expect("a present");
    let chain_b = membership.get("b").expect("b present");
    let chain_c = membership.get("c").expect("c present");
    assert!(Arc::ptr_eq(chain_a, chain_b) && Arc::ptr_eq(chain_b, chain_c));
    assert_eq!(
        chain_a.ids,
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
    );
}

#[test]
fn with_tool_chain_summary_meta_creates_fresh_when_none() {
    let meta =
        with_tool_chain_summary_meta(None, "applied dark mode", 4).expect("meta should be created");
    assert_eq!(
        meta.get("gosling"),
        Some(&serde_json::json!({
            "toolChainSummary": { "summary": "applied dark mode", "count": 4 },
        })),
    );
}

#[test]
fn with_tool_chain_summary_meta_preserves_existing_tool_call_identity() {
    let existing = tool_call_identity_meta(&ToolRequest {
        id: "req_1".to_string(),
        tool_call: Ok(CallToolRequestParams::new("developer__shell")),
        metadata: None,
        tool_meta: None,
    });
    let meta = with_tool_chain_summary_meta(existing, "ran two commands", 2)
        .expect("meta should be created");
    let gosling = meta.get("gosling").expect("gosling key");
    assert_eq!(
        gosling.get("toolCall"),
        Some(&serde_json::json!({ "toolName": "developer__shell", "extensionName": "developer" }))
    );
    assert_eq!(
        gosling.get("toolChainSummary"),
        Some(&serde_json::json!({ "summary": "ran two commands", "count": 2 }))
    );
}

#[test]
fn replay_attaches_chain_summary_meta_for_first_tool_request_with_persisted_summary() {
    let tool_request = ToolRequest {
        id: "req_first".to_string(),
        tool_call: Ok(CallToolRequestParams::new("developer__shell")),
        metadata: None,
        tool_meta: Some(serde_json::json!({
            crate::conversation::message::TOOL_META_CHAIN_SUMMARY_KEY: {
                "summary": "applied dark mode polish",
                "count": 3,
            },
        })),
    };

    let pending_tool_call = pending_tool_call_from_request(&tool_request);
    let mut meta = pending_tool_call.identity_meta;
    let chain_summary = tool_request
        .persisted_chain_summary()
        .expect("chain summary should be present");
    meta = with_tool_chain_summary_meta(meta, &chain_summary.summary, chain_summary.count);

    let gosling = meta
        .as_ref()
        .and_then(|m| m.get("gosling"))
        .expect("replay meta must include a gosling namespace");
    assert_eq!(
        gosling.get("toolCall"),
        Some(&serde_json::json!({ "toolName": "developer__shell", "extensionName": "developer" })),
        "replay must preserve identity meta alongside the chain summary",
    );
    assert_eq!(
        gosling.get("toolChainSummary"),
        Some(&serde_json::json!({ "summary": "applied dark mode polish", "count": 3 })),
        "replay must attach toolChainSummary so the chain header renders on first paint",
    );
}

#[test]
fn replay_does_not_attach_chain_summary_for_tool_requests_without_persisted_summary() {
    let tool_request = ToolRequest {
        id: "req_second".to_string(),
        tool_call: Ok(CallToolRequestParams::new("developer__shell")),
        metadata: None,
        tool_meta: None,
    };

    let chain_summary = tool_request.persisted_chain_summary();
    assert!(
        chain_summary.is_none(),
        "non-first tool requests must not carry chain summaries",
    );
}

#[test]
fn test_summarize_tool_call_long_value_truncated() {
    let long_path = "a".repeat(80);
    let args = serde_json::json!({"path": long_path});
    let result = summarize_tool_call("developer__read_file", Some(&args));
    assert!(result.ends_with('…'));
    assert!(result.len() < 90);
}

#[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("allow_once".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AllowOnce };
        "allow_once_maps_to_allow_once"
    )]
#[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("allow_always".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AlwaysAllow };
        "allow_always_maps_to_always_allow"
    )]
#[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("reject_once".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::DenyOnce };
        "reject_once_maps_to_deny_once"
    )]
#[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("reject_always".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AlwaysDeny };
        "reject_always_maps_to_always_deny"
    )]
#[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("unknown".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::Cancel };
        "unknown_option_maps_to_cancel"
    )]
#[test_case(
        RequestPermissionOutcome::Cancelled,
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::Cancel };
        "cancelled_maps_to_cancel"
    )]
fn test_outcome_to_confirmation(input: RequestPermissionOutcome, expected: PermissionConfirmation) {
    assert_eq!(outcome_to_confirmation(&input), expected);
}

fn json_object(pairs: Vec<(&str, serde_json::Value)>) -> rmcp::model::JsonObject {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[test_case(None => None ; "none arguments")]
#[test_case(Some(json_object(vec![])) => None ; "missing line key")]
#[test_case(Some(json_object(vec![("line", serde_json::json!(5))])) => Some(5) ; "line present")]
#[test_case(Some(json_object(vec![("line", serde_json::json!("not_a_number"))])) => None ; "line not a number")]
fn test_get_requested_line(arguments: Option<rmcp::model::JsonObject>) -> Option<u32> {
    get_requested_line(arguments.as_ref())
}

#[test_case("read", true ; "read is developer file tool")]
#[test_case("write", true ; "write is developer file tool")]
#[test_case("edit", true ; "edit is developer file tool")]
#[test_case("shell", false ; "shell is not developer file tool")]
#[test_case("summarize", false ; "summarize is not developer file tool")]
fn test_is_developer_file_tool(tool_name: &str, expected: bool) {
    assert_eq!(is_developer_file_tool(tool_name), expected);
}

#[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("read").with_arguments(serde_json::json!({"path": "/tmp/f.txt", "line": 5}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => vec![(PathBuf::from("/tmp/f.txt"), Some(5))]
        ; "read returns requested line"
    )]
#[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("read").with_arguments(serde_json::json!({"path": "/tmp/f.txt"}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => vec![(PathBuf::from("/tmp/f.txt"), None)]
        ; "read without line"
    )]
#[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("write").with_arguments(serde_json::json!({"path": "/tmp/f.txt", "content": "hi"}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => vec![(PathBuf::from("/tmp/f.txt"), Some(1))]
        ; "write returns line 1"
    )]
#[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("edit").with_arguments(serde_json::json!({"path": "/tmp/f.txt", "before": "a", "after": "b"}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => vec![(PathBuf::from("/tmp/f.txt"), Some(1))]
        ; "edit returns line 1"
    )]
#[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("shell").with_arguments(serde_json::json!({"command": "ls"}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => Vec::<(PathBuf, Option<u32>)>::new()
        ; "non file tool returns empty"
    )]
fn test_extract_tool_locations(
    request: ToolRequest,
    response: ToolResponse,
) -> Vec<(PathBuf, Option<u32>)> {
    extract_tool_locations(&request, &response)
        .into_iter()
        .map(|loc| (loc.path, loc.line))
        .collect()
}

fn response_with_meta(meta: Option<serde_json::Value>) -> ToolResponse {
    let mut result = CallToolResult::success(vec![RmcpContent::text("")]);
    result.meta = meta.map(|v| serde_json::from_value(v).unwrap());
    ToolResponse {
        id: "req_1".to_string(),
        tool_result: Ok(result),
        metadata: None,
    }
}

#[test_case(
        response_with_meta(Some(serde_json::json!({"tool_locations": [{"path": "/tmp/f.txt", "line": 5}]})))
        => Some(vec![(PathBuf::from("/tmp/f.txt"), Some(5))])
        ; "meta with path and line"
    )]
#[test_case(
        response_with_meta(Some(serde_json::json!({"tool_locations": [{"path": "/tmp/f.txt"}]})))
        => Some(vec![(PathBuf::from("/tmp/f.txt"), None)])
        ; "meta with path no line"
    )]
#[test_case(
        response_with_meta(Some(serde_json::json!({})))
        => None
        ; "meta without tool_locations key"
    )]
#[test_case(
        response_with_meta(None)
        => None
        ; "no meta"
    )]
fn test_extract_locations_from_meta(response: ToolResponse) -> Option<Vec<(PathBuf, Option<u32>)>> {
    extract_locations_from_meta(&response)
        .map(|locs| locs.into_iter().map(|loc| (loc.path, loc.line)).collect())
}

#[test]
fn test_extract_tool_call_update_meta_ignores_untrusted_gosling_meta() {
    let response = response_with_meta(Some(serde_json::json!({
        "gosling": {
            "mcpApp": {
                "resourceUri": "ui://spoofed/app",
            },
        },
    })));

    assert_eq!(extract_tool_call_update_meta(&response), None);
}

#[test]
fn test_extract_tool_call_update_meta_uses_trusted_meta_only() {
    let response = response_with_meta(Some(serde_json::json!({
        "gosling": {
            "mcpApp": {
                "resourceUri": "ui://spoofed/app",
            },
        },
        TRUSTED_TOOL_UPDATE_META_KEY: {
            "mcpApp": {
                "resourceUri": "ui://trusted/app",
                "extensionName": "weather",
                "toolName": "weather__render",
            },
        },
    })));

    let extracted = extract_tool_call_update_meta(&response).expect("expected trusted meta");
    assert_eq!(
        extracted.get("gosling"),
        Some(&serde_json::json!({
            "mcpApp": {
                "resourceUri": "ui://trusted/app",
                "extensionName": "weather",
                "toolName": "weather__render",
            },
        })),
    );
}

#[test]
fn test_merge_replay_message_meta_preserves_existing_gosling_meta() {
    let message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_1");
    let existing = serde_json::from_value(serde_json::json!({
        "gosling": {
            "mcpApp": {
                "resourceUri": "ui://trusted/app",
                "extensionName": "weather",
                "toolName": "weather__render",
            },
        },
    }))
    .unwrap();

    let merged = merge_replay_message_meta(Some(existing), &message);

    assert_eq!(
        merged.get("gosling"),
        Some(&serde_json::json!({
            "created": 1_700_000_000,
            "messageId": "msg_1",
            "mcpApp": {
                "resourceUri": "ui://trusted/app",
                "extensionName": "weather",
                "toolName": "weather__render",
            },
        })),
    );
}

#[test]
fn test_merge_replay_message_meta_creates_fresh_when_none() {
    let message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_2");

    let merged = merge_replay_message_meta(None, &message);

    assert_eq!(
        merged.get("gosling"),
        Some(&serde_json::json!({
            "created": 1_700_000_000,
            "messageId": "msg_2",
        })),
    );
}

#[test]
fn test_merge_replay_message_meta_includes_steer_marker() {
    let message = Message::new(Role::User, 1_700_000_000, vec![])
        .with_id("msg_steer")
        .with_steer();

    let merged = merge_replay_message_meta(None, &message);

    assert_eq!(
        merged.get("gosling"),
        Some(&serde_json::json!({
            "created": 1_700_000_000,
            "messageId": "msg_steer",
            "steer": true,
        })),
        "replay must carry the steer marker so the boundary survives reload"
    );
}

#[test]
fn test_merge_replay_message_meta_omits_steer_when_not_set() {
    let message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_plain");

    let merged = merge_replay_message_meta(None, &message);

    assert_eq!(merged.get("gosling").and_then(|g| g.get("steer")), None);
}

#[test]
fn test_replay_message_meta_marks_imported_history_untrusted() {
    let mut message = Message::new(Role::User, 1_700_000_000, vec![]).with_id("msg_imported");
    message.metadata = message.metadata.with_imported_untrusted();

    let merged = merge_replay_message_meta(None, &message);

    assert_eq!(
        merged
            .get("gosling")
            .and_then(|gosling| gosling.get("importedUntrusted")),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn test_message_update_meta_includes_created_and_message_id() {
    let meta = message_update_meta(Some("msg_live"), 1_700_000_000, false);

    assert_eq!(
        meta.get("gosling"),
        Some(&serde_json::json!({
            "created": 1_700_000_000,
            "messageId": "msg_live",
        })),
    );
}

#[test]
fn test_replay_message_meta_bounds_imported_message_id() {
    let oversized_id = format!("prefix-{}-suffix", "x".repeat(10_000));
    let message =
        Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id(oversized_id.clone());

    let replay = merge_replay_message_meta(None, &message);
    let replayed_id = replay["gosling"]["messageId"]
        .as_str()
        .expect("message ID should be present");

    assert!(replayed_id.len() <= 1_024);
    assert!(replayed_id.starts_with("prefix-"));
    assert!(replayed_id.ends_with("-suffix"));
    assert_eq!(message.id.as_deref(), Some(oversized_id.as_str()));
}

#[test]
fn test_credits_exhausted_system_notification_maps_to_prompt_error() {
    let content = MessageContent::SystemNotification(SystemNotificationContent {
        notification_type: SystemNotificationType::CreditsExhausted,
        msg: "Please add credits to your account, then resend your message to continue."
            .to_string(),
        data: Some(serde_json::json!({
            "top_up_url": "https://router.tetrate.ai/billing"
        })),
    });

    let error = prompt_error_from_message_content(&content).expect("expected prompt error");
    let value = serde_json::to_value(error).unwrap();

    assert_eq!(
        value,
        serde_json::json!({
            "code": -32603,
            "message": "Please add credits to your account, then resend your message to continue.",
            "data": {
                "reason": "credits_exhausted",
                "url": "https://router.tetrate.ai/billing"
            }
        })
    );
}

#[test]
fn test_non_credit_system_notification_does_not_map_to_prompt_error() {
    let content = MessageContent::SystemNotification(SystemNotificationContent {
        notification_type: SystemNotificationType::InlineMessage,
        msg: "Compaction complete".to_string(),
        data: None,
    });

    assert!(prompt_error_from_message_content(&content).is_none());
}

#[test]
fn test_merge_replay_message_meta_omits_message_id_when_none() {
    let message = Message::new(Role::Assistant, 1_700_000_000, vec![]);

    let merged = merge_replay_message_meta(None, &message);

    assert_eq!(
        merged.get("gosling"),
        Some(&serde_json::json!({
            "created": 1_700_000_000,
        })),
    );
}

#[test]
fn test_extract_tool_raw_output_preserves_structured_content() {
    let mut result = CallToolResult::success(vec![RmcpContent::text("fallback")]);
    result.structured_content = Some(serde_json::json!({
        "restaurants": [
            {
                "name": "Coffee Shop",
                "unitToken": "unit-1",
            },
        ],
    }));

    assert_eq!(
        extract_tool_raw_output(&Ok(result)),
        Some(serde_json::json!({
            "restaurants": [
                {
                    "name": "Coffee Shop",
                    "unitToken": "unit-1",
                },
            ],
        })),
    );
}

fn make_session_with_usage(usage: TokenUsage, accumulated_usage: TokenUsage) -> Session {
    Session {
        id: "session-1".to_string(),
        working_dir: PathBuf::from("/tmp"),
        name: "ACP Session".to_string(),
        session_type: SessionType::Acp,
        usage,
        accumulated_usage,
        ..Default::default()
    }
}

#[test]
fn test_build_prompt_usage_uses_current_turn_tokens() {
    let session = make_session_with_usage(
        TokenUsage::new(Some(80), Some(40), Some(120)),
        TokenUsage::new(Some(210), Some(150), Some(360)),
    );
    let usage = build_prompt_usage(&session).expect("usage should be present");
    assert_eq!(usage.total_tokens, 120);
    assert_eq!(usage.input_tokens, 80);
    assert_eq!(usage.output_tokens, 40);
}

#[test]
fn test_build_prompt_usage_falls_back_to_current_tokens() {
    let session = make_session_with_usage(
        TokenUsage::new(Some(80), Some(40), Some(120)),
        TokenUsage::default(),
    );
    let usage = build_prompt_usage(&session).expect("usage should be present");
    assert_eq!(usage.total_tokens, 120);
    assert_eq!(usage.input_tokens, 80);
    assert_eq!(usage.output_tokens, 40);
}

#[test]
fn test_build_prompt_usage_requires_total_tokens() {
    let session = make_session_with_usage(
        TokenUsage {
            input_tokens: Some(80),
            output_tokens: Some(40),
            total_tokens: None,
            ..Default::default()
        },
        TokenUsage::default(),
    );
    assert!(build_prompt_usage(&session).is_none());
}

#[test]
fn test_build_usage_update_clamps_negative_used_to_zero() {
    let mut session = make_session_with_usage(
        TokenUsage::new(Some(0), Some(0), Some(-7)),
        TokenUsage::default(),
    );
    session.model_config = Some(
        gosling_providers::model::ModelConfig::new("test-model").with_context_limit(Some(258_000)),
    );
    let updates = build_usage_updates(&session).expect("usage updates should be present");
    assert_eq!(updates.custom.session_id, "session-1");
    let usage = match updates.custom.update {
        GoslingSessionUpdate::UsageUpdate(usage) => usage,
        other => panic!("expected usage update, got {other:?}"),
    };
    assert_eq!(usage.used, 0);
    assert_eq!(usage.context_limit, 258_000);
    assert_eq!(updates.standard.used, 0);
    assert_eq!(updates.standard.size, 258_000);
}

#[test]
fn test_build_usage_update_requires_model_config() {
    let session = make_session_with_usage(
        TokenUsage::new(Some(80), Some(40), Some(120)),
        TokenUsage::default(),
    );
    assert!(build_usage_updates(&session).is_none());
}

#[test]
fn shell_capability_methods_come_from_the_custom_method_registry() {
    let methods = custom_method_names();
    let expected =
        GoslingAcpAgent::custom_method_schemas(&mut schemars::SchemaGenerator::default())
            .into_iter()
            .map(|schema| schema.method)
            .collect::<std::collections::HashSet<_>>();

    assert_eq!(
        methods
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>(),
        expected
    );
    assert!(methods.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(methods.contains(&"_gosling/unstable/shell/provisioning/read".to_string()));
    assert!(methods.contains(&"_gosling/unstable/shell/session/artifacts/list".to_string()));
    for method in [
        "_gosling/unstable/shell/session/library/list",
        "_gosling/unstable/shell/session/library/add_text",
        "_gosling/unstable/shell/session/library/add_image",
        "_gosling/unstable/shell/session/library/link_file",
        "_gosling/unstable/shell/session/library/remove",
        "_gosling/unstable/shell/session/library/resolve",
    ] {
        assert!(methods.contains(&method.to_string()), "missing {method}");
    }
    assert!(methods.contains(&"_gosling/unstable/shell/handoff/prepare".to_string()));
}

#[test]
fn test_gosling_custom_notifications_capability_defaults_to_false() {
    let request = InitializeRequest::new(agent_client_protocol::schema::ProtocolVersion::LATEST);
    let gosling_client_capabilities =
        extract_client_capabilities_meta(&request).and_then(|meta| meta.gosling);

    assert!(!extract_client_supports_gosling_custom_notifications(
        gosling_client_capabilities.as_ref()
    ));
}

#[test]
fn protocol_negotiation_accepts_latest_and_rejects_legacy_fallback() {
    assert_eq!(
        negotiate_protocol_version(ProtocolVersion::LATEST).unwrap(),
        ProtocolVersion::V1
    );
    let error = negotiate_protocol_version(ProtocolVersion::V0).unwrap_err();
    assert!(error
        .to_string()
        .contains("Unsupported ACP protocol version 0"));
}

#[tokio::test]
async fn eof_aware_reader_notifies_when_input_closes() {
    let (sender, receiver) = oneshot::channel();
    let mut reader = EofAwareReader::new(futures::io::Cursor::new(Vec::<u8>::new()), sender);
    let mut buffer = [0_u8; 1];

    let read = futures::AsyncReadExt::read(&mut reader, &mut buffer)
        .await
        .expect("EOF read should succeed");

    assert_eq!(read, 0);
    receiver.await.expect("EOF notification should be sent");
}

#[tokio::test]
async fn input_eof_terminates_a_pending_connection() {
    let (sender, receiver) = oneshot::channel();
    sender.send(()).unwrap();
    let connection = futures::future::pending::<Result<(), agent_client_protocol::Error>>();

    finish_connection_on_eof(connection, receiver)
        .await
        .expect("EOF should terminate the pending connection");
}

#[tokio::test]
async fn input_eof_allows_an_in_flight_response_to_finish() {
    let (sender, receiver) = oneshot::channel();
    sender.send(()).unwrap();
    let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let completion = completed.clone();
    let connection = async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        completion.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok::<(), agent_client_protocol::Error>(())
    };

    finish_connection_on_eof(connection, receiver)
        .await
        .expect("EOF drain should finish an in-flight response");

    assert!(completed.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn input_eof_drain_preserves_connection_errors() {
    let (sender, receiver) = oneshot::channel();
    sender.send(()).unwrap();
    let connection = async {
        tokio::task::yield_now().await;
        Err(agent_client_protocol::Error::internal_error())
    };

    finish_connection_on_eof(connection, receiver)
        .await
        .expect_err("connection errors during EOF drain must remain visible");
}

#[test]
fn test_gosling_custom_notifications_capability_reads_client_meta() {
    let mut gosling_meta = serde_json::Map::new();
    gosling_meta.insert(
        "customNotifications".to_string(),
        serde_json::Value::Bool(true),
    );
    let mut meta = serde_json::Map::new();
    meta.insert(
        "gosling".to_string(),
        serde_json::Value::Object(gosling_meta),
    );

    let request = InitializeRequest::new(agent_client_protocol::schema::ProtocolVersion::LATEST)
        .client_capabilities(
            agent_client_protocol::schema::v1::ClientCapabilities::new().meta(meta),
        );
    let gosling_client_capabilities =
        extract_client_capabilities_meta(&request).and_then(|meta| meta.gosling);

    assert!(extract_client_supports_gosling_custom_notifications(
        gosling_client_capabilities.as_ref()
    ));
}

#[test]
fn terminal_message_metadata_maps_to_prompt_failure() {
    let ordinary = Message::assistant().with_text("Compaction complete");
    assert!(prompt_error_from_message(&ordinary).is_none());
    let failed = Message::assistant()
        .with_text("Could not compact")
        .with_terminal_error("provider unavailable");
    assert_eq!(
        prompt_error_from_message(&failed).unwrap().message,
        "provider unavailable"
    );
    let credits =
        failed.with_system_notification(SystemNotificationType::CreditsExhausted, "Add credits");
    let error = serde_json::to_value(prompt_error_from_message(&credits).unwrap()).unwrap();
    assert_eq!(error["data"]["reason"], "credits_exhausted");
}
