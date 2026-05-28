use sqlx::{SqlitePool, Row};
use coevo_core::opc::*;

pub struct ExecutorRepo;
impl ExecutorRepo {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ExternalExecutorPassport, sqlx::Error> {
        let st: String = row.get("source_type"); let sl: String = row.get("sandbox_level");
        let ms: String = row.get("memory_scope"); let s: String = row.get("status");
        fn de<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, sqlx::Error> { serde_json::from_str(s).map_err(|e| sqlx::Error::Protocol(format!("Deserialize failed: {}", e))) }
        Ok(ExternalExecutorPassport{executor_id:row.get("executor_id"),display_name:row.get("display_name"),
            source_type:de(&format!("\"{}\"",st))?,
            runtime_endpoint:row.get("runtime_endpoint"),capabilities:de(row.get("capabilities_json"))?,
            required_credentials:de(row.get("required_credentials_json"))?,
            permission_boundary:de(row.get("permission_boundary_json"))?,
            file_scope:de(row.get("file_scope_json"))?,network_scope:de(row.get("network_scope_json"))?,
            memory_scope:de(&format!("\"{}\"",ms))?,risk_ceiling:row.get("risk_ceiling"),
            supported_actions:de(row.get("supported_actions_json"))?,
            sandbox_level:de(&format!("\"{}\"",sl))?,health_check_url:row.get("health_check_url"),
            audit_callback_url:row.get("audit_callback_url"),status:de(&format!("\"{}\"",s))?,
            created_at_ms:row.get::<i64,_>("created_at_ms") as u64,updated_at_ms:row.get::<i64,_>("updated_at_ms") as u64})
    }
    pub async fn list(pool: &SqlitePool) -> Result<Vec<ExternalExecutorPassport>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM external_executors ORDER BY created_at_ms DESC").fetch_all(pool).await?;
        rows.iter().map(|r| Self::from_row(r)).collect()
    }
    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<ExternalExecutorPassport>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM external_executors WHERE executor_id=?").bind(id).fetch_optional(pool).await?;
        row.as_ref().map(|r| Self::from_row(r)).transpose()
    }
    pub async fn register(pool: &SqlitePool, p: &ExternalExecutorPassport) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR REPLACE INTO external_executors VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&p.executor_id).bind(&p.display_name).bind(serde_json::to_string(&p.source_type).unwrap().trim_matches('"')).bind(&p.runtime_endpoint)
            .bind(serde_json::to_string(&p.capabilities).unwrap()).bind(serde_json::to_string(&p.required_credentials).unwrap())
            .bind(serde_json::to_string(&p.permission_boundary).unwrap()).bind(serde_json::to_string(&p.file_scope).unwrap())
            .bind(serde_json::to_string(&p.network_scope).unwrap()).bind(serde_json::to_string(&p.memory_scope).unwrap().trim_matches('"'))
            .bind(p.risk_ceiling).bind(serde_json::to_string(&p.supported_actions).unwrap())
            .bind(serde_json::to_string(&p.sandbox_level).unwrap().trim_matches('"')).bind(&p.health_check_url).bind(&p.audit_callback_url)
            .bind(serde_json::to_string(&p.status).unwrap().trim_matches('"')).bind(p.created_at_ms as i64).bind(p.updated_at_ms as i64)
            .execute(pool).await?; Ok(())
    }
    pub async fn disable(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE external_executors SET status='Disabled',updated_at_ms=? WHERE executor_id=?").bind(chrono::Utc::now().timestamp_millis()).bind(id).execute(pool).await?; Ok(())
    }
}
