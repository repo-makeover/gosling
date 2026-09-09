use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::base::{
    ConfigKey, MessageStream, ModelInfo, Provider, ProviderDef, ProviderMetadata,
    DEFAULT_PROVIDER_TIMEOUT_SECS,
};
use super::openai_compatible::OpenAiCompatibleProvider;
use super::xai::{SUPERGROK_API_HOST, SUPERGROK_DEFAULT_MODEL, SUPERGROK_MODELS};
use crate::config::paths::Paths;
use crate::conversation::message::Message;
use crate::providers::PkceChallenge;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use axum::{extract::Query, response::Html, routing::get, Router};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use gosling_providers::errors::ProviderError;
use gosling_providers::model::ModelConfig;
use gosling_providers::thinking::ThinkingEffort;
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use tokio::sync::{oneshot, Mutex as TokioMutex};

// Public Grok-CLI OAuth client. xAI's auth server rejects loopback OAuth from
// non-allowlisted clients, so we reuse the Grok-CLI client_id that xAI ships
// for desktop OAuth flows.
const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";

const AUTHORIZE_URL: &str = "https://auth.x.ai/oauth2/authorize";
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
// RFC 8628 device authorization endpoint, surfaced via xAI's
// /.well-known/openid-configuration as `device_authorization_endpoint`.
const DEVICE_AUTHORIZATION_URL: &str = "https://auth.x.ai/oauth2/device/code";
const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

const OAUTH_SCOPES: &[&str] = &[
    "openid",
    "profile",
    "email",
    "offline_access",
    "grok-cli:access",
    "api:access",
];

// xAI rejects redirect_uris that don't match what was registered for the
// Grok-CLI client. The host:port pair is part of the registration, so we have
// to bind the loopback server to this exact port.
const OAUTH_HOST: [u8; 4] = [127, 0, 0, 1];
const OAUTH_PORT: u16 = 56121;
const OAUTH_REDIRECT_PATH: &str = "/callback";

const OAUTH_TIMEOUT_SECS: u64 = 300;
const HTML_AUTO_CLOSE_TIMEOUT_MS: u64 = 2000;

/// Builds the HTTP client used for the token-exchange/refresh/device-code
/// requests below. Without a timeout, a stalled xAI auth endpoint hangs the
/// calling turn indefinitely (unlike the provider's own API client, which
/// already sets `DEFAULT_PROVIDER_TIMEOUT_SECS`).
fn oauth_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            DEFAULT_PROVIDER_TIMEOUT_SECS,
        ))
        .build()
        .map_err(|e| anyhow!("Failed to build xAI OAuth HTTP client: {}", e))
}

// Refresh skew: refresh tokens this long before stored expiry so a single
// long-running tool call doesn't have to recover from a mid-flight 401.
const ACCESS_TOKEN_REFRESH_SKEW_SECS: i64 = 120;

// Device-code polling bounds.
const DEVICE_CODE_DEFAULT_INTERVAL_SECS: u64 = 5;
const DEVICE_CODE_MIN_INTERVAL_SECS: u64 = 1;
const DEVICE_CODE_SLOW_DOWN_INCREMENT_SECS: u64 = 5;
const DEVICE_CODE_DEFAULT_EXPIRES_SECS: u64 = 5 * 60;

const XAI_OAUTH_PROVIDER_NAME: &str = "xai_oauth";
const XAI_OAUTH_DOC_URL: &str = "https://x.ai/grok";

// The chat proxy rejects requests without a recent client version, replying
// HTTP 426 "Your Grok CLI version (none) is outdated". These headers clear that
// gate; the identifier matches the value the Grok CLI itself sends.
const GROK_CLIENT_VERSION_HEADER: &str = "x-grok-client-version";
const GROK_CLIENT_IDENTIFIER_HEADER: &str = "x-grok-client-identifier";
const GROK_CLIENT_IDENTIFIER: &str = "grok-shell";
// Fallback when the installed Grok CLI's version.json is missing or older than
// the proxy's minimum. Must be >= GROK_CLIENT_VERSION_MIN.
const GROK_CLIENT_VERSION_FALLBACK: &str = "0.2.93";
// Proxy's minimum accepted client version; anything below still gets a 426, so
// a stale installed version is treated as unusable.
const GROK_CLIENT_VERSION_MIN: (u32, u32, u32) = (0, 1, 202);

