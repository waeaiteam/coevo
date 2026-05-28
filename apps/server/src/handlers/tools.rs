use axum::{extract::{Path, State}, Json, http::StatusCode};
use coevo_worker::tool_registry::ToolRegistry;
use crate::state::AppState;

macro_rules! ok { ($v:expr) => { (StatusCode::OK, Json($v)) } }
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

pub async fn list_tools() -> (StatusCode, Json<serde_json::Value>) {
    let registry = ToolRegistry::default_registry();
    ok!(serde_json::to_value(registry.list()).unwrap())
}
pub async fn get_tool(Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let registry = ToolRegistry::default_registry();
    match registry.list().iter().find(|t| t.tool_id == id) {
        Some(t) => ok!(serde_json::to_value(t).unwrap()),
        None => err!(StatusCode::NOT_FOUND, "Tool not found"),
    }
}
pub async fn tool_health(Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let registry = ToolRegistry::default_registry();
    match registry.execute(&id, serde_json::json!({"action":"health"})).await {
        Ok(_) => ok!(serde_json::json!({"online":true})),
        _ => ok!(serde_json::json!({"online":false})),
    }
}
pub async fn tool_dry_run(Path(id): Path<String>, Json(input): Json<serde_json::Value>) -> (StatusCode, Json<serde_json::Value>) {
    let registry = ToolRegistry::default_registry();
    match registry.get(&id) {
        Some(_) => ok!(serde_json::json!({"dry_run":true,"tool_id":id})),
        None => err!(StatusCode::NOT_FOUND, "Tool not found"),
    }
}
pub async fn tool_execute(Path(id): Path<String>, Json(input): Json<serde_json::Value>) -> (StatusCode, Json<serde_json::Value>) {
    let registry = ToolRegistry::default_registry();
    match registry.execute(&id, input).await {
        Ok(r) => ok!(r),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

// Worker operations
pub async fn assign_worker(State(_s): State<AppState>, Json(body): Json<serde_json::Value>) -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::json!({"ok":true,"worker_id":body["agent_id"].as_str().unwrap_or("default")}))
}
pub async fn run_worker(Path(id): Path<String>, State(_s): State<AppState>, Json(body): Json<serde_json::Value>) -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::json!({"ok":true,"worker_id":id}))
}
pub async fn cancel_worker(Path(id): Path<String>, State(_s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::json!({"ok":true,"worker_id":id,"status":"Cancelled"}))
}
