//! Test-only mock executor adapter.
//!
//! **Not used by production code.** The real adapters live in
//! [`crate::local_process`], [`crate::http_runtime`], [`crate::docker`], and
//! [`crate::mcp`], and are constructed via [`crate::factory::build_executor`].
//!
//! This module is gated behind `#[cfg(any(test, feature = "mock-adapters"))]`
//! so it never compiles into a default release build. It exists for:
//!
//! * this crate's own unit tests, and
//! * an explicit, opt-in dev/test path (enable the `mock-adapters` feature) for
//!   callers that still want a deterministic fake — analogous to the server's
//!   `COEVO_ENABLE_MOCK_ADAPTERS` switch for the protocol adapters.
//!
//! [`build_mock_executor`] mirrors [`crate::factory::build_executor`]'s
//! signature so a gated caller can swap factories without other changes.

#![cfg(any(test, feature = "mock-adapters"))]

use crate::traits::*;
use async_trait::async_trait;
use coevo_core::lease::EmergencyLease;
use coevo_core::opc::*;

/// A deterministic fake adapter for tests. Returns canned health/dry-run/execute
/// results and performs no IO. The single struct replaces the old per-source
/// `mock_executor!`-generated types; `source_type`/`display_name` come from the
/// passport it is constructed with.
pub struct MockExecutor {
    passport: ExternalExecutorPassport,
}

impl MockExecutor {
    /// Build a mock with a synthetic registered passport for `source_type`.
    pub fn new(source_type: ExecutorSourceType, display_name: &str) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let slug = format!("{source_type:?}").to_lowercase();
        Self {
            passport: ExternalExecutorPassport {
                executor_id: uuid::Uuid::new_v4().to_string(),
                display_name: display_name.to_string(),
                source_type,
                runtime_endpoint: format!("coevo://executor/{slug}/mock"),
                capabilities: vec![
                    format!("executor.{slug}.inspect"),
                    format!("executor.{slug}.execute"),
                ],
                required_credentials: vec![],
                permission_boundary: PermissionBoundary {
                    max_risk_score: 0.5,
                    can_write_fact: false,
                    can_write_decision: false,
                    can_access_network: false,
                    can_access_filesystem: false,
                    can_call_external_executor: false,
                    can_propose_skill: false,
                },
                file_scope: vec![],
                network_scope: vec![],
                memory_scope: MemoryScope::Executor,
                risk_ceiling: 0.5,
                supported_actions: vec!["inspect".to_string(), "execute".to_string()],
                sandbox_level: SandboxLevel::None,
                health_check_url: format!("coevo://executor/{slug}/health"),
                audit_callback_url: format!("coevo://executor/{slug}/audit"),
                status: ExecutorStatus::Registered,
                created_at_ms: now,
                updated_at_ms: now,
            },
        }
    }

    /// Build a mock wrapping a caller-supplied passport (keeps its endpoint etc).
    pub fn from_passport(passport: ExternalExecutorPassport) -> Self {
        Self { passport }
    }
}

#[async_trait]
impl ExternalExecutorAdapter for MockExecutor {
    fn passport(&self) -> &ExternalExecutorPassport {
        &self.passport
    }

    async fn health_check(&self) -> Result<ExecutorHealth, ExecutorError> {
        Ok(ExecutorHealth {
            online: true,
            latency_ms: 1,
            version: format!(
                "{}-mock",
                format!("{:?}", self.passport.source_type).to_lowercase()
            ),
        })
    }

    async fn describe_capabilities(&self) -> Result<Vec<String>, ExecutorError> {
        Ok(self.passport.capabilities.clone())
    }

    async fn dry_run(&self, wo: &WorkOrder) -> Result<DryRunResult, ExecutorError> {
        if wo.track == "red" && self.passport.risk_ceiling < 0.8 {
            return Err(ExecutorError::RiskCeilingExceeded {
                wo: 0.9,
                ceiling: self.passport.risk_ceiling,
            });
        }
        Ok(DryRunResult {
            passed: true,
            estimated_cost_usd: 0.01,
            estimated_duration_ms: 100,
            warnings: vec![],
        })
    }

    async fn execute(
        &self,
        wo: &WorkOrder,
        _lease: Option<&EmergencyLease>,
    ) -> Result<ExecutorResult, ExecutorError> {
        if self.passport.status != ExecutorStatus::Registered {
            return Err(ExecutorError::NotRegistered);
        }
        Ok(ExecutorResult {
            run_id: uuid::Uuid::new_v4().to_string(),
            success: true,
            output: serde_json::json!({
                "mock": true,
                "executor_id": self.passport.executor_id,
                "executor": self.passport.display_name,
                "source_type": self.passport.source_type,
                "work_order_id": wo.work_order_id,
                "track": wo.track,
            }),
            audit_trace: format!(
                "executor:{}:{}:{}",
                format!("{:?}", self.passport.source_type).to_lowercase(),
                self.passport.executor_id,
                uuid::Uuid::new_v4()
            ),
            cost_usd: 0.01,
        })
    }

    async fn cancel(&self, _run_id: &str) -> Result<(), ExecutorError> {
        Ok(())
    }

    async fn fetch_audit(&self, _run_id: &str) -> Result<String, ExecutorError> {
        Ok(format!(
            "audit:{}:{}",
            self.passport.executor_id, self.passport.display_name
        ))
    }
}

/// Mock counterpart to [`crate::factory::build_executor`]: always returns a
/// [`MockExecutor`] regardless of source type. Behind the same gate.
pub fn build_mock_executor(passport: ExternalExecutorPassport) -> Box<dyn ExternalExecutorAdapter> {
    Box::new(MockExecutor::from_passport(passport))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_work_order;

    #[tokio::test]
    async fn mock_executes_with_mock_flag() {
        let exec = MockExecutor::new(ExecutorSourceType::MCP, "MCP Runtime");
        assert!(exec
            .passport()
            .capabilities
            .iter()
            .any(|c| c.contains("executor.mcp")));
        let result = exec.execute(&test_work_order(), None).await.unwrap();
        assert_eq!(result.output["mock"], serde_json::json!(true));
        assert!(result.success);
    }

    #[tokio::test]
    async fn mock_health_is_online() {
        let exec = MockExecutor::new(ExecutorSourceType::Hermes, "Hermes");
        assert!(exec.health_check().await.unwrap().online);
    }
}