// Parses a dotted `major.minor.patch` version into a comparable tuple, ignoring
// any pre-release/build suffix. Returns None if the numeric core is unparseable.
fn parse_version(version: &str) -> Option<(u32, u32, u32)> {
    let core = version.trim().split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

// Reads the installed Grok CLI's version so we advertise whatever the user has
// on disk, falling back to a known-good pin when it's missing, unparseable, or
// older than the proxy minimum (a stale value would just earn another 426).
fn grok_client_version() -> String {
    #[derive(Deserialize)]
    struct VersionFile {
        version: String,
    }
    dirs::home_dir()
        .map(|home| home.join(".grok/version.json"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|contents| serde_json::from_str::<VersionFile>(&contents).ok())
        .map(|v| v.version)
        .filter(|v| parse_version(v).is_some_and(|parsed| parsed >= GROK_CLIENT_VERSION_MIN))
        .unwrap_or_else(|| GROK_CLIENT_VERSION_FALLBACK.to_string())
}

// SuperGrok's grok-4.6 accepts a reasoning-effort suffix (e.g. `grok-4.6-high`),
// mirroring the Grok CLI's `/model <model> [effort]`. Returns the bare model id
// plus the effort to send in `reasoning_effort`, if any.
fn split_effort_suffix(model_name: &str) -> (String, Option<String>) {
    for effort in ["high", "medium", "low"] {
        if let Some(base) = model_name.strip_suffix(&format!("-{effort}")) {
            return (base.to_string(), Some(effort.to_string()));
        }
    }
    (model_name.to_string(), None)
}

// Maps a standard thinking-effort setting to the proxy's `reasoning_effort`.
// The proxy only accepts low/medium/high, so Off carries no effort and Max
// clamps to high.
fn proxy_reasoning_effort(effort: ThinkingEffort) -> Option<&'static str> {
    match effort {
        ThinkingEffort::Off => None,
        ThinkingEffort::Low => Some("low"),
        ThinkingEffort::Medium => Some("medium"),
        ThinkingEffort::High | ThinkingEffort::Max | ThinkingEffort::Ultra => Some("high"),
    }
}

// Rewrites the model name to its bare id and sets `reasoning_effort` from either
// an explicit `-high|medium|low` suffix or the standard `thinking_effort` set by
// the UI/ACP/GOSLING_THINKING_EFFORT. Without this the OpenAI formatter drops
// `thinking_effort` for this non-OpenAI model and the proxy never sees it.
fn apply_reasoning_effort(model_config: &ModelConfig) -> ModelConfig {
    let (base_model, suffix_effort) = split_effort_suffix(&model_config.model_name);
    let effort = suffix_effort.or_else(|| {
        model_config
            .thinking_effort()
            .and_then(proxy_reasoning_effort)
            .map(str::to_string)
    });
    let mut config = model_config.clone();
    config.model_name = base_model;
    if let Some(effort) = effort {
        config
            .request_params
            .get_or_insert_with(Default::default)
            .insert("reasoning_effort".to_string(), serde_json::json!(effort));
    }
    config
}

fn prepare_supergrok_model_config(
    model_config: &ModelConfig,
) -> Result<ModelConfig, ProviderError> {
    let config = apply_reasoning_effort(model_config);
    if config.model_name != SUPERGROK_DEFAULT_MODEL {
        return Err(ProviderError::RequestFailed(format!(
            "SuperGrok supports only {SUPERGROK_DEFAULT_MODEL} in Gosling"
        )));
    }
    Ok(config)
}

#[derive(Debug)]
struct XaiAuthState {
    oauth_mutex: TokioMutex<()>,
    refresh_mutex: TokioMutex<()>,
}

impl XaiAuthState {
    pub(crate) fn new() -> Self {
        Self {
            oauth_mutex: TokioMutex::new(()),
            refresh_mutex: TokioMutex::new(()),
        }
    }

    fn instance() -> Arc<Self> {
        Arc::clone(&XAI_AUTH_STATE)
    }
}

static XAI_AUTH_STATE: LazyLock<Arc<XaiAuthState>> =
    LazyLock::new(|| Arc::new(XaiAuthState::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenData {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    id_token: Option<String>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct TokenCache {
    cache_path: PathBuf,
}

fn get_cache_path() -> PathBuf {
    Paths::in_config_dir("xai_oauth/tokens.json")
}

impl TokenCache {
    pub(crate) fn new() -> Self {
        let cache_path = get_cache_path();
        if let Some(parent) = cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self { cache_path }
    }

    fn load(&self) -> Option<TokenData> {
        let contents = std::fs::read_to_string(&self.cache_path).ok()?;
        serde_json::from_str(&contents).ok()
    }
    pub(crate) fn has_token(&self) -> bool {
        self.load().is_some()
    }

    fn save(&self, token_data: &TokenData) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string(token_data)?;
        crate::config::base::write_secrets_file(&self.cache_path, &contents)?;
        Ok(())
    }

    fn clear(&self) {
        let _ = std::fs::remove_file(&self.cache_path);
    }
}

fn redirect_uri() -> String {
    format!(
        "http://{}.{}.{}.{}:{}{}",
        OAUTH_HOST[0], OAUTH_HOST[1], OAUTH_HOST[2], OAUTH_HOST[3], OAUTH_PORT, OAUTH_REDIRECT_PATH
    )
}

fn build_authorize_url(pkce: &PkceChallenge, state: &str, nonce: &str) -> Result<String> {
    let scopes = OAUTH_SCOPES.join(" ");
    let redirect = redirect_uri();
    // `plan=generic` opts the consent screen into xAI's generic OAuth plan
    // tier; without it, accounts.x.ai rejects loopback OAuth from
    // non-allowlisted clients. `referrer=gosling` lets xAI attribute
    // gosling-originated logins.
    let params = [
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect.as_str()),
        ("scope", scopes.as_str()),
        ("code_challenge", pkce.challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("nonce", nonce),
        ("plan", "generic"),
        ("referrer", "gosling"),
    ];
    let query = serde_urlencoded::to_string(params)?;
    Ok(format!("{}?{}", AUTHORIZE_URL, query))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

async fn exchange_code_for_tokens(code: &str, pkce: &PkceChallenge) -> Result<TokenResponse> {
    let client = oauth_http_client()?;
    let redirect = redirect_uri();
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect.as_str()),
        ("client_id", CLIENT_ID),
        ("code_verifier", pkce.verifier.as_str()),
    ];

    let resp = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("xAI token exchange failed ({}): {}", status, text));
    }

    Ok(resp.json().await?)
}

