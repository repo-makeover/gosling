use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell as ClapShell};
use clap_complete_nushell::Nushell as ClapNushell;
use gosling::acp::custom_requests::{
    ShellAuthorityMode, ShellIdentity, ShellProtocolPolicy, ShellProvisioning,
    ShellSessionProvisioning, SHELL_PROVISIONING_SCHEMA_VERSION,
};
use gosling::acp::domain_adapter::McpDomainAdapter;
use gosling::acp::shell::{DomainAdapter, ShellRuntime};
use gosling::agents::GoslingPlatform;
use gosling::builtin_extension::register_builtin_extensions;
use gosling::config::paths::{Paths, RuntimePaths};
use gosling::config::{get_domain_adapter_registration, Config, ConfigError, GoslingMode};
use gosling::source_roots::SourceRoot;
use gosling_mcp::mcp_server_runner::{serve, McpCommand};
use gosling_mcp::{AutoVisualiserRouter, ComputerControllerServer};

use crate::commands::configure::handle_configure;
use crate::commands::info::handle_info;
use crate::commands::plugin::{handle_plugin_install, handle_plugin_trust, handle_plugin_update};
use crate::commands::project::{handle_project_default, handle_projects_interactive};
use crate::commands::term::{
    handle_term_info, handle_term_init, handle_term_log, handle_term_run, Shell,
};

use crate::commands::session::{handle_session_list, handle_session_remove};
use crate::commands::skills::handle_skills_list;
use crate::session::{build_session, SessionBuilderConfig};
use gosling::agents::Container;
use gosling::session::session_manager::SessionType;
use gosling::session::SessionManager;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use tracing::warn;

const GOSLING_SERVER_SECRET_KEY_ENV: &str = "GOSLING_SERVER__SECRET_KEY";

fn warn_about_invalid_config_values() {
    let config = Config::global();

    if let Err(error) = config.get_gosling_mode() {
        if !matches!(error, ConfigError::NotFound(_)) {
            eprintln!("Warning: Invalid GOSLING_MODE: {error}. Falling back to smart_approve.");
        }
    }

    if let Err(error) = config.get_param::<u32>("GOSLING_MAX_TURNS") {
        if !matches!(error, ConfigError::NotFound(_)) {
            eprintln!("Warning: Invalid GOSLING_MAX_TURNS: {error}. Falling back to the default.");
        }
    }

    match config.get_param::<f64>("GOSLING_AUTO_COMPACT_THRESHOLD") {
        Ok(threshold) if threshold != 0.0 && !(0.0..1.0).contains(&threshold) => {
            eprintln!(
                "Warning: Invalid GOSLING_AUTO_COMPACT_THRESHOLD: {threshold}. Use 0 to disable auto-compaction or a value greater than 0 and less than 1."
            );
        }
        Err(error) if !matches!(error, ConfigError::NotFound(_)) => {
            eprintln!(
                "Warning: Invalid GOSLING_AUTO_COMPACT_THRESHOLD: {error}. Falling back to the default."
            );
        }
        _ => {}
    }

    match config.get_param::<f64>("GOSLING_AUTO_COMPACT_REDUCTION") {
        Ok(reduction) if reduction != 0.0 && !(0.0..1.0).contains(&reduction) => {
            eprintln!(
                "Warning: Invalid GOSLING_AUTO_COMPACT_REDUCTION: {reduction}. Use 0 to always fully collapse on auto-compaction, or a value greater than 0 and less than 1."
            );
        }
        Err(error) if !matches!(error, ConfigError::NotFound(_)) => {
            eprintln!(
                "Warning: Invalid GOSLING_AUTO_COMPACT_REDUCTION: {error}. Falling back to the default."
            );
        }
        _ => {}
    }
}

fn generate_serve_secret_key() -> String {
    use rand::distr::{Alphanumeric, SampleString};

    format!(
        "gosling-acp-{}",
        Alphanumeric.sample_string(&mut rand::rng(), 32)
    )
}

fn resolve_serve_builtins(builtins: Vec<String>, shell: bool) -> Vec<String> {
    if builtins.is_empty() && !shell {
        vec!["developer".to_string()]
    } else {
        builtins
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ServePlatform {
    #[default]
    Cli,
    Desktop,
}

impl From<ServePlatform> for GoslingPlatform {
    fn from(platform: ServePlatform) -> Self {
        match platform {
            ServePlatform::Cli => GoslingPlatform::GoslingCli,
            ServePlatform::Desktop => GoslingPlatform::GoslingDesktop,
        }
    }
}

#[derive(Parser)]
#[command(name = "gosling", author, version, display_name = "", about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Args, Debug, Clone)]
#[group(required = false, multiple = false)]
pub struct Identifier {
    #[arg(
        short = 'n',
        long,
        value_name = "NAME",
        help = "Name for the chat session (e.g., 'project-x')",
        long_help = "Specify a name for your chat session. When used with --resume, will resume this specific session if it exists."
    )]
    pub name: Option<String>,

    #[arg(
        long = "session-id",
        alias = "id",
        value_name = "SESSION_ID",
        help = "Session ID (e.g., '20250921_143022')",
        long_help = "Specify a session ID to resume. Requires --resume."
    )]
    pub session_id: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Legacy: Path for the chat session",
        long_help = "Legacy parameter for backward compatibility. Extracts session ID from the file path (e.g., '/path/to/20250325_200615.
jsonl' -> '20250325_200615')."
    )]
    pub path: Option<PathBuf>,
}

/// Session behavior options shared between Session and Run commands
#[derive(Args, Debug, Clone, Default)]
pub struct SessionOptions {
    #[arg(
        long,
        help = "Enable debug output mode with full content and no truncation",
        long_help = "When enabled, shows complete tool responses without truncation and full paths."
    )]
    pub debug: bool,

    #[arg(
        long = "max-tool-repetitions",
        value_name = "NUMBER",
        help = "Maximum number of consecutive identical tool calls allowed",
        long_help = "Set a limit on how many times the same tool can be called consecutively with identical parameters. Helps prevent infinite loops."
    )]
    pub max_tool_repetitions: Option<u32>,

    #[arg(
        long = "max-turns",
        value_name = "NUMBER",
        help = "Maximum number of turns allowed without user input (default: 1000)",
        long_help = "Set a limit on how many turns (iterations) the agent can take without asking for user input to continue."
    )]
    pub max_turns: Option<u32>,

    #[arg(
        long = "container",
        value_name = "CONTAINER_ID",
        help = "Docker container ID to run extensions inside",
        long_help = "Run extensions (stdio and built-in) inside the specified container. The extension must exist in the container. For built-in extensions, gosling must be installed inside the container."
    )]
    pub container: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StreamableHttpOptions {
    pub url: String,
    pub timeout: u64,
}

fn parse_streamable_http_extension(input: &str) -> Result<StreamableHttpOptions, String> {
    let mut input_iter = input.split_whitespace();
    let (mut url, mut timeout) = (String::new(), gosling::config::DEFAULT_EXTENSION_TIMEOUT);

    if let Some(url_str) = input_iter.next() {
        url.push_str(url_str);
    }

    for kv_pair in input_iter {
        if !kv_pair.contains('=') {
            continue;
        }

        let (key, value) = kv_pair.split_once('=').unwrap();

        // We Can have more keys here for setting other properties
        if key == "timeout" {
            if let Ok(seconds) = value.parse::<u64>() {
                timeout = seconds;
            }
        }
    }

    Ok(StreamableHttpOptions { url, timeout })
}

/// Extension configuration options shared between Session and Run commands
#[derive(Args, Debug, Clone, Default)]
pub struct ExtensionOptions {
    #[arg(
        long = "with-extension",
        value_name = "COMMAND",
        help = "Add stdio extensions (can be specified multiple times)",
        long_help = "Add stdio extensions from full commands with environment variables. Can be specified multiple times. Format: 'ENV1=val1 ENV2=val2 command args...'",
        action = clap::ArgAction::Append
    )]
    pub extensions: Vec<String>,

    #[arg(
        long = "with-streamable-http-extension",
        value_name = "URL",
        help = "Add streamable HTTP extensions (can be specified multiple times)",
        long_help = "Add streamable HTTP extensions from a URL. Can be specified multiple times. Format: 'url...' or 'url... timeout=100' to set up timeout other than default",
        action = clap::ArgAction::Append,
        value_parser = parse_streamable_http_extension
    )]
    pub streamable_http_extensions: Vec<StreamableHttpOptions>,

    #[arg(
        long = "with-builtin",
        value_name = "NAME",
        help = "Add builtin extensions by name (e.g., 'developer' or multiple: 'developer,github')",
        long_help = "Add one or more builtin extensions that are bundled with gosling by specifying their names, comma-separated",
        value_delimiter = ','
    )]
    pub builtins: Vec<String>,

    #[arg(
        long = "no-profile",
        help = "Don't load your default extensions, only use CLI-specified extensions"
    )]
    pub no_profile: bool,
}

/// Input source options for the run command
#[derive(Args, Debug, Clone, Default)]
pub struct InputOptions {
    /// Path to instruction file containing commands
    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Path to instruction file containing commands. Use - for stdin.",
        conflicts_with = "input_text"
    )]
    pub instructions: Option<String>,

    /// Input text containing commands
    #[arg(
        short = 't',
        long = "text",
        value_name = "TEXT",
        help = "Input text to provide to gosling directly",
        long_help = "Input text containing commands for gosling. Use this in lieu of the instructions argument.",
        conflicts_with = "instructions"
    )]
    pub input_text: Option<String>,

    /// Additional system prompt to customize agent behavior
    #[arg(
        long = "system",
        value_name = "TEXT",
        help = "Additional system prompt to customize agent behavior",
        long_help = "Provide additional system instructions to customize the agent's behavior"
    )]
    pub system: Option<String>,
}

