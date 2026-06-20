//! Real HTTP-runtime executor.
//!
//! Binds external runtimes exposed over HTTP — Hermes, OpenClaw, 302AI, and the
//! generic `Custom` source. It POSTs the task as JSON to the passport's
//! `runtime_endpoint`, parses a JSON response into an [`ExecutorResult`], and
//! authenticates with headers resolved from `required_credentials` (env-backed
//! references — never literal secrets).
//!
//! Response contract (strict): the runtime MUST return a JSON object with an
//! explicit top-level `success` bool and a stable `run_id`/`id` string. Optional
//! `output`/`result` and `cost_usd` fields are preserved when present.

use crate::config::{self, AuthHeader};
use crate::traits::*;
use async_trait::async_trait;
use coevo_core::lease::EmergencyLease;
use coevo_core::opc::{ExternalExecutorPassport, WorkOrder};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::redirect::Policy;
use reqwest::Url;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

/// Default per-request timeout for runtime calls.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
/// Health-check timeout (short — it is a liveness probe).
const HEALTH_TIMEOUT: Duration = Duration::from_secs(10);

/// Executes work orders against an HTTP runtime.
pub struct HttpRuntimeExecutor {
    passport: ExternalExecutorPassport,
    client: reqwest::Client,
    timeout: Duration,
    allow_private_network_endpoints: bool,
}

impl HttpRuntimeExecutor {
    pub fn new(passport: ExternalExecutorPassport) -> Self {
        Self {
            passport,
            client: reqwest::Client::builder()
                .redirect(Policy::none())
                .build()
                .expect("default reqwest client should build"),
            timeout: DEFAULT_TIMEOUT,
            allow_private_network_endpoints: allow_private_http_executors_from_env(),
        }
    }

    /// Inject a pre-built client (tests point this at a local stub server).
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_private_network_endpoints_allowed(mut self, allowed: bool) -> Self {
        self.allow_private_network_endpoints = allowed;
        self
    }

    async fn base_url(&self) -> Result<String, ExecutorError> {
        let base = config::parse_http_base(&self.passport.runtime_endpoint).ok_or_else(|| {
            ExecutorError::Internal(format!(
                "HTTP runtime '{}' has a non-http runtime_endpoint '{}'",
                self.passport.executor_id, self.passport.runtime_endpoint
            ))
        })?;
        validate_http_endpoint_url(
            &base,
            self.allow_private_network_endpoints,
            "runtime_endpoint",
        )
        .await?;
        Ok(base)
    }

    fn auth_headers(&self) -> HeaderMap {
        let resolved = config::resolve_credentials(&self.passport.required_credentials);
        build_header_map(&resolved.headers)
    }
}

fn build_header_map(headers: &[AuthHeader]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for h in headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(h.name.as_bytes()),
            HeaderValue::from_str(&h.value),
        ) {
            map.insert(name, value);
        }
    }
    map
}

fn allow_private_http_executors_from_env() -> bool {
    matches!(
        std::env::var("COEVO_ALLOW_PRIVATE_HTTP_EXECUTORS")
            .ok()
            .as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

async fn validate_http_endpoint_url(
    url: &str,
    allow_private: bool,
    label: &str,
) -> Result<(), ExecutorError> {
    let parsed = Url::parse(url)
        .map_err(|e| ExecutorError::Internal(format!("invalid HTTP runtime {label}: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ExecutorError::Internal(format!(
            "HTTP runtime {label} must use http or https"
        )));
    }
    if allow_private {
        return Ok(());
    }

    let host = parsed.host_str().ok_or_else(|| {
        ExecutorError::Internal(format!("HTTP runtime {label} is missing a host"))
    })?;
    let normalized_host = host.trim_end_matches('.').to_ascii_lowercase();
    if normalized_host == "localhost" || normalized_host.ends_with(".localhost") {
        return Err(private_endpoint_error(label, host));
    }

    if let Ok(ip) = normalized_host.parse::<IpAddr>() {
        reject_private_ip(ip, label, host)?;
        return Ok(());
    }

    let port = parsed.port_or_known_default().ok_or_else(|| {
        ExecutorError::Internal(format!("HTTP runtime {label} is missing a port"))
    })?;
    let mut addrs = tokio::net::lookup_host((host, port)).await.map_err(|e| {
        ExecutorError::Internal(format!("could not resolve HTTP runtime {label} host: {e}"))
    })?;
    let mut resolved_any = false;
    for addr in &mut addrs {
        resolved_any = true;
        reject_private_ip(addr.ip(), label, host)?;
    }
    if !resolved_any {
        return Err(ExecutorError::Internal(format!(
            "HTTP runtime {label} host resolved to no addresses"
        )));
    }
    Ok(())
}

fn reject_private_ip(ip: IpAddr, label: &str, host: &str) -> Result<(), ExecutorError> {
    if is_forbidden_endpoint_ip(ip) {
        Err(private_endpoint_error(label, host))
    } else {
        Ok(())
    }
}

fn private_endpoint_error(label: &str, host: &str) -> ExecutorError {
    ExecutorError::Internal(format!(
        "HTTP runtime {label} resolves to private or local endpoint '{host}'; set COEVO_ALLOW_PRIVATE_HTTP_EXECUTORS=1 only for trusted local runtimes"
    ))
}

fn is_forbidden_endpoint_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_forbidden_ipv4(ip),
        IpAddr::V6(ip) => is_forbidden_ipv6(ip),
    }
}

