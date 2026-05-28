use crate::types::*; use crate::error::WorkerError;
pub struct MemoryContextBuilder;
impl MemoryContextBuilder {
    pub async fn build(_pool: &sqlx::SqlitePool) -> Result<MemoryContext,WorkerError> { Ok(MemoryContext{user_profile:None,company_memory:vec![],agent_memory:vec![],task_memory:vec![],relevant_skill_memory:vec![],stale_memory_ids:vec![],excluded_revoked_count:0,context_budget_chars:0}) }
}