/// Output configuration options for the run command
#[derive(Args, Debug, Clone)]
pub struct OutputOptions {
    /// Quiet mode - suppress non-response output
    #[arg(
        short = 'q',
        long = "quiet",
        help = "Quiet mode. Suppress non-response output, printing only the model response to stdout"
    )]
    pub quiet: bool,

    /// Output format (text, json, stream-json)
    #[arg(
        long = "output-format",
        value_name = "FORMAT",
        help = "Output format (text, json, stream-json)",
        default_value = "text",
        value_parser = clap::builder::PossibleValuesParser::new(["text", "json", "stream-json"])
    )]
    pub output_format: String,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            quiet: false,
            output_format: "text".to_string(),
        }
    }
}

/// Model/provider override options for the run command
#[derive(Args, Debug, Clone, Default)]
pub struct ModelOptions {
    /// Provider to use for this run (overrides environment variable)
    #[arg(
        long = "provider",
        value_name = "PROVIDER",
        help = "Specify the LLM provider to use (e.g., 'openai', 'anthropic')",
        long_help = "Override the GOSLING_PROVIDER environment variable for this run. Available providers include openai, anthropic, ollama, databricks, gemini-cli, claude-code, and others."
    )]
    pub provider: Option<String>,

    /// Model to use for this run (overrides environment variable)
    #[arg(
        long = "model",
        value_name = "MODEL",
        help = "Specify the model to use (e.g., 'gpt-4o', 'claude-sonnet-4-20250514')",
        long_help = "Override the GOSLING_MODEL environment variable for this run. The model must be supported by the specified provider."
    )]
    pub model: Option<String>,
}

/// Run execution behavior options
#[derive(Args, Debug, Clone, Default)]
pub struct RunBehavior {
    /// Continue in interactive mode after processing input
    #[arg(
        short = 's',
        long = "interactive",
        help = "Continue in interactive mode after processing initial input"
    )]
    pub interactive: bool,

    /// Run without storing a session file
    #[arg(
        long = "no-session",
        help = "Run without storing a session file",
        long_help = "Execute commands without creating or using a session file. Useful for automated runs.",
        conflicts_with_all = ["resume", "name", "path"]
    )]
    pub no_session: bool,

    /// Resume a previous run
    #[arg(
        short,
        long,
        action = clap::ArgAction::SetTrue,
        help = "Resume from a previous run",
        long_help = "Continue from a previous run, maintaining the execution state and context."
    )]
    pub resume: bool,

    /// Print generation statistics after completion
    #[arg(
        long = "stats",
        help = "Print generation statistics after the run completes"
    )]
    pub stats: bool,
}

async fn get_or_create_session_id(
    identifier: Option<Identifier>,
    resume: bool,
    no_session: bool,
    gosling_mode: GoslingMode,
) -> Result<Option<String>> {
    if no_session {
        return Ok(None);
    }

    let session_manager = SessionManager::instance();

    let resolved_id = if resume {
        let Some(id) = identifier else {
            let sessions = session_manager.list_sessions().await?;
            let session_id = sessions
                .first()
                .map(|s| s.id.clone())
                .ok_or_else(|| anyhow::anyhow!("No session found to resume"))?;
            return Ok(Some(session_id));
        };

        if let Some(session_id) = id.session_id {
            session_id
        } else if let Some(name) = id.name {
            let sessions = session_manager.list_sessions().await?;
            sessions
                .into_iter()
                .find(|s| s.name == name || s.id == name)
                .map(|s| s.id)
                .ok_or_else(|| anyhow::anyhow!("No session found with name '{}'", name))?
        } else if let Some(path) = id.path {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!("Could not extract session ID from path: {:?}", path)
                })?
        } else {
            return Err(anyhow::anyhow!("Invalid identifier"));
        }
    } else {
        let Some(id) = identifier else {
            let session = session_manager
                .create_session(
                    std::env::current_dir()?,
                    "CLI Session".to_string(),
                    SessionType::User,
                    gosling_mode,
                )
                .await?;
            return Ok(Some(session.id));
        };

        if id.session_id.is_some() {
            return Err(anyhow::anyhow!("Cannot use --session-id without --resume"));
        }

        let has_user_provided_name = id.name.is_some();
        let name = id.name.unwrap_or_else(|| "CLI Session".to_string());
        let session = session_manager
            .create_session(
                std::env::current_dir()?,
                name.clone(),
                SessionType::User,
                gosling_mode,
            )
            .await?;

        if has_user_provided_name {
            session_manager
                .update(&session.id)
                .user_provided_name(name)
                .apply()
                .await?;
        }

        return Ok(Some(session.id));
    };

    Ok(Some(resolved_id))
}

async fn lookup_session_id(identifier: Identifier) -> Result<String> {
    let session_manager = SessionManager::instance();

    if let Some(session_id) = identifier.session_id {
        Ok(session_id)
    } else if let Some(name) = identifier.name {
        let sessions = session_manager.list_sessions().await?;
        sessions
            .into_iter()
            .find(|s| s.name == name || s.id == name)
            .map(|s| s.id)
            .ok_or_else(|| anyhow::anyhow!("No session found with name '{}'", name))
    } else if let Some(path) = identifier.path {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Could not extract session ID from path: {:?}", path))
    } else {
        Err(anyhow::anyhow!("No identifier provided"))
    }
}

#[derive(Subcommand)]
enum SessionCommand {
    #[command(about = "List all available sessions")]
    List {
        #[arg(
            short,
            long,
            help = "Output format (text, json)",
            default_value = "text"
        )]
        format: String,

        #[arg(
            long = "ascending",
            help = "Sort by date in ascending order (oldest first)",
            long_help = "Sort sessions by date in ascending order (oldest first). Default is descending order (newest first)."
        )]
        ascending: bool,

        #[arg(
            short = 'w',
            short_alias = 'p',
            long = "working_dir",
            help = "Filter sessions by working directory"
        )]
        working_dir: Option<PathBuf>,

        #[arg(short = 'l', long = "limit", help = "Limit the number of results")]
        limit: Option<usize>,
    },
    #[command(about = "Remove sessions. Runs interactively if no ID, name, or regex is provided.")]
    Remove {
        #[command(flatten)]
        identifier: Option<Identifier>,
        #[arg(
            short = 'r',
            long,
            help = "Regex for removing matched sessions (optional)"
        )]
        regex: Option<String>,
        #[arg(
            short = 'y',
            long,
            help = "Remove matched sessions without an interactive confirmation"
        )]
        yes: bool,
    },
    #[command(about = "Export a session")]
    Export {
        #[command(flatten)]
        identifier: Option<Identifier>,

        #[arg(
            short,
            long,
            help = "Output file path (default: stdout)",
            long_help = "Path to save the exported Markdown. If not provided, output will be sent to stdout"
        )]
        output: Option<PathBuf>,

        #[arg(
            long = "format",
            value_name = "FORMAT",
            help = "Output format (markdown, json, yaml)",
            default_value = "markdown"
        )]
        format: String,

        #[arg(
            long = "nostr",
            help = "Publish the JSON session export as an encrypted Nostr event and print a Gosling share link"
        )]
        nostr: bool,

        #[arg(
            long = "relay",
            value_name = "RELAY",
            help = "Nostr relay URL to publish to (can be specified multiple times)",
            action = clap::ArgAction::Append
        )]
        relays: Vec<String>,
    },
    #[command(
        about = "Import a session from JSON, a Claude Code / Codex / Pi .jsonl, or an encrypted Nostr share link"
    )]
    Import {
        #[arg(
            help = "Path to a gosling session export, a Claude Code, Codex, or Pi .jsonl transcript, or a gosling://sessions/nostr share link"
        )]
        input: String,

        #[arg(long = "nostr", help = "Treat input as an encrypted Nostr share link")]
        nostr: bool,

        #[arg(
            long = "working-dir",
            value_name = "DIR",
            help = "Trusted working directory for the imported session (defaults to the current directory)"
        )]
        working_dir: Option<PathBuf>,
    },
    #[command(name = "diagnostics")]
    Diagnostics {
        #[command(flatten)]
        identifier: Option<Identifier>,

        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Install a plugin from a git repository URL
    #[command(about = "Install a plugin from a git repository URL")]
    Install {
        #[arg(
            long,
            help = "Automatically update this plugin before plugin skills are loaded"
        )]
        auto_update: bool,

        #[arg(help = "URL to a git repository containing a supported plugin")]
        url: String,
    },

    /// Update an installed git-backed plugin
    #[command(about = "Update an installed git-backed plugin")]
    Update {
        #[arg(help = "Name of the installed plugin to update")]
        name: String,
    },

    /// Trust a project's plugins so their hooks and MCP servers may run
    #[command(about = "Trust a project's plugins so their hooks and MCP servers may run")]
    Trust {
        #[arg(
            help = "Project directory to trust (defaults to the current directory)",
            default_value = "."
        )]
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum SkillsCommand {
    /// List all skills available to the gosling agent
    #[command(about = "List all skills available to the gosling agent")]
    List,
}

#[derive(Subcommand)]
enum SecretCommand {
    /// Store login credentials for a named server (e.g. a VPS) in the system keyring
    #[command(about = "Store login credentials for a named server in the system keyring")]
    Set {
        /// Server name, used as the credential key prefix (e.g. "racknerd" -> RACKNERD_PASSWORD)
        name: String,

        #[arg(long, help = "Login/username for the server")]
        login: Option<String>,

        #[arg(long, help = "Password for the server")]
        password: Option<String>,

        #[arg(long, help = "Hostname or IP address of the server")]
        host: Option<String>,

        #[arg(long, help = "SSH/connection port for the server")]
        port: Option<String>,
    },