async fn refresh_access_token(refresh_token: &str) -> Result<TokenResponse> {
    let client = oauth_http_client()?;
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ];

    let resp = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("xAI token refresh failed ({}): {}", status, text));
    }

    Ok(resp.json().await?)
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct DeviceTokenErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

async fn request_device_code() -> Result<DeviceCodeResponse> {
    let client = oauth_http_client()?;
    let scopes = OAUTH_SCOPES.join(" ");
    let params = [("client_id", CLIENT_ID), ("scope", scopes.as_str())];
    let resp = client
        .post(DEVICE_AUTHORIZATION_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "xAI device code request failed ({}): {}",
            status,
            text
        ));
    }
    Ok(resp.json().await?)
}

async fn poll_device_code_token(device: &DeviceCodeResponse) -> Result<TokenResponse> {
    let expires_secs = device
        .expires_in
        .filter(|v| *v > 0)
        .unwrap_or(DEVICE_CODE_DEFAULT_EXPIRES_SECS);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(expires_secs);
    let mut interval_secs = device
        .interval
        .filter(|v| *v > 0)
        .unwrap_or(DEVICE_CODE_DEFAULT_INTERVAL_SECS)
        .max(DEVICE_CODE_MIN_INTERVAL_SECS);

    let client = oauth_http_client()?;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!("xAI device authorization timed out"));
        }
        let params = [
            ("grant_type", DEVICE_CODE_GRANT_TYPE),
            ("client_id", CLIENT_ID),
            ("device_code", device.device_code.as_str()),
        ];
        let resp = client
            .post(TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(resp.json().await?);
        }
        let status = resp.status();
        let body: DeviceTokenErrorBody = resp.json().await.unwrap_or_default();
        match body.error.as_deref() {
            Some("authorization_pending") => {
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
            Some("slow_down") => {
                interval_secs += DEVICE_CODE_SLOW_DOWN_INCREMENT_SECS;
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
            Some("access_denied") | Some("authorization_denied") => {
                return Err(anyhow!("xAI device authorization was denied"));
            }
            Some("expired_token") => {
                return Err(anyhow!(
                    "xAI device code expired - please re-run gosling configure"
                ));
            }
            other => {
                let detail = body
                    .error_description
                    .or_else(|| other.map(String::from))
                    .unwrap_or_default();
                return Err(anyhow!(
                    "xAI device token exchange failed ({}): {}",
                    status,
                    detail
                ));
            }
        }
    }
}

