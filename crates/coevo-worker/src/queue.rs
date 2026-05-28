use crate::types::*; use crate::error::WorkerError;
pub struct WorkerQueueService;
impl WorkerQueueService { pub async fn acquire(_p: &sqlx::SqlitePool, _s: &str, _r: &str, _t: i64) -> Result<(),WorkerError> { Ok(()) } pub async fn release(_p: &sqlx::SqlitePool, _s: &str, _r: &str) -> Result<(),WorkerError> { Ok(()) } }
