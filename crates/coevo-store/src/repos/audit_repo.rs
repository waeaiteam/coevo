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

    pub async fn list_by_tenant(
        pool: &SqlitePool,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditEventRow>, sqlx::Error> {
        sqlx::query_as::<_, AuditEventRow>(
            "SELECT * FROM audit_events WHERE tenant_id = ? ORDER BY recorded_at_ms DESC LIMIT ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    pub async fn list_by_tenant_filtered(
        pool: &SqlitePool,
        tenant_id: &str,
        limit: i64,
        work_order_id: Option<&str>,
        run_id: Option<&str>,
    ) -> Result<Vec<AuditEventRow>, sqlx::Error> {
        let mut sql = String::from("SELECT * FROM audit_events WHERE tenant_id = ?");
        if work_order_id.is_some() {
            sql.push_str(" AND json_extract(event_data_json, '$.work_order_id') = ?");
        }
        if run_id.is_some() {
            sql.push_str(" AND json_extract(event_data_json, '$.run_id') = ?");
        }
        sql.push_str(" ORDER BY recorded_at_ms DESC LIMIT ?");

        let mut query = sqlx::query_as::<_, AuditEventRow>(&sql).bind(tenant_id);
        if let Some(value) = work_order_id {
            query = query.bind(value);
        }
        if let Some(value) = run_id {
            query = query.bind(value);
        }
        query.bind(limit).fetch_all(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::AuditRepo;
    use crate::migrate::run_migrations;
    use crate::pool::create_test_pool;

    #[tokio::test]
    async fn list_by_tenant_filtered_filters_on_work_order_and_run_id() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        AuditRepo::insert(
            &pool,
            "worker.governance",
            Some("contract-a"),
            Some("agent-founder-01"),
            None,
            "default-opc",
            &serde_json::json!({
                "work_order_id": "wo-audit-1",
                "run_id": "run-audit-1"
            })
            .to_string(),
        )
        .await
        .unwrap();
        AuditRepo::insert(
            &pool,
            "worker.tool.start",
            Some("contract-b"),
            Some("agent-founder-01"),
            None,
            "default-opc",
            &serde_json::json!({
                "work_order_id": "wo-audit-2",
                "run_id": "run-audit-2"
            })
            .to_string(),
        )
        .await
        .unwrap();

        let rows =
            AuditRepo::list_by_tenant_filtered(&pool, "default-opc", 10, Some("wo-audit-1"), None)
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_type, "worker.governance");

        let rows =
            AuditRepo::list_by_tenant_filtered(&pool, "default-opc", 10, None, Some("run-audit-2"))
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_type, "worker.tool.start");
    }
}
