use crate::models::LeaseRow;
use coevo_core::lease::EmergencyLease;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct LeaseRepo;

impl LeaseRepo {
    pub async fn insert(pool: &SqlitePool, lease: &EmergencyLease) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO leases (id, lease_id, contract_hash, agent_id, lease_scope_json, lease_budget, operations_used, granted_at_ms, expires_at_ms, ttl_ms, monitoring_signature, diagnostic_signature, is_active, was_revoked) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,1,0)"
        )
        .bind(&id)
        .bind(&lease.lease_id)
        .bind(&lease.contract_hash)
        .bind(&lease.agent_id)
        .bind(serde_json::to_string(&lease.lease_scope).unwrap())
        .bind(lease.lease_budget as i32)
        .bind(lease.operations_used as i32)
        .bind(lease.granted_at_ms as i64)
        .bind(lease.expires_at_ms as i64)
        .bind(lease.ttl_ms as i64)
        .bind(&lease.monitoring_signature)
        .bind(&lease.diagnostic_signature)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_active(
        pool: &SqlitePool,
        lease_id: &str,
    ) -> Result<Option<LeaseRow>, sqlx::Error> {
        sqlx::query_as::<_, LeaseRow>(
            "SELECT * FROM leases WHERE lease_id = ? AND is_active = 1 AND was_revoked = 0",
        )
        .bind(lease_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn consume_operation(pool: &SqlitePool, lease_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE leases SET operations_used = operations_used + 1 WHERE lease_id = ?")
            .bind(lease_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn revoke(pool: &SqlitePool, lease_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE leases SET is_active = 0, was_revoked = 1 WHERE lease_id = ?")
            .bind(lease_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn expire_all(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            "UPDATE leases SET is_active = 0 WHERE is_active = 1 AND expires_at_ms < ?",
        )
        .bind(now)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}
