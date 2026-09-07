// Owns extension environment merging, substitution, and static OAuth registration.
// Transport startup receives resolved values while callers keep existing helper paths.
// The extension_manager compatibility facade re-exports crate-visible helpers.

use super::*;

static RE_ENV_REFERENCE: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"\$(?:\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}|([A-Za-z_][A-Za-z0-9_]*))")
        .expect("valid regex")
});

/// An OS keychain item holding a secret that some other program owns.
///
/// Declared at the top level of `config.yaml`, keyed by the same name an
/// extension lists in `env_keys`:
///
/// ```yaml
/// secret_sources:
///   MUNINN_MCP_BEARER_TOKEN:
///     keychain_service: ai.muninn.mcp
///     keychain_account: alice      # optional, defaults to $USER
/// ```
///
/// This exists so a credential gosling did not mint stays in exactly one place.
/// The two alternatives are worse: copying the value into gosling's own secret
/// store turns every rotation into a two-step operation that silently half-fails,
/// and reading it from the process environment makes the value depend on where
/// gosling happens to sit in the login sequence, which is not something a config
/// file can express or a user can predict.
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub(crate) struct KeychainSecretSource {
    keychain_service: String,
    #[serde(default)]
    keychain_account: Option<String>,
}

/// Top-level `config.yaml` key mapping a secret name to where it really lives.
pub(crate) const SECRET_SOURCES_KEY: &str = "secret_sources";

impl KeychainSecretSource {
    // Only the keyring-backed reader needs to resolve an account.
    #[cfg(feature = "system-keyring")]
    fn account(&self) -> Option<String> {
        self.keychain_account
            .clone()
            .or_else(|| std::env::var("USER").ok())
            .filter(|account| !account.is_empty())
    }

    fn describe(&self) -> String {
        match &self.keychain_account {
            Some(account) => format!("'{}' (account '{}')", self.keychain_service, account),
            None => format!("'{}'", self.keychain_service),
        }
    }
}

/// Read one declared keychain item. Separate from the resolution policy above it
/// so tests can exercise the wiring without touching the real keychain.
#[cfg(feature = "system-keyring")]
fn read_keychain_secret(source: &KeychainSecretSource) -> Result<String, String> {
    let Some(account) = source.account() else {
        return Err("no keychain_account given and USER is unset".to_string());
    };
    let entry =
        keyring::Entry::new(&source.keychain_service, &account).map_err(|err| err.to_string())?;
    entry.get_password().map_err(|err| err.to_string())
}

#[cfg(not(feature = "system-keyring"))]
fn read_keychain_secret(_source: &KeychainSecretSource) -> Result<String, String> {
    Err("this build was compiled without the system-keyring feature".to_string())
}

fn declared_secret_source(config: &Config, key: &str) -> Option<KeychainSecretSource> {
    // An absent block is the ordinary case and means nothing is declared, so it
    // is not worth a warning. A block that exists but is not a mapping is a
    // mistake worth naming.
    let block: Value = config.get_param(SECRET_SOURCES_KEY).ok()?;
    let Some(entries) = block.as_object() else {
        warn!(
            key = %SECRET_SOURCES_KEY,
            "Config value is not a mapping of secret name to source; ignoring it."
        );
        return None;
    };

    let entry = entries.get(key)?;

    // Deserialize one entry at a time. Parsing the whole block at once would let
    // a typo in an unrelated entry silently disable every declared source --
    // the same quiet loss of a credential that `secret_sources` exists to
    // prevent, and equally invisible at the point of use.
    match serde_json::from_value::<KeychainSecretSource>(entry.clone()) {
        Ok(source) => Some(source),
        Err(err) => {
            warn!(
                key = %key,
                error = %err,
                "Malformed secret_sources entry; ignoring it."
            );
            None
        }
    }
}