const HTML_SUCCESS_TEMPLATE: &str = r#"<!doctype html>
<html>
  <head>
    <title>gosling - xAI Authorization Successful</title>
    <style>
      body {
        font-family: system-ui, -apple-system, sans-serif;
        display: flex;
        justify-content: center;
        align-items: center;
        height: 100vh;
        margin: 0;
        background: #131010;
        color: #f1ecec;
      }
      .container { text-align: center; padding: 2rem; }
      h1 { color: #f1ecec; margin-bottom: 1rem; }
      p { color: #b7b1b1; }
    </style>
  </head>
  <body>
    <div class="container">
      <h1>Authorization Successful</h1>
      <p>You can close this window and return to gosling.</p>
    </div>
    <script>const AUTO_CLOSE_TIMEOUT_MS = __AUTO_CLOSE_TIMEOUT_MS__; setTimeout(() => window.close(), AUTO_CLOSE_TIMEOUT_MS)</script>
  </body>
</html>"#;

fn html_success() -> String {
    HTML_SUCCESS_TEMPLATE.replace(
        "__AUTO_CLOSE_TIMEOUT_MS__",
        &HTML_AUTO_CLOSE_TIMEOUT_MS.to_string(),
    )
}

fn html_error(error: &str) -> String {
    let safe_error = v_htmlescape::escape_fmt(error);
    format!(
        r#"<!doctype html>
<html>
  <head>
    <title>gosling - xAI Authorization Failed</title>
    <style>
      body {{
        font-family: system-ui, -apple-system, sans-serif;
        display: flex; justify-content: center; align-items: center;
        height: 100vh; margin: 0; background: #131010; color: #f1ecec;
      }}
      .container {{ text-align: center; padding: 2rem; }}
      h1 {{ color: #fc533a; margin-bottom: 1rem; }}
      p {{ color: #b7b1b1; }}
      .error {{
        color: #ff917b; font-family: monospace; margin-top: 1rem;
        padding: 1rem; background: #3c140d; border-radius: 0.5rem;
      }}
    </style>
  </head>
  <body>
    <div class="container">
      <h1>Authorization Failed</h1>
      <p>An error occurred during authorization.</p>
      <div class="error">{}</div>
    </div>
  </body>
</html>"#,
        safe_error
    )
}

#[derive(Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

fn oauth_callback_router(
    expected_state: String,
    tx: Arc<TokioMutex<Option<oneshot::Sender<Result<String>>>>>,
) -> Router {
    Router::new().route(
        OAUTH_REDIRECT_PATH,
        get(move |Query(params): Query<CallbackParams>| {
            let tx = tx.clone();
            let expected = expected_state.clone();
            async move {
                if let Some(error) = params.error {
                    let msg = params.error_description.unwrap_or(error);
                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(Err(anyhow!("{}", msg)));
                    }
                    return Html(html_error(&msg));
                }

                let code = match params.code {
                    Some(c) => c,
                    None => {
                        let msg = "Missing authorization code";
                        if let Some(sender) = tx.lock().await.take() {
                            let _ = sender.send(Err(anyhow!("{}", msg)));
                        }
                        return Html(html_error(msg));
                    }
                };

                if params.state.as_deref() != Some(&expected) {
                    let msg = "Invalid state - potential CSRF attack";
                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(Err(anyhow!("{}", msg)));
                    }
                    return Html(html_error(msg));
                }

                if let Some(sender) = tx.lock().await.take() {
                    let _ = sender.send(Ok(code));
                }
                Html(html_success())
            }
        }),
    )
}

async fn spawn_oauth_server(app: Router) -> Result<tokio::task::JoinHandle<()>> {
    let addr = SocketAddr::from((OAUTH_HOST, OAUTH_PORT));
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        if e.kind() == io::ErrorKind::AddrInUse {
            anyhow!(
                "xAI OAuth callback server failed to bind to {}: port {} is already in use \
                 (likely another OAuth client like Grok-CLI is bound to the same port). \
                 Stop the process using this port and try again, or use the device-code flow.",
                addr,
                OAUTH_PORT
            )
        } else {
            anyhow!(
                "xAI OAuth callback server failed to bind to {}: {}",
                addr,
                e
            )
        }
    })?;
    Ok(tokio::spawn(async move {
        let server = axum::serve(listener, app);
        let _ = server.await;
    }))
}

struct ServerHandleGuard(Option<tokio::task::JoinHandle<()>>);

impl ServerHandleGuard {
    fn new(handle: tokio::task::JoinHandle<()>) -> Self {
        Self(Some(handle))
    }

    fn abort(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

impl Drop for ServerHandleGuard {
    fn drop(&mut self) {
        self.abort();
    }
}

async fn wait_for_oauth_code(rx: oneshot::Receiver<Result<String>>) -> Result<String> {
    let code_result =
        tokio::time::timeout(std::time::Duration::from_secs(OAUTH_TIMEOUT_SECS), rx).await;
    code_result
        .map_err(|_| anyhow!("xAI OAuth flow timed out"))??
        .map_err(|e| anyhow!("xAI OAuth callback error: {}", e))
}

async fn perform_loopback_oauth_flow(auth_state: &XaiAuthState) -> Result<TokenData> {
    let _guard = auth_state.oauth_mutex.try_lock().map_err(|_| {
        anyhow!("Another xAI OAuth flow is already in progress; please try again later")
    })?;

    let pkce = crate::providers::generate_pkce(64);
    let csrf_state = crate::providers::generate_oauth_state();
    let nonce = crate::providers::generate_oauth_state();
    let auth_url = build_authorize_url(&pkce, &csrf_state, &nonce)?;

    let (tx, rx) = oneshot::channel::<Result<String>>();
    let tx = Arc::new(TokioMutex::new(Some(tx)));
    let app = oauth_callback_router(csrf_state.clone(), tx);
    let server_handle = spawn_oauth_server(app).await?;
    let mut server_guard = ServerHandleGuard::new(server_handle);

    if webbrowser::open(&auth_url).is_err() {
        tracing::info!(
            "Please open this URL in your browser to authorize gosling with xAI:\n{}",
            auth_url
        );
    }

    let code_result = wait_for_oauth_code(rx).await;
    server_guard.abort();
    let code = code_result?;

    let tokens = exchange_code_for_tokens(&code, &pkce).await?;
    Ok(token_data_from_response(tokens))
}

async fn perform_device_code_flow() -> Result<TokenData> {
    let device = request_device_code().await?;
    let url = device
        .verification_uri_complete
        .clone()
        .unwrap_or_else(|| device.verification_uri.clone());
    tracing::info!(
        "xAI device authorization: open {} and enter code {}",
        device.verification_uri,
        device.user_code
    );
    eprintln!(
        "\nTo authorize gosling with xAI, open this URL in any browser:\n  {}\nand enter code: {}\n",
        url, device.user_code
    );
    let tokens = poll_device_code_token(&device).await?;
    Ok(token_data_from_response(tokens))
}

fn token_data_from_response(tokens: TokenResponse) -> TokenData {
    let expires_at = Utc::now() + chrono::Duration::seconds(tokens.expires_in.unwrap_or(3600));
    TokenData {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        id_token: tokens.id_token,
        expires_at,
    }
}

#[derive(Debug)]
struct XaiOAuthAuthProvider {
    cache: TokenCache,
    state: Arc<XaiAuthState>,
}

impl XaiOAuthAuthProvider {
    fn new(state: Arc<XaiAuthState>) -> Self {
        Self {
            cache: TokenCache::new(),
            state,
        }
    }

    async fn get_valid_token(&self) -> Result<TokenData> {
        if let Some(mut token_data) = self.cache.load() {
            if token_data.expires_at
                > Utc::now() + chrono::Duration::seconds(ACCESS_TOKEN_REFRESH_SKEW_SECS)
            {
                return Ok(token_data);
            }

            // Single-flight refresh: collapse concurrent fetches onto one
            // HTTP call so we don't replay a rotating refresh_token.
            let _refresh_guard = self.state.refresh_mutex.lock().await;
            if let Some(reloaded) = self.cache.load() {
                if reloaded.expires_at
                    > Utc::now() + chrono::Duration::seconds(ACCESS_TOKEN_REFRESH_SKEW_SECS)
                {
                    return Ok(reloaded);
                }
                token_data = reloaded;
            }

            tracing::debug!("xAI access token expiring, attempting refresh");
            match refresh_access_token(&token_data.refresh_token).await {
                Ok(new_tokens) => {
                    token_data.access_token = new_tokens.access_token;
                    if !new_tokens.refresh_token.is_empty() {
                        token_data.refresh_token = new_tokens.refresh_token;
                    }
                    if new_tokens.id_token.is_some() {
                        token_data.id_token = new_tokens.id_token;
                    }
                    token_data.expires_at = Utc::now()
                        + chrono::Duration::seconds(new_tokens.expires_in.unwrap_or(3600));
                    self.cache.save(&token_data)?;
                    tracing::info!("xAI access token refreshed");
                    return Ok(token_data);
                }
                Err(e) => {
                    tracing::warn!("xAI token refresh failed, will re-authenticate: {}", e);
                    self.cache.clear();
                }
            }
        }

        tracing::info!("Starting xAI OAuth flow (SuperGrok subscription)");
        let token_data = match perform_loopback_oauth_flow(self.state.as_ref()).await {
            Ok(td) => td,
            Err(e) => {
                tracing::warn!(
                    "xAI loopback OAuth failed ({}); falling back to device-code flow",
                    e
                );
                perform_device_code_flow().await?
            }
        };
        self.cache.save(&token_data)?;
        Ok(token_data)
    }
}

#[async_trait]
impl AuthProvider for XaiOAuthAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let token_data = self.get_valid_token().await?;
        Ok((
            "Authorization".to_string(),
            format!("Bearer {}", token_data.access_token),
        ))
    }
}

/// Delegating Provider that forwards chat/stream/etc. to an inner
/// `OpenAiCompatibleProvider` pointed at `https://api.x.ai/v1`, but overrides
/// `configure_oauth` so the desktop "Sign in" button (and any other caller of
/// `Provider::configure_oauth`) drives the loopback / device-code flow.
#[derive(serde::Serialize)]
pub struct XaiOAuthProvider {
    #[serde(skip)]
    inner: OpenAiCompatibleProvider,
    #[serde(skip)]
    auth_provider: Arc<XaiOAuthAuthProvider>,
}

impl XaiOAuthProvider {
    pub async fn cleanup() -> Result<()> {
        TokenCache::new().clear();
        Ok(())
    }
}

#[async_trait]
impl Provider for XaiOAuthProvider {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let model_config = prepare_supergrok_model_config(model_config)?;
        self.inner
            .stream(&model_config, system, messages, tools)
            .await
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![SUPERGROK_DEFAULT_MODEL.to_string()])
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        // Preserve the previous token so a partially-completed sign-in
        // attempt (e.g. user closes the browser) doesn't sign them out.
        let previous_token = self.auth_provider.cache.load();
        self.auth_provider.cache.clear();

        let flow_result = match perform_loopback_oauth_flow(self.auth_provider.state.as_ref()).await
        {
            Ok(td) => Ok(td),
            Err(e) => {
                tracing::warn!(
                    "xAI loopback OAuth failed ({}); falling back to device-code flow",
                    e
                );
                perform_device_code_flow().await
            }
        };

        let save_result =
            flow_result.and_then(|token_data| self.auth_provider.cache.save(&token_data));

        if let Err(e) = save_result {
            if let Some(previous_token) = previous_token.as_ref() {
                if self.auth_provider.cache.load().is_none() {
                    let _ = self.auth_provider.cache.save(previous_token);
                }
            }
            return Err(ProviderError::Authentication(format!(
                "xAI OAuth flow failed: {}",
                e
            )));
        }
        Ok(())
    }
}

