use sqlx::{SqlitePool, Row};
use coevo_core::opc::*;

pub struct WorkOrderRepo;
impl WorkOrderRepo {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> WorkOrder {
        let s: String = row.get("status");
        WorkOrder{work_order_id:row.get("work_order_id"),contract_hash:row.get("contract_hash"),plan_hash:row.get("plan_hash"),
            user_id:row.get("user_id"),opc_id:row.get("opc_id"),mission_intent:row.get("mission_intent"),
            selected_agents:serde_json::from_str(row.get("selected_agents_json")).unwrap_or_default(),
            selected_executors:serde_json::from_str(row.get("selected_executors_json")).unwrap_or_default(),
            required_skills:serde_json::from_str(row.get("required_skills_json")).unwrap_or_default(),
            track:row.get("track"),
            status:serde_json::from_str(&format!("\"{}\"",s)).unwrap_or(WorkOrderStatus::Draft),
            allowed_actions:serde_json::from_str(row.get("allowed_actions_json")).unwrap_or_default(),
            restricted_actions:serde_json::from_str(row.get("restricted_actions_json")).unwrap_or_default(),
            risk_summary:row.get("risk_summary"),
            created_at_ms:row.get::<i64,_>("created_at_ms") as u64,updated_at_ms:row.get::<i64,_>("updated_at_ms") as u64}
    }
    pub async fn list(pool: &SqlitePool) -> Result<Vec<WorkOrder>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM work_orders ORDER BY created_at_ms DESC LIMIT 100").fetch_all(pool).await?;
        Ok(rows.iter().map(|r| Self::from_row(r)).collect())
    }
    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<WorkOrder>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM work_orders WHERE work_order_id=?").bind(id).fetch_optional(pool).await?;
        Ok(row.as_ref().map(|r| Self::from_row(r)))
    }
    pub async fn create(pool: &SqlitePool, w: &WorkOrder) -> Result<(), sqlx::Error> {
        if w.contract_hash.is_empty() || w.plan_hash.is_empty() { return Err(sqlx::Error::Protocol("contract_hash/plan_hash required".into())); }
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO work_orders VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&w.work_order_id).bind(&w.contract_hash).bind(&w.plan_hash).bind(&w.user_id).bind(&w.opc_id)
            .bind(&w.mission_intent).bind(serde_json::to_string(&w.selected_agents).unwrap())
            .bind(serde_json::to_string(&w.selected_executors).unwrap()).bind(serde_json::to_string(&w.required_skills).unwrap())
            .bind(&w.track).bind(serde_json::to_string(&w.status).unwrap().trim_matches('"'))
            .bind(serde_json::to_string(&w.allowed_actions).unwrap()).bind(serde_json::to_string(&w.restricted_actions).unwrap())
            .bind(&w.risk_summary).bind(now).bind(now).execute(pool).await?; Ok(())
    }
    pub async fn update_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE work_orders SET status=?,updated_at_ms=? WHERE work_order_id=?")
            .bind(status).bind(chrono::Utc::now().timestamp_millis()).bind(id).execute(pool).await?; Ok(())
    }
}
