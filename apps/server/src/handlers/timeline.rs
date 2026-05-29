use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_store::repos::worker_session_repo::WorkerSessionRepo;
use sqlx::{Column, Row};

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

fn to_json(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<serde_json::Value> {
    rows.iter()
        .map(|r| {
            let mut m = serde_json::Map::new();
            for (i, c) in r.columns().iter().enumerate() {
                let n = c.name().to_string();
                if let Ok(v) = r.try_get::<String, _>(i) {
                    m.insert(n, serde_json::Value::String(v));
                } else if let Ok(v) = r.try_get::<i64, _>(i) {
                    m.insert(n, serde_json::Value::Number(v.into()));
                }
            }
            serde_json::Value::Object(m)
        })
        .collect()
}

pub async fn timeline(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Check WorkOrder exists
    let wo_exists = sqlx::query("SELECT 1 FROM work_orders WHERE work_order_id=?")
        .bind(&id)
        .fetch_optional(&s.pool)
        .await
        .ok()
        .flatten()
        .is_some();
    if !wo_exists {
        return err!(StatusCode::NOT_FOUND, "WorkOrder not found");
    }

    let mut items: Vec<serde_json::Value> = vec![];
    // Load sessions
    if let Ok(sessions) = WorkerSessionRepo::list_by_work_order(&s.pool, &id).await {
        for sess in &sessions {
            let sid: String = sess.get("session_id");
            let st: String = sess.get("status");
            let start: i64 = sess.get("created_at_ms");
            items.push(serde_json::json!({"time_ms":start,"type":"WorkerSessionCreated","title":format!("Worker session started"),"details":{"session_id":sid,"status":st}}));
            // Load steps
            if let Ok(steps) = sqlx::query(
                "SELECT * FROM worker_run_steps WHERE session_id=? ORDER BY created_at_ms",
            )
            .bind(&sid)
            .fetch_all(&s.pool)
            .await
            {
                for step in &steps {
                    let tp: String = step.get("step_type");
                    let tm: i64 = step.get("created_at_ms");
                    items.push(serde_json::json!({"time_ms":tm,"type":tp,"title":tp,"details":{"step_id":step.get::<String,_>("step_id"),"session_id":&sid}}));
                }
            }
            // Load events
            if let Ok(evts) =
                sqlx::query("SELECT * FROM worker_events WHERE session_id=? ORDER BY created_at_ms")
                    .bind(&sid)
                    .fetch_all(&s.pool)
                    .await
            {
                for evt in &evts {
                    let et: String = evt.get("event_type");
                    let tm: i64 = evt.get("created_at_ms");
                    items.push(serde_json::json!({"time_ms":tm,"type":et,"title":et,"details":{"event_id":evt.get::<String,_>("event_id"),"session_id":&sid}}));
                }
            }
            if let Ok(runs) = sqlx::query("SELECT run_id, status, started_at_ms FROM worker_runs WHERE session_id=? ORDER BY started_at_ms")
                .bind(&sid)
                .fetch_all(&s.pool)
                .await
            {
                for run in &runs {
                    let run_id: String = run.get("run_id");
                    let status: String = run.get("status");
                    let started: i64 = run.get("started_at_ms");
                    items.push(serde_json::json!({"time_ms":started,"type":"WorkerRunCreated","title":"Worker run started","details":{"run_id":run_id,"session_id":&sid,"status":status}}));
                    if let Ok(steps) = sqlx::query("SELECT * FROM worker_steps WHERE run_id=? ORDER BY step_index")
                        .bind(&run_id)
                        .fetch_all(&s.pool)
                        .await
                    {
                        for step in &steps {
                            let tp: String = step.get("step_type");
                            let tm: i64 = step.get("started_at_ms");
                            items.push(serde_json::json!({"time_ms":tm,"type":tp,"title":tp,"details":{"step_id":step.get::<String,_>("step_id"),"run_id":&run_id,"session_id":&sid}}));
                        }
                    }
                    if let Ok(evts) = sqlx::query("SELECT * FROM worker_events WHERE run_id=? ORDER BY event_seq")
                        .bind(&run_id)
                        .fetch_all(&s.pool)
                        .await
                    {
                        for evt in &evts {
                            let et: String = evt.get("event_type");
                            let tm: i64 = evt.get("created_at_ms");
                            items.push(serde_json::json!({"time_ms":tm,"type":et,"title":et,"details":{"event_id":evt.get::<String,_>("event_id"),"run_id":&run_id,"session_id":&sid}}));
                        }
                    }
                }
            }
        }
    }
    items.sort_by_key(|i| i["time_ms"].as_i64().unwrap_or(0));
    ok!(serde_json::json!(items))
}

pub async fn list_worker_sessions(
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let rows = WorkerSessionRepo::list_all(&s.pool)
        .await
        .unwrap_or_default();
    ok!(serde_json::json!(to_json(&rows)))
}
pub async fn get_worker_session(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match WorkerSessionRepo::get(&s.pool, &id).await {
        Ok(Some(r)) => ok!(serde_json::json!(to_json(&[r])[0])),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Session not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn get_session_steps(
    State(s): State<AppState>,
    Path(sid): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let rows =
        sqlx::query("SELECT * FROM worker_run_steps WHERE session_id=? ORDER BY created_at_ms")
            .bind(&sid)
            .fetch_all(&s.pool)
            .await
            .unwrap_or_default();
    ok!(serde_json::json!(to_json(&rows)))
}
pub async fn get_session_events(
    State(s): State<AppState>,
    Path(sid): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let rows = sqlx::query("SELECT * FROM worker_events WHERE session_id=? ORDER BY created_at_ms")
        .bind(&sid)
        .fetch_all(&s.pool)
        .await
        .unwrap_or_default();
    ok!(serde_json::json!(to_json(&rows)))
}
