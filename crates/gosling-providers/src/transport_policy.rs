//! Transport constraints applied to every provider HTTP client (ARC-GSL-006).
//!
//! Provider credentials travel as request headers -- `Authorization: Bearer`,
//! `x-api-key`, and vendor-specific header names -- so the transport that
//! carries them is part of the credential's security boundary. Individual
//! providers used to enforce that boundary one at a time (Snowflake bails on a
//! plaintext host; nothing else checked), which left the guarantee dependent on
//! which provider happened to be selected.
//!
//! The policy has three parts:
//!
//! 1. **Scheme.** A base URL must be `https`. Plaintext is accepted only for a
//!    loopback host, where the request never leaves the machine -- that is the
//!    documented local-inference story (Ollama defaults to `localhost`, and
//!    LM Studio and local proxies follow the same shape).
//! 2. **Escape hatch.** A self-hosted model server on a trusted LAN is a real
//!    deployment, so a plaintext non-loopback host is reachable by setting
//!    `GOSLING_ALLOW_INSECURE_PROVIDER_TRANSPORT=true`. It fails closed by
//!    default and logs a security event when the operator opts out, rather than
//!    silently allowing plaintext or making the LAN case impossible.
//! 3. **Redirects.** A redirect can move a request off the host and scheme the
//!    policy just approved. `reqwest` drops `Authorization` across an origin
//!    change but knows nothing about vendor API-key headers, which are set as
//!    ordinary headers and would follow the redirect. So a redirect may not
//!    downgrade `https` to `http`, may not change host or port, and is capped
//!    at [`MAX_REDIRECTS`] hops.

use anyhow::Result;
use std::net::IpAddr;
use url::{Host, Url};

/// A provider that needs more than this many hops to answer is misconfigured,
/// not redirecting.
const MAX_REDIRECTS: usize = 4;

const INSECURE_TRANSPORT_ENV: &str = "GOSLING_ALLOW_INSECURE_PROVIDER_TRANSPORT";

/// Whether a base URL may use plaintext HTTP to a non-loopback host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaintextPolicy {
    /// Plaintext is confined to loopback.
    #[default]
    LoopbackOnly,
    /// The operator accepted plaintext to any host.
    Allowed,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TransportPolicy {
    plaintext: PlaintextPolicy,
}

impl TransportPolicy {
    /// The policy as configured for this process.
    pub fn from_env() -> Self {
        let plaintext = if insecure_transport_opt_out() {
            PlaintextPolicy::Allowed
        } else {
            PlaintextPolicy::LoopbackOnly
        };
        Self { plaintext }
    }

    pub fn with_plaintext(plaintext: PlaintextPolicy) -> Self {
        Self { plaintext }
    }

    /// Checks a provider base URL against the scheme rules.
    ///
    /// A host that parses as a URL but carries no scheme is rejected rather
    /// than upgraded: an operator who typed `http://` should find out, not have
    /// it silently fixed. Providers that accept a bare hostname normalize it to
    /// a scheme before reaching here.
    pub fn validate_base_url(&self, base_url: &str) -> Result<()> {
        let url = Url::parse(base_url)
            .map_err(|error| anyhow::anyhow!("invalid provider base URL {base_url}: {error}"))?;
        match url.scheme() {
            "https" => Ok(()),
            "http" => {
                if is_loopback(&url) {
                    return Ok(());
                }
                if self.plaintext == PlaintextPolicy::Allowed {
                    tracing::warn!(
                        security.event_type = "insecure_provider_transport_allowed",
                        security.host = url.host_str().unwrap_or("<none>"),
                        "provider base URL uses plaintext HTTP to a non-loopback host; \
                         credentials are sent unencrypted because {INSECURE_TRANSPORT_ENV} is set"
                    );
                    return Ok(());
                }
                anyhow::bail!(
                    "provider base URL {base_url} uses plaintext HTTP to a non-loopback host. \
                     Provider credentials are sent as request headers, so they would travel \
                     unencrypted. Use https, point at a loopback address, or set \
                     {INSECURE_TRANSPORT_ENV}=true to accept the risk."
                )
            }
            other => anyhow::bail!(
                "provider base URL {base_url} uses unsupported scheme {other}; \
                 only https (or http on loopback) is allowed"
            ),
        }
    }