fn is_forbidden_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || o[0] == 0
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        || o[0] >= 240
}

fn is_forbidden_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_forbidden_ipv4(v4);
    }
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
}

fn validate_run_id(run_id: &str) -> Result<(), ExecutorError> {
    let valid = !run_id.is_empty()
        && run_id.len() <= 128
        && run_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':'));
    if valid {
        Ok(())
    } else {
        Err(ExecutorError::Internal(
            "HTTP runtime run_id must be 1-128 chars of [A-Za-z0-9_:-]".to_string(),
        ))
    }
}

#[async_trait]
impl ExternalExecutorAdapter for HttpRuntimeExecutor {
    fn passport(&self) -> &ExternalExecutorPassport {
        &self.passport
    }

    async fn health_check(&self) -> Result<ExecutorHealth, ExecutorError> {
        let start = Instant::now();
        let Some(url) = config::health_url(&self.passport) else {
            return Ok(ExecutorHealth {
                online: false,
                latency_ms: 0,
                version: format!(
                    "no http health url (runtime_endpoint '{}')",
                    self.passport.runtime_endpoint
                ),
            });
        };
        let url = match validate_http_endpoint_url(
            &url,
            self.allow_private_network_endpoints,
            "health_check_url",
        )
        .await
        {
            Ok(()) => url,
            Err(e) => {
                return Ok(ExecutorHealth {
                    online: false,
                    latency_ms: start.elapsed().as_millis() as u64,
                    version: e.to_string(),
                })
            }
        };
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await;
        let latency_ms = start.elapsed().as_millis() as u64;
        match resp {
            Ok(r) => {
                let status = r.status();
                let version = r
                    .headers()
                    .get("x-runtime-version")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("http {}", status.as_u16()));
                Ok(ExecutorHealth {
                    online: status.is_success(),
                    latency_ms,
                    version,
                })
            }
            Err(e) => Ok(ExecutorHealth {
                online: false,
                latency_ms,
                version: format!("unreachable: {e}"),
            }),
        }
    }

    async fn describe_capabilities(&self) -> Result<Vec<String>, ExecutorError> {
        Ok(self.passport.capabilities.clone())
    }

    async fn dry_run(&self, work_order: &WorkOrder) -> Result<DryRunResult, ExecutorError> {
        let mut warnings = Vec::new();

        // Surface missing credential env vars without failing hard.
        let resolved = config::resolve_credentials(&self.passport.required_credentials);
        for missing in &resolved.missing {
            warnings.push(format!("credential reference unresolved: {missing}"));
        }

        let base = match self.base_url().await {
            Ok(b) => b,
            Err(e) => {
                return Ok(DryRunResult {
                    passed: false,
                    estimated_cost_usd: 0.0,
                    estimated_duration_ms: 0,
                    warnings: vec![e.to_string()],
                })
            }
        };

        // Prefer an explicit /dry-run endpoint when the runtime advertises it via
        // a capability; otherwise validate connectivity (health) with no side
        // effects and confirm the payload serialises.
        let supports_dry_run = self
            .passport
            .capabilities
            .iter()
            .any(|c| c.contains("dry-run") || c.contains("dry_run"));

        if supports_dry_run {
            let payload = config::task_payload(&self.passport, work_order, true);
            let resp = self
                .client
                .post(format!("{base}/dry-run"))
                .headers(self.auth_headers())
                .timeout(self.timeout)
                .json(&payload)
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    return Ok(DryRunResult {
                        passed: body.get("passed").and_then(|v| v.as_bool()).unwrap_or(true),
                        estimated_cost_usd: body
                            .get("estimated_cost_usd")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        estimated_duration_ms: body
                            .get("estimated_duration_ms")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        warnings,
                    });
                }
                Ok(r) => {
                    warnings.push(format!("dry-run endpoint returned {}", r.status()));
                }
                Err(e) => {
                    warnings.push(format!("dry-run request failed: {e}"));
                    return Ok(DryRunResult {
                        passed: false,
                        estimated_cost_usd: 0.0,
                        estimated_duration_ms: 0,
                        warnings,
                    });
                }
            }
        }

        // Fallback: connectivity probe via health.
        let health = self.health_check().await?;
        if !health.online {
            warnings.push(format!("runtime not reachable: {}", health.version));
        }
        Ok(DryRunResult {
            passed: health.online,
            estimated_cost_usd: 0.0,
            estimated_duration_ms: health.latency_ms,
            warnings,
        })
    }

    async fn execute(
        &self,
        work_order: &WorkOrder,
        _lease: Option<&EmergencyLease>,
    ) -> Result<ExecutorResult, ExecutorError> {
        let base = self.base_url().await?;
        let payload = config::task_payload(&self.passport, work_order, false);
        let resp = self
            .client
            .post(&base)
            .headers(self.auth_headers())
            .timeout(self.timeout)
            .json(&payload)
            .send()
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(e) if e.is_timeout() => return Err(ExecutorError::Timeout),
            Err(e) => {
                return Err(ExecutorError::Internal(format!(
                    "runtime request failed: {e}"
                )))
            }
        };

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
        if !status.is_success() {
            return Err(ExecutorError::Internal(format!(
                "runtime returned {}: {}",
                status,
                body.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no error body)")
            )));
        }

        let success = body
            .get("success")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| {
                ExecutorError::Internal(
                    "HTTP runtime response missing explicit success boolean".to_string(),
                )
            })?;
        let run_id = body
            .get("run_id")
            .or_else(|| body.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ExecutorError::Internal(
                    "HTTP runtime response missing explicit run_id/id string".to_string(),
                )
            })?
            .to_string();
        validate_run_id(&run_id)?;
        let output = body
            .get("output")
            .or_else(|| body.get("result"))
            .cloned()
            .unwrap_or_else(|| body.clone());
        let cost_usd = body.get("cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0);

        Ok(ExecutorResult {
            run_id: run_id.clone(),
            success,
            output,
            audit_trace: format!("http-runtime:{}:{}", self.passport.executor_id, run_id),
            cost_usd,
        })
    }

    async fn cancel(&self, run_id: &str) -> Result<(), ExecutorError> {
        validate_run_id(run_id)?;
        let base = self.base_url().await?;
        // Best effort: POST {base}/cancel/{id}. A non-2xx or unreachable runtime
        // surfaces honestly rather than pretending the cancel happened.
        let resp = self
            .client
            .post(format!("{base}/cancel/{run_id}"))
            .headers(self.auth_headers())
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => Ok(()),
            Ok(r) if r.status().as_u16() == 404 => Err(ExecutorError::Internal(format!(
                "cancel not supported by runtime '{}' (no /cancel endpoint)",
                self.passport.executor_id
            ))),
            Ok(r) => Err(ExecutorError::Internal(format!(
                "runtime cancel returned {}",
                r.status()
            ))),
            Err(e) => Err(ExecutorError::Internal(format!(
                "cancel request failed: {e}"
            ))),
        }
    }

    async fn fetch_audit(&self, run_id: &str) -> Result<String, ExecutorError> {
        validate_run_id(run_id)?;
        let base = self.base_url().await?;
        let resp = self
            .client
            .get(format!("{base}/audit/{run_id}"))
            .headers(self.auth_headers())
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => {
                Ok(r.text().await.unwrap_or_else(|_| String::new()))
            }
            _ => Ok(format!(
                "http-runtime:audit:{}:{}",
                self.passport.executor_id, run_id
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{stub_server, test_passport, test_work_order, StubResponse};
    use coevo_core::opc::ExecutorSourceType;

    fn http_passport(endpoint: &str) -> ExternalExecutorPassport {
        let mut p = test_passport(ExecutorSourceType::Hermes, "Hermes Runtime");
        p.runtime_endpoint = endpoint.to_string();
        p.health_check_url = format!("{endpoint}/health");
        p
    }

    #[tokio::test]
    async fn execute_rejects_missing_success_flag() {
        let server = stub_server(StubResponse {
            status: 200,
            body: serde_json::json!({
                "run_id": "run-123",
                "output": {"answer": 42}
            })
            .to_string(),
        })
        .await;
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()))
            .with_private_network_endpoints_allowed(true);
        let err = exec.execute(&test_work_order(), None).await.unwrap_err();
        match err {
            ExecutorError::Internal(msg) => assert!(msg.contains("success")),
            other => panic!("expected strict success error, got {other:?}"),
        }
        server.shutdown();
    }

    #[tokio::test]
    async fn execute_rejects_missing_run_id() {
        let server = stub_server(StubResponse {
            status: 200,
            body: serde_json::json!({
                "success": true,
                "output": {"answer": 42}
            })
            .to_string(),
        })
        .await;
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()))
            .with_private_network_endpoints_allowed(true);
        let err = exec.execute(&test_work_order(), None).await.unwrap_err();
        match err {
            ExecutorError::Internal(msg) => assert!(msg.contains("run_id")),
            other => panic!("expected strict run_id error, got {other:?}"),
        }
        server.shutdown();
    }

    #[tokio::test]
    async fn fetch_audit_rejects_path_like_run_id() {
        let exec = HttpRuntimeExecutor::new(http_passport("https://runtime.example.com"));
        let err = exec.fetch_audit("../secret").await.unwrap_err();
        match err {
            ExecutorError::Internal(msg) => assert!(msg.contains("run_id")),
            other => panic!("expected run_id validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_private_http_runtime_by_default() {
        let exec = HttpRuntimeExecutor::new(http_passport("http://127.0.0.1:9000"));
        let err = exec.execute(&test_work_order(), None).await.unwrap_err();
        match err {
            ExecutorError::Internal(msg) => assert!(msg.contains("private")),
            other => panic!("expected private endpoint error, got {other:?}"),
        }
    }
    #[tokio::test]
    async fn execute_posts_and_parses_response() {
        let server = stub_server(StubResponse {
            status: 200,
            body: serde_json::json!({
                "run_id": "run-123",
                "success": true,
                "output": {"answer": 42},
                "cost_usd": 0.25
            })
            .to_string(),
        })
        .await;
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()))
            .with_private_network_endpoints_allowed(true);
        let result = exec.execute(&test_work_order(), None).await.unwrap();
        assert_eq!(result.run_id, "run-123");
        assert!(result.success);
        assert_eq!(result.output["answer"], 42);
        assert_eq!(result.cost_usd, 0.25);
        server.shutdown();
    }

    #[tokio::test]
    async fn execute_maps_5xx_to_error() {
        let server = stub_server(StubResponse {
            status: 500,
            body: serde_json::json!({"error": "boom"}).to_string(),
        })
        .await;
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()))
            .with_private_network_endpoints_allowed(true);
        let err = exec.execute(&test_work_order(), None).await.unwrap_err();
        assert!(matches!(err, ExecutorError::Internal(_)));
        server.shutdown();
    }

    #[tokio::test]
    async fn health_check_reports_online_on_2xx() {
        let server = stub_server(StubResponse {
            status: 200,
            body: "ok".to_string(),
        })
        .await;
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()))
            .with_private_network_endpoints_allowed(true);
        let health = exec.health_check().await.unwrap();
        assert!(health.online);
        server.shutdown();
    }

    #[tokio::test]
    async fn health_check_offline_for_non_http_endpoint() {
        let exec = HttpRuntimeExecutor::new(http_passport("coevo://executor/hermes"));
        let health = exec.health_check().await.unwrap();
        assert!(!health.online);
    }

    #[tokio::test]
    async fn cancel_404_reports_unsupported() {
        let server = stub_server(StubResponse {
            status: 404,
            body: String::new(),
        })
        .await;
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()))
            .with_private_network_endpoints_allowed(true);
        let err = exec.cancel("run-1").await.unwrap_err();
        match err {
            ExecutorError::Internal(msg) => assert!(msg.contains("cancel not supported")),
            other => panic!("expected unsupported cancel, got {other:?}"),
        }
        server.shutdown();
    }
}
