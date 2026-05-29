use axum::{extract::{Path, State}, Json, http::StatusCode};
use sqlx::{Row, Column};
use coevo_store::repos::worker_session_repo::WorkerSessionRepo;
use crate::state::AppState;

macro_rules! ok { ($v:expr) => { (StatusCode::OK, Json($v)) } }
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

fn to_json(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<serde_json::Value> {
    rows.iter().map(|r| { let mut m = serde_json::Map::new(); for (i,c) in r.columns().iter().enumerate() { let n = c.name().to_string(); if let Ok(v) = r.try_get::<String,_>(i) { m.insert(n,serde_json::Value::String(v)); } else if let Ok(v) = r.try_get::<i64,_>(i) { m.insert(n,serde_json::Value::Number(v.into())); } } serde_json::Value::Object(m) }).collect()
}

pub async fn timeline(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let mut items: Vec<serde_json::Value> = vec![];
    let now = chrono::Utc::now().timestamp_millis();

    // Load sessions
    if let Ok(sessions) = WorkerSessionRepo::list_by_work_order(&s.pool, &id).await {
        for sess in &sessions {
            let sid: String = sess.get("session_id");
            let st: String = sess.get("status");
            let start: i64 = sess.get("started_at_ms");
            items.push(serde_json::json!({"time_ms":start,"type":"WorkerSessionCreated","title":format!("Session {} created", &sid[..8.min(sid.len())]),"details":{"session_id":sid,"status":st}}));
            // Load steps
            if let Ok(steps) = sqlx::query("SELECT * FROM worker_steps WHERE run_id IN (SELECT run_id FROM worker_runs WHERE work_order_id=?) ORDER BY step_index").bind(&id).fetch_all(&s.pool).await {
                for step in &steps {
                    let tp: String = step.get("step_type");
                    let tm: i64 = step.get("started_at_ms");
                    items.push(serde_json::json!({"time_ms":tm,"type":format!("WorkerStep_{}",tp),"title":tp,"details":{"step_id":step.get::<String,_>("step_id")}}));
                }
            }
        }
    }

    // Sort by time
    items.sort_by_key(|i| i["time_ms"].as_i64().unwrap_or(0));
    ok!(serde_json::json!(items))
}

pub async fn list_worker_sessions(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let rows = sqlx::query("SELECT * FROM worker_sessions ORDER BY started_at_ms DESC LIMIT 50").fetch_all(&s.pool).await.unwrap_or_default();
    ok!(serde_json::json!(to_json(&rows)))
}
pub async fn get_worker_session(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    match WorkerSessionRepo::get(&s.pool, &id).await {
        Ok(Some(r)) => ok!(serde_json::json!(to_json(&[r])[0])),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Session not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn get_session_steps(State(s): State<AppState>, Path(sid): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let rows = sqlx::query("SELECT * FROM worker_run_steps WHERE session_id=? ORDER BY created_at_ms").bind(&sid).fetch_all(&s.pool).await.unwrap_or_default();
    ok!(serde_json::json!(to_json(&rows)))
}
pub async fn get_session_events(State(s): State<AppState>, Path(sid): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let rows = sqlx::query("SELECT * FROM worker_events WHERE session_id=? ORDER BY created_at_ms").bind(&sid).fetch_all(&s.pool).await.unwrap_or_default();
    ok!(serde_json::json!(to_json(&rows)))
}
