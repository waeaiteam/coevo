use sqlx::SqlitePool;
use coevo_core::skills::*;

pub struct SkillRepo;
impl SkillRepo {
    pub async fn list(_pool: &SqlitePool) -> Result<Vec<AgentSkillPackage>, sqlx::Error> { Ok(vec![]) }
    pub async fn upsert(_pool: &SqlitePool, _s: &AgentSkillPackage) -> Result<(), sqlx::Error> { Ok(()) }
}
