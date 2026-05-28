use crate::types::*; use crate::error::WorkerError;
pub struct SelfUpgradeLoop;
impl SelfUpgradeLoop {
    pub async fn run(_pool: &sqlx::SqlitePool, _run: &WorkerRun) -> Result<serde_json::Value,WorkerError> { Ok(serde_json::json!({"proposal_id":null,"needs_human_review":false})) }
}
