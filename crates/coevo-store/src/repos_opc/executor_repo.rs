use sqlx::SqlitePool;
use coevo_core::opc::ExternalExecutorPassport;

pub struct ExecutorRepo;
impl ExecutorRepo {
    pub async fn list(_pool: &SqlitePool) -> Result<Vec<ExternalExecutorPassport>, sqlx::Error> { Ok(vec![]) }
    pub async fn register(_pool: &SqlitePool, _p: &ExternalExecutorPassport) -> Result<(), sqlx::Error> { Ok(()) }
    pub async fn disable(_pool: &SqlitePool, _id: &str) -> Result<(), sqlx::Error> { Ok(()) }
}
