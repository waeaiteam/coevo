use sqlx::SqlitePool;
use coevo_core::opc::OPCProfile;

pub struct OPCProfileRepo;
impl OPCProfileRepo {
    pub async fn get(_pool: &SqlitePool, _opc_id: &str) -> Result<Option<OPCProfile>, sqlx::Error> { Ok(None) }
    pub async fn upsert(_pool: &SqlitePool, _p: &OPCProfile) -> Result<(), sqlx::Error> { Ok(()) }
}