impl gosling_providers::base::ProviderDescriptor for XaiOAuthProvider {
    fn metadata() -> ProviderMetadata {
        // with_models (not new): these models have no canonical registry entry,
        // so ModelInfo::reasoning must be set explicitly or grok-4.6's effort
        // selector is hidden in the desktop switcher.
        let models = SUPERGROK_MODELS
            .iter()
            .map(|m| ModelInfo {
                reasoning: m.reasoning,
                ..ModelInfo::new(m.name, m.context_limit)
            })
            .collect();
        ProviderMetadata::with_models(
            XAI_OAUTH_PROVIDER_NAME,
            "xAI (SuperGrok Subscription)",
            "Use your xAI SuperGrok subscription via OAuth instead of an API key. Falls back to a device-code flow on headless / remote machines.",
            SUPERGROK_DEFAULT_MODEL,
            models,
            XAI_OAUTH_DOC_URL,
            vec![
                ConfigKey::new_oauth("XAI_OAUTH_TOKEN", true, true, None, false),
                // Deliberately not XAI_HOST: that key is shared with the API-key
                // `xai` provider, whose value (api.x.ai) would misroute OAuth
                // requests to the wrong host.
                ConfigKey::new(
                    "XAI_OAUTH_HOST",
                    false,
                    false,
                    Some(SUPERGROK_API_HOST),
                    false,
                ),
            ],
        )
    }
}

