use crate::types::*; use crate::error::WorkerError;
pub struct SkillRuntime;
impl SkillRuntime {
    pub async fn load_skill_index(_pool: &sqlx::SqlitePool, _agent_id: &str) -> Result<Vec<serde_json::Value>,WorkerError> { Ok(vec![]) }
    pub async fn load_full_skill(_pool: &sqlx::SqlitePool, _skill_id: &str) -> Result<serde_json::Value,WorkerError> { Ok(serde_json::json!({})) }
    pub async fn record_usage(_pool: &sqlx::SqlitePool, _r: &SkillUsageRecord) -> Result<(),WorkerError> { Ok(()) }
}
