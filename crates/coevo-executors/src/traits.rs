//! External Executor Adapter trait.
//! All external executors (Hermes, OpenClaw, MCP, 302AI, etc.) must implement this.

use async_trait::async_trait;
use coevo_core::opc::{ExternalExecutorPassport, WorkOrder};

/// Result of an executor health check.
#[derive(Debug, Clone)]
pub struct ExecutorHealth {
    pub online: bool,
    pub latency_ms: u64,
    pub version: String,
}

/// Result of a dry-run execution.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DryRunResult {
    pub passed: bool,
    pub estimated_cost_usd: f64,
    pub estimated_duration_ms: u64,
    pub warnings: Vec<String>,
}

/// Result of a full execution.
#[derive(Debug, Clone)]
pub struct ExecutorResult {
    pub run_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub audit_trace: String,
    pub cost_usd: f64,
}

/// The External Executor Adapter trait.
#[async_trait]
pub trait ExternalExecutorAdapter: Send + Sync {
    fn passport(&self) -> &ExternalExecutorPassport;

    async fn health_check(&self) -> Result<ExecutorHealth, ExecutorError>;

    async fn describe_capabilities(&self) -> Result<Vec<String>, ExecutorError>;

    async fn dry_run(&self, work_order: &WorkOrder) -> Result<DryRunResult, ExecutorError>;

    async fn execute(
        &self,
        work_order: &WorkOrder,
        lease: Option<&coevo_core::lease::EmergencyLease>,
    ) -> Result<ExecutorResult, ExecutorError>;

    async fn cancel(&self, run_id: &str) -> Result<(), ExecutorError>;

    async fn fetch_audit(&self, run_id: &str) -> Result<String, ExecutorError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("executor not registered")]
    NotRegistered,
    #[error("executor disabled")]
    Disabled,
    #[error("risk ceiling exceeded: work_order={wo}, ceiling={ceiling}")]
    RiskCeilingExceeded { wo: f64, ceiling: f64 },
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("executor internal error: {0}")]
    Internal(String),
    #[error("timeout")]
    Timeout,
}