/// Resolve `key` from a declared external keychain item.
///
/// `Ok(None)` means nothing was declared for this key and the caller should keep
/// its existing behaviour. `Err` means a source was declared and could not be
/// read, which is more actionable than the generic not-found error it replaces.
#[allow(clippy::result_large_err)]
fn resolve_declared_secret_with(
    reader: impl Fn(&KeychainSecretSource) -> Result<String, String>,
    config: &Config,
    key: &str,
    ext_name: &str,
) -> Result<Option<String>, ExtensionError> {
    let Some(source) = declared_secret_source(config, key) else {
        return Ok(None);
    };

    match reader(&source) {
        Ok(secret) if secret.is_empty() => Err(ExtensionError::ConfigError(format!(
            "Secret '{}' for extension '{}' resolved to an empty value from keychain item {}",
            key,
            ext_name,
            source.describe()
        ))),
        Ok(secret) => {
            tracing::debug!(
                key = %key,
                ext_name = %ext_name,
                "Resolved secret from declared keychain source."
            );
            Ok(Some(secret))
        }
        Err(err) => Err(ExtensionError::ConfigError(format!(
            "Failed to read secret '{}' for extension '{}' from keychain item {}: {}",
            key,
            ext_name,
            source.describe(),
            err
        ))),
    }
}

#[allow(clippy::result_large_err)]
fn resolve_declared_secret(
    config: &Config,
    key: &str,
    ext_name: &str,
) -> Result<Option<String>, ExtensionError> {
    resolve_declared_secret_with(read_keychain_secret, config, key, ext_name)
}

pub(crate) async fn merge_environments(
    envs: &Envs,
    env_keys: &[String],
    ext_name: &str,
    config: &Config,
) -> Result<HashMap<String, String>, ExtensionError> {
    let mut all_envs = envs.get_env();

    for key in env_keys {
        if all_envs.contains_key(key) {
            continue;
        }

        match config.get(key, true) {
            Ok(value) => {
                if value.is_null() {
                    if let Some(secret) = resolve_declared_secret(config, key, ext_name)? {
                        all_envs.insert(key.clone(), secret);
                        continue;
                    }
                    warn!(
                        key = %key,
                        ext_name = %ext_name,
                        "Secret key not found in config (returned null)."
                    );
                    continue;
                }

                if let Some(str_val) = value.as_str() {
                    all_envs.insert(key.clone(), str_val.to_string());
                } else {
                    warn!(
                        key = %key,
                        ext_name = %ext_name,
                        value_type = %value.get("type").and_then(|t| t.as_str()).unwrap_or("unknown"),
                        "Secret value is not a string; skipping."
                    );
                }
            }
            Err(e) => {
                // A key absent from gosling's own store is exactly the case a
                // declared external source exists to answer, so consult it
                // before reporting the credential missing.
                if let Some(secret) = resolve_declared_secret(config, key, ext_name)? {
                    all_envs.insert(key.clone(), secret);
                    continue;
                }
                error!(
                    key = %key,
                    ext_name = %ext_name,
                    error = %e,
                    "Failed to fetch secret from config."
                );
                return Err(ExtensionError::ConfigError(format!(
                    "Failed to fetch secret '{}' from config: {}",
                    key, e
                )));
            }
        }
    }

    Ok(Envs::new(all_envs).get_env())
}

/// Substitute environment variables in a string. Supports both ${VAR} and $VAR syntax.
pub(crate) fn substitute_env_vars(value: &str, env_map: &HashMap<String, String>) -> String {
    RE_ENV_REFERENCE
        .replace_all(value, |cap: &regex::Captures<'_>| {
            cap.get(1)
                .or_else(|| cap.get(2))
                .and_then(|name| env_map.get(name.as_str()))
                .cloned()
                .unwrap_or_else(|| cap[0].to_string())
        })
        .into_owned()
}