    /// Print stored credential fields for a named server
    #[command(about = "Print stored credential fields for a named server")]
    Get {
        /// Server name
        name: String,

        /// Only print a single field (LOGIN, PASSWORD, HOST, or PORT)
        #[arg(long)]
        field: Option<String>,
    },

    /// Remove all stored credentials for a named server
    #[command(about = "Remove all stored credentials for a named server")]
    Remove {
        /// Server name
        name: String,
    },

    /// List server names with stored credentials
    #[command(about = "List server names with stored credentials")]
    List,
}

#[derive(Subcommand)]
enum McpSubcommand {
    /// Run one of the MCP servers bundled with gosling
    #[command(about = "Run one of the mcp servers bundled with gosling")]
    Serve {
        #[arg(value_parser = clap::value_parser!(McpCommand), value_name = "SERVER")]
        server: McpCommand,
    },

    /// Add an MCP server to the gosling config as an extension
    #[command(about = "Add an MCP server to the gosling config as an extension")]
    Install {
        /// Name for the extension
        name: String,

        /// Command that launches the MCP server, e.g. "npx -y @block/gdrive"
        #[arg(
            long,
            required_unless_present = "from_goose",
            conflicts_with = "from_goose"
        )]
        cmd: Option<String>,

        /// Environment variable for the server process (repeatable)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        envs: Vec<String>,

        /// Secret from an environment variable or KEY=VALUE stored outside config.yaml (repeatable)
        #[arg(long = "secret", value_name = "KEY[=VALUE]")]
        secrets: Vec<String>,

        /// Startup timeout in seconds
        #[arg(long, value_name = "SECS")]
        timeout: Option<u64>,

        /// Description shown in extension listings
        #[arg(long)]
        description: Option<String>,

        /// Working directory for the server process
        #[arg(long, value_name = "DIR")]
        cwd: Option<String>,

        /// Import extension configuration from Goose; keyring values are not copied
        #[arg(
            long,
            long_help = "Import extension configuration from Goose's config.yaml. Goose keyring values are not copied; export referenced keys and pass --secret KEY, or provide --secret KEY=VALUE."
        )]
        from_goose: bool,

        /// Goose config file to import from (defaults to Goose's standard location)
        #[arg(long, value_name = "PATH", requires = "from_goose")]
        goose_config: Option<PathBuf>,
    },

    /// Remove an extension from the gosling config
    #[command(about = "Remove an extension from the gosling config")]
    Remove {
        /// Name of the extension to remove
        name: String,
    },

    /// List extensions configured in the gosling config
    #[command(about = "List extensions configured in the gosling config")]
    List,
}

#[derive(Subcommand)]
enum Command {
    /// Configure gosling settings
    #[command(about = "Configure gosling settings")]
    Configure {},

    /// Display gosling configuration information
    #[command(about = "Display gosling information")]
    Info {
        /// Show verbose information including current configuration
        #[arg(short, long, help = "Show verbose information including config.yaml")]
        verbose: bool,
        #[arg(long, help = "Test provider connection and show status")]
        check: bool,
    },

    #[command(about = "Check that your Gosling setup is working")]
    Doctor {},

