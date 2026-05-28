use crate::models::ApprovalRequestRow;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ApprovalRepo;

impl ApprovalRepo {
    pub async fn create(
        pool: &SqlitePool,
        contract_hash: &str,
        action_urn: &str,
        approval_mode: &str,
        requested_by: &str,
        timeout_ms: i64,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + timeout_ms;
        sqlx::query(
            "INSERT INTO approval_requests (id, contract_hash, action_urn, approval_mode, status, requested_by, requested_at_ms, expires_at_ms) VALUES (?,?,?,?,?,?,?,?)"
        )
        .bind(&id)
        .bind(contract_hash)
        .bind(action_urn)
        .bind(approval_mode)
        .bind("pending")
        .bind(requested_by)
        .bind(now)
        .bind(expires_at)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_by_id(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<Option<ApprovalRequestRow>, sqlx::Error> {
        sqlx::query_as::<_, ApprovalRequestRow>("SELECT * FROM approval_requests WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn approve(
        pool: &SqlitePool,
        id: &str,
        approved_by: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE approval_requests SET status = 'approved', approved_by = ?, decided_at_ms = ? WHERE id = ?")
            .bind(approved_by)
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn deny(pool: &SqlitePool, id: &str, denied_by: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE approval_requests SET status = 'denied', approved_by = ?, decided_at_ms = ? WHERE id = ?")
            .bind(denied_by)
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn expire_pending(pool: &SqlitePool) -> Result<Vec<ApprovalRequestRow>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let rows = sqlx::query_as::<_, ApprovalRequestRow>(
            "SELECT * FROM approval_requests WHERE status = 'pending' AND expires_at_ms < ?",
        )
        .bind(now)
        .fetch_all(pool)
        .await?;
        for row in &rows {
            sqlx::query("UPDATE approval_requests SET status = 'expired' WHERE id = ?")
                .bind(&row.id)
                .execute(pool)
                .await?;
        }
        Ok(rows)
    }
}
