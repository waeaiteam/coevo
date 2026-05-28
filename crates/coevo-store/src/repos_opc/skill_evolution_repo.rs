use sqlx::SqlitePool;
use coevo_core::skills::*;

pub struct SkillEvolutionRepo;
impl SkillEvolutionRepo {
    pub async fn create_proposal(_pool: &SqlitePool, _p: &SkillEvolutionProposal) -> Result<(), sqlx::Error> { Ok(()) }
    pub async fn list(_pool: &SqlitePool) -> Result<Vec<SkillEvolutionProposal>, sqlx::Error> { Ok(vec![]) }
}
