//! OPC API handlers: Profile, Memory, Employees, Skills, Executors, Work Orders
use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;
use coevo_core::opc::*;
use coevo_store::repos_opc::{user_profile_repo::UserProfileRepo, memory_repo::MemoryRepo, agent_employee_repo::AgentEmployeeRepo};
use crate::state::AppState;

#[derive(Deserialize)] pub struct MemoryQuery { pub scope: Option<String>, pub owner_id: Option<String>, pub include_revoked: Option<bool>, pub q: Option<String> }

// === User Profile ===
pub async fn get_user_profile(State(s): State<AppState>) -> Json<serde_json::Value> {
    let p = UserProfileRepo::get(&s.pool, "default-founder").await.ok().flatten();
    Json(serde_json::to_value(p).unwrap_or(serde_json::json!(null)))
}
pub async fn put_user_profile(State(s): State<AppState>, Json(p): Json<UserProfile>) -> Json<serde_json::Value> {
    UserProfileRepo::upsert(&s.pool, &p).await.ok();
    Json(serde_json::json!({"ok":true}))
}

// === Memory ===
pub async fn list_memory(State(s): State<AppState>, Query(q): Query<MemoryQuery>) -> Json<serde_json::Value> {
    if let Some(ref query) = q.q {
        let items = MemoryRepo::search(&s.pool, query, q.scope.as_deref(), q.owner_id.as_deref()).await.unwrap_or_default();
        return Json(serde_json::to_value(items).unwrap());
    }
    let items = MemoryRepo::list(&s.pool, q.scope.as_deref(), q.owner_id.as_deref(), q.include_revoked.unwrap_or(false)).await.unwrap_or_default();
    Json(serde_json::to_value(items).unwrap())
}
pub async fn create_memory(State(s): State<AppState>, Json(m): Json<MemoryRecord>) -> Json<serde_json::Value> {
    match MemoryRepo::create(&s.pool, &m).await {
        Ok(()) => Json(serde_json::json!({"ok":true})),
        Err(e) => Json(serde_json::json!({"ok":false,"error":e.to_string()})),
    }
}
pub async fn stale_memory(State(s): State<AppState>, Path(id): Path<String>) -> Json<serde_json::Value> {
    MemoryRepo::mark_stale(&s.pool, &id).await.ok(); Json(serde_json::json!({"ok":true}))
}
pub async fn revoke_memory(State(s): State<AppState>, Path(id): Path<String>) -> Json<serde_json::Value> {
    MemoryRepo::revoke(&s.pool, &id).await.ok(); Json(serde_json::json!({"ok":true}))
}

// === Employees ===
pub async fn list_employees(State(s): State<AppState>) -> Json<serde_json::Value> {
    let items = AgentEmployeeRepo::list(&s.pool).await.unwrap_or_default();
    Json(serde_json::to_value(items).unwrap())
}
pub async fn seed_employees_handler(State(s): State<AppState>) -> Json<serde_json::Value> {
    AgentEmployeeRepo::seed(&s.pool).await.ok(); Json(serde_json::json!({"ok":true}))
}

// === Work Orders ===
pub async fn create_work_order(State(s): State<AppState>, Json(wo): Json<WorkOrder>) -> Json<serde_json::Value> {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO work_orders VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&wo.work_order_id).bind(&wo.contract_hash).bind(&wo.plan_hash).bind(&wo.user_id).bind(&wo.opc_id)
        .bind(&wo.mission_intent).bind(serde_json::to_string(&wo.selected_agents).unwrap())
        .bind(serde_json::to_string(&wo.selected_executors).unwrap()).bind(serde_json::to_string(&wo.required_skills).unwrap())
        .bind(&wo.track).bind(serde_json::to_string(&wo.status).unwrap().trim_matches('"')).bind(serde_json::to_string(&wo.allowed_actions).unwrap())
        .bind(serde_json::to_string(&wo.restricted_actions).unwrap()).bind(&wo.risk_summary)
        .bind(now).bind(now)
        .execute(&s.pool).await.ok();
    Json(serde_json::json!({"ok":true,"work_order_id":wo.work_order_id}))
}
pub async fn list_work_orders(State(s): State<AppState>) -> Json<serde_json::Value> {
    let rows: Vec<(String,String,String,String,String,String,String,String,String,String,String,String,String,String,i64,i64)> = sqlx::query_as("SELECT * FROM work_orders ORDER BY created_at_ms DESC LIMIT 50").fetch_all(&s.pool).await.unwrap_or_default();
    let items: Vec<serde_json::Value> = rows.into_iter().map(|r| serde_json::json!({
        "work_order_id":r.0,"contract_hash":r.1,"plan_hash":r.2,"user_id":r.3,"opc_id":r.4,
        "mission_intent":r.5,"selected_agents":serde_json::from_str::<Vec<String>>(&r.6).unwrap_or_default(),
        "selected_executors":serde_json::from_str::<Vec<String>>(&r.7).unwrap_or_default(),
        "required_skills":serde_json::from_str::<Vec<String>>(&r.8).unwrap_or_default(),
        "track":r.9,"status":r.10,
        "allowed_actions":serde_json::from_str::<Vec<String>>(&r.11).unwrap_or_default(),
        "restricted_actions":serde_json::from_str::<Vec<String>>(&r.12).unwrap_or_default(),
        "risk_summary":r.13,"created_at_ms":r.14,"updated_at_ms":r.15
    })).collect();
    Json(serde_json::json!(items))
}
