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
pub const USER_AGENT: &str = concat!("PolarEffects/", env!("CARGO_PKG_VERSION"));

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

/// A blocking client with the given per-request timeout.
///
/// # Errors
/// [`TrackerError::Network`] if TLS or the client cannot be set up.
pub fn client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .tls_backend_preconfigured(tls_config()?)
        .timeout(timeout)
        .user_agent(USER_AGENT)
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
}