    /// Run bundled MCP servers or manage MCP server extensions
    #[command(
        about = "Run bundled MCP servers or manage MCP server extensions",
        args_conflicts_with_subcommands = true,
        arg_required_else_help = true
    )]
    Mcp {
        #[command(subcommand)]
        command: Option<McpSubcommand>,

        /// Bundled MCP server to run (shorthand for `mcp serve`)
        #[arg(value_parser = clap::value_parser!(McpCommand), value_name = "SERVER")]
        server: Option<McpCommand>,
    },

    /// Run gosling as an ACP (Agent Client Protocol) agent
    #[command(about = "Run gosling as an ACP agent server on stdio")]
    Acp {
        /// Add builtin extensions by name
        #[arg(
            long = "with-builtin",
            value_name = "NAME",
            help = "Add builtin extensions by name (e.g., 'developer' or multiple: 'developer,github')",
            long_help = "Add one or more builtin extensions that are bundled with gosling by specifying their names, comma-separated",
            value_delimiter = ','
        )]
        builtins: Vec<String>,
    },

    /// Validate a shell provisioning document against main Gosling settings
    #[command(about = "Validate a shell provisioning document")]
    ShellValidate {
        #[arg(long = "shell-id", value_name = "ID")]
        shell_id: String,

        #[arg(long = "shell-display-name", value_name = "NAME")]
        shell_display_name: String,

        #[arg(long = "shell-version", default_value = "1")]
        shell_version: String,

        #[arg(long = "shell-provisioning", value_name = "PATH")]
        shell_provisioning: PathBuf,

        #[arg(long = "with-builtin", value_name = "NAME", value_delimiter = ',')]
        builtins: Vec<String>,
    },

    /// Start ACP server over HTTP and WebSocket
    #[command(about = "Start ACP server over HTTP and WebSocket")]
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        #[arg(long, default_value = "3284")]
        port: u16,

        #[arg(long, help = "Serve ACP over TLS")]
        tls: bool,

        #[arg(long = "tls-cert-path", value_name = "PATH")]
        tls_cert_path: Option<String>,

        #[arg(long = "tls-key-path", value_name = "PATH")]
        tls_key_path: Option<String>,

        #[arg(long, value_enum, default_value_t = ServePlatform::Cli)]
        platform: ServePlatform,

        #[arg(
            long = "with-builtin",
            value_name = "NAME",
            help = "Add builtin extensions by name (e.g., 'developer' or multiple: 'developer,github')",
            long_help = "Add one or more builtin extensions that are bundled with gosling by specifying their names, comma-separated",
            value_delimiter = ',',
            action = clap::ArgAction::Append
        )]
        builtins: Vec<String>,

        #[arg(
            long = "shell-id",
            value_name = "ID",
            requires = "shell_display_name",
            help = "Run with a server-enforced shell identity"
        )]
        shell_id: Option<String>,

        #[arg(
            long = "shell-display-name",
            value_name = "NAME",
            requires = "shell_id"
        )]
        shell_display_name: Option<String>,

        #[arg(long = "shell-version", default_value = "1", requires = "shell_id")]
        shell_version: String,

        #[arg(
            long = "shell-provisioning",
            value_name = "PATH",
            requires = "shell_id",
            help = "Read a shell provisioning document without exposing secret values"
        )]
        shell_provisioning: Option<PathBuf>,

        #[arg(
            long = "shell-runtime-namespace",
            value_name = "NAMESPACE",
            requires = "shell_id",
            help = "Isolate shell data, state, sessions and caches while retaining shared Gosling configuration"
        )]
        shell_runtime_namespace: Option<String>,

        #[arg(
            long = "dangerously-unauthenticated",
            help = "Start the ACP endpoint without requiring GOSLING_SERVER__SECRET_KEY"
        )]
        dangerously_unauthenticated: bool,

        #[arg(
            long = "allowed-origin",
            value_name = "ORIGIN",
            action = clap::ArgAction::Append,
            help = "Allow an exact Origin value for ACP CORS; may be specified multiple times and replaces the default loopback origins"
        )]
        allowed_origins: Vec<String>,
    },

    /// Start or resume interactive chat sessions
    #[command(
        about = "Start or resume interactive chat sessions",
        visible_alias = "s"
    )]
    Session {
        #[command(subcommand)]
        command: Option<SessionCommand>,

        #[command(flatten)]
        identifier: Option<Identifier>,

        /// Resume a previous session
        #[arg(
            short,
            long,
            help = "Resume a previous session (last used or specified by --name/--session-id)",
            long_help = "Continue from a previous session. If --name or --session-id is provided, resumes that specific session. Otherwise, resumes the most recently used session."
        )]
        resume: bool,

        /// Fork a previous session (creates new session with copied history)
        #[arg(
            long,
            requires = "resume",
            help = "Fork a previous session (creates new session with copied history)",
            long_help = "Create a new session by copying all messages from a previous session. Must be used with --resume. If --name or --session-id is provided, forks that specific session. Otherwise, forks the most recently used session."
        )]
        fork: bool,

        /// Open the session's conversation in $EDITOR before starting
        #[arg(
            long,
            requires = "resume",
            help = "Edit the session conversation in $EDITOR before starting",
            long_help = "Open the session's conversation in your editor ($VISUAL / $EDITOR / vi) for modification before resuming. When combined with --fork, creates a new session from the edited result."
        )]
        edit: bool,

        /// Show message history when resuming
        #[arg(
            long,
            help = "Show previous messages when resuming a session",
            requires = "resume"
        )]
        history: bool,

        #[command(flatten)]
        session_opts: SessionOptions,

        #[command(flatten)]
        extension_opts: ExtensionOptions,
    },

    /// Open the last project directory
    #[command(about = "Open the last project directory", visible_alias = "p")]
    Project {},

    /// List recent project directories
    #[command(about = "List recent project directories", visible_alias = "ps")]
    Projects,

    /// Execute commands from an instruction file
    #[command(about = "Execute commands from an instruction file or stdin")]
    Run {
        #[command(flatten)]
        input_opts: InputOptions,

        #[command(flatten)]
        identifier: Option<Identifier>,

        #[command(flatten)]
        run_behavior: RunBehavior,

        #[command(flatten)]
        session_opts: SessionOptions,

        #[command(flatten)]
        extension_opts: ExtensionOptions,

        #[command(flatten)]
        output_opts: OutputOptions,

        #[command(flatten)]
        model_opts: ModelOptions,
    },

    /// Skill utilities
    #[command(about = "Skill utilities")]
    Skills {
        #[command(subcommand)]
        command: SkillsCommand,
    },

    /// Manage plugins
    #[command(about = "Manage plugins")]
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },

    /// Manage stored server credentials (e.g. VPS login/password)
    #[command(about = "Manage stored server credentials (e.g. VPS login/password)")]
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },

    /// Update the gosling CLI version
    #[cfg(feature = "update")]
    #[command(about = "Update the gosling CLI version")]
    Update {
        /// Update to canary version
        #[arg(
            short,
            long,
            help = "Update to canary version",
            long_help = "Update to the latest canary version of the gosling CLI, otherwise updates to the latest stable version."
        )]
        canary: bool,

        /// Enforce to re-configure gosling during update
        #[arg(short, long, help = "Enforce to re-configure gosling during update")]
        reconfigure: bool,
    },

    /// Terminal-integrated session (one session per terminal)
    #[command(
        about = "Terminal-integrated gosling session",
        long_about = "Runs a gosling session tied to your terminal window.\n\
                      Each terminal maintains its own persistent session that resumes automatically.\n\n\
                      Setup:\n  \
                        eval \"$(gosling term init zsh)\"  # zsh/bash\n  \
                        let init = ($nu.cache-dir | path join \"gosling-term-init.nu\"); ^gosling term init nu | save --force $init; source $init\n\n\
                      Usage:\n  \
                        gosling term run \"list files in this directory\"\n  \
                        @gosling \"create a python script\"  # using alias\n  \
                        @g \"quick question\"  # short alias"
    )]
    Term {
        #[command(subcommand)]
        command: TermCommand,
    },

    /// Launch the gosling terminal UI (TUI)
    #[cfg(feature = "tui")]
    #[command(
        about = "Launch the gosling terminal UI",
        long_about = "Launch the gosling terminal UI (the @repo-makeover/gosling npm package).\n\
                      \n\
                      Resolution order:\n  \
                      1. GOSLING_TUI_SCRIPT, if set to an existing dist/tui.js\n  \
                      2. A local checkout's ui/text/dist/tui.js (dev workflow)\n  \
                      3. `npx --yes --package <spec> -- gosling-tui` (deployed installs)\n\
                      \n\
                      Override the npm spec via GOSLING_TUI_NPM_SPEC (default: @repo-makeover/gosling@latest).\n\
                      Local script mode requires `node` on PATH; npx mode requires `npx` on PATH.\n\
                      Any extra arguments are passed through to the TUI."
    )]
    Tui {
        /// Arguments forwarded to the TUI
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Generate completions for various shells
    #[command(
        about = "Generate the autocompletion script or Nushell module for the specified shell"
    )]
    Completion {
        #[arg(value_enum)]
        shell: CompletionShell,

        #[arg(long, default_value = "gosling", help = "Provide a custom binary name")]
        bin_name: String,
    },

    /// Local code review.
    ///
    /// Discovers `**/.agents/checks/*.md` subagent reviewers and
    /// `**/.agents/REVIEW.md` scoped prompt overrides, builds a review
    /// request from the working tree (or an explicit diff range), and
    /// runs the review through gosling.
    #[command(about = "Review the current diff using gosling")]
    Review {
        /// Diff range to review (e.g. "main...HEAD"). Defaults to the working
        /// tree vs HEAD.
        #[arg(value_name = "RANGE")]
        range: Option<String>,

        /// Path to a Markdown file with a custom base review prompt. Replaces
        /// the embedded default prompt.
        #[arg(long = "prompt", value_name = "FILE")]
        prompt: Option<PathBuf>,

        /// Default model used for the main review agent and for any check
        /// that does not declare its own `model:` in frontmatter.
        #[arg(long = "model", value_name = "MODEL")]
        model: Option<String>,

        /// Provider for the main review agent.
        #[arg(long = "provider", value_name = "PROVIDER")]
        provider: Option<String>,

        /// Force every discovered check to use this model, regardless of
        /// the check's own `model:` field.
        #[arg(long = "override-model", value_name = "MODEL")]
        override_model: Option<String>,

        /// Default `turn-limit` for orchestrated main-pass subprocesses and
        /// for checks that do not declare their own. Does not cap the legacy
        /// `--no-orchestrate` in-process main agent.
        #[arg(long = "turn-limit", value_name = "N")]
        turn_limit: Option<usize>,

        /// Print the assembled review prompt and discovered checks instead of
        /// running the review.
        #[arg(long = "dry-run")]
        dry_run: bool,

        /// Suppress non-result output from the underlying agent.
        #[arg(long, short = 'q')]
        quiet: bool,

        /// Disable the Rust-driven parallel orchestrator and fall back to
        /// the single-prompt path that asks the main agent to delegate
        /// each check via `delegate(... async: true ...)`. The default
        /// orchestrator dispatches one `gosling run` subprocess per check
        /// (capped at 4 concurrent), bounding wall-clock to the slowest
        /// single check rather than waiting on the model to issue
        /// dispatches.
        #[arg(long = "no-orchestrate")]
        no_orchestrate: bool,

        /// Additional free-form instructions to prepend to the review
        /// (e.g. PR intent, commit-message context, "this is a refactor,
        /// flag any behavior change"). Mirrors `amp review --instructions`
        /// for drop-in compatibility with existing reviewer wrappers.
        #[arg(long = "instructions", short = 'i', value_name = "TEXT")]
        instructions: Option<String>,

        /// Restrict the review to a specific set of files. Other files in
        /// the diff are still passed to the agent for context but are
        /// excluded from the assembled diff sent to checks. Mirrors
        /// `amp review --files`.
        #[arg(long = "files", short = 'f', value_name = "FILE", num_args = 1..)]
        files: Vec<String>,

        /// Only run checks whose `name` matches one of these. Other
        /// discovered checks are skipped. Mirrors `amp review --check-filter`.
        #[arg(long = "check-filter", short = 'c', value_name = "NAME", num_args = 1..)]
        check_filter: Vec<String>,

        /// Alternate directory to search for `.agents/checks/*.md` instead
        /// of the repo root. Mirrors `amp review --check-scope`.
        #[arg(long = "check-scope", short = 's', value_name = "DIR")]
        check_scope: Option<PathBuf>,

        /// Skip the main correctness pass and only run check subagents.
        /// Mirrors `amp review --checks-only`.
        #[arg(long = "checks-only")]
        checks_only: bool,

        /// Print only the diff summary; skip the full review.
        /// Mirrors `amp review --summary-only`.
        #[arg(long = "summary-only")]
        summary_only: bool,

        /// Minimum severity to display. Findings below this rank are
        /// dropped from the output. Default is `medium`, matching
        /// Amp's CLI which hides `low` from review output. Pass
        /// `--severity low` to surface every finding.
        #[arg(long = "severity", value_name = "LEVEL", default_value = "medium")]
        severity: String,
    },
    #[command(
        name = "validate-extensions",
        about = "Validate a bundled-extensions.json file",
        hide = true
    )]
    ValidateExtensions {
        #[arg(help = "Path to the bundled-extensions.json file")]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum TermCommand {
    /// Print shell initialization script
    #[command(
        about = "Print shell initialization script",
        long_about = "Prints shell configuration to set up terminal-integrated sessions.\n\
                      Each terminal gets a persistent gosling session that automatically resumes.\n\n\
                      Setup:\n  \
                        echo 'eval \"$(gosling term init zsh)\"' >> ~/.zshrc\n  \
                        source ~/.zshrc\n\n\
                        Nushell:\n  \
                        let init = ($nu.cache-dir | path join \"gosling-term-init.nu\")\n  \
                        ^gosling term init nu | save --force $init\n  \
                        source $init\n\n\
                      With --default (anything typed that isn't a command goes to gosling):\n  \
                        echo 'eval \"$(gosling term init zsh --default)\"' >> ~/.zshrc\n  \
                        ^gosling term init nu --default | save --force $init"
    )]
    Init {
        /// Shell type (bash, zsh, fish, nu, powershell)
        #[arg(value_enum)]
        shell: Shell,

        #[arg(short, long, help = "Name for the terminal session")]
        name: Option<String>,

        /// Make gosling the default handler for unknown commands
        #[arg(
            long = "default",
            help = "Make gosling the default handler for unknown commands",
            long_help = "When enabled, anything you type that isn't a valid command will be sent to gosling. Supported for zsh, bash, and nu."
        )]
        default: bool,
    },

    /// Log a shell command (called by shell hook)
    #[command(about = "Log a shell command to the session", hide = true)]
    Log {
        /// The command that was executed
        command: String,
    },

    /// Run a prompt in the terminal session
    #[command(
        about = "Run a prompt in the terminal session",
        long_about = "Run a prompt in the terminal-integrated session.\n\n\
                      Examples:\n  \
                        gosling term run list files in this directory\n  \
                        @gosling list files  # using alias\n  \
                        @g why did that fail  # short alias"
    )]
    Run {
        /// The prompt to send to gosling (multiple words allowed without quotes)
        #[arg(required = true, num_args = 1..)]
        prompt: Vec<String>,
    },

    /// Print session info for prompt integration
    #[command(
        about = "Print session info for prompt integration",
        long_about = "Prints compact session info (token usage, model) for shell prompt integration.\n\
                      Example output: ●○○○○ sonnet"
    )]
    Info,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum CliProviderVariant {
    OpenAi,
    Databricks,
    Ollama,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    #[value(alias = "pwsh")]
    Powershell,
    #[value(alias = "nushell")]
    Nu,
    Zsh,
}

impl CompletionShell {
    fn generate(self, cmd: &mut clap::Command, bin_name: &str, writer: &mut dyn std::io::Write) {
        match self {
            CompletionShell::Bash => generate(ClapShell::Bash, cmd, bin_name, writer),
            CompletionShell::Elvish => generate(ClapShell::Elvish, cmd, bin_name, writer),
            CompletionShell::Fish => generate(ClapShell::Fish, cmd, bin_name, writer),
            CompletionShell::Powershell => generate(ClapShell::PowerShell, cmd, bin_name, writer),
            CompletionShell::Nu => generate(ClapNushell, cmd, bin_name, writer),
            CompletionShell::Zsh => generate(ClapShell::Zsh, cmd, bin_name, writer),
        }
    }
}

