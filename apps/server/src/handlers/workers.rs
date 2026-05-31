use axum::{extract::{Path, State}, Json, http::StatusCode};
use sqlx::{Row, Column};
use coevo_store::repos::agent_worker_repo::AgentWorkerRepo;
use coevo_store::repos::worker_run_repo::{WorkerRunRepo, WorkerStepRepo, WorkerEventRepo, WorkerReflectionRepo};
use crate::state::AppState;

macro_rules! ok { ($v:expr) => { (StatusCode::OK, Json($v)) } }
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

fn row_to_json(r: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (i, col) in r.columns().iter().enumerate() {
        let name = col.name().to_string();
        // Try multiple types
        if let Ok(v) = r.try_get::<String, _>(i) { m.insert(name, serde_json::Value::String(v)); continue; }
        if let Ok(v) = r.try_get::<i64, _>(i) { m.insert(name, serde_json::Value::Number(v.into())); continue; }
        if let Ok(v) = r.try_get::<f64, _>(i) { m.insert(name, serde_json::json!(v)); continue; }
        m.insert(name, serde_json::Value::Null);
    }
    serde_json::Value::Object(m)
}

fn rows_to_json(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<serde_json::Value> {
    rows.iter().map(row_to_json).collect()
}

pub async fn list_workers(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    AgentWorkerRepo::list(&s.pool).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |rows| ok!(serde_json::json!(rows_to_json(rows))))
}
pub async fn get_worker(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    match AgentWorkerRepo::get(&s.pool, &id).await {
        Ok(Some(row)) => ok!(row_to_json(&row)),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Worker not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn get_worker_runs(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    WorkerRunRepo::list_by_work_order(&s.pool, &id).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |rows| ok!(serde_json::json!(rows_to_json(rows))))
}
pub async fn get_run(State(s): State<AppState>, Path(run_id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    match WorkerRunRepo::get(&s.pool, &run_id).await {
        Ok(Some(row)) => ok!(row_to_json(&row)),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Run not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn get_run_steps(State(s): State<AppState>, Path(run_id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    WorkerStepRepo::list_by_run(&s.pool, &run_id).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |rows| ok!(serde_json::json!(rows_to_json(rows))))
}
pub async fn get_run_events(State(s): State<AppState>, Path(run_id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    WorkerEventRepo::list_by_run(&s.pool, &run_id).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |rows| ok!(serde_json::json!(rows_to_json(rows))))
}
pub async fn get_run_reflection(State(s): State<AppState>, Path(run_id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    match WorkerReflectionRepo::get_by_run(&s.pool, &run_id).await {
        Ok(Some(row)) => ok!(row_to_json(&row)),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Reflection not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
