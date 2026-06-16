//! Process-wide shared HTTP clients for all model gateways.
//!
//! Creating a `reqwest::Client` per request defeats connection pooling and
//! leaks a new connector/DNS resolver each call. These two shared instances
//! are initialized once per process via `OnceLock`.

use std::sync::OnceLock;
use std::time::Duration;

fn build_client(total_timeout: Option<Duration>, disable_proxy: bool) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .pool_idle_timeout(Duration::from_secs(90));
    if disable_proxy {
        // Local test servers and sidecars should never be routed through
        // system HTTP proxies; doing so causes flaky loopback failures.
        builder = builder.no_proxy();
    }
    if let Some(timeout) = total_timeout {
        builder = builder.timeout(timeout);
    }
    builder.build().expect("failed to build shared HTTP client")
}

fn should_bypass_system_proxy(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    matches!(
        parsed.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("::1") | Some("[::1]")
    )
}

/// Shared client for model calls, including long-lived streaming responses.
///
/// Deliberately has NO total request timeout: SSE streams can legitimately
/// stay open for many minutes. Connection establishment is still bounded by
/// `connect_timeout`, and callers of non-streaming endpoints apply their own
/// per-request timeout (`RequestBuilder::timeout`) from the provider config.
pub(crate) fn streaming_client_for(url: &str) -> &'static reqwest::Client {
    static DEFAULT_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    static LOOPBACK_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if should_bypass_system_proxy(url) {
        LOOPBACK_CLIENT.get_or_init(|| build_client(None, true))
    } else {
        DEFAULT_CLIENT.get_or_init(|| build_client(None, false))
    }
}

/// Shared client for short non-streaming control calls
/// (`test_connection`, `discover_models`): bounded by a 30s total timeout
/// in addition to the per-request timeout from the provider config.
pub(crate) fn short_call_client_for(url: &str) -> &'static reqwest::Client {
    static DEFAULT_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    static LOOPBACK_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if should_bypass_system_proxy(url) {
        LOOPBACK_CLIENT.get_or_init(|| build_client(Some(Duration::from_secs(30)), true))
    } else {
        DEFAULT_CLIENT.get_or_init(|| build_client(Some(Duration::from_secs(30)), false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_loopback_urls_for_proxy_bypass() {
        assert!(should_bypass_system_proxy("http://127.0.0.1:8000/v1"));
        assert!(should_bypass_system_proxy("http://localhost:8717"));
        assert!(should_bypass_system_proxy("http://[::1]:9000"));
        assert!(!should_bypass_system_proxy("https://api.openai.com/v1"));
        assert!(!should_bypass_system_proxy("not-a-url"));
    }
}