#[derive(Debug)]
pub struct InputConfig {
    pub contents: Option<String>,
    pub additional_system_prompt: Option<String>,
}

fn get_command_name(command: &Option<Command>) -> &'static str {
    match command {
        Some(Command::Configure {}) => "configure",
        Some(Command::Doctor {}) => "doctor",
        Some(Command::Info { .. }) => "info",
        Some(Command::Mcp { .. }) => "mcp",
        Some(Command::Acp { .. }) => "acp",
        Some(Command::ShellValidate { .. }) => "shell-validate",
        Some(Command::Serve { .. }) => "serve",
        Some(Command::Session { .. }) => "session",
        Some(Command::Project {}) => "project",
        Some(Command::Projects) => "projects",
        Some(Command::Run { .. }) => "run",
        #[cfg(feature = "update")]
        Some(Command::Update { .. }) => "update",
        Some(Command::Skills { .. }) => "skills",
        Some(Command::Plugin { .. }) => "plugin",
        Some(Command::Secret { .. }) => "secret",
        Some(Command::Term { .. }) => "term",
        #[cfg(feature = "tui")]
        Some(Command::Tui { .. }) => "tui",
        Some(Command::Completion { .. }) => "completion",
        Some(Command::Review { .. }) => "review",
        Some(Command::ValidateExtensions { .. }) => "validate-extensions",
        None => "default_session",
    }
}

async fn handle_mcp_command(server: McpCommand) -> Result<()> {
    let name = server.name();
    let _ = crate::logging::setup_logging(Some(&format!("mcp-{name}")));
    match server {
        McpCommand::AutoVisualiser => serve(AutoVisualiserRouter::new()).await?,
        McpCommand::ComputerController => serve(ComputerControllerServer::new()).await?,
    }
    Ok(())
}

async fn handle_mcp_subcommand(command: McpSubcommand) -> Result<()> {
    use crate::commands::mcp;

    match command {
        McpSubcommand::Serve { server } => handle_mcp_command(server).await,
        McpSubcommand::Install {
            name,
            cmd,
            envs,
            secrets,
            timeout,
            description,
            cwd,
            from_goose,
            goose_config,
        } => {
            mcp::handle_install(mcp::InstallArgs {
                name,
                cmd,
                envs,
                secrets,
                timeout,
                description,
                cwd,
                from_goose,
                goose_config,
            })
            .await
        }
        McpSubcommand::Remove { name } => mcp::handle_remove(&name),
        McpSubcommand::List => mcp::handle_list(),
    }
}

struct ServeCommandArgs {
    host: String,

    port: u16,
    tls: bool,
    tls_cert_path: Option<String>,
    tls_key_path: Option<String>,
    platform: ServePlatform,
    builtins: Vec<String>,
    shell_id: Option<String>,
    shell_display_name: Option<String>,
    shell_version: String,
    shell_provisioning: Option<PathBuf>,
    shell_runtime_namespace: Option<String>,
    dangerously_unauthenticated: bool,
    allowed_origins: Vec<String>,
}

const DANGEROUS_UNAUTHENTICATED_WARNING: &str =
    "WARNING: ACP authentication is disabled. Any client that can reach this server may invoke agent capabilities.";

#[cfg(test)]
mod serve_warning_tests {
    use super::DANGEROUS_UNAUTHENTICATED_WARNING;

    #[test]
    fn dangerous_serve_warning_names_authentication_and_reachability() {
        assert!(DANGEROUS_UNAUTHENTICATED_WARNING.contains("authentication is disabled"));
        assert!(DANGEROUS_UNAUTHENTICATED_WARNING.contains("Any client"));
    }
}

fn validate_shell_id(shell_id: &str) -> Result<()> {
    let valid = !shell_id.is_empty()
        && shell_id.len() <= 64
        && shell_id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        });
    if valid && shell_id != "." && shell_id != ".." {
        Ok(())
    } else {
        anyhow::bail!("shell ID must use 1-64 lowercase letters, digits, '-' or '_'")
    }
}

async fn build_shell_runtime(
    shell_id: Option<String>,
    shell_display_name: Option<String>,
    shell_version: String,
    provisioning_path: Option<&std::path::Path>,
    shell_runtime_namespace: Option<&str>,
    working_dir: PathBuf,
) -> Result<ShellRuntime> {
    let Some(shell_id) = shell_id else {
        return Ok(ShellRuntime::default());
    };
    validate_shell_id(&shell_id)?;
    let display_name = shell_display_name
        .ok_or_else(|| anyhow::anyhow!("--shell-display-name is required with --shell-id"))?;
    let runtime_namespace = shell_runtime_namespace.unwrap_or(&shell_id).to_owned();
    validate_shell_id(&runtime_namespace)?;
    let mut provisioning = match provisioning_path {
        Some(path) => serde_json::from_slice::<ShellProvisioning>(&std::fs::read(path)?)?,
        None => ShellProvisioning {
            schema_version: SHELL_PROVISIONING_SCHEMA_VERSION,
            protocol_policy: ShellProtocolPolicy {
                mode: ShellAuthorityMode::Inherit,
                denied_methods: Vec::new(),
            },
            session: ShellSessionProvisioning::default(),
            ..ShellProvisioning::default()
        },
    };
    if provisioning.schema_version != 0
        && provisioning.schema_version != SHELL_PROVISIONING_SCHEMA_VERSION
    {
        anyhow::bail!(
            "unsupported shell provisioning schema version {}",
            provisioning.schema_version
        );
    }
    provisioning.identity = ShellIdentity {
        id: shell_id,
        display_name,
        version: shell_version,
        runtime_namespace,
    };
    provisioning.schema_version = SHELL_PROVISIONING_SCHEMA_VERSION;
    let domain_adapter: Option<std::sync::Arc<dyn DomainAdapter>> = match provisioning
        .domain_adapter
        .clone()
    {
        Some(expected_descriptor) => {
            let registration =
                get_domain_adapter_registration(Config::global(), &expected_descriptor.domain_id)?
                    .ok_or_else(|| anyhow::anyhow!("ADAPTER_NOT_REGISTERED"))?;
            Some(std::sync::Arc::new(
                McpDomainAdapter::connect(registration, expected_descriptor, working_dir).await?,
            ))
        }
        None => None,
    };
    Ok(ShellRuntime::new(provisioning, domain_adapter))
}

async fn handle_shell_validate_command(
    shell_id: String,
    shell_display_name: String,
    shell_version: String,
    shell_provisioning: PathBuf,
    builtins: Vec<String>,
) -> Result<()> {
    use gosling::workspace::WorkspaceService;

    let default_working_dir = std::env::current_dir()?;
    let runtime = build_shell_runtime(
        Some(shell_id.clone()),
        Some(shell_display_name),
        shell_version,
        Some(&shell_provisioning),
        Some(&shell_id),
        default_working_dir.clone(),
    )
    .await?;
    let base_paths = RuntimePaths::new(Paths::config_dir(), Paths::data_dir(), Paths::state_dir());
    let workspace_service =
        WorkspaceService::initialize(&base_paths.data_dir, &default_working_dir).await?;
    let builtins = resolve_serve_builtins(builtins, true);
    let report = gosling::acp::shell_validation::validate_shell_provisioning(
        runtime.provisioning(),
        Config::global(),
        &workspace_service,
        &builtins,
        &default_working_dir,
    )
    .await;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.valid {
        anyhow::bail!("shell provisioning is invalid");
    }
    Ok(())
}

