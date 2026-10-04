//! The HTTP client every tracker request goes through.
//!
//! reqwest's own `rustls` feature pulls in the aws-lc-rs crypto provider,
//! which is a C library; the five-target build allows none (D6). The client
//! is therefore built with `rustls-no-provider` and handed a `ClientConfig`
//! on the pure-Rust `ring` provider, verifying certificates against the
//! platform's own trust store.
//!
//! The same as `pe-env`'s `net.rs`: the two network crates do not depend on
//! each other (CLAUDE.md dependency direction), so each builds its own.

use std::sync::Arc;
use std::time::Duration;

use rustls_platform_verifier::BuilderVerifierExt;

use crate::error::{Result, TrackerError};

/// Sent with every request so tracker operators can see who is reading.
pub const USER_AGENT: &str = concat!("PolarExplorer/", env!("CARGO_PKG_VERSION"));

/// The per-request timeout when the caller has no setting to pass
/// (spec.md 3.4 default).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// A TLS configuration on the `ring` provider with the platform verifier.
fn tls_config() -> Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| TrackerError::Network(format!("TLS setup: {e}")))?
        .with_platform_verifier()
        .map_err(|e| TrackerError::Network(format!("TLS certificate verifier: {e}")))?
        .with_no_client_auth();
    // A preconfigured config is used as given, so ALPN has to be set here for
    // HTTP/2 to be offered at all.
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

/// The most redirects one request follows.
const MAX_REDIRECTS: usize = 10;

/// The YellowBrick and Blue Water Tracks hosts (spec.md 7.2). Geovoile's
/// are every subdomain of `geovoile.com`, see [`is_geovoile_host`].
const HOSTS: [&str; 7] = [
    "yb.tl",
    "www.yb.tl",
    "cf.yb.tl",
    "app.yb.tl",
    "api.bluewatertracks.com",
    "www.regattaman.com",
    "data.orc.org",
];

/// Whether `host` is `geovoile.com` or one of its subdomains, compared
/// exactly: `geovoile.com.example.invalid` and `xgeovoile.com` are not.
pub fn is_geovoile_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    if host == "geovoile.com" {
        return true;
    }
    host.strip_suffix(".geovoile.com").is_some_and(|sub| {
        sub.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    })
}

/// Whether `host` is one of the trackers' allow-listed hosts (invariant 4).
pub fn allowed_host(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    HOSTS.contains(&lower.as_str()) || is_geovoile_host(&lower)
}

/// Whether a redirect from `from` to `to` may be followed: never to a
/// userinfo URL or from HTTPS down to HTTP; to the same host, or over HTTPS
/// to an allow-listed one. Anything else would let a tracker (or whoever
/// answers for it) send the app to a host invariant 4 does not allow.
pub fn redirect_allowed(from: Option<&reqwest::Url>, to: &reqwest::Url) -> bool {
    let Some(host) = to.host_str() else {
        return false;
    };
    if !to.username().is_empty() || to.password().is_some() {
        return false;
    }
    let secure = to.scheme() == "https";
    if !secure && (to.scheme() != "http" || from.is_some_and(|f| f.scheme() == "https")) {
        return false;
    }
    let same_host = from
        .and_then(reqwest::Url::host_str)
        .is_some_and(|h| h.eq_ignore_ascii_case(host));
    same_host || (secure && allowed_host(host))
}

fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }
        if redirect_allowed(attempt.previous().last(), attempt.url()) {
            attempt.follow()
        } else {
            let to = attempt.url().host_str().unwrap_or("").to_owned();
            attempt.error(format!(
                "a redirect to {to:?}, which is not an allowed host"
            ))
        }
    })
}

/// A blocking client with the given per-request timeout, following
/// redirects only as [`redirect_allowed`] says.
///
/// # Errors
/// [`TrackerError::Network`] if TLS or the client cannot be set up.
pub fn client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .tls_backend_preconfigured(tls_config()?)
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .redirect(redirect_policy())
        .build()
        .map_err(|e| TrackerError::Network(format!("no HTTP client: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building the client exercises the ring provider and the platform
    /// verifier without touching the network.
    #[test]
    fn a_client_builds_on_the_ring_provider() {
        assert!(client(DEFAULT_TIMEOUT).is_ok());
    }

    fn url(text: &str) -> reqwest::Url {
        reqwest::Url::parse(text).expect(text)
    }

    #[test]
    fn geovoile_hosts_are_matched_exactly() {
        for host in [
            "geovoile.com",
            "vendeeglobe.geovoile.com",
            "a.b-c.GEOVOILE.com",
        ] {
            assert!(is_geovoile_host(host), "{host}");
        }
        for host in [
            "xgeovoile.com",
            "geovoile.com.example.invalid",
            ".geovoile.com",
            "a..geovoile.com",
            "a_b.geovoile.com",
            "geovoile.co",
        ] {
            assert!(!is_geovoile_host(host), "{host}");
        }
        assert!(allowed_host("cf.yb.tl") && allowed_host("API.bluewatertracks.com"));
        assert!(!allowed_host("yb.tl.example.invalid") && !allowed_host("evil.invalid"));
    }

    #[test]
    fn redirects_stay_on_the_host_or_the_allow_list() {
        let from = url("https://cf.yb.tl/JSON/x/RaceSetup");
        // Same host, another allow-listed host.
        assert!(redirect_allowed(
            Some(&from),
            &url("https://cf.yb.tl/other")
        ));
        assert!(redirect_allowed(Some(&from), &url("https://yb.tl/x")));
        assert!(redirect_allowed(
            Some(&from),
            &url("https://static.geovoile.com/r/")
        ));
        // Elsewhere, look-alikes, userinfo, and a downgrade to HTTP.
        for to in [
            "https://evil.invalid/x",
            "https://yb.tl.evil.invalid/x",
            "https://geovoile.com.evil.invalid/x",
            &format!("{}user@yb.tl/x", "https://"),
            "https://yb.tl:pw@evil.invalid/x",
            "http://cf.yb.tl/x",
            "ftp://cf.yb.tl/x",
        ] {
            assert!(!redirect_allowed(Some(&from), &url(to)), "{to}");
        }
        // Plain HTTP on the same host (a local test server) is followed.
        let local = url("http://127.0.0.1:8080/a");
        assert!(redirect_allowed(
            Some(&local),
            &url("http://127.0.0.1:8080/b")
        ));
        assert!(!redirect_allowed(
            Some(&local),
            &url("http://evil.invalid/b")
        ));
    }
}
