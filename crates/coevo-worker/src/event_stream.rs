use crate::types::*; use crate::error::WorkerError;
pub struct WorkerEventStream;
impl WorkerEventStream { pub async fn append(_p: &sqlx::SqlitePool, _run_id: &str, _event_type: WorkerEventType, _payload: serde_json::Value) -> Result<(),WorkerError> { Ok(()) } }