async fn handle_serve_command(args: ServeCommandArgs) -> Result<()> {
    use axum::http::HeaderValue;
    use gosling::acp::server_factory::{AcpServer, AcpServerFactoryConfig};
    use gosling::acp::transport::create_router;
    use gosling::config::paths::Paths;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tracing::{info, warn};

    let ServeCommandArgs {
        host,
        port,
        tls,
        tls_cert_path,
        tls_key_path,
        platform,
        builtins,
        shell_id,
        shell_display_name,
        shell_version,
        shell_provisioning,
        shell_runtime_namespace,
        dangerously_unauthenticated,
        allowed_origins,
    } = args;

    let shell = shell_id.is_some();
    let builtins = resolve_serve_builtins(builtins, shell);

    let base_paths = RuntimePaths::new(Paths::config_dir(), Paths::data_dir(), Paths::state_dir());
    let runtime_paths = match shell_runtime_namespace.as_deref() {
        Some(namespace) => {
            RuntimePaths::for_namespace(&base_paths, namespace).map_err(anyhow::Error::msg)?
        }
        None => base_paths.clone(),
    };
    let default_working_dir = std::env::current_dir()?;
    let shell_runtime = build_shell_runtime(
        shell_id,
        shell_display_name,
        shell_version,
        shell_provisioning.as_deref(),
        shell_runtime_namespace.as_deref(),
        default_working_dir,
    )
    .await?;

    let additional_source_roots = Config::global()
        .get_param::<String>("ADDITIONAL_AGENT_SOURCE_ROOTS")
        .ok()
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|path| {
            let path = path.canonicalize().unwrap_or(path);
            SourceRoot::read_only(path)
        })
        .collect();

    let server = Arc::new(AcpServer::new(AcpServerFactoryConfig {
        builtins,
        state_dir: runtime_paths.state_dir,
        data_dir: runtime_paths.data_dir,
        platform_data_dir: base_paths.data_dir,
        config_dir: runtime_paths.config_dir,
        gosling_platform: platform.into(),
        additional_source_roots,
        shell_runtime,
    }));
    let env_secret = std::env::var(GOSLING_SERVER_SECRET_KEY_ENV)
        .ok()
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty());
    let require_token = env_secret.is_some();
    if !require_token && !dangerously_unauthenticated {
        anyhow::bail!(
            "{GOSLING_SERVER_SECRET_KEY_ENV} must be set to start `gosling serve`; pass --dangerously-unauthenticated to run without ACP authentication"
        );
    }
    if dangerously_unauthenticated && !require_token {
        eprintln!("{DANGEROUS_UNAUTHENTICATED_WARNING}");
        warn!(
            "{GOSLING_SERVER_SECRET_KEY_ENV} is not set and --dangerously-unauthenticated was passed; the ACP endpoint will accept unauthenticated connections"
        );
    }
    let additional_allowed_origins = allowed_origins
        .into_iter()
        .map(|origin| {
            let origin = origin.trim();
            if origin.is_empty() || origin == "*" {
                anyhow::bail!("--allowed-origin must be a non-wildcard Origin value");
            }
            HeaderValue::from_str(origin).map_err(|error| {
                anyhow::anyhow!("invalid --allowed-origin value `{origin}`: {error}")
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let secret_key = env_secret.unwrap_or_else(generate_serve_secret_key);
    let router = create_router(
        server,
        secret_key,
        require_token,
        additional_allowed_origins,
    );

    let config = Config::global();
    let tls_cert_path =
        tls_cert_path.or_else(|| config.get_param::<String>("GOSLING_TLS_CERT_PATH").ok());
    let tls_key_path =
        tls_key_path.or_else(|| config.get_param::<String>("GOSLING_TLS_KEY_PATH").ok());
    let tls = tls
        || config.get_param::<bool>("GOSLING_TLS").unwrap_or(false)
        || tls_cert_path.is_some()
        || tls_key_path.is_some();

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    // Running without ACP authentication is only defensible while the socket
    // is unreachable from off-box. The warning above said "dangerous" but
    // nothing stopped `--host 0.0.0.0`, so a single flag could publish an
    // unauthenticated agent to the whole network. Refuse the combination
    // rather than warn about it. (SEC-GOS-012)
    if !require_token && !addr.ip().is_loopback() {
        anyhow::bail!(
            "--dangerously-unauthenticated only permits a loopback bind, but --host resolved to \
             {}. Either bind 127.0.0.1 (or ::1), or set {GOSLING_SERVER_SECRET_KEY_ENV} and drop \
             the flag.",
            addr.ip()
        );
    }
    if tls {
        #[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
        {
            let tls_setup = gosling::acp::transport::tls::setup_tls(
                tls_cert_path.as_deref(),
                tls_key_path.as_deref(),
            )
            .await?;
            info!("Starting ACP server on https://{}", addr);
            let shutdown_handle = axum_server::Handle::new();
            let signal_handle = shutdown_handle.clone();
            tokio::spawn(async move {
                crate::signal::shutdown_signal().await;
                signal_handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
            });

            #[cfg(feature = "rustls-tls")]
            axum_server::bind_rustls(addr, tls_setup.config)
                .handle(shutdown_handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await?;

            #[cfg(feature = "native-tls")]
            axum_server::bind_openssl(addr, tls_setup.config)
                .handle(shutdown_handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await?;
        }

        #[cfg(not(any(feature = "rustls-tls", feature = "native-tls")))]
        {
            let _ = (tls_cert_path, tls_key_path);
            anyhow::bail!(
                "TLS was requested but no TLS backend is enabled. \
                 Enable the `rustls-tls` or `native-tls` feature."
            );
        }
    } else {
        info!("Starting ACP server on http://{}", addr);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(crate::signal::shutdown_signal())
        .await?;
    }

    Ok(())
}

async fn handle_session_subcommand(command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::List {
            format,
            ascending,
            working_dir,
            limit,
        } => {
            handle_session_list(format, ascending, working_dir, limit).await?;
        }
        SessionCommand::Remove {
            identifier,
            regex,
            yes,
        } => {
            let (session_id, name) = if let Some(id) = identifier {
                (id.session_id, id.name)
            } else {
                (None, None)
            };
            handle_session_remove(session_id, name, regex, yes).await?;
        }
        SessionCommand::Export {
            identifier,
            output,
            format,
            nostr,
            relays,
        } => {
            let session_manager = SessionManager::instance();
            let session_identifier = if let Some(id) = identifier {
                lookup_session_id(id).await?
            } else {
                match crate::commands::session::prompt_interactive_session_selection(
                    &session_manager,
                )
                .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        return Ok(());
                    }
                }
            };
            crate::commands::session::handle_session_export(
                session_identifier,
                output,
                format,
                nostr,
                relays,
            )
            .await?;
        }
        SessionCommand::Import {
            input,
            nostr,
            working_dir,
        } => {
            crate::commands::session::handle_session_import(input, nostr, working_dir).await?;
        }
        SessionCommand::Diagnostics { identifier, output } => {
            let session_manager = SessionManager::instance();
            let session_id = if let Some(id) = identifier {
                lookup_session_id(id).await?
            } else {
                match crate::commands::session::prompt_interactive_session_selection(
                    &session_manager,
                )
                .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        return Ok(());
                    }
                }
            };
            crate::commands::session::handle_diagnostics(&session_id, output).await?;
        }
    }
    Ok(())
}

async fn handle_interactive_session(
    identifier: Option<Identifier>,
    resume: bool,
    fork: bool,
    edit: bool,
    history: bool,
    session_opts: SessionOptions,
    extension_opts: ExtensionOptions,
) -> Result<()> {
    let session_start = std::time::Instant::now();
    let session_type = if fork {
        "forked"
    } else if resume {
        "resumed"
    } else {
        "new"
    };

    tracing::info!(
        monotonic_counter.gosling.session_starts = 1,
        session_type,
        interactive = true,
        "Session started"
    );

    if let Some(Identifier {
        session_id: Some(_),
        ..
    }) = &identifier
    {
        if !resume {
            eprintln!("Error: --session-id can only be used with --resume flag");
            std::process::exit(1);
        }
    }

    if fork && !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "--fork requires an interactive terminal; no session was copied. Use the Desktop or an interactive terminal to fork a session."
        );
    }

    let gosling_mode = Config::global().get_gosling_mode().unwrap_or_default();
    let mut session_id = get_or_create_session_id(identifier, resume, false, gosling_mode).await?;

    if edit || fork {
        if let Some(ref id) = session_id {
            let session_manager = SessionManager::instance();
            let original = session_manager.get_session(id, true).await?;

            let target_id = if fork {
                let copied = session_manager
                    .copy_session(id, original.name.clone())
                    .await?;
                let copied_id = copied.id.clone();
                session_id = Some(copied.id);
                copied_id
            } else {
                id.clone()
            };

            if edit {
                let conversation = original
                    .conversation
                    .ok_or_else(|| anyhow::anyhow!("session has no messages to edit"))?;
                let edited = crate::session::editor::edit_conversation(&conversation)?;
                session_manager
                    .replace_conversation(&target_id, &edited)
                    .await?;
            }
        }
    }

    let mut session: crate::CliSession = build_session(SessionBuilderConfig {
        session_id,
        resume,
        fork,
        no_session: false,
        extensions: extension_opts.extensions,
        streamable_http_extensions: extension_opts.streamable_http_extensions,
        builtins: extension_opts.builtins,
        no_profile: extension_opts.no_profile,
        additional_system_prompt: None,
        provider: None,
        model: None,
        debug: session_opts.debug,
        max_tool_repetitions: session_opts.max_tool_repetitions,
        max_turns: session_opts.max_turns,
        interactive: true,
        quiet: false,
        output_format: "text".to_string(),
        container: session_opts.container.map(Container::new),
        stats: false,
    })
    .await;

    if (resume || fork) && history {
        session.render_message_history();
    }

    let result = session.interactive(None).await;
    log_session_completion(&session, session_start, session_type, result.is_ok()).await;
    result
}

async fn log_session_completion(
    session: &crate::CliSession,
    session_start: std::time::Instant,
    session_type: &str,
    success: bool,
) {
    let session_duration = session_start.elapsed();
    let exit_type = if success { "normal" } else { "error" };

    let (total_tokens, message_count) = session
        .get_session()
        .await
        .map(|m| (m.usage.total_tokens.unwrap_or(0), m.message_count))
        .unwrap_or((0, 0));

    tracing::info!(
        monotonic_counter.gosling.session_completions = 1,
        session_type,
        exit_type,
        duration_ms = session_duration.as_millis() as u64,
        total_tokens,
        message_count,
        "Session completed"
    );

    tracing::info!(
        monotonic_counter.gosling.session_duration_ms = session_duration.as_millis() as u64,
        session_type,
        "Session duration"
    );

    if total_tokens > 0 {
        tracing::info!(
            monotonic_counter.gosling.session_tokens = total_tokens,
            session_type,
            "Session tokens"
        );
    }
}

fn parse_run_input(input_opts: &InputOptions) -> Result<Option<InputConfig>> {
    parse_run_input_from_reader(input_opts, &mut std::io::stdin())
}

fn parse_run_input_from_reader(
    input_opts: &InputOptions,
    stdin: &mut impl Read,
) -> Result<Option<InputConfig>> {
    let input: Option<InputConfig> = match (&input_opts.instructions, &input_opts.input_text) {
        (Some(file), _) if file == "-" => {
            let mut contents = String::new();
            stdin.read_to_string(&mut contents).map_err(|error| {
                anyhow::anyhow!("Failed to read instructions from stdin: {error}")
            })?;
            Some(InputConfig {
                contents: Some(contents),
                additional_system_prompt: input_opts.system.clone(),
            })
        }
        (Some(file), _) => {
            let contents = std::fs::read_to_string(file).unwrap_or_else(|err| {
                eprintln!(
                    "Instruction file not found — did you mean to use gosling run --text?\n{}",
                    err
                );
                std::process::exit(1);
            });
            Some(InputConfig {
                contents: Some(contents),
                additional_system_prompt: input_opts.system.clone(),
            })
        }
        (_, Some(text)) => Some(InputConfig {
            contents: Some(text.clone()),
            additional_system_prompt: input_opts.system.clone(),
        }),
        _ => {
            eprintln!("Error: Must provide either --instructions (-i) or --text (-t). Use -i - for stdin.");
            std::process::exit(1);
        }
    };

    if input
        .as_ref()
        .and_then(|input| input.contents.as_deref())
        .is_some_and(|contents| contents.trim().is_empty())
    {
        anyhow::bail!("Instructions must not be empty");
    }

    Ok(input)
}

