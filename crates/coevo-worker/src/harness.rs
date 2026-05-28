// Stub modules — complete implementations in coevo-worker v0.2
use crate::types::*; use crate::error::WorkerError;

pub struct WorkerHarness;
impl WorkerHarness {
    pub async fn run_work_order(_pool: &sqlx::SqlitePool, _work_order_id: &str) -> Result<WorkerHarnessResult, WorkerError> {
        Err(WorkerError::Internal("WorkerHarness v0.2 stub — awaiting full harness integration".into()))
    }
}
pub struct WorkerHarnessResult { pub work_order_id: String, pub status: String }
