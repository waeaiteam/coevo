use axum::{extract::{Path, State}, Json, http::StatusCode};
use sqlx::Row;
use serde::Deserialize;
use coevo_worker::tool_registry::ToolRegistry;
use coevo_worker::tool_policy::ToolPolicyEngine;
use coevo_store::repos_opc::work_order_repo::WorkOrderRepo;
use coevo_store::repos::agent_worker_repo::AgentWorkerRepo;
use coevo_worker::harness::{WorkerHarness, WorkerHarnessOptions};
use coevo_worker::queue::WorkerQueueService;
use coevo_store::repos::worker_run_repo::WorkerRunRepo;
use crate::state::AppState;

macro_rules! ok { ($v:expr) => { (StatusCode::OK, Json($v)) } }
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

#[derive(Deserialize)] pub struct ToolExecReq { pub work_order_id: String, pub input: serde_json::Value }
#[derive(Deserialize)] pub struct AssignReq { pub agent_id: String, pub work_order_id: Option<String> }
#[derive(Deserialize)] pub struct RunReq { pub work_order_id: String }

pub async fn list_tools() -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::to_value(ToolRegistry::default_registry().list()).unwrap())
}
pub async fn get_tool(Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let r = ToolRegistry::default_registry();
    match r.list().iter().find(|t| t.tool_id == id) {
        Some(t) => ok!(serde_json::to_value(t).unwrap()),
        None => err!(StatusCode::NOT_FOUND, "Tool not found"),
    }
}
pub async fn tool_health(Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let r = ToolRegistry::default_registry();
    match r.execute(&id, serde_json::json!({"action":"health"})).await {
        Ok(_) => ok!(serde_json::json!({"online":true})),
        _ => ok!(serde_json::json!({"online":false})),
    }
}
pub async fn tool_dry_run(Path(id): Path<String>, Json(_input): Json<serde_json::Value>) -> (StatusCode, Json<serde_json::Value>) {
    let r = ToolRegistry::default_registry();
    match r.get(&id) {
        Some(_) => ok!(serde_json::json!({"dry_run":true,"tool_id":id})),
        None => err!(StatusCode::NOT_FOUND, "Tool not found"),
    }
}
pub async fn tool_execute(State(s): State<AppState>, Path(id): Path<String>, Json(req): Json<ToolExecReq>) -> (StatusCode, Json<serde_json::Value>) {
    let wo = match WorkOrderRepo::get(&s.pool, &req.work_order_id).await {
        Ok(Some(w)) => w, _ => return err!(StatusCode::BAD_REQUEST, "WorkOrder not found"),
    };
    let r = ToolRegistry::default_registry();
    let tool = match r.list().iter().find(|t| t.tool_id == id) {
        Some(t) => t, None => return err!(StatusCode::NOT_FOUND, "Tool not found"),
    };
    let decision = ToolPolicyEngine::evaluate(tool, &wo.track, &wo.allowed_actions, &wo.restricted_actions);
    if !decision.allowed { return err!(StatusCode::FORBIDDEN, format!("ToolDeniedByPolicy: {}", decision.reason)); }
    match r.execute(&id, req.input).await {
        Ok(result) => ok!(result),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}
// Worker operations — real implementations
pub async fn assign_worker(State(s): State<AppState>, Json(body): Json<AssignReq>) -> (StatusCode, Json<serde_json::Value>) {
    let now = chrono::Utc::now().timestamp_millis();
    let wid = format!("worker-{}", body.agent_id);
    if let Err(e) = AgentWorkerRepo::upsert(&s.pool, &wid, &body.agent_id, "Default", "Assigned", body.work_order_id.as_deref(), None, "[]", "Task", "[]", now, now).await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    match AgentWorkerRepo::get(&s.pool, &wid).await {
        Ok(Some(row)) => ok!(serde_json::json!({"worker_id":row.get::<String,_>("worker_id"),"agent_id":row.get::<String,_>("agent_id"),"status":row.get::<String,_>("status")})),
        _ => ok!(serde_json::json!({"worker_id":wid}))
    }
}
pub async fn run_worker(State(s): State<AppState>, Path(_id): Path<String>, Json(body): Json<RunReq>) -> (StatusCode, Json<serde_json::Value>) {
    match WorkerHarness::run_work_order(&s.pool, &body.work_order_id, WorkerHarnessOptions{approval_receipt:None,max_runtime_ms:None,deterministic_mode:true,preferred_tool_ids:vec![],allow_mock_model_routing:false}).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("MODEL_PROVIDER_NOT_CONFIGURED") {
                err!(StatusCode::CONFLICT, msg)
            } else {
                err!(StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
        }
    }
}
pub async fn cancel_worker(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let mut cancelled_run = None;
    let mut cancelled_session = None;
    let mut queue_released = false;
    if let Ok(Some(row)) = AgentWorkerRepo::get(&s.pool, &id).await {
        let sid: Option<String> = row.get("current_session_id");
        if let Some(ref session_id) = sid {
            // Get active_run_id from queue lane
            let active_run_id: Option<String> = sqlx::query("SELECT active_run_id FROM worker_queue_lanes WHERE session_id=?")
                .bind(session_id).fetch_optional(&s.pool).await.ok().flatten()
                .and_then(|r| r.get::<Option<String>,_>("active_run_id"));
            if let Some(ref run_id) = active_run_id {
                WorkerRunRepo::set_status(&s.pool, run_id, "Cancelled").await.ok();
                cancelled_run = Some(run_id.clone());
                // Release queue
                WorkerQueueService::release(&s.pool, session_id, run_id).await.ok();
                queue_released = true;
                // Emit cancel event
                let _ = sqlx::query("INSERT INTO worker_events VALUES (?,?,?,?,?,?)")
                    .bind(format!("ev-{}-cancel", run_id)).bind(run_id).bind(999i64)
                    .bind("LifecycleEnd").bind(serde_json::to_string(&serde_json::json!({"reason":"CancelledByUser"})).unwrap())
                    .bind(chrono::Utc::now().timestamp_millis()).execute(&s.pool);
            }
            // Update session
            sqlx::query("UPDATE worker_sessions SET status='Cancelled',updated_at_ms=? WHERE session_id=?")
                .bind(chrono::Utc::now().timestamp_millis()).bind(session_id).execute(&s.pool).await.ok();
            cancelled_session = Some(session_id.clone());
        }
    }
    AgentWorkerRepo::set_status(&s.pool, &id, "Cancelled").await.map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())).ok();
    ok!(serde_json::json!({"worker_id":id,"status":"Cancelled","cancelled_run_id":cancelled_run,"session_id":cancelled_session,"queue_released":queue_released}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::extract::Path;
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
    use coevo_store::migrate::run_migrations;
    use coevo_store::pool::create_test_pool;
    use coevo_store::repos_opc::work_order_repo::WorkOrderRepo;

    #[tokio::test]
    async fn run_worker_rejects_public_mock_routing_when_no_model_provider() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let wo = WorkOrder {
            work_order_id: "wo-run-worker-provider-required".to_string(),
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Analyze README".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec!["delete".to_string()],
            risk_summary: "test".to_string(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&pool, &wo).await.unwrap();

        let (status, Json(body)) = run_worker(
            State(state),
            Path("worker-agent-founder-01".to_string()),
            Json(RunReq {
                work_order_id: wo.work_order_id.clone(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("MODEL_PROVIDER_NOT_CONFIGURED"));
    }
}