async fn handle_run_command(
    input_opts: InputOptions,
    identifier: Option<Identifier>,
    run_behavior: RunBehavior,
    session_opts: SessionOptions,
    extension_opts: ExtensionOptions,
    output_opts: OutputOptions,
    model_opts: ModelOptions,
) -> Result<()> {
    let Some(input_config) = parse_run_input(&input_opts)? else {
        return Ok(());
    };

    if let Some(Identifier {
        session_id: Some(_),
        ..
    }) = &identifier
    {
        if !run_behavior.resume {
            eprintln!("Error: --session-id can only be used with --resume flag");
            std::process::exit(1);
        }
    }

    let gosling_mode = Config::global().get_gosling_mode().unwrap_or_default();
    let session_id = get_or_create_session_id(
        identifier,
        run_behavior.resume,
        run_behavior.no_session,
        gosling_mode,
    )
    .await?;

    let mut session = build_session(SessionBuilderConfig {
        session_id,
        resume: run_behavior.resume,
        fork: false,
        no_session: run_behavior.no_session,
        extensions: extension_opts.extensions,
        streamable_http_extensions: extension_opts.streamable_http_extensions,
        builtins: extension_opts.builtins,
        no_profile: extension_opts.no_profile,
        additional_system_prompt: input_config.additional_system_prompt,
        provider: model_opts.provider,
        model: model_opts.model,
        debug: session_opts.debug,
        max_tool_repetitions: session_opts.max_tool_repetitions,
        max_turns: session_opts.max_turns,
        interactive: run_behavior.interactive,
        quiet: output_opts.quiet,
        output_format: output_opts.output_format,
        container: session_opts.container.map(Container::new),
        stats: run_behavior.stats,
    })
    .await;

    if run_behavior.interactive {
        session.interactive(input_config.contents).await
    } else if let Some(contents) = input_config.contents {
        let session_start = std::time::Instant::now();
        let session_type = "run";

        tracing::info!(
            monotonic_counter.gosling.session_starts = 1,
            session_type,
            interactive = false,
            "Headless session started"
        );

        let result = session.headless(contents).await;
        log_session_completion(&session, session_start, session_type, result.is_ok()).await;
        result
    } else {
        Err(anyhow::anyhow!(
            "no text provided for prompt in headless mode"
        ))
    }
}

fn handle_plugin_subcommand(command: PluginCommand) -> Result<()> {
    match command {
        PluginCommand::Install { url, auto_update } => handle_plugin_install(&url, auto_update),
        PluginCommand::Update { name } => handle_plugin_update(&name),
        PluginCommand::Trust { path } => handle_plugin_trust(&path),
    }
}

async fn handle_skills_subcommand(command: SkillsCommand) -> Result<()> {
    match command {
        SkillsCommand::List => handle_skills_list().await,
    }
}

fn handle_secret_subcommand(command: SecretCommand) -> Result<()> {
    match command {
        SecretCommand::Set {
            name,
            login,
            password,
            host,
            port,
        } => crate::commands::secret::handle_set(crate::commands::secret::SetArgs {
            name,
            login,
            password,
            host,
            port,
        }),
        SecretCommand::Get { name, field } => {
            crate::commands::secret::handle_get(&name, field.as_deref())
        }
        SecretCommand::Remove { name } => crate::commands::secret::handle_remove(&name),
        SecretCommand::List => crate::commands::secret::handle_list(),
    }
}

async fn handle_term_subcommand(command: TermCommand) -> Result<()> {
    match command {
        TermCommand::Init {
            shell,
            name,
            default,
        } => handle_term_init(shell, name, default).await,
        TermCommand::Log { command } => handle_term_log(command).await,
        TermCommand::Run { prompt } => handle_term_run(prompt).await,
        TermCommand::Info => handle_term_info().await,
    }
}

async fn handle_default_session() -> Result<()> {
    if !Config::global().exists() {
        return handle_configure().await;
    }

    let gosling_mode = Config::global().get_gosling_mode().unwrap_or_default();
    let session_id = get_or_create_session_id(None, false, false, gosling_mode).await?;

    let mut session = build_session(SessionBuilderConfig {
        session_id,
        resume: false,
        fork: false,
        no_session: false,
        extensions: Vec::new(),
        streamable_http_extensions: Vec::new(),
        builtins: Vec::new(),
        no_profile: false,
        additional_system_prompt: None,
        provider: None,
        model: None,
        debug: false,
        max_tool_repetitions: None,
        max_turns: None,
        interactive: true,
        quiet: false,
        output_format: "text".to_string(),
        container: None,
        stats: false,
    })
    .await;
    session.interactive(None).await
}