#[allow(clippy::result_large_err)]
pub(super) fn resolve_static_oauth_client(
    client_id: Option<&str>,
    client_secret_key: Option<&str>,
    scopes: &[String],
    envs: &HashMap<String, String>,
    config: &Config,
) -> ExtensionResult<Option<StaticOAuthClientConfig>> {
    let Some(client_id) = client_id else {
        if client_secret_key.is_some() || !scopes.is_empty() {
            return Err(ExtensionError::ConfigError(
                "client_secret_key and scopes require client_id for streamable_http OAuth"
                    .to_string(),
            ));
        }
        return Ok(None);
    };

    let client_id = substitute_env_vars(client_id, envs);
    if client_id.trim().is_empty() {
        return Err(ExtensionError::ConfigError(
            "client_id for streamable_http OAuth cannot be empty".to_string(),
        ));
    }

    let client_secret = match client_secret_key {
        Some(key) => match envs.get(key) {
            Some(value) => Some(value.clone()),
            None => config
                .get_secret::<String>(key)
                .ok()
                .filter(|value| !value.is_empty()),
        },
        None => None,
    };
    if client_secret_key.is_some() && client_secret.is_none() {
        return Err(ExtensionError::ConfigError(format!(
            "OAuth client secret '{}' was not found",
            client_secret_key.unwrap_or_default()
        )));
    }

    Ok(Some(StaticOAuthClientConfig {
        client_id,
        client_secret,
        scopes: scopes.to_vec(),
    }))
}

#[cfg(test)]
mod secret_source_tests {
    use super::*;
    use crate::config::Config;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn config_with(body: &str) -> (TempDir, Config) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, body).expect("write config");
        let config = Config::new(&path, "gosling-secret-source-test").expect("config");
        (dir, config)
    }

    const DECLARED: &str = "\
secret_sources:
  MUNINN_MCP_BEARER_TOKEN:
    keychain_service: ai.muninn.mcp
    keychain_account: alice