impl ProviderDef for XaiOAuthProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let host: String = config
                .get_param("XAI_OAUTH_HOST")
                .unwrap_or_else(|_| SUPERGROK_API_HOST.to_string());

            let auth_provider = Arc::new(XaiOAuthAuthProvider::new(XaiAuthState::instance()));
            let auth_for_client = Arc::clone(&auth_provider);
            let api_client = ApiClient::new_with_tls(
                host,
                AuthMethod::Custom(Box::new(SharedAuthProvider(auth_for_client))),
                tls_config,
            )?
            .with_header(GROK_CLIENT_VERSION_HEADER, &grok_client_version())?
            .with_header(GROK_CLIENT_IDENTIFIER_HEADER, GROK_CLIENT_IDENTIFIER)?
            .with_request_builder(crate::session_context::session_id_request_builder());

            // Grok's reasoning deltas rarely carry a `thought_signature`, so
            // dedupe_signed_thinking never collapses repeats and the model
            // ends up re-reading its own past reasoning verbatim every turn.
            let inner = OpenAiCompatibleProvider::new(
                XAI_OAUTH_PROVIDER_NAME.to_string(),
                api_client,
                String::new(),
            )
            .with_preserve_thinking_context(false);

            Ok(Self {
                inner,
                auth_provider,
            })
        })
    }
}

