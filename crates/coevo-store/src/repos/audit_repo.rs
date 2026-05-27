use crate::models::AuditEventRow;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct AuditRepo;

impl AuditRepo {
    pub async fn insert(
        pool: &SqlitePool,
        event_type: &str,
        contract_hash: Option<&str>,
        agent_id: Option<&str>,
        traceparent: Option<&str>,
        tenant_id: &str,
        event_data_json: &str,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO audit_events (id, event_type, contract_hash, agent_id, traceparent, tenant_id, event_data_json, recorded_at_ms) VALUES (?,?,?,?,?,?,?,?)"
        )
        .bind(&id)
        .bind(event_type)
        .bind(contract_hash)
        .bind(agent_id)
        .bind(traceparent)
        .bind(tenant_id)
        .bind(event_data_json)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn list_by_tenant(pool: &SqlitePool, tenant_id: &str, limit: i64) -> Result<Vec<AuditEventRow>, sqlx::Error> {
        sqlx::query_as::<_, AuditEventRow>(
            "SELECT * FROM audit_events WHERE tenant_id = ? ORDER BY recorded_at_ms DESC LIMIT ?"
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}
