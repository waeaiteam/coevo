use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use coevo_store::models::AuditEventRow;
use coevo_store::pool::create_pool;
use coevo_store::repos::audit_repo::AuditRepo;
use coevo_store::repos::worker_session_repo::WorkerSessionRepo;
use coevo_store::repos_opc::{memory_repo, work_order_repo};
use serde::Deserialize;
use sqlx::{Column, Row};

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

const LEGACY_OPC_ID_HEADER: &str = "x-coevo-opc-id";

fn legacy_opc_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get(LEGACY_OPC_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| is_plain_identifier(value))
        .map(ToString::to_string)
}

fn require_legacy_opc_id(
    headers: &HeaderMap,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    legacy_opc_id(headers).ok_or_else(|| {
        err!(
            StatusCode::BAD_REQUEST,
            format!(
                "LEGACY_OPC_ID_REQUIRED: header {LEGACY_OPC_ID_HEADER} is required for legacy /opc/work-orders timeline routes"
            )
        )
    })
}

async fn company_pool(
    state: &AppState,
    opc_id: &str,
) -> Result<sqlx::SqlitePool, (StatusCode, Json<serde_json::Value>)> {
    let company_dir = state.company_workspace.company_dir(opc_id);
    if !company_dir.exists() {
        return Err(err!(StatusCode::NOT_FOUND, "company not found"));
    }
    create_pool(
        &state
            .company_workspace
            .company_db_path(opc_id)
            .to_string_lossy(),
    )
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn load_scoped_work_order(
    state: &AppState,
    headers: &HeaderMap,
    work_order_id: &str,
) -> Result<
    (coevo_core::opc::WorkOrder, Option<sqlx::SqlitePool>),
    (StatusCode, Json<serde_json::Value>),
> {
    let opc_id = require_legacy_opc_id(headers)?;
    let scoped_pool = Some(company_pool(state, &opc_id).await?);
    let pool_ref = scoped_pool.as_ref().unwrap();
    let work_order = match work_order_repo::WorkOrderRepo::get(pool_ref, work_order_id).await {
        Ok(Some(work_order)) => work_order,
        Ok(None) => return Err(err!(StatusCode::NOT_FOUND, "WorkOrder not found")),
        Err(e) => return Err(err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    };
    if work_order.opc_id != opc_id {
        return Err(err!(
            StatusCode::CONFLICT,
            format!(
                "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match stored opc_id={}",
                opc_id, work_order.opc_id
            )
        ));
    }
    Ok((work_order, scoped_pool))
}

async fn company_work_order_ids(
    state: &AppState,
    headers: &HeaderMap,
    route_label: &str,
) -> Result<
    (sqlx::SqlitePool, std::collections::BTreeSet<String>),
    (StatusCode, Json<serde_json::Value>),
> {
    let opc_id = require_legacy_opc_id(headers)?;
    let pool = company_pool(state, &opc_id).await?;
    let work_order_ids = work_order_repo::WorkOrderRepo::list_by_opc(&pool, &opc_id)
        .await
        .map(|items| {
            items
                .into_iter()
                .map(|work_order| work_order.work_order_id)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if work_order_ids.is_empty() && route_label == "/opc/timeline routes" {
        return Ok((pool, work_order_ids));
    }
    Ok((pool, work_order_ids))
}

async fn query_json_scoped(
    pool: &sqlx::SqlitePool,
    sql: &str,
    opc_id: &str,
    id: &str,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    let rows = sqlx::query(sql)
        .bind(opc_id)
        .bind(id)
        .fetch_all(pool)
        .await?;
    Ok(to_json(&rows))
}

#[derive(Debug, Deserialize)]
pub struct AuditEventsQuery {
    pub limit: Option<i64>,
    pub work_order_id: Option<String>,
    pub run_id: Option<String>,
}

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
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let (_, scoped_work_order_pool) = match load_scoped_work_order(&s, &headers, &id).await {
        Ok(result) => result,
        Err(err) => return err,
    };
    if let Some(pool) = scoped_work_order_pool {
        pool.close().await;
    }

    let mut items: Vec<serde_json::Value> = vec![];
    // Load sessions
    if let Ok(sessions) = sqlx::query(
        "SELECT * FROM worker_sessions WHERE opc_id=? AND work_order_id=? ORDER BY created_at_ms",
    )
    .bind(&opc_id)
    .bind(&id)
    .fetch_all(&s.pool)
    .await
    {
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
            if let Ok(runs) = sqlx::query("SELECT run_id, opc_id, status, started_at_ms FROM worker_runs WHERE session_id=? ORDER BY started_at_ms")
                .bind(&sid)
                .fetch_all(&s.pool)
                .await
            {
                for run in &runs {
                    if run.try_get::<String, _>("opc_id").unwrap_or_default() != opc_id {
                        continue;
                    }
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

pub async fn work_order_audit_export(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let (work_order, scoped_work_order_pool) = match load_scoped_work_order(&s, &headers, &id).await
    {
        Ok(result) => result,
        Err(err) => return err,
    };
    let scoped_work_order_pool_ref = scoped_work_order_pool.as_ref().unwrap_or(&s.pool);

    let worker_sessions = query_json_scoped(
        &s.pool,
        "SELECT * FROM worker_sessions WHERE opc_id=? AND work_order_id=? ORDER BY created_at_ms",
        &opc_id,
        &id,
    )
    .await
    .unwrap_or_default();
    let worker_runs = query_json_scoped(
        &s.pool,
        "SELECT * FROM worker_runs WHERE opc_id=? AND work_order_id=? ORDER BY started_at_ms",
        &opc_id,
        &id,
    )
    .await
    .unwrap_or_default();
    let worker_steps = query_json_scoped(
        &s.pool,
        "SELECT ws.* FROM worker_steps ws JOIN worker_runs wr ON ws.run_id=wr.run_id WHERE wr.opc_id=? AND wr.work_order_id=? ORDER BY wr.started_at_ms, ws.step_index",
        &opc_id,
        &id,
    )
    .await
    .unwrap_or_default();
    let worker_events = query_json_scoped(
        &s.pool,
        "SELECT we.* FROM worker_events we JOIN worker_runs wr ON we.run_id=wr.run_id WHERE wr.opc_id=? AND wr.work_order_id=? ORDER BY wr.started_at_ms, we.event_seq",
        &opc_id,
        &id,
    )
    .await
    .unwrap_or_default();
    let tool_calls = query_json_scoped(
        &s.pool,
        "SELECT wtc.* FROM worker_tool_calls wtc JOIN worker_runs wr ON wtc.run_id=wr.run_id WHERE wr.opc_id=? AND wr.work_order_id=? ORDER BY wtc.started_at_ms",
        &opc_id,
        &id,
    )
    .await
    .unwrap_or_default();
    let memory_records =
        memory_repo::MemoryRepo::list(scoped_work_order_pool_ref, Some("Task"), Some(&id), false)
            .await
            .map(|items| serde_json::to_value(items).unwrap_or_else(|_| serde_json::json!([])))
            .unwrap_or_else(|_| serde_json::json!([]));
    let (timeline_status, Json(timeline_items)) =
        timeline(headers, State(s.clone()), Path(id.clone())).await;
    let timeline_items = if timeline_status == StatusCode::OK {
        timeline_items
    } else {
        serde_json::json!([])
    };
    if let Some(pool) = scoped_work_order_pool {
        pool.close().await;
    }

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

pub async fn list_audit_events(
    headers: HeaderMap,
    Query(query): Query<AuditEventsQuery>,
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let header_opc_id = match require_legacy_opc_id(&headers) {
        Ok(value) => value,
        Err(err) => return err,
    };
    if header_opc_id != opc_id {
        return err!(
            StatusCode::CONFLICT,
            format!(
                "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match path opc_id={}",
                header_opc_id, opc_id
            )
        );
    }

    let rows: Vec<AuditEventRow> = match AuditRepo::list_by_tenant_filtered(
        &s.pool,
        &opc_id,
        query.limit.unwrap_or(100).clamp(1, 500),
        query.work_order_id.as_deref(),
        query.run_id.as_deref(),
    )
    .await
    {
        Ok(rows) => rows,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    ok!(serde_json::to_value(rows).unwrap_or_else(|_| serde_json::json!([])))
}

pub async fn list_company_audit_events(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Query(query): Query<AuditEventsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut headers = HeaderMap::new();
    match opc_id.parse() {
        Ok(value) => {
            headers.insert(LEGACY_OPC_ID_HEADER, value);
        }
        Err(_) => {
            return err!(
                StatusCode::BAD_REQUEST,
                format!(
                    "LEGACY_OPC_ID_REQUIRED: header {LEGACY_OPC_ID_HEADER} is required for legacy /opc/work-orders timeline routes"
                )
            );
        }
    }

    list_audit_events(headers, Query(query), State(s), Path(opc_id)).await
}

pub async fn list_worker_sessions(
    headers: HeaderMap,
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let (company_pool, work_order_ids) =
        match company_work_order_ids(&s, &headers, "/opc/workers/sessions routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let rows = WorkerSessionRepo::list_all(&s.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| {
            row.try_get::<String, _>("opc_id").unwrap_or_default() == opc_id
                && work_order_ids.contains(row.get::<String, _>("work_order_id").as_str())
        })
        .collect::<Vec<_>>();
    company_pool.close().await;
    ok!(serde_json::json!(to_json(&rows)))
}

pub async fn global_timeline(
    headers: HeaderMap,
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (company_pool, work_order_ids) =
        match company_work_order_ids(&s, &headers, "/opc/timeline routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let orders = match work_order_repo::WorkOrderRepo::list(&company_pool).await {
        Ok(items) => items
            .into_iter()
            .filter(|order| work_order_ids.contains(&order.work_order_id))
            .collect::<Vec<_>>(),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let mut items: Vec<serde_json::Value> = Vec::new();
    for order in orders {
        items.push(serde_json::json!({
            "time_ms": order.created_at_ms as i64,
            "type": "WorkOrderCreated",
            "title": "Task created",
            "work_order_id": order.work_order_id,
            "track": order.track,
            "status": order.status,
            "mission_intent": order.mission_intent,
            "details": {
                "work_order_id": order.work_order_id,
                "track": order.track,
                "status": order.status,
                "risk_summary": order.risk_summary,
            }
        }));
        if let Ok(sessions) =
            WorkerSessionRepo::list_by_work_order(&s.pool, &order.work_order_id).await
        {
            for session in sessions {
                if session.try_get::<String, _>("opc_id").unwrap_or_default() != order.opc_id {
                    continue;
                }
                let session_id: String = session.get("session_id");
                let status: String = session.get("status");
                let created_at_ms: i64 = session.get("created_at_ms");
                items.push(serde_json::json!({
                    "time_ms": created_at_ms,
                    "type": "WorkerSessionCreated",
                    "title": "Task run started",
                    "work_order_id": order.work_order_id,
                    "track": order.track,
                    "status": status,
                    "mission_intent": order.mission_intent,
                    "details": {
                        "session_id": session_id,
                        "worker_id": session.get::<String, _>("worker_id"),
                        "agent_id": session.get::<String, _>("agent_id"),
                    }
                }));
            }
        }
    }
    items.sort_by(|a, b| {
        b["time_ms"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&a["time_ms"].as_i64().unwrap_or(0))
    });
    company_pool.close().await;
    ok!(serde_json::json!(items))
}
pub async fn get_worker_session(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (company_pool, work_order_ids) =
        match company_work_order_ids(&s, &headers, "/opc/workers/sessions routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let result = match WorkerSessionRepo::get(&s.pool, &id).await {
        Ok(Some(r))
            if r.try_get::<String, _>("opc_id").unwrap_or_default() == opc_id
                && work_order_ids.contains(r.get::<String, _>("work_order_id").as_str()) =>
        {
            ok!(serde_json::json!(to_json(&[r])[0]))
        }
        Ok(None) => err!(StatusCode::NOT_FOUND, "Session not found"),
        Ok(Some(_)) => err!(StatusCode::NOT_FOUND, "Session not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    company_pool.close().await;
    result
}
pub async fn get_session_steps(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(sid): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (company_pool, work_order_ids) =
        match company_work_order_ids(&s, &headers, "/opc/workers/sessions routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let session = match WorkerSessionRepo::get(&s.pool, &sid).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            company_pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Session not found");
        }
        Err(e) => {
            company_pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    if session.try_get::<String, _>("opc_id").unwrap_or_default() != opc_id
        || !work_order_ids.contains(session.get::<String, _>("work_order_id").as_str())
    {
        company_pool.close().await;
        return err!(StatusCode::NOT_FOUND, "Session not found");
    }
    let rows =
        sqlx::query("SELECT * FROM worker_run_steps WHERE session_id=? ORDER BY created_at_ms")
            .bind(&sid)
            .fetch_all(&s.pool)
            .await
            .unwrap_or_default();
    company_pool.close().await;
    ok!(serde_json::json!(to_json(&rows)))
}
pub async fn get_session_events(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(sid): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (company_pool, work_order_ids) =
        match company_work_order_ids(&s, &headers, "/opc/workers/sessions routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let session = match WorkerSessionRepo::get(&s.pool, &sid).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            company_pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Session not found");
        }
        Err(e) => {
            company_pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    if session.try_get::<String, _>("opc_id").unwrap_or_default() != opc_id
        || !work_order_ids.contains(session.get::<String, _>("work_order_id").as_str())
    {
        company_pool.close().await;
        return err!(StatusCode::NOT_FOUND, "Session not found");
    }
    let rows = sqlx::query("SELECT * FROM worker_events WHERE session_id=? ORDER BY created_at_ms")
        .bind(&sid)
        .fetch_all(&s.pool)
        .await
        .unwrap_or_default();
    company_pool.close().await;
    ok!(serde_json::json!(to_json(&rows)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::opc::{create_company, CreateCompanyRequest};
    use crate::state::AppState;
    use axum::http::HeaderValue;
    use coevo_core::opc::{MemoryRecord, MemoryScope, MemoryStatus, WorkOrder, WorkOrderStatus};
    use coevo_store::{
        migrate::run_migrations,
        pool::create_test_pool,
        repos::audit_repo::AuditRepo,
        repos::worker_session_repo::WorkerSessionRepo,
        repos_opc::{memory_repo::MemoryRepo, work_order_repo::WorkOrderRepo},
    };
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn global_timeline_merges_tasks_and_worker_sessions() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-global-timeline-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());

        let (_, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Global Timeline Co".to_string(),
                mission: Some("Validate legacy timeline isolation".to_string()),
            }),
        )
        .await;
        let opc_id = created["opc_id"].as_str().unwrap().to_string();
        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        let work_order = WorkOrder {
            work_order_id: "wo-global-timeline".to_string(),
            conversation_id: Some("conv-global".to_string()),
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "user-1".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Summarize local notes".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: "green".to_string(),
            status: WorkOrderStatus::Completed,
            allowed_actions: vec!["read".to_string(), "analyze".to_string()],
            restricted_actions: vec!["delete".to_string()],
            risk_summary: "Green task".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE work_orders SET created_at_ms=1, updated_at_ms=1 WHERE work_order_id=?",
        )
        .bind("wo-global-timeline")
        .execute(&company_pool)
        .await
        .unwrap();
        company_pool.close().await;
        WorkerSessionRepo::create(
            &pool,
            &opc_id,
            "session-global-timeline",
            "wo-global-timeline",
            "agent-founder-01",
            "worker-agent-founder-01",
            "Completed",
            "[]",
            "[]",
            "[]",
            2,
        )
        .await
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_OPC_ID_HEADER,
            HeaderValue::from_str(&opc_id).expect("valid opc header"),
        );
        let (status, Json(body)) = global_timeline(headers, State(state)).await;
        std::fs::remove_dir_all(root).ok();
        assert_eq!(status, StatusCode::OK);
        let items = body.as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "WorkerSessionCreated");
        assert_eq!(items[0]["work_order_id"], "wo-global-timeline");
        assert_eq!(items[1]["type"], "WorkOrderCreated");
        assert_eq!(items[1]["mission_intent"], "Summarize local notes");
    }

    #[tokio::test]
    async fn legacy_timeline_uses_company_scoped_work_order_with_global_worker_rows() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-timeline-company-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());

        let (_, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Timeline Scope Co".to_string(),
                mission: Some("Validate company-scoped timeline lookup".to_string()),
            }),
        )
        .await;
        let opc_id = created["opc_id"].as_str().unwrap().to_string();
        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        let work_order = WorkOrder {
            work_order_id: "wo-company-timeline".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Summarize company timeline".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Completed,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "company".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: 10,
            updated_at_ms: 10,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;

        WorkerSessionRepo::create(
            &pool,
            &opc_id,
            "session-company-timeline",
            "wo-company-timeline",
            "agent-founder-01",
            "worker-agent-founder-01",
            "Completed",
            "[]",
            "[]",
            "[]",
            20,
        )
        .await
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_OPC_ID_HEADER,
            opc_id.parse().expect("valid opc header"),
        );
        let (status, Json(body)) = timeline(
            headers,
            State(state),
            Path("wo-company-timeline".to_string()),
        )
        .await;

        std::fs::remove_dir_all(root).ok();

        assert_eq!(status, StatusCode::OK, "{body:?}");
        let items = body.as_array().unwrap();
        assert!(items
            .iter()
            .any(|item| item["type"] == "WorkerSessionCreated"));
    }

    #[tokio::test]
    async fn legacy_audit_export_uses_company_scoped_work_order_and_memory() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-audit-export-company-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());

        let (_, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Audit Export Scope Co".to_string(),
                mission: Some("Validate company-scoped audit export lookup".to_string()),
            }),
        )
        .await;
        let opc_id = created["opc_id"].as_str().unwrap().to_string();
        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        let work_order = WorkOrder {
            work_order_id: "wo-company-audit".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Export company audit".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Completed,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "company".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: 10,
            updated_at_ms: 10,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        MemoryRepo::create(
            &company_pool,
            &MemoryRecord {
                memory_id: "mem-company-audit".to_string(),
                scope: MemoryScope::Task,
                owner_id: "wo-company-audit".to_string(),
                title: "Company audit note".to_string(),
                content: "stored in company db".to_string(),
                tags: vec![],
                source: "test".to_string(),
                provenance: "worker-run-company".to_string(),
                confidence: 0.9,
                ttl_seconds: 60,
                created_at_ms: 1,
                updated_at_ms: 1,
                access_policy: "opc-local".to_string(),
                status: MemoryStatus::Active,
                cognitive_layer: coevo_core::cognitive::CognitiveLayer::Suggestion,
                linked_contract_hash: None,
                linked_plan_hash: None,
                linked_adr_id: None,
            },
        )
        .await
        .unwrap();
        company_pool.close().await;

        WorkerSessionRepo::create(
            &pool,
            &opc_id,
            "session-company-audit",
            "wo-company-audit",
            "agent-founder-01",
            "worker-agent-founder-01",
            "Completed",
            "[]",
            "[]",
            "[]",
            20,
        )
        .await
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_OPC_ID_HEADER,
            opc_id.parse().expect("valid opc header"),
        );
        let (status, Json(body)) =
            work_order_audit_export(headers, State(state), Path("wo-company-audit".to_string()))
                .await;

        std::fs::remove_dir_all(root).ok();

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["work_order"]["work_order_id"], "wo-company-audit");
        assert_eq!(body["worker_sessions"].as_array().unwrap().len(), 1);
        assert!(body["memory_records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["memory_id"] == "mem-company-audit"));
    }

    #[tokio::test]
    async fn legacy_session_routes_require_company_scope_and_hide_cross_company_sessions() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-timeline-session-scope-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());

        let (_, Json(alpha_created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Alpha Timeline".to_string(),
                mission: Some("Alpha scope".to_string()),
            }),
        )
        .await;
        let alpha = alpha_created["opc_id"].as_str().unwrap().to_string();
        let (_, Json(beta_created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Beta Timeline".to_string(),
                mission: Some("Beta scope".to_string()),
            }),
        )
        .await;
        let beta = beta_created["opc_id"].as_str().unwrap().to_string();

        let alpha_pool = company_pool(&state, &alpha).await.unwrap();
        let alpha_work_order = WorkOrder {
            work_order_id: "wo-alpha-session".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: alpha.clone(),
            mission_intent: "Alpha session".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Completed,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "alpha".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: 10,
            updated_at_ms: 10,
        };
        WorkOrderRepo::create(&alpha_pool, &alpha_work_order)
            .await
            .unwrap();
        alpha_pool.close().await;

        WorkerSessionRepo::create(
            &pool,
            &alpha,
            "session-alpha-scope",
            "wo-alpha-session",
            "agent-founder-01",
            "worker-agent-founder-01",
            "Completed",
            "[]",
            "[]",
            "[]",
            20,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO worker_run_steps (step_id, session_id, step_type, input_json, output_json, status, created_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("session-step-alpha-scope")
        .bind("session-alpha-scope")
        .bind("SessionStarted")
        .bind("{\"ok\":true}")
        .bind(Option::<String>::None)
        .bind("Completed")
        .bind(21_i64)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO worker_events (event_id, run_id, event_seq, event_type, payload_json, created_at_ms, session_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("event-alpha-scope")
        .bind("run-alpha-scope")
        .bind(0_i64)
        .bind("ReasoningDelta")
        .bind("{\"delta\":\"alpha\"}")
        .bind(22_i64)
        .bind("session-alpha-scope")
        .execute(&pool)
        .await
        .unwrap();

        let no_header = get_worker_session(
            HeaderMap::new(),
            State(state.clone()),
            Path("session-alpha-scope".to_string()),
        )
        .await;
        assert_eq!(no_header.0, StatusCode::BAD_REQUEST);

        let mut beta_headers = HeaderMap::new();
        beta_headers.insert(
            LEGACY_OPC_ID_HEADER,
            beta.parse().expect("valid beta header"),
        );
        let wrong_company = get_worker_session(
            beta_headers.clone(),
            State(state.clone()),
            Path("session-alpha-scope".to_string()),
        )
        .await;
        assert_eq!(wrong_company.0, StatusCode::NOT_FOUND);

        let mut alpha_headers = HeaderMap::new();
        alpha_headers.insert(
            LEGACY_OPC_ID_HEADER,
            alpha.parse().expect("valid alpha header"),
        );
        let right_company = get_worker_session(
            alpha_headers.clone(),
            State(state.clone()),
            Path("session-alpha-scope".to_string()),
        )
        .await;
        assert_eq!(right_company.0, StatusCode::OK);
        assert_eq!(right_company.1["session_id"], "session-alpha-scope");

        let session_steps = get_session_steps(
            alpha_headers.clone(),
            State(state.clone()),
            Path("session-alpha-scope".to_string()),
        )
        .await;
        assert_eq!(session_steps.0, StatusCode::OK);
        assert_eq!(session_steps.1.as_array().unwrap().len(), 1);

        let session_events = get_session_events(
            alpha_headers.clone(),
            State(state.clone()),
            Path("session-alpha-scope".to_string()),
        )
        .await;
        assert_eq!(session_events.0, StatusCode::OK);
        assert_eq!(session_events.1.as_array().unwrap().len(), 1);

        let timeline_items = global_timeline(alpha_headers, State(state)).await;
        assert_eq!(timeline_items.0, StatusCode::OK);
        assert!(timeline_items
            .1
            .as_array()
            .unwrap()
            .iter()
            .all(|item| { item["work_order_id"] == "wo-alpha-session" }));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_timeline_and_audit_export_hide_foreign_rows_when_work_order_ids_collide() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-timeline-collision-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());

        let (_, Json(alpha_created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Alpha Collision".to_string(),
                mission: Some("Alpha".to_string()),
            }),
        )
        .await;
        let alpha = alpha_created["opc_id"].as_str().unwrap().to_string();

        let (_, Json(beta_created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Beta Collision".to_string(),
                mission: Some("Beta".to_string()),
            }),
        )
        .await;
        let beta = beta_created["opc_id"].as_str().unwrap().to_string();

        for opc_id in [&alpha, &beta] {
            let company_pool = company_pool(&state, opc_id).await.unwrap();
            WorkOrderRepo::create(
                &company_pool,
                &WorkOrder {
                    work_order_id: "wo-collision".to_string(),
                    conversation_id: None,
                    contract_hash: "a".repeat(64),
                    plan_hash: "b".repeat(64),
                    user_id: "default-founder".to_string(),
                    opc_id: opc_id.to_string(),
                    mission_intent: format!("{opc_id} mission"),
                    selected_agents: vec!["agent-founder-01".to_string()],
                    selected_executors: vec![],
                    required_skills: vec![],
                    track: "green".to_string(),
                    status: WorkOrderStatus::Completed,
                    allowed_actions: vec!["read".to_string()],
                    restricted_actions: vec![],
                    risk_summary: "collision".to_string(),
                    governance_proposal: None,
                    governance_verdict: None,
                    created_at_ms: 10,
                    updated_at_ms: 10,
                },
            )
            .await
            .unwrap();
            company_pool.close().await;
        }

        WorkerSessionRepo::create(
            &pool,
            &alpha,
            "session-alpha-collision",
            "wo-collision",
            "agent-founder-01",
            "worker-agent-founder-01",
            "Completed",
            "[]",
            "[]",
            "[]",
            20,
        )
        .await
        .unwrap();
        WorkerSessionRepo::create(
            &pool,
            &beta,
            "session-beta-collision",
            "wo-collision",
            "agent-founder-01",
            "worker-agent-founder-01",
            "Completed",
            "[]",
            "[]",
            "[]",
            30,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO worker_events (event_id, run_id, event_seq, event_type, payload_json, created_at_ms, session_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("event-alpha-collision")
        .bind("run-alpha-collision")
        .bind(0_i64)
        .bind("ReasoningDelta")
        .bind("{\"delta\":\"alpha\"}")
        .bind(21_i64)
        .bind("session-alpha-collision")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO worker_events (event_id, run_id, event_seq, event_type, payload_json, created_at_ms, session_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("event-beta-collision")
        .bind("run-beta-collision")
        .bind(0_i64)
        .bind("ReasoningDelta")
        .bind("{\"delta\":\"beta\"}")
        .bind(31_i64)
        .bind("session-beta-collision")
        .execute(&pool)
        .await
        .unwrap();

        let mut alpha_headers = HeaderMap::new();
        alpha_headers.insert(
            LEGACY_OPC_ID_HEADER,
            alpha.parse().expect("valid alpha header"),
        );

        let (timeline_status, Json(timeline_body)) =
            global_timeline(alpha_headers.clone(), State(state.clone())).await;
        assert_eq!(timeline_status, StatusCode::OK);
        let timeline_json = timeline_body.to_string();
        assert!(timeline_json.contains("session-alpha-collision"));
        assert!(!timeline_json.contains("session-beta-collision"));

        let (audit_status, Json(audit_body)) = work_order_audit_export(
            alpha_headers.clone(),
            State(state.clone()),
            Path("wo-collision".to_string()),
        )
        .await;
        assert_eq!(audit_status, StatusCode::OK);
        let audit_json = audit_body.to_string();
        assert!(audit_json.contains("session-alpha-collision"));
        assert!(!audit_json.contains("session-beta-collision"));
        assert!(!audit_json.contains("event-beta-collision"));

        let wrong_session = get_worker_session(
            alpha_headers,
            State(state),
            Path("session-beta-collision".to_string()),
        )
        .await;
        assert_eq!(wrong_session.0, StatusCode::NOT_FOUND);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_timeline_routes_reject_malformed_opc_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-timeline-bad-header-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool, root.clone());
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, "../escape".parse().unwrap());

        let (status, Json(body)) = global_timeline(headers, State(state)).await;

        std::fs::remove_dir_all(root).ok();

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));
    }

    #[tokio::test]
    async fn list_audit_events_returns_recent_rows_for_tenant_and_optional_work_order_filter() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-audit-list-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());

        let (_, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Audit List Co".to_string(),
                mission: Some("Validate audit listing".to_string()),
            }),
        )
        .await;
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        AuditRepo::insert(
            &pool,
            "worker.governance",
            Some("contract-a"),
            Some("agent-founder-01"),
            None,
            &opc_id,
            &serde_json::json!({
                "work_order_id": "wo-audit-1",
                "run_id": "run-audit-1",
                "round": 1
            })
            .to_string(),
        )
        .await
        .unwrap();
        sleep(Duration::from_millis(5)).await;
        AuditRepo::insert(
            &pool,
            "worker.tool.start",
            Some("contract-a"),
            Some("agent-founder-01"),
            None,
            &opc_id,
            &serde_json::json!({
                "work_order_id": "wo-audit-2",
                "run_id": "run-audit-2",
                "round": 2
            })
            .to_string(),
        )
        .await
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_OPC_ID_HEADER,
            HeaderValue::from_str(&opc_id).unwrap(),
        );

        let (status, Json(body)) = list_audit_events(
            headers.clone(),
            axum::extract::Query(AuditEventsQuery {
                limit: Some(1),
                work_order_id: None,
                run_id: None,
            }),
            State(state.clone()),
            Path(opc_id.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let rows = body.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["event_type"], "worker.tool.start");

        let (filtered_status, Json(filtered_body)) = list_audit_events(
            headers,
            axum::extract::Query(AuditEventsQuery {
                limit: Some(10),
                work_order_id: Some("wo-audit-1".to_string()),
                run_id: None,
            }),
            State(state),
            Path(opc_id),
        )
        .await;
        assert_eq!(filtered_status, StatusCode::OK);
        let filtered_rows = filtered_body.as_array().unwrap();
        assert_eq!(filtered_rows.len(), 1);
        assert_eq!(filtered_rows[0]["event_type"], "worker.governance");

        std::fs::remove_dir_all(root).ok();
    }
}