pub async fn cli() -> anyhow::Result<()> {
    register_builtin_extensions(gosling_mcp::BUILTIN_EXTENSIONS.clone());

    let cli = Cli::parse();
    warn_about_invalid_config_values();

    if let Err(e) = crate::project_tracker::update_project_tracker(None, None) {
        warn!("Warning: Failed to update project tracker: {}", e);
    }

    let command_name = get_command_name(&cli.command);
    tracing::info!(
        monotonic_counter.gosling.cli_commands = 1,
        command = command_name,
        "CLI command executed"
    );

    match cli.command {
        Some(Command::Completion { shell, bin_name }) => {
            // Generate into a buffer first: clap_complete panics if the writer
            // fails, which turns `gosling completion bash | head` (early-closed
            // pipe) into a panic instead of a silent broken-pipe exit.
            let mut cmd = Cli::command();
            let mut buffer = Vec::new();
            shell.generate(&mut cmd, &bin_name, &mut buffer);
            use std::io::Write;
            match std::io::stdout().write_all(&buffer) {
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                other => other.map_err(Into::into),
            }
        }
        Some(Command::Configure {}) => handle_configure().await,
        Some(Command::Doctor {}) => crate::commands::doctor::handle_doctor().await,
        Some(Command::Info { verbose, check }) => handle_info(verbose, check).await,
        Some(Command::Mcp { command, server }) => match (command, server) {
            (Some(command), _) => handle_mcp_subcommand(command).await,
            (None, Some(server)) => handle_mcp_command(server).await,
            (None, None) => {
                anyhow::bail!("specify a bundled server or subcommand; see `gosling mcp --help`")
            }
        },
        Some(Command::Acp { builtins }) => gosling::acp::server::run(builtins).await,
        Some(Command::ShellValidate {
            shell_id,
            shell_display_name,
            shell_version,
            shell_provisioning,
            builtins,
        }) => {
            handle_shell_validate_command(
                shell_id,
                shell_display_name,
                shell_version,
                shell_provisioning,
                builtins,
            )
            .await
        }
        Some(Command::Serve {
            host,
            port,
            tls,
            tls_cert_path,
            tls_key_path,
            platform,
            builtins,
            shell_id,
            shell_display_name,
            shell_version,
            shell_provisioning,
            shell_runtime_namespace,
            dangerously_unauthenticated,
            allowed_origins,
        }) => {
            handle_serve_command(ServeCommandArgs {
                host,
                port,
                tls,
                tls_cert_path,
                tls_key_path,
                platform,
                builtins,
                shell_id,
                shell_display_name,
                shell_version,
                shell_provisioning,
                shell_runtime_namespace,
                dangerously_unauthenticated,
                allowed_origins,
            })
            .await
        }
        Some(Command::Session {
            command: Some(cmd), ..
        }) => handle_session_subcommand(cmd).await,
        Some(Command::Session {
            command: None,
            identifier,
            resume,
            fork,
            edit,
            history,
            session_opts,
            extension_opts,
        }) => {
            handle_interactive_session(
                identifier,
                resume,
                fork,
                edit,
                history,
                session_opts,
                extension_opts,
            )
            .await
        }
        Some(Command::Project {}) => {
            handle_project_default()?;
            Ok(())
        }
        Some(Command::Projects) => {
            handle_projects_interactive()?;
            Ok(())
        }
        Some(Command::Run {
            input_opts,
            identifier,
            run_behavior,
            session_opts,
            extension_opts,
            output_opts,
            model_opts,
        }) => {
            handle_run_command(
                input_opts,
                identifier,
                run_behavior,
                session_opts,
                extension_opts,
                output_opts,
                model_opts,
            )
            .await
        }
        #[cfg(feature = "update")]
        Some(Command::Update {
            canary,
            reconfigure,
        }) => {
            crate::commands::update::update(canary, reconfigure).await?;
            Ok(())
        }
        Some(Command::Skills { command }) => handle_skills_subcommand(command).await,
        Some(Command::Plugin { command }) => handle_plugin_subcommand(command),
        Some(Command::Secret { command }) => handle_secret_subcommand(command),
        Some(Command::Term { command }) => handle_term_subcommand(command).await,
        #[cfg(feature = "tui")]
        Some(Command::Tui { args }) => crate::commands::tui::handle_tui(args),
        Some(Command::Review {
            range,
            prompt,
            model,
            provider,
            override_model,
            turn_limit,
            dry_run,
            quiet,
            no_orchestrate,
            instructions,
            files,
            check_filter,
            check_scope,
            checks_only,
            summary_only,
            severity,
        }) => {
            use crate::commands::review::{handle_review, ReviewOptions};
            handle_review(ReviewOptions {
                range,
                prompt_file: prompt,
                default_model: model,
                provider,
                override_model,
                default_turn_limit: turn_limit,
                dry_run,
                quiet,
                no_orchestrate,
                instructions,
                files,
                check_filter,
                check_scope,
                checks_only,
                summary_only,
                severity,
            })
            .await
        }
        Some(Command::ValidateExtensions { file }) => {
            use gosling::agents::validate_extensions::validate_bundled_extensions;
            match validate_bundled_extensions(&file) {
                Ok(msg) => {
                    println!("{msg}");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        None => handle_default_session().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_remove_accepts_non_interactive_confirmation() {
        let cli =
            Cli::try_parse_from(["gosling", "session", "remove", "--name", "fixture", "--yes"])
                .expect("parse failed");

        match cli.command {
            Some(Command::Session {
                command: Some(SessionCommand::Remove { yes: true, .. }),
                ..
            }) => {}
            _ => panic!("expected confirmed session removal"),
        }
    }

    #[test]
    fn run_input_preserves_valid_stdin() {
        let input_options = InputOptions {
            instructions: Some("-".to_string()),
            system: Some("system marker".to_string()),
            ..Default::default()
        };
        let mut stdin = std::io::Cursor::new(b"line one\nline two\n");

        let input = parse_run_input_from_reader(&input_options, &mut stdin)
            .expect("valid stdin should parse")
            .expect("stdin should produce an input config");

        assert_eq!(input.contents.as_deref(), Some("line one\nline two\n"));
        assert_eq!(
            input.additional_system_prompt.as_deref(),
            Some("system marker")
        );
    }

    #[test]
    fn run_input_rejects_invalid_utf8_without_panicking() {
        let input_options = InputOptions {
            instructions: Some("-".to_string()),
            ..Default::default()
        };
        let mut stdin = std::io::Cursor::new([0xff, 0xfe]);

        let result = parse_run_input_from_reader(&input_options, &mut stdin);
        let Err(error) = result else {
            panic!("invalid UTF-8 stdin should return an error");
        };

        let message = error.to_string();
        assert!(message.contains("Failed to read instructions from stdin"));
        assert!(message.contains("valid UTF-8"));
    }

    #[test]
    fn run_input_rejects_empty_and_whitespace_only_instructions() {
        for contents in ["", "  \n\t"] {
            let input_options = InputOptions {
                instructions: Some("-".to_string()),
                ..Default::default()
            };
            let mut stdin = std::io::Cursor::new(contents.as_bytes());

            let error = parse_run_input_from_reader(&input_options, &mut stdin).unwrap_err();

            assert_eq!(error.to_string(), "Instructions must not be empty");
        }
    }

    #[test]
    fn run_input_propagates_stdin_io_errors() {
        struct FailingReader;

        impl std::io::Read for FailingReader {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fixture read failure"))
            }
        }

        let input_options = InputOptions {
            instructions: Some("-".to_string()),
            ..Default::default()
        };

        let result = parse_run_input_from_reader(&input_options, &mut FailingReader);
        let Err(error) = result else {
            panic!("stdin I/O failures should return an error");
        };

        let message = error.to_string();
        assert!(message.contains("Failed to read instructions from stdin"));
        assert!(message.contains("fixture read failure"));
    }

    #[test]
    fn completion_command_accepts_nushell_alias() {
        let cli = Cli::try_parse_from(["gosling", "completion", "nushell"]).expect("parse failed");

        match cli.command {
            Some(Command::Completion {
                shell: CompletionShell::Nu,
                ..
            }) => {}
            _ => panic!("expected nu completion shell"),
        }
    }

    #[test]
    fn nushell_completion_generation_emits_module() {
        let mut cmd = Cli::command();
        let mut buffer = Vec::new();

        CompletionShell::Nu.generate(&mut cmd, "gosling", &mut buffer);

        let script = String::from_utf8(buffer).expect("utf8");
        assert!(script.contains("module completions"));
        assert!(script.contains("export extern gosling"));
        assert!(script.contains("export use completions *"));
    }

    #[test]
    fn term_init_help_mentions_nushell() {
        let mut cmd = Cli::command();
        let term = cmd.find_subcommand_mut("term").expect("term command");
        let init = term.find_subcommand_mut("init").expect("init command");
        let mut buffer = Vec::new();

        init.write_long_help(&mut buffer).expect("write help");

        let help = String::from_utf8(buffer).expect("utf8");
        assert!(help.contains("gosling term init nu"));
        assert!(help.contains("Supported for zsh, bash, and nu"));
    }

    #[test]
    fn completion_help_lists_nu() {
        let mut cmd = Cli::command();
        let completion = cmd
            .find_subcommand_mut("completion")
            .expect("completion command");
        let mut buffer = Vec::new();

        completion.write_long_help(&mut buffer).expect("write help");

        let help = String::from_utf8(buffer).expect("utf8");
        assert!(help.contains("nu"));
    }

    #[test]
    fn skills_command_accepts_list_subcommand() {
        let cli = Cli::try_parse_from(["gosling", "skills", "list"]).expect("parse failed");

        match cli.command {
            Some(Command::Skills {
                command: SkillsCommand::List,
            }) => {}
            _ => panic!("expected skills list command"),
        }
    }

    #[test]
    fn serve_command_accepts_dangerously_unauthenticated_flag() {
        let cli = Cli::try_parse_from([
            "gosling",
            "serve",
            "--dangerously-unauthenticated",
            "--allowed-origin",
            "app://localhost",
            "--allowed-origin",
            "https://app.example",
        ])
        .expect("parse failed");

        match cli.command {
            Some(Command::Serve {
                dangerously_unauthenticated,
                allowed_origins,
                ..
            }) => {
                assert!(dangerously_unauthenticated);
                assert_eq!(
                    allowed_origins,
                    vec!["app://localhost", "https://app.example"]
                );
            }
            _ => panic!("expected serve command"),
        }
    }

    #[test]
    fn shell_serve_options_default_to_permissive_identity() {
        let cli = Cli::try_parse_from([
            "gosling",
            "serve",
            "--shell-id",
            "math_mcp",
            "--shell-display-name",
            "Math",
            "--shell-runtime-namespace",
            "math_mcp",
        ])
        .expect("parse failed");

        let Some(Command::Serve {
            shell_id,
            shell_display_name,
            shell_runtime_namespace,
            shell_version,
            ..
        }) = cli.command
        else {
            panic!("expected serve command");
        };
        assert_eq!(shell_id.as_deref(), Some("math_mcp"));
        assert_eq!(shell_display_name.as_deref(), Some("Math"));
        assert_eq!(shell_runtime_namespace.as_deref(), Some("math_mcp"));
        assert_eq!(shell_version, "1");
    }

    #[test]
    fn shell_serve_does_not_enable_developer_by_default() {
        assert!(resolve_serve_builtins(Vec::new(), true).is_empty());
        assert_eq!(
            resolve_serve_builtins(Vec::new(), false),
            vec!["developer".to_string()]
        );
        assert_eq!(
            resolve_serve_builtins(vec!["developer".to_string()], true),
            vec!["developer".to_string()]
        );
    }

    #[test]
    fn shell_id_rejects_path_like_values() {
        assert!(validate_shell_id("../math").is_err());
        assert!(validate_shell_id("Math").is_err());
        assert!(validate_shell_id("math_mcp").is_ok());
    }

    #[test]
    fn review_command_accepts_options() {
        let cli = Cli::try_parse_from([
            "gosling",
            "review",
            "origin/main...HEAD",
            "--prompt",
            "REVIEW.md",
            "--model",
            "test-model",
            "--provider",
            "openai",
            "--override-model",
            "check-model",
            "--turn-limit",
            "4",
            "--dry-run",
            "--quiet",
            "--no-orchestrate",
            "--instructions",
            "focus on correctness",
            "--files",
            "src/lib.rs",
            "--check-filter",
            "security",
            "--check-scope",
            ".agents",
            "--checks-only",
            "--summary-only",
            "--severity",
            "low",
        ])
        .expect("parse failed");

        match cli.command {
            Some(Command::Review {
                range,
                prompt,
                model,
                provider,
                override_model,
                turn_limit,
                dry_run,
                quiet,
                no_orchestrate,
                instructions,
                files,
                check_filter,
                check_scope,
                checks_only,
                summary_only,
                severity,
            }) => {
                assert_eq!(range.as_deref(), Some("origin/main...HEAD"));
                assert_eq!(prompt.as_deref(), Some(std::path::Path::new("REVIEW.md")));
                assert_eq!(model.as_deref(), Some("test-model"));
                assert_eq!(provider.as_deref(), Some("openai"));
                assert_eq!(override_model.as_deref(), Some("check-model"));
                assert_eq!(turn_limit, Some(4));
                assert!(dry_run);
                assert!(quiet);
                assert!(no_orchestrate);
                assert_eq!(instructions.as_deref(), Some("focus on correctness"));
                assert_eq!(files, vec!["src/lib.rs"]);
                assert_eq!(check_filter, vec!["security"]);
                assert_eq!(
                    check_scope.as_deref(),
                    Some(std::path::Path::new(".agents"))
                );
                assert!(checks_only);
                assert!(summary_only);
                assert_eq!(severity, "low");
            }
            _ => panic!("expected review command"),
        }
    }

    #[cfg(feature = "tui")]
    #[test]
    fn tui_command_accepts_trailing_args() {
        let cli =
            Cli::try_parse_from(["gosling", "tui", "--", "--theme", "dark"]).expect("parse failed");

        match cli.command {
            Some(Command::Tui { args }) => assert_eq!(args, vec!["--theme", "dark"]),
            _ => panic!("expected tui command"),
        }
    }
}
