use crate::error::WorkerError;
use crate::types::*;
use coevo_store::repos::worker_run_repo::WorkerEventRepo;
pub struct WorkerEventStream;
impl WorkerEventStream {
    pub async fn append(
        p: &sqlx::SqlitePool,
        run_id: &str,
        event_type: WorkerEventType,
        payload: serde_json::Value,
    ) -> Result<(), WorkerError> {
        let event_type = serde_json::to_value(event_type)
            .map_err(|e| WorkerError::Internal(e.to_string()))?
            .as_str()
            .ok_or_else(|| WorkerError::Internal("failed to serialize worker event type".into()))?
            .to_string();
        WorkerEventRepo::append(p, run_id, &event_type, &payload.to_string())
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn append_approval_required(
        p: &sqlx::SqlitePool,
        run_id: &str,
        round: usize,
        reason: &str,
        action_digest: &str,
        source: &str,
    ) -> Result<(), WorkerError> {
        Self::append(
            p,
            run_id,
            WorkerEventType::ApprovalRequired,
            serde_json::json!({
                "round": round,
                "reason": reason,
                "action_digest": action_digest,
                "source": source,
            }),
        )
        .await
    }
}
