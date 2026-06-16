//! Bridges real `coevo-executors` adapters into the worker's
//! [`ExternalAgentAdapter`] trait so registered external executors (Docker,
//! Local process, HTTP runtimes, …) actually run through the governed agent
//! loop instead of reporting "no adapter bound".

use async_trait::async_trait;
use coevo_core::lease::EmergencyLease;
use coevo_core::opc::{ExternalExecutorPassport, WorkOrder};
use coevo_executors::ExternalExecutorAdapter;

use crate::error::WorkerError;
use crate::r#loop::external_agent::{
    ExternalAgentAdapter, ExternalAgentRunResult, ExternalAgentTask,
};

/// Wraps a concrete [`ExternalExecutorAdapter`] (built by
/// `coevo_executors::build_executor_ref`) and exposes it as the worker-side
/// [`ExternalAgentAdapter`]. The bound work order is carried so the executor
/// receives the governed mission context; the per-call task payload proposed by
/// the model is appended to the mission intent.
pub struct BoundExecutorAdapter {
    executor_id: String,
    work_order: WorkOrder,
    lease: Option<EmergencyLease>,
    inner: Box<dyn ExternalExecutorAdapter>,
}

impl BoundExecutorAdapter {
    /// Build a bound adapter from a registered passport and the current work order.
    pub fn new(passport: ExternalExecutorPassport, work_order: WorkOrder) -> Self {
        Self::with_lease(passport, work_order, None)
    }

    /// Same as [`new`](Self::new) but carries an emergency lease (required for
    /// Red-track physical-world executors).
    pub fn with_lease(
        passport: ExternalExecutorPassport,
        work_order: WorkOrder,
        lease: Option<EmergencyLease>,
    ) -> Self {
        let executor_id = passport.executor_id.clone();
        let inner = coevo_executors::build_executor_ref(&passport);
        Self {
            executor_id,
            work_order,
            lease,
            inner,
        }
    }
}

#[async_trait]
impl ExternalAgentAdapter for BoundExecutorAdapter {
    fn executor_id(&self) -> &str {
        &self.executor_id
    }

    async fn run_in_sandbox(
        &self,
        task: ExternalAgentTask,
    ) -> Result<ExternalAgentRunResult, WorkerError> {
        let mut work_order = self.work_order.clone();
        let extra = match &task.task {
            serde_json::Value::Null => String::new(),
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !extra.trim().is_empty() {
            work_order.mission_intent = format!(
                "{}\n\n[executor task]\n{}",
                work_order.mission_intent, extra
            );
        }

        match self.inner.execute(&work_order, self.lease.as_ref()).await {
            Ok(result) => Ok(ExternalAgentRunResult {
                success: result.success,
                output: result.output,
                produced_items: Vec::new(),
                side_effects: Vec::new(),
                egress_log: Vec::new(),
                self_reported_trace: serde_json::json!({
                    "executor_id": self.executor_id,
                    "run_id": result.run_id,
                    "audit_trace": result.audit_trace,
                    "cost_usd": result.cost_usd,
                }),
            }),
            Err(err) => Err(WorkerError::Internal(format!(
                "executor {} failed: {err}",
                self.executor_id
            ))),
        }
    }
}
