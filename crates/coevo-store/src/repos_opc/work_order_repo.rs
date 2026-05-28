use sqlx::SqlitePool;
use coevo_core::opc::WorkOrder;

pub struct WorkOrderRepo;
impl WorkOrderRepo {
    pub async fn list(_pool: &SqlitePool) -> Result<Vec<WorkOrder>, sqlx::Error> { Ok(vec![]) }
    pub async fn create(_pool: &SqlitePool, _wo: &WorkOrder) -> Result<(), sqlx::Error> { Ok(()) }
}