    /// The redirect rule for a client built under this policy.
    pub fn redirect_policy(&self) -> reqwest::redirect::Policy {
        reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                return attempt.error(format!(
                    "provider redirected more than {MAX_REDIRECTS} times"
                ));
            }
            let Some(previous) = attempt.previous().last().cloned() else {
                return attempt.follow();
            };
            let next = attempt.url();
            let downgraded = previous.scheme() == "https" && next.scheme() != "https";
            let moved_origin = previous.host_str() != next.host_str()
                || previous.port_or_known_default() != next.port_or_known_default();
            let next_scheme = next.scheme().to_string();
            let next_authority = authority(next);
            if downgraded {
                return attempt.error(format!(
                    "provider redirected from https to {next_scheme}, which would send \
                     credentials unencrypted"
                ));
            }
            if moved_origin {
                return attempt.error(format!(
                    "provider redirected to a different origin ({} -> {next_authority}); \
                     credential headers are not carried across origins",
                    authority(&previous)
                ));
            }
            attempt.follow()
        })
    }
}

fn authority(url: &Url) -> String {
    match url.port_or_known_default() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or("<none>")),
        None => url.host_str().unwrap_or("<none>").to_string(),
    }
}

fn insecure_transport_opt_out() -> bool {
    std::env::var(INSECURE_TRANSPORT_ENV)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            value == "1" || value == "true" || value == "yes"
        })
        .unwrap_or(false)
}

/// Whether the request would stay on this machine.
///
/// `localhost` is treated as loopback even though a hosts file can point it
/// elsewhere: every local-inference default in the ecosystem spells it that
/// way, and an operator who has repointed `localhost` has already redefined
/// what local means on that machine.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => {
            let domain = domain.to_ascii_lowercase();
            domain == "localhost" || domain.ends_with(".localhost")
        }
        Some(Host::Ipv4(addr)) => IpAddr::V4(addr).is_loopback(),
        Some(Host::Ipv6(addr)) => IpAddr::V6(addr).is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loopback_only() -> TransportPolicy {
        TransportPolicy::with_plaintext(PlaintextPolicy::LoopbackOnly)
    }

    #[test]
    fn https_is_always_accepted() {
        assert!(loopback_only()
            .validate_base_url("https://api.openai.com/v1")
            .is_ok());
    }

    #[test]
    fn plaintext_loopback_is_accepted() {
        for host in [
            "http://localhost:11434",
            "http://127.0.0.1:11434",
            "http://[::1]:11434",
            "http://ollama.localhost/v1",
        ] {
            assert!(
                loopback_only().validate_base_url(host).is_ok(),
                "{host} should be accepted"
            );
        }
    }

    #[test]
    fn plaintext_to_a_remote_host_is_rejected_by_default() {
        let error = loopback_only()
            .validate_base_url("http://inference.internal:11434")
            .unwrap_err()
            .to_string();
        assert!(error.contains("plaintext HTTP"), "{error}");
        assert!(error.contains(INSECURE_TRANSPORT_ENV), "{error}");
    }

    #[test]
    fn a_lan_address_is_not_loopback() {
        assert!(loopback_only()
            .validate_base_url("http://192.168.1.10:11434")
            .is_err());
        assert!(loopback_only()
            .validate_base_url("http://127.0.0.1.evil.example.com")
            .is_err());
    }

    #[test]
    fn the_opt_out_allows_plaintext_anywhere() {
        let policy = TransportPolicy::with_plaintext(PlaintextPolicy::Allowed);
        assert!(policy
            .validate_base_url("http://192.168.1.10:11434")
            .is_ok());
    }

    #[test]
    fn non_http_schemes_are_rejected() {
        for host in ["file:///etc/passwd", "ftp://example.com", "ws://localhost"] {
            assert!(
                loopback_only().validate_base_url(host).is_err(),
                "{host} should be rejected"
            );
        }
    }

    #[test]
    fn a_schemeless_host_is_not_silently_upgraded() {
        assert!(loopback_only()
            .validate_base_url("api.openai.com/v1")
            .is_err());
    }
}