/// Adapter so the same `XaiOAuthAuthProvider` can be both owned by the
/// wrapper (for `configure_oauth`) and embedded as an `AuthMethod::Custom`
/// boxed `AuthProvider` in the inner `ApiClient`.
struct SharedAuthProvider(Arc<XaiOAuthAuthProvider>);

#[async_trait]
impl AuthProvider for SharedAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        self.0.get_auth_header().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_url_safe_base64_of_sha256_of_verifier() {
        let pkce = crate::providers::generate_pkce(64);
        assert_eq!(pkce.verifier.len(), 64);
        // S256 of a 64-char ASCII verifier => 32-byte digest => 43 base64url chars (no padding).
        assert_eq!(pkce.challenge.len(), 43);
        assert!(!pkce.challenge.contains('='));
        assert!(!pkce.challenge.contains('+'));
        assert!(!pkce.challenge.contains('/'));
    }

    #[test]
    fn authorize_url_contains_required_oauth_params() {
        let pkce = PkceChallenge {
            verifier: "v".repeat(64),
            challenge: "challenge-fixture".to_string(),
        };
        let url = build_authorize_url(&pkce, "state-fixture", "nonce-fixture").unwrap();
        assert!(url.starts_with(AUTHORIZE_URL));
        assert!(url.contains(&format!("client_id={}", CLIENT_ID)));
        assert!(url.contains("code_challenge=challenge-fixture"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state-fixture"));
        assert!(url.contains("nonce=nonce-fixture"));
        assert!(url.contains("plan=generic"));
        assert!(url.contains("referrer=gosling"));
        assert!(url.contains("scope=openid"));
        assert!(url.contains("offline_access"));
        assert!(url.contains("grok-cli%3Aaccess"));
    }

    #[test]
    fn redirect_uri_matches_registered_grok_cli_value() {
        // xAI rejects mismatched redirect_uris for the Grok-CLI client_id.
        // This pins the loopback host/port that pairs with that client.
        assert_eq!(redirect_uri(), "http://127.0.0.1:56121/callback");
    }

    #[test]
    fn token_cache_path_lives_under_gosling_config_dir() {
        let path = get_cache_path();
        let s = path.to_string_lossy().into_owned();
        assert!(
            s.contains("xai_oauth"),
            "expected token path under xai_oauth/, got {}",
            s
        );
        assert!(s.ends_with("tokens.json"));
    }

    #[test]
    fn split_effort_suffix_parses_supported_efforts() {
        assert_eq!(
            split_effort_suffix("grok-4.6-high"),
            ("grok-4.6".to_string(), Some("high".to_string()))
        );
        assert_eq!(
            split_effort_suffix("grok-4.6-low"),
            ("grok-4.6".to_string(), Some("low".to_string()))
        );
        // No suffix and unrelated suffixes are left intact.
        assert_eq!(
            split_effort_suffix("grok-4.6"),
            ("grok-4.6".to_string(), None)
        );
        assert_eq!(
            split_effort_suffix("custom-model-fast"),
            ("custom-model-fast".to_string(), None)
        );
    }

    #[test]
    fn apply_reasoning_effort_rewrites_model_and_sets_param() {
        let config = apply_reasoning_effort(&ModelConfig::new("grok-4.6-medium"));
        assert_eq!(config.model_name, "grok-4.6");
        assert_eq!(
            config
                .request_params
                .as_ref()
                .and_then(|p| p.get("reasoning_effort")),
            Some(&serde_json::json!("medium"))
        );
    }

    #[test]
    fn apply_reasoning_effort_omits_param_without_suffix_or_setting() {
        let config = apply_reasoning_effort(&ModelConfig::new("grok-4.6"));
        assert_eq!(config.model_name, "grok-4.6");
        assert!(config
            .request_params
            .as_ref()
            .and_then(|p| p.get("reasoning_effort"))
            .is_none());
    }

    #[test]
    fn apply_reasoning_effort_maps_standard_thinking_effort() {
        // A standard thinking_effort setting (from UI/ACP) is translated even
        // without a name suffix; Max clamps to the proxy's highest, "high".
        let config = apply_reasoning_effort(
            &ModelConfig::new("grok-4.6").with_thinking_effort(ThinkingEffort::Max),
        );
        assert_eq!(config.model_name, "grok-4.6");
        assert_eq!(
            config
                .request_params
                .as_ref()
                .and_then(|p| p.get("reasoning_effort")),
            Some(&serde_json::json!("high"))
        );
    }

    #[test]
    fn apply_reasoning_effort_suffix_beats_setting() {
        let config = apply_reasoning_effort(
            &ModelConfig::new("grok-4.6-low").with_thinking_effort(ThinkingEffort::High),
        );
        assert_eq!(config.model_name, "grok-4.6");
        assert_eq!(
            config
                .request_params
                .as_ref()
                .and_then(|p| p.get("reasoning_effort")),
            Some(&serde_json::json!("low"))
        );
    }

    #[test]
    fn prepare_supergrok_model_config_rejects_removed_models() {
        let error = prepare_supergrok_model_config(&ModelConfig::new("grok-4.5"))
            .expect_err("grok-4.5 must not remain usable");
        assert_eq!(
            error,
            ProviderError::RequestFailed("SuperGrok supports only grok-4.6 in Gosling".to_string())
        );

        let config = prepare_supergrok_model_config(&ModelConfig::new("grok-4.6-high"))
            .expect("grok-4.6 must remain usable");
        assert_eq!(config.model_name, "grok-4.6");
    }

    #[test]
    fn proxy_reasoning_effort_maps_and_clamps() {
        assert_eq!(proxy_reasoning_effort(ThinkingEffort::Off), None);
        assert_eq!(proxy_reasoning_effort(ThinkingEffort::Low), Some("low"));
        assert_eq!(
            proxy_reasoning_effort(ThinkingEffort::Medium),
            Some("medium")
        );
        assert_eq!(proxy_reasoning_effort(ThinkingEffort::High), Some("high"));
        assert_eq!(proxy_reasoning_effort(ThinkingEffort::Max), Some("high"));
    }

    #[test]
    fn metadata_exposes_only_grok_4_6_as_reasoning_capable() {
        let meta = <XaiOAuthProvider as gosling_providers::base::ProviderDescriptor>::metadata();
        assert_eq!(meta.default_model, "grok-4.6");
        assert_eq!(meta.known_models.len(), 1);
        let grok = &meta.known_models[0];
        assert_eq!(grok.name, "grok-4.6");
        assert!(grok.reasoning, "grok-4.6 must be reasoning-capable");
        assert_eq!(grok.context_limit, 500_000);
    }

    #[test]
    fn parse_version_reads_core_and_ignores_suffix() {
        assert_eq!(parse_version("0.2.93"), Some((0, 2, 93)));
        assert_eq!(parse_version("1.0"), Some((1, 0, 0)));
        assert_eq!(parse_version(" 0.1.202-beta.1 "), Some((0, 1, 202)));
        assert_eq!(parse_version("2.3.4+build5"), Some((2, 3, 4)));
        assert_eq!(parse_version("not-a-version"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn version_min_boundary_is_ordered() {
        // A version at or above the minimum is accepted; below is rejected.
        assert!(parse_version("0.1.202").unwrap() >= GROK_CLIENT_VERSION_MIN);
        assert!(parse_version("0.2.93").unwrap() >= GROK_CLIENT_VERSION_MIN);
        assert!(parse_version("0.1.201").unwrap() < GROK_CLIENT_VERSION_MIN);
        assert!(parse_version("0.0.999").unwrap() < GROK_CLIENT_VERSION_MIN);
        // The fallback pin itself must satisfy the minimum.
        assert!(parse_version(GROK_CLIENT_VERSION_FALLBACK).unwrap() >= GROK_CLIENT_VERSION_MIN);
    }
}
