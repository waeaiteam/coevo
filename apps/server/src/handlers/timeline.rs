use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_store::repos_opc::{memory_repo, work_order_repo};
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
                            let ended_at_ms = step.try_get::<Option<i64>, _>("ended_at_ms").ok().flatten();
                            let input_json = step.try_get::<String, _>("input_json").unwrap_or_else(|_| "{}".to_string());
                            let output_json = step.try_get::<Option<String>, _>("output_json").ok().flatten();
                            let status = step.try_get::<String, _>("status").unwrap_or_else(|_| "Completed".to_string());
                            items.push(serde_json::json!({
                                "time_ms":tm,
                                "type":tp,
                                "title":tp,
                                "details":{
                                    "step_id":step.get::<String,_>("step_id"),
                                    "run_id":&run_id,
                                    "session_id":&sid,
                                    "step_index":step.get::<i64,_>("step_index"),
                                    "status":status,
                                    "started_at_ms":tm,
                                    "ended_at_ms":ended_at_ms,
                                    "input_json":input_json,
                                    "output_json":output_json,
                                }
                            }));
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
                            let payload_json = evt.try_get::<String, _>("payload_json").unwrap_or_else(|_| "{}".to_string());
                            items.push(serde_json::json!({"time_ms":tm,"type":et,"title":et,"details":{"event_id":evt.get::<String,_>("event_id"),"run_id":&run_id,"session_id":&sid,"event_seq":evt.get::<i64,_>("event_seq"),"payload_json":payload_json}}));
                        }
                    }
                }
            }
        }
    }
    items.sort_by_key(|i| i["time_ms"].as_i64().unwrap_or(0));
    ok!(serde_json::json!(items))
}

async fn query_json(
    pool: &sqlx::SqlitePool,
    sql: &str,
    id: &str,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    let rows = sqlx::query(sql).bind(id).fetch_all(pool).await?;
    Ok(to_json(&rows))
}

pub async fn work_order_audit_export(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let work_order = match work_order_repo::WorkOrderRepo::get(&s.pool, &id).await {
        Ok(Some(wo)) => wo,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "WorkOrder not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let worker_sessions = query_json(
        &s.pool,
        "SELECT * FROM worker_sessions WHERE work_order_id=? ORDER BY created_at_ms",
        &id,
    )
    .await
    .unwrap_or_default();
    let worker_runs = query_json(
        &s.pool,
        "SELECT * FROM worker_runs WHERE work_order_id=? ORDER BY started_at_ms",
        &id,
    )
    .await
    .unwrap_or_default();
    let worker_steps = query_json(
        &s.pool,
        "SELECT ws.* FROM worker_steps ws JOIN worker_runs wr ON ws.run_id=wr.run_id WHERE wr.work_order_id=? ORDER BY wr.started_at_ms, ws.step_index",
        &id,
    )
    .await
    .unwrap_or_default();
    let worker_events = query_json(
        &s.pool,
        "SELECT we.* FROM worker_events we JOIN worker_runs wr ON we.run_id=wr.run_id WHERE wr.work_order_id=? ORDER BY wr.started_at_ms, we.event_seq",
        &id,
    )
    .await
    .unwrap_or_default();
    let tool_calls = query_json(
        &s.pool,
        "SELECT wtc.* FROM worker_tool_calls wtc JOIN worker_runs wr ON wtc.run_id=wr.run_id WHERE wr.work_order_id=? ORDER BY wtc.started_at_ms",
        &id,
    )
    .await
    .unwrap_or_default();
    let memory_records = memory_repo::MemoryRepo::list(&s.pool, Some("Task"), Some(&id), false)
        .await
        .map(|items| serde_json::to_value(items).unwrap_or_else(|_| serde_json::json!([])))
        .unwrap_or_else(|_| serde_json::json!([]));
    let (timeline_status, Json(timeline_items)) = timeline(State(s.clone()), Path(id.clone())).await;
    let timeline_items = if timeline_status == StatusCode::OK {
        timeline_items
    } else {
        serde_json::json!([])
    };

    ok!(serde_json::json!({
        "schema_version":"coevo.audit_export.v1",
        "generated_at_ms":chrono::Utc::now().timestamp_millis(),
        "work_order":work_order,
        "governance":{
            "track":work_order.track,
            "status":work_order.status,
            "contract_hash":work_order.contract_hash,
            "plan_hash":work_order.plan_hash,
            "allowed_actions":work_order.allowed_actions,
            "restricted_actions":work_order.restricted_actions,
            "risk_summary":work_order.risk_summary
        },
        "worker_sessions":worker_sessions,
        "worker_runs":worker_runs,
        "worker_steps":worker_steps,
        "worker_events":worker_events,
        "tool_calls":tool_calls,
        "memory_records":memory_records,
        "timeline":timeline_items
    }))
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
