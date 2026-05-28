use sqlx::SqlitePool;
use coevo_core::opc::AgentMemory;

pub struct AgentMemoryRepo;
impl AgentMemoryRepo {
    pub async fn get(_pool: &SqlitePool, _agent_id: &str) -> Result<Option<AgentMemory>, sqlx::Error> { Ok(None) }
    pub async fn upsert(_pool: &SqlitePool, _m: &AgentMemory) -> Result<(), sqlx::Error> { Ok(()) }
}