";

    #[test]
    fn parses_a_declared_source() {
        let (_dir, config) = config_with(DECLARED);
        let source =
            declared_secret_source(&config, "MUNINN_MCP_BEARER_TOKEN").expect("source declared");
        assert_eq!(
            source,
            KeychainSecretSource {
                keychain_service: "ai.muninn.mcp".to_string(),
                keychain_account: Some("alice".to_string()),
            }
        );
    }

    #[test]
    fn account_is_optional() {
        let (_dir, config) = config_with(
            "\
secret_sources:
  SOME_TOKEN:
    keychain_service: com.example.token
",
        );
        let source = declared_secret_source(&config, "SOME_TOKEN").expect("source declared");
        assert_eq!(source.keychain_account, None);
        assert_eq!(source.describe(), "'com.example.token'");
    }

    /// A typo in one entry must not take the others down with it. Parsing the
    /// whole block at once did exactly that, and silently.
    #[test]
    fn a_malformed_entry_does_not_disable_its_siblings() {
        let (_dir, config) = config_with(
            "\
secret_sources:
  BROKEN_TOKEN:
    keychain_servce: typo.in.the.field.name
  MUNINN_MCP_BEARER_TOKEN:
    keychain_service: ai.muninn.mcp
    keychain_account: alice
",
        );
        assert!(
            declared_secret_source(&config, "BROKEN_TOKEN").is_none(),
            "the malformed entry itself declares nothing"
        );
        let source = declared_secret_source(&config, "MUNINN_MCP_BEARER_TOKEN")
            .expect("a well-formed sibling still resolves");
        assert_eq!(source.keychain_service, "ai.muninn.mcp");
    }

    #[test]
    fn an_entry_of_the_wrong_shape_declares_nothing() {
        let (_dir, config) = config_with(
            "\
secret_sources:
  MUNINN_MCP_BEARER_TOKEN: ai.muninn.mcp
",
        );
        assert!(declared_secret_source(&config, "MUNINN_MCP_BEARER_TOKEN").is_none());
    }

    #[test]
    fn undeclared_keys_have_no_source() {
        let (_dir, config) = config_with(DECLARED);
        assert!(declared_secret_source(&config, "SOMETHING_ELSE").is_none());
    }

    /// A `secret_sources` block written wrong must not take down extensions that
    /// never asked for one, so it degrades to "nothing declared".
    #[test]
    fn a_malformed_block_declares_nothing() {
        let (_dir, config) = config_with("secret_sources: \"not-a-mapping\"\n");
        assert!(declared_secret_source(&config, "MUNINN_MCP_BEARER_TOKEN").is_none());
    }

    #[test]
    fn resolves_through_the_reader() {
        let (_dir, config) = config_with(DECLARED);
        let resolved = resolve_declared_secret_with(
            |source| {
                assert_eq!(source.keychain_service, "ai.muninn.mcp");
                Ok("a-real-token".to_string())
            },
            &config,
            "MUNINN_MCP_BEARER_TOKEN",
            "muninn",
        )
        .expect("resolution succeeds");
        assert_eq!(resolved, Some("a-real-token".to_string()));
    }

    #[test]
    fn an_undeclared_key_resolves_to_none_without_reading() {
        let (_dir, config) = config_with(DECLARED);
        let resolved = resolve_declared_secret_with(
            |_| panic!("reader must not run for an undeclared key"),
            &config,
            "SOMETHING_ELSE",
            "muninn",
        )
        .expect("resolution succeeds");
        assert!(resolved.is_none());
    }

    /// A declared-but-unreadable item is a different failure from "no credential
    /// configured", and the message has to say which item it could not read.
    #[test]
    fn a_failed_read_names_the_keychain_item() {
        let (_dir, config) = config_with(DECLARED);
        let err = resolve_declared_secret_with(
            |_| Err("No matching entry found in secure storage".to_string()),
            &config,
            "MUNINN_MCP_BEARER_TOKEN",
            "muninn",
        )
        .expect_err("resolution fails");
        let ExtensionError::ConfigError(message) = err else {
            panic!("expected a ConfigError");
        };
        assert!(message.contains("ai.muninn.mcp"), "message: {message}");
        assert!(message.contains("account 'alice'"), "message: {message}");
        assert!(
            message.contains("MUNINN_MCP_BEARER_TOKEN"),
            "message: {message}"
        );
    }

    #[test]
    fn an_empty_secret_is_rejected() {
        let (_dir, config) = config_with(DECLARED);
        let err = resolve_declared_secret_with(
            |_| Ok(String::new()),
            &config,
            "MUNINN_MCP_BEARER_TOKEN",
            "muninn",
        )
        .expect_err("an empty credential is not a credential");
        let ExtensionError::ConfigError(message) = err else {
            panic!("expected a ConfigError");
        };
        assert!(message.contains("empty value"), "message: {message}");
    }

    /// Regression guard: with nothing declared, the original not-found error is
    /// preserved verbatim rather than replaced by a keychain message.
    ///
    /// `merge_environments` falls back to the process environment, so the key
    /// has to be unset for the duration: an operator who exports
    /// `MUNINN_MCP_BEARER_TOKEN` (running the Muninn MCP server does exactly
    /// that) would otherwise have the lookup succeed and fail this test for a
    /// reason unrelated to the code under test.
    #[tokio::test]
    async fn merge_environments_keeps_the_original_error_when_nothing_is_declared() {
        let _guard = env_lock::lock_env([("MUNINN_MCP_BEARER_TOKEN", None::<&str>)]);
        let (_dir, config) = config_with("secret_sources: {}\n");
        let err = merge_environments(
            &Envs::new(HashMap::new()),
            &["MUNINN_MCP_BEARER_TOKEN".to_string()],
            "muninn",
            &config,
        )
        .await
        .expect_err("no credential anywhere");
        let ExtensionError::ConfigError(message) = err else {
            panic!("expected a ConfigError");
        };
        assert!(
            message.starts_with("Failed to fetch secret 'MUNINN_MCP_BEARER_TOKEN' from config:"),
            "message: {message}"
        );
    }

    #[tokio::test]
    async fn an_environment_value_still_wins_over_a_declared_source() {
        let (_dir, config) = config_with(DECLARED);
        let mut preset = HashMap::new();
        preset.insert(
            "MUNINN_MCP_BEARER_TOKEN".to_string(),
            "from-the-environment".to_string(),
        );
        let merged = merge_environments(
            &Envs::new(preset),
            &["MUNINN_MCP_BEARER_TOKEN".to_string()],
            "muninn",
            &config,
        )
        .await
        .expect("already present, no lookup needed");
        assert_eq!(
            merged.get("MUNINN_MCP_BEARER_TOKEN").map(String::as_str),
            Some("from-the-environment")
        );
    }
}
