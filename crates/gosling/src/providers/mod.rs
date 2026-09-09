mod acp_tooling;
pub mod antigravity;
pub mod anthropic {
    pub use gosling_providers::anthropic::*;
}
pub mod anthropic_def;
pub mod api_client {
    pub use gosling_providers::api_client::*;
}
#[cfg(feature = "aws-providers")]
pub(crate) mod aws_env;
pub mod azure;
pub mod azureauth;
pub mod base;
#[cfg(feature = "aws-providers")]
pub mod bedrock;
pub mod canonical {
    pub use gosling_providers::canonical::*;
}
mod catalog_util;
pub mod catalog {
    pub use super::catalog_util::*;
}
pub mod chatgpt_codex;
pub mod claude_acp;
pub mod claude_code;
pub(crate) mod cli_common;
pub mod codex_acp;
pub mod copilot_acp;
pub mod cursor_agent;
pub mod custom_provider_config;
pub mod databricks;
pub mod databricks_auth;
pub mod databricks_v2;
pub mod formats;
mod gcpauth;
pub mod gcpvertexai;
pub mod gemini_cli;
pub mod githubcopilot;
pub mod google;
pub mod http_status {
    pub use gosling_providers::http_status::*;
}
mod init;
pub mod inventory;
pub mod kimicode;
pub mod litellm;
pub mod nanogpt;
pub mod oauth;
pub mod oauth_device_flow;
pub mod ollama {
    pub use gosling_providers::ollama::*;
}
pub mod ollama_def;
pub mod openai {
    pub use gosling_providers::openai::*;
}
pub mod openai_compatible {
    pub use gosling_providers::openai_compatible::*;
}
pub mod openrouter;
pub mod pi_acp;
pub mod provider_registry;
pub mod provider_secrets;
pub mod provider_test;
pub mod vibe_acp;
mod retry {
    pub use gosling_providers::retry::*;
}
pub mod openai_def;
#[cfg(feature = "aws-providers")]
pub mod sagemaker_tgi;
pub mod snowflake;
pub mod testprovider;
pub mod tetrate;
pub mod toolshim;
pub mod usage_estimator;
pub mod utils;

pub mod xai;
pub mod xai_oauth;

pub use init::{
    cleanup_provider, create, create_with_default_model, create_with_named_model,
    create_with_working_dir, get_from_registry, inventory_identity, providers,
    refresh_custom_providers,
};
pub use retry::{retry_operation, RetryConfig};

/// Whether `model_name` names an Anthropic Claude model, matched case-insensitively.
pub(crate) fn is_claude_model(model_name: &str) -> bool {
    model_name.to_lowercase().contains("claude")
}

/// A PKCE (RFC 7636) verifier and its S256 code challenge.
pub(crate) struct PkceChallenge {
    pub(crate) verifier: String,
    pub(crate) challenge: String,
}

/// Generate a PKCE verifier/challenge pair. `verifier_length` must be within
/// RFC 7636's 43-128 character range.
pub(crate) fn generate_pkce(verifier_length: usize) -> PkceChallenge {
    use base64::Engine;
    use sha2::Digest;

    let verifier = nanoid::nanoid!(verifier_length);
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    PkceChallenge {
        verifier,
        challenge,
    }
}

/// Generate a random OAuth `state` parameter.
pub(crate) fn generate_oauth_state() -> String {
    nanoid::nanoid!(32)
}

/// Build a [`RetryConfig`] from `{prefix}_MAX_RETRIES`, `{prefix}_INITIAL_RETRY_INTERVAL_MS`,
/// `{prefix}_BACKOFF_MULTIPLIER`, and `{prefix}_MAX_RETRY_INTERVAL_MS` config values, falling
/// back to `defaults` for any value that is unset or fails to parse.
pub fn load_retry_config_from_env(
    config: &crate::config::Config,
    prefix: &str,
    defaults: RetryConfig,
) -> RetryConfig {
    let max_retries = config
        .get_param(&format!("{prefix}_MAX_RETRIES"))
        .ok()
        .and_then(|v: String| v.parse::<usize>().ok())
        .unwrap_or(defaults.max_retries);

    let initial_interval_ms = config
        .get_param(&format!("{prefix}_INITIAL_RETRY_INTERVAL_MS"))
        .ok()
        .and_then(|v: String| v.parse::<u64>().ok())
        .unwrap_or(defaults.initial_interval_ms);

    let backoff_multiplier = config
        .get_param(&format!("{prefix}_BACKOFF_MULTIPLIER"))
        .ok()
        .and_then(|v: String| v.parse::<f64>().ok())
        .unwrap_or(defaults.backoff_multiplier);

    let max_interval_ms = config
        .get_param(&format!("{prefix}_MAX_RETRY_INTERVAL_MS"))
        .ok()
        .and_then(|v: String| v.parse::<u64>().ok())
        .unwrap_or(defaults.max_interval_ms);

    RetryConfig::new(
        max_retries,
        initial_interval_ms,
        backoff_multiplier,
        max_interval_ms,
    )
}
