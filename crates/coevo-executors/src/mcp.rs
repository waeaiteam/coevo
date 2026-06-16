//! MCP executor source — thin, honest adapter.
//!
//! **MCP is not executed through this crate.** The real Model Context Protocol
//! client lives in `coevo-adapters` (`RealMcpClient` implementing `McpProvider`,
//! JSON-RPC 2.0 over stdio / Streamable HTTP) and is reached via the *MCP tool
//! path*: an MCP server's tools are surfaced into the worker's `ToolRegistry` as
//! `urn:mcp:{server}:{tool}` tools and invoked with `CallTool`, not `CallExecutor`.
//!
//! Why not delegate here? `RealMcpClient` is keyed by *server configs* (the
//! `mcp_servers` table) and addresses work by *tool URN*. An
//! [`ExternalExecutorPassport`] carries neither a server-config id nor a tool
//! URN, so there is no faithful 1:1 mapping from a single passport to an MCP
//! `tools/call`. Re-deriving one here would duplicate the routing that already
//! exists, correctly, at the tool layer (`coevo-executors → coevo-adapters` is
//! acyclic, so the dependency *would* be allowed — it is simply the wrong seam).
//!
//! So this adapter:
//! * `health_check` — if `runtime_endpoint`/`health_check_url` is HTTP(S) (a
//!   Streamable-HTTP MCP server), do a best-effort liveness GET. Otherwise it
//!   reports online with a note that MCP runs via the tool path (a stdio MCP
//!   server has no executor-level endpoint to probe).
//! * `dry_run` — passes with a warning directing callers to the MCP tool path.
//! * `execute` — returns an explicit error: MCP must be invoked as a tool, not
//!   as an executor. It never fabricates a success result.
//! * `cancel` — explicit "not applicable" error.

use crate::config;
use crate::traits::*;
use async_trait::async_trait;
use coevo_core::lease::EmergencyLease;
use coevo_core::opc::{ExternalExecutorPassport, WorkOrder};
use std::time::{Duration, Instant};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(10);

const ROUTING_NOTE: &str =
    "MCP executors run via the MCP tool path (urn:mcp:{server}:{tool} through the worker \
     ToolRegistry / RealMcpClient in coevo-adapters), not as an ExternalExecutorAdapter.";

/// Thin adapter for the `MCP` executor source. Does not execute MCP itself.
pub struct McpExecutorSource {
    passport: ExternalExecutorPassport,
    client: reqwest::Client,
}

impl McpExecutorSource {
    pub fn new(passport: ExternalExecutorPassport) -> Self {
        Self {
            passport,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExternalExecutorAdapter for McpExecutorSource {
    fn passport(&self) -> &ExternalExecutorPassport {
        &self.passport
    }

    async fn health_check(&self) -> Result<ExecutorHealth, ExecutorError> {
        // Streamable-HTTP MCP server: probe it. stdio server: nothing to probe.
        if let Some(url) = config::health_url(&self.passport) {
            let start = Instant::now();
            let resp = self.client.get(&url).timeout(HEALTH_TIMEOUT).send().await;
            let latency_ms = start.elapsed().as_millis() as u64;
            return Ok(match resp {
                Ok(r) => ExecutorHealth {
                    online: r.status().is_success(),
                    latency_ms,
                    version: format!("mcp-http {}", r.status().as_u16()),
                },
                Err(e) => ExecutorHealth {
                    online: false,
                    latency_ms,
                    version: format!("mcp-http unreachable: {e}"),
                },
            });
        }
        Ok(ExecutorHealth {
            online: true,
            latency_ms: 0,
            version: format!("mcp-tool-path; {ROUTING_NOTE}"),
        })
    }

    async fn describe_capabilities(&self) -> Result<Vec<String>, ExecutorError> {
        Ok(self.passport.capabilities.clone())
    }

    async fn dry_run(&self, _work_order: &WorkOrder) -> Result<DryRunResult, ExecutorError> {
        Ok(DryRunResult {
            passed: true,
            estimated_cost_usd: 0.0,
            estimated_duration_ms: 0,
            warnings: vec![ROUTING_NOTE.to_string()],
        })
    }

    async fn execute(
        &self,
        _work_order: &WorkOrder,
        _lease: Option<&EmergencyLease>,
    ) -> Result<ExecutorResult, ExecutorError> {
        Err(ExecutorError::Internal(format!(
            "executor '{}' is an MCP source. {ROUTING_NOTE}",
            self.passport.executor_id
        )))
    }

    async fn cancel(&self, _run_id: &str) -> Result<(), ExecutorError> {
        Err(ExecutorError::Internal(format!(
            "cancel not applicable for MCP executor source. {ROUTING_NOTE}"
        )))
    }

    async fn fetch_audit(&self, run_id: &str) -> Result<String, ExecutorError> {
        Ok(format!(
            "mcp-source:audit:{}:{}",
            self.passport.executor_id, run_id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{test_passport, test_work_order};
    use coevo_core::opc::ExecutorSourceType;

    fn mcp_passport(endpoint: &str) -> ExternalExecutorPassport {
        let mut p = test_passport(ExecutorSourceType::MCP, "MCP Runtime");
        p.runtime_endpoint = endpoint.to_string();
        p.health_check_url = endpoint.to_string();
        p
    }

    #[tokio::test]
    async fn execute_refuses_with_explicit_routing_error() {
        let exec = McpExecutorSource::new(mcp_passport("coevo://executor/mcp"));
        let err = exec.execute(&test_work_order(), None).await.unwrap_err();
        match err {
            ExecutorError::Internal(msg) => assert!(msg.contains("MCP")),
            other => panic!("expected routing error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dry_run_passes_with_routing_warning() {
        let exec = McpExecutorSource::new(mcp_passport("coevo://executor/mcp"));
        let dry = exec.dry_run(&test_work_order()).await.unwrap();
        assert!(dry.passed);
        assert!(dry.warnings.iter().any(|w| w.contains("MCP tool path")));
    }

    #[tokio::test]
    async fn health_non_http_endpoint_reports_tool_path() {
        let exec = McpExecutorSource::new(mcp_passport("coevo://executor/mcp"));
        let health = exec.health_check().await.unwrap();
        assert!(health.online);
        assert!(health.version.contains("mcp-tool-path"));
    }
}
