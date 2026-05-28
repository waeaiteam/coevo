use crate::types::*; use crate::error::WorkerError;
pub struct SkillManageRuntime;
impl SkillManageRuntime {
    pub async fn propose(_pool: &sqlx::SqlitePool, _req: &serde_json::Value) -> Result<serde_json::Value,WorkerError> { Ok(serde_json::json!({"proposal_id":"stub"})) }
}
