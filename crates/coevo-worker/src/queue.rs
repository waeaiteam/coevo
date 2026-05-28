use sqlx::{SqlitePool, Row};
use crate::error::WorkerError;
use coevo_store::repos::worker_run_repo::WorkerQueueRepo;
use chrono::Utc;

pub struct WorkerQueueService;
impl WorkerQueueService {
    pub async fn acquire(pool: &SqlitePool, session_id: &str, run_id: &str, ttl_ms: i64) -> Result<(), WorkerError> {
        let now = Utc::now().timestamp_millis();
        if let Some(row) = WorkerQueueRepo::get_lane(pool, session_id).await.map_err(|e| WorkerError::Internal(e.to_string()))? {
            let active: Option<String> = row.get("active_run_id");
            let locked: Option<i64> = row.get("locked_until_ms");
            if active.as_ref().map(|a| !a.is_empty()).unwrap_or(false) && locked.unwrap_or(0) > now {
                return Err(WorkerError::SessionBusy);
            }
        }
        WorkerQueueRepo::acquire(pool, session_id, run_id, ttl_ms).await.map_err(|e| WorkerError::Internal(e.to_string()))
    }
    pub async fn release(pool: &SqlitePool, session_id: &str, run_id: &str) -> Result<(), WorkerError> {
        WorkerQueueRepo::release(pool, session_id, run_id).await.map_err(|e| WorkerError::Internal(e.to_string()))
    }
}
