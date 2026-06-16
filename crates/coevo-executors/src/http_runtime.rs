//! Real HTTP-runtime executor.
//!
//! Binds external runtimes exposed over HTTP — Hermes, OpenClaw, 302AI, and the
//! generic `Custom` source. It POSTs the task as JSON to the passport's
//! `runtime_endpoint`, parses a JSON response into an [`ExecutorResult`], and
//! authenticates with headers resolved from `required_credentials` (env-backed
//! references — never literal secrets).
//!
//! Response contract (lenient): the runtime SHOULD return a JSON object. A
//! top-level `success` bool, `run_id`/`id` string, `output`/`result` object,
//! and `cost_usd` number are recognised; anything missing falls back to a
//! reasonable default and the raw body is preserved under `output`.

use crate::config::{self, AuthHeader};
use crate::traits::*;
use async_trait::async_trait;
use coevo_core::lease::EmergencyLease;
use coevo_core::opc::{ExternalExecutorPassport, WorkOrder};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
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
}

impl HttpRuntimeExecutor {
    pub fn new(passport: ExternalExecutorPassport) -> Self {
        Self {
            passport,
            client: reqwest::Client::new(),
            timeout: DEFAULT_TIMEOUT,
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

    fn base_url(&self) -> Result<String, ExecutorError> {
        config::parse_http_base(&self.passport.runtime_endpoint).ok_or_else(|| {
            ExecutorError::Internal(format!(
                "HTTP runtime '{}' has a non-http runtime_endpoint '{}'",
                self.passport.executor_id, self.passport.runtime_endpoint
            ))
        })
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

        let base = match self.base_url() {
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
        let base = self.base_url()?;
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

        let run_id = body
            .get("run_id")
            .or_else(|| body.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let success = body
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
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
        let base = self.base_url()?;
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
        let base = self.base_url()?;
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
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()));
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
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()));
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
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()));
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
        let exec = HttpRuntimeExecutor::new(http_passport(&server.base_url()));
        let err = exec.cancel("run-1").await.unwrap_err();
        match err {
            ExecutorError::Internal(msg) => assert!(msg.contains("cancel not supported")),
            other => panic!("expected unsupported cancel, got {other:?}"),
        }
        server.shutdown();
    }
}
