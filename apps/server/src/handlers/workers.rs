use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use coevo_store::pool::create_pool;
use coevo_store::repos::agent_worker_repo::AgentWorkerRepo;
use coevo_store::repos::worker_run_repo::{
    WorkerEventRepo, WorkerReflectionRepo, WorkerRunRepo, WorkerStepRepo,
};
use sqlx::{Column, Row};
use std::{collections::BTreeSet, convert::Infallible, time::Duration};

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
    route_label: &str,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    legacy_opc_id(headers).ok_or_else(|| {
        err!(
            StatusCode::BAD_REQUEST,
            format!(
                "LEGACY_OPC_ID_REQUIRED: header {LEGACY_OPC_ID_HEADER} is required for legacy {route_label}"
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

async fn company_work_order_ids(
    company_pool: &sqlx::SqlitePool,
) -> Result<BTreeSet<String>, (StatusCode, Json<serde_json::Value>)> {
    coevo_store::repos_opc::work_order_repo::WorkOrderRepo::list(company_pool)
        .await
        .map(|items| {
            items
                .into_iter()
                .map(|work_order| work_order.work_order_id)
                .collect::<BTreeSet<_>>()
        })
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn require_company_work_order_ids(
    state: &AppState,
    headers: &HeaderMap,
    route_label: &str,
) -> Result<(sqlx::SqlitePool, BTreeSet<String>), (StatusCode, Json<serde_json::Value>)> {
    let opc_id = require_legacy_opc_id(headers, route_label)?;
    let company_pool = company_pool(state, &opc_id).await?;
    let work_order_ids = company_work_order_ids(&company_pool).await?;
    Ok((company_pool, work_order_ids))
}

async fn worker_belongs_to_company(
    pool: &sqlx::SqlitePool,
    row: &sqlx::sqlite::SqliteRow,
    opc_id: &str,
    work_order_ids: &BTreeSet<String>,
) -> bool {
    let Some(row_opc_id) = row
        .try_get::<Option<String>, _>("opc_id")
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
    else {
        return false;
    };
    if row_opc_id != opc_id {
        return false;
    }
    if row
        .try_get::<Option<String>, _>("current_work_order_id")
        .ok()
        .flatten()
        .is_some_and(|work_order_id| work_order_ids.contains(&work_order_id))
    {
        return true;
    }
    let Some(session_id) = row
        .try_get::<Option<String>, _>("current_session_id")
        .ok()
        .flatten()
    else {
        return false;
    };
    sqlx::query("SELECT work_order_id, opc_id FROM worker_sessions WHERE session_id=?")
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some_and(|session| {
            let work_order_id = session.try_get::<String, _>("work_order_id").ok();
            let session_opc_id = session
                .try_get::<Option<String>, _>("opc_id")
                .ok()
                .flatten()
                .filter(|value| !value.trim().is_empty());
            work_order_id.is_some_and(|work_order_id| work_order_ids.contains(&work_order_id))
                && session_opc_id.as_deref() == Some(row_opc_id.as_str())
        })
}

async fn require_visible_run(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    work_order_ids: &BTreeSet<String>,
) -> Result<sqlx::sqlite::SqliteRow, (StatusCode, Json<serde_json::Value>)> {
    let Some(row) = WorkerRunRepo::get(pool, run_id)
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    else {
        return Err(err!(StatusCode::NOT_FOUND, "Run not found"));
    };
    let work_order_id = row
        .try_get::<String, _>("work_order_id")
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !work_order_ids.contains(&work_order_id) {
        return Err(err!(StatusCode::NOT_FOUND, "Run not found"));
    }
    Ok(row)
}

fn row_to_json(r: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (i, col) in r.columns().iter().enumerate() {
        let name = col.name().to_string();
        // Try multiple types
        if let Ok(v) = r.try_get::<String, _>(i) {
            m.insert(name, serde_json::Value::String(v));
            continue;
        }
        if let Ok(v) = r.try_get::<i64, _>(i) {
            m.insert(name, serde_json::Value::Number(v.into()));
            continue;
        }
        if let Ok(v) = r.try_get::<f64, _>(i) {
            m.insert(name, serde_json::json!(v));
            continue;
        }
        m.insert(name, serde_json::Value::Null);
    }
    serde_json::Value::Object(m)
}

fn rows_to_json(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<serde_json::Value> {
    rows.iter().map(row_to_json).collect()
}

fn worker_event_parts(row: &sqlx::sqlite::SqliteRow, default_seq: i64) -> (i64, String, String) {
    let seq = row.try_get::<i64, _>("event_seq").unwrap_or(default_seq);
    let event_type = row
        .try_get::<String, _>("event_type")
        .unwrap_or_else(|_| "WorkerEvent".to_string());
    let mut data = row_to_json(row);
    if let Some(payload_json) = data
        .get("payload_json")
        .and_then(|value| value.as_str())
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
    {
        if let Some(object) = data.as_object_mut() {
            object.insert("payload".to_string(), payload_json);
        }
    }
    let data = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    (seq, event_type, data)
}

fn worker_event_to_sse(row: &sqlx::sqlite::SqliteRow, default_seq: i64) -> Event {
    let (seq, event_type, data) = worker_event_parts(row, default_seq);
    Event::default()
        .id(seq.to_string())
        .event(event_type)
        .data(data)
}

async fn next_worker_events(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    last_seq: i64,
) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
    sqlx::query(
        "SELECT * FROM worker_events WHERE run_id=? AND event_seq>? ORDER BY event_seq LIMIT 25",
    )
    .bind(run_id)
    .bind(last_seq)
    .fetch_all(pool)
    .await
}

fn parse_last_event_id(headers: &HeaderMap) -> i64 {
    headers
        .get("Last-Event-ID")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(-1)
}

fn is_terminal_stream_event(event_type: &str) -> bool {
    matches!(event_type, "Done" | "LifecycleEnd")
}

pub async fn list_workers(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/workers routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let (company_pool, work_order_ids) =
        match require_company_work_order_ids(&s, &headers, "/opc/workers routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let rows = match AgentWorkerRepo::list(&s.pool).await {
        Ok(rows) => rows,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let mut visible_rows = Vec::new();
    for row in rows {
        if worker_belongs_to_company(&s.pool, &row, &opc_id, &work_order_ids).await {
            visible_rows.push(row);
        }
    }
    company_pool.close().await;
    ok!(serde_json::json!(rows_to_json(visible_rows)))
}
pub async fn get_worker(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/workers routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let (company_pool, work_order_ids) =
        match require_company_work_order_ids(&s, &headers, "/opc/workers routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let result = match AgentWorkerRepo::get(&s.pool, &id).await {
        Ok(Some(row))
            if worker_belongs_to_company(&s.pool, &row, &opc_id, &work_order_ids).await =>
        {
            ok!(row_to_json(&row))
        }
        Ok(None) => err!(StatusCode::NOT_FOUND, "Worker not found"),
        Ok(Some(_)) => err!(StatusCode::NOT_FOUND, "Worker not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    company_pool.close().await;
    result
}
pub async fn get_worker_runs(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/workers routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let (company_pool, work_order_ids) =
        match require_company_work_order_ids(&s, &headers, "/opc/workers routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let worker = match AgentWorkerRepo::get(&s.pool, &id).await {
        Ok(Some(row))
            if worker_belongs_to_company(&s.pool, &row, &opc_id, &work_order_ids).await =>
        {
            row
        }
        Ok(None) => {
            company_pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Worker not found");
        }
        Ok(Some(_)) => {
            company_pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Worker not found");
        }
        Err(e) => {
            company_pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    let worker_id = worker
        .try_get::<String, _>("worker_id")
        .unwrap_or_else(|_| id.clone());
    let result = WorkerRunRepo::list_by_worker(&s.pool, &worker_id)
        .await
        .map(|rows| {
            rows.into_iter()
                .filter(|row| {
                    row.try_get::<String, _>("work_order_id")
                        .ok()
                        .is_some_and(|work_order_id| work_order_ids.contains(&work_order_id))
                })
                .collect::<Vec<_>>()
        })
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |rows| ok!(serde_json::json!(rows_to_json(rows))),
        );
    company_pool.close().await;
    result
}
pub async fn get_run(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (company_pool, work_order_ids) =
        match require_company_work_order_ids(&s, &headers, "/opc/workers routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let result = match require_visible_run(&s.pool, &run_id, &work_order_ids).await {
        Ok(row) => ok!(row_to_json(&row)),
        Err(err) => err,
    };
    company_pool.close().await;
    result
}
pub async fn get_run_steps(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (company_pool, work_order_ids) =
        match require_company_work_order_ids(&s, &headers, "/opc/workers routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let result = match require_visible_run(&s.pool, &run_id, &work_order_ids).await {
        Ok(_) => WorkerStepRepo::list_by_run(&s.pool, &run_id)
            .await
            .map_or_else(
                |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
                |rows| ok!(serde_json::json!(rows_to_json(rows))),
            ),
        Err(err) => err,
    };
    company_pool.close().await;
    result
}
pub async fn get_run_events(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (company_pool, work_order_ids) =
        match require_company_work_order_ids(&s, &headers, "/opc/workers routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let result = match require_visible_run(&s.pool, &run_id, &work_order_ids).await {
        Ok(_) => WorkerEventRepo::list_by_run(&s.pool, &run_id)
            .await
            .map_or_else(
                |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
                |rows| ok!(serde_json::json!(rows_to_json(rows))),
            ),
        Err(err) => err,
    };
    company_pool.close().await;
    result
}
pub async fn stream_run_events(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<String>,
) -> Result<
    Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<serde_json::Value>),
> {
    let (company_pool, work_order_ids) =
        require_company_work_order_ids(&s, &headers, "/opc/workers routes").await?;
    require_visible_run(&s.pool, &run_id, &work_order_ids).await?;
    company_pool.close().await;
    let pool = s.pool.clone();
    let initial_last_seq = parse_last_event_id(&headers);
    let stream = async_stream::stream! {
        let mut last_seq = initial_last_seq;
        let mut interval = tokio::time::interval(Duration::from_millis(750));
        loop {
            interval.tick().await;
            match next_worker_events(&pool, &run_id, last_seq).await {
                Ok(rows) => {
                    let mut saw_terminal = false;
                    for row in rows {
                        let seq = row.try_get::<i64, _>("event_seq").unwrap_or(last_seq + 1);
                        let event_type = row
                            .try_get::<String, _>("event_type")
                            .unwrap_or_else(|_| "Unknown".to_string());
                        last_seq = seq;
                        yield Ok(worker_event_to_sse(&row, seq));
                        if is_terminal_stream_event(&event_type) {
                            saw_terminal = true;
                            break;
                        }
                    }
                    if saw_terminal {
                        break;
                    }
                }
                Err(err) => {
                    yield Ok(Event::default()
                        .event("Error")
                        .data(serde_json::json!({"error": err.to_string()}).to_string()));
                    break;
                }
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
pub async fn get_run_reflection(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (company_pool, work_order_ids) =
        match require_company_work_order_ids(&s, &headers, "/opc/workers routes").await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let result = match require_visible_run(&s.pool, &run_id, &work_order_ids).await {
        Ok(_) => match WorkerReflectionRepo::get_by_run(&s.pool, &run_id).await {
            Ok(Some(row)) => ok!(row_to_json(&row)),
            Ok(None) => err!(StatusCode::NOT_FOUND, "Reflection not found"),
            Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        },
        Err(err) => err,
    };
    company_pool.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::opc::{
        create_company, create_work_order, CreateCompanyRequest, CreateWORequest,
    };
    use crate::{router::build_router, state::AppState};
    use axum::{
        body::Body,
        http::{header, HeaderMap, Request, StatusCode},
        routing::post,
        Router,
    };
    use coevo_models::openai::OpenAICompatibleGateway;
    use coevo_models::router::default_model_profiles;
    use coevo_models::types::{ModelProviderConfig, ModelProviderKind};
    use coevo_store::repos::worker_run_repo::WorkerReflectionRepo;
    use coevo_store::repos::worker_session_repo::WorkerSessionRepo;
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};
    use coevo_worker::agent_harness::{AgentRunContract, AgentSubHarness, RunAuthorization};
    use coevo_worker::r#loop::SandboxProfile;
    use http_body_util::BodyExt;
    use sqlx::Row;
    use tower::ServiceExt;

    const LEGACY_OPC_ID_HEADER: &str = "x-coevo-opc-id";

    async fn create_company_work_order(
        state: &AppState,
        opc_id: &str,
        work_order_id: &str,
        agent_id: &str,
    ) {
        let req = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            contract_hash: "c".repeat(64),
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.to_string(),
            mission_intent: format!("Mission for {agent_id}"),
            selected_agents: vec![agent_id.to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            conversation_id: None,
            governance_proposal: None,
        };
        let (status, body) = create_work_order(
            HeaderMap::from_iter([(
                LEGACY_OPC_ID_HEADER.parse().unwrap(),
                opc_id.parse().unwrap(),
            )]),
            State(state.clone()),
            Json(req),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
    }

    async fn seed_company_for_worker_visibility(
        state: &AppState,
        opc_id: &str,
        work_order_id: &str,
        agent_id: &str,
        worker_id: &str,
        session_id: &str,
        run_id: &str,
    ) {
        create_company_work_order(state, opc_id, work_order_id, agent_id).await;
        WorkerSessionRepo::create(
            &state.pool,
            opc_id,
            session_id,
            work_order_id,
            agent_id,
            worker_id,
            "Completed",
            "[]",
            "[]",
            "[]",
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        AgentWorkerRepo::upsert(
            &state.pool,
            worker_id,
            opc_id,
            agent_id,
            "Default",
            "Completed",
            Some(work_order_id),
            Some(session_id),
            "[]",
            "Task",
            "[]",
            now,
            now,
        )
        .await
        .unwrap();
        WorkerRunRepo::create(
            &state.pool,
            opc_id,
            run_id,
            work_order_id,
            agent_id,
            worker_id,
            session_id,
            "Completed",
            "{\"ok\":true}",
            "[]",
            "[]",
            None,
            now,
            Some(now + 10),
        )
        .await
        .unwrap();
        WorkerStepRepo::create(
            &state.pool,
            &format!("step-{run_id}"),
            run_id,
            0,
            "ModelCall",
            "{\"prompt\":\"hello\"}",
            Some("{\"output\":\"ok\"}"),
            "Completed",
            now,
            Some(now + 5),
            None,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO worker_run_steps (step_id, session_id, step_type, input_json, output_json, status, created_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("session-step-{run_id}"))
        .bind(session_id)
        .bind("SessionStarted")
        .bind("{\"ok\":true}")
        .bind(Option::<String>::None)
        .bind("Completed")
        .bind(now)
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO worker_events (event_id, run_id, event_seq, event_type, payload_json, created_at_ms, session_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("evt-{run_id}-0"))
        .bind(run_id)
        .bind(0_i64)
        .bind("ReasoningDelta")
        .bind("{\"delta\":\"thinking\"}")
        .bind(now)
        .bind(session_id)
        .execute(&state.pool)
        .await
        .unwrap();
        WorkerReflectionRepo::create(
            &state.pool,
            &format!("reflection-{run_id}"),
            work_order_id,
            run_id,
            agent_id,
            worker_id,
            "Worked",
            "Failed",
            "[]",
            "{}",
            "{}",
            false,
            now,
        )
        .await
        .unwrap();
    }

    async fn create_company_id(state: &AppState, name: &str) -> String {
        let (_, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: name.to_string(),
                mission: Some(format!("Mission for {name}")),
            }),
        )
        .await;
        created["opc_id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn worker_runs_route_uses_worker_id_not_work_order_id() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-worker-runs-route-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let opc_id = create_company_id(&state, "Worker Runs Co").await;
        seed_company_for_worker_visibility(
            &state,
            &opc_id,
            "wo-worker-runs-route",
            "agent-founder-01",
            "worker-runs-route",
            "session-worker-runs-route",
            "run-worker-runs-route",
        )
        .await;

        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, opc_id.parse().unwrap());

        let (status, Json(body)) = get_worker_runs(
            State(state.clone()),
            headers,
            Path("worker-runs-route".to_string()),
        )
        .await;

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(status, StatusCode::OK, "{body:?}");
        let rows = body
            .as_array()
            .expect("worker runs response should be an array");
        assert_eq!(rows.len(), 1, "expected one run for the worker route");
        assert_eq!(
            rows[0]["run_id"],
            serde_json::json!("run-worker-runs-route")
        );
        assert_eq!(rows[0]["worker_id"], serde_json::json!("worker-runs-route"));
    }

    #[tokio::test]
    async fn events_stream_route_responds_with_sse_content_type() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-workers-sse-type-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let opc_id = create_company_id(&state, "SSE Content Type Co").await;
        seed_company_for_worker_visibility(
            &state,
            &opc_id,
            "wo-stream-route",
            "agent-founder-01",
            "worker-stream-route",
            "session-stream-route",
            "run-stream-route",
        )
        .await;

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers/runs/run-stream-route/events/stream")
                    .header(LEGACY_OPC_ID_HEADER, &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn events_stream_route_emits_observable_sse_frame() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-workers-sse-frame-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let opc_id = create_company_id(&state, "SSE Frame Co").await;
        seed_company_for_worker_visibility(
            &state,
            &opc_id,
            "wo-stream-frame",
            "agent-founder-01",
            "worker-stream-frame",
            "session-stream-frame",
            "run-stream-frame",
        )
        .await;

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers/runs/run-stream-frame/events/stream")
                    .header(LEGACY_OPC_ID_HEADER, &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let mut body = response.into_body();
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame())
            .await
            .expect("timed out waiting for SSE frame")
            .expect("SSE stream ended unexpectedly")
            .expect("failed to read SSE frame");
        let bytes = frame.into_data().expect("expected SSE data frame");
        let text = std::str::from_utf8(&bytes).unwrap();

        assert!(text.contains("id: 0"));
        assert!(text.contains("event: ReasoningDelta"));
        assert!(text.contains("data: "));
        assert!(text.contains("thinking"));
        assert!(text.contains("\"payload\":{\"delta\":\"thinking\"}"));
        assert!(text.contains("\n\n"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn worker_event_parts_preserve_streaming_event_names() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO worker_events (event_id, run_id, event_seq, event_type, payload_json, created_at_ms, session_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("evt-stream-map")
        .bind("run-stream-map")
        .bind(7_i64)
        .bind("ToolCallDelta")
        .bind("{\"tool_name\":\"file-readonly\"}")
        .bind(chrono::Utc::now().timestamp_millis())
        .bind("session-stream-map")
        .execute(&pool)
        .await
        .unwrap();

        let row = sqlx::query("SELECT * FROM worker_events WHERE event_id = ?")
            .bind("evt-stream-map")
            .fetch_one(&pool)
            .await
            .unwrap();

        let (seq, event_type, data) = worker_event_parts(&row, 7);

        assert_eq!(seq, 7);
        assert_eq!(event_type, "ToolCallDelta");
        assert!(data.contains("\"payload_json\":\"{"));
        assert!(data.contains("\"payload\":{\"tool_name\":\"file-readonly\"}"));
        assert!(data.contains("file-readonly"));
    }

    #[tokio::test]
    async fn next_worker_events_returns_streaming_rows_in_sequence_order() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        for (seq, event_type, payload) in [
            (0_i64, "ReasoningDelta", "{\"delta\":\"think\"}"),
            (1_i64, "ContentDelta", "{\"delta\":\"answer\"}"),
            (2_i64, "Done", "{\"finish_reason\":\"stop\"}"),
        ] {
            sqlx::query(
                "INSERT INTO worker_events (event_id, run_id, event_seq, event_type, payload_json, created_at_ms, session_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("evt-seq-{seq}"))
            .bind("run-sequenced-stream")
            .bind(seq)
            .bind(event_type)
            .bind(payload)
            .bind(now + seq)
            .bind("session-sequenced-stream")
            .execute(&pool)
            .await
            .unwrap();
        }

        let rows = next_worker_events(&pool, "run-sequenced-stream", -1)
            .await
            .unwrap();
        let seen = rows
            .iter()
            .map(|row| {
                (
                    row.get::<i64, _>("event_seq"),
                    row.get::<String, _>("event_type"),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            seen,
            vec![
                (0, "ReasoningDelta".to_string()),
                (1, "ContentDelta".to_string()),
                (2, "Done".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn next_worker_events_respects_last_seq_cursor() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        for (seq, event_type) in [
            (0_i64, "ReasoningDelta"),
            (1_i64, "ContentDelta"),
            (2_i64, "Done"),
        ] {
            sqlx::query(
                "INSERT INTO worker_events (event_id, run_id, event_seq, event_type, payload_json, created_at_ms, session_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("evt-resume-{seq}"))
            .bind("run-resume-stream")
            .bind(seq)
            .bind(event_type)
            .bind("{}")
            .bind(now + seq)
            .bind("session-resume-stream")
            .execute(&pool)
            .await
            .unwrap();
        }

        let rows = next_worker_events(&pool, "run-resume-stream", 1)
            .await
            .unwrap();
        let seen = rows
            .iter()
            .map(|row| row.get::<i64, _>("event_seq"))
            .collect::<Vec<_>>();
        assert_eq!(seen, vec![2]);
    }

    #[tokio::test]
    async fn next_worker_events_surfaces_query_failures() {
        let pool = create_test_pool().await.unwrap();
        let err = next_worker_events(&pool, "run-without-migrations", -1).await;
        assert!(err.is_err(), "query failures should not be swallowed");
    }

    #[tokio::test]
    async fn worker_stream_events_round_trip_from_gateway_to_sse_route() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        coevo_store::repos_opc::skill_repo::SkillRepo::seed_default(&pool)
            .await
            .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let provider_app = Router::new().route(
            "/v1/chat/completions",
            post(|| async move {
                (
                    [(header::CONTENT_TYPE, "text/event-stream")],
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Need evidence. \"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"thought\\\":\\\"I have enough evidence.\\\",\\\"proposal\\\":{\\\"kind\\\":\\\"finish\\\",\\\"summary\\\":\\\"Streaming route observable.\\\",\\\"result\\\":{\\\"ok\\\":true}},\\\"confidence\\\":0.91}\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n\n",
                        "data: [DONE]\n\n"
                    ),
                )
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, provider_app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        let work_order_id = "wo-worker-stream-sse";
        let run_id = "run-worker-stream-sse";
        let contract = AgentRunContract {
            work_order_id: work_order_id.to_string(),
            mission_intent: "Verify that streamed worker events are visible over the SSE route."
                .to_string(),
            required_skills: vec!["skill-mission-draft".to_string()],
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
        };
        let auth = RunAuthorization {
            work_order_id: work_order_id.to_string(),
            agent_id: "agent-founder-01".to_string(),
            worker_id: "worker-agent-founder-01".to_string(),
            session_id: format!("session-{work_order_id}"),
            run_id: run_id.to_string(),
            track: "green".to_string(),
            allowed_actions: vec!["read".to_string(), "analyze".to_string()],
            restricted_actions: vec![],
            approval_receipt: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            sandbox_profile: SandboxProfile::from_track("green", Some(std::env::temp_dir())),
            model_preference: None,
        };
        let provider_config = ModelProviderConfig {
            provider_id: "stream-test".to_string(),
            kind: ModelProviderKind::DeepSeek,
            base_url: format!("http://{}/v1", addr),
            api_key: "sk-test".to_string(),
            default_model: "deepseek-v4-flash".to_string(),
            fast_model: "deepseek-v4-flash".to_string(),
            reasoning_model: "deepseek-v4-flash".to_string(),
            structured_output_model: "deepseek-v4-flash".to_string(),
            max_tokens: 512,
            temperature: 0.2,
            timeout_ms: 1000,
            max_cost_per_task_usd: 5.0,
        };

        let result = AgentSubHarness::execute(
            &pool,
            &contract,
            &auth,
            &default_model_profiles(),
            None,
            &OpenAICompatibleGateway,
            &provider_config,
            &[],
            &[],
        )
        .await
        .unwrap();
        let _ = shutdown_tx.send(());
        server.await.unwrap();

        assert_eq!(result.final_status, "Completed");
        WorkerSessionRepo::create(
            &pool,
            &contract.opc_id,
            &auth.session_id,
            work_order_id,
            &auth.agent_id,
            &auth.worker_id,
            "Completed",
            "[]",
            "[]",
            "[]",
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .unwrap();
        WorkerRunRepo::create(
            &pool,
            &contract.opc_id,
            run_id,
            work_order_id,
            &auth.agent_id,
            &auth.worker_id,
            &auth.session_id,
            "Completed",
            "{\"ok\":true}",
            "[]",
            "[]",
            None,
            chrono::Utc::now().timestamp_millis(),
            Some(chrono::Utc::now().timestamp_millis()),
        )
        .await
        .unwrap();
        let seen = next_worker_events(&pool, run_id, -1)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.get::<String, _>("event_type"))
            .collect::<Vec<_>>();
        assert!(seen.contains(&"ReasoningDelta".to_string()));
        assert!(seen.contains(&"ContentDelta".to_string()));
        assert!(seen.contains(&"Usage".to_string()));
        assert!(seen.contains(&"Done".to_string()));

        let root =
            std::env::temp_dir().join(format!("coevo-worker-sse-company-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let company_opc_id = create_company_id(&state, "Worker SSE Co").await;
        create_company_work_order(&state, &company_opc_id, work_order_id, "agent-founder-01").await;
        let company_pool = company_pool(&state, &company_opc_id).await.unwrap();
        coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo::seed(&company_pool)
            .await
            .unwrap();
        coevo_store::repos_opc::skill_repo::SkillRepo::seed_default(&company_pool)
            .await
            .unwrap();
        let stored_work_order = coevo_store::repos_opc::work_order_repo::WorkOrderRepo::get(
            &company_pool,
            work_order_id,
        )
        .await
        .unwrap();
        assert!(stored_work_order.is_some());
        company_pool.close().await;

        let headers = HeaderMap::from_iter([(
            LEGACY_OPC_ID_HEADER.parse().unwrap(),
            company_opc_id.parse().unwrap(),
        )]);
        let (_, work_order_ids) =
            require_company_work_order_ids(&state, &headers, "/opc/workers routes")
                .await
                .unwrap();
        assert!(work_order_ids.contains(work_order_id));
        let saved_run = WorkerRunRepo::get(&pool, run_id).await.unwrap().unwrap();
        assert_eq!(
            saved_run.get::<String, _>("work_order_id"),
            work_order_id.to_string()
        );
        let visible_run = require_visible_run(&pool, run_id, &work_order_ids).await;
        assert!(visible_run.is_ok());

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/opc/workers/runs/{run_id}/events/stream"))
                    .header(LEGACY_OPC_ID_HEADER, &company_opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let mut body = response.into_body();
        let mut frames = Vec::new();
        for _ in 0..4 {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame())
                .await
                .expect("timed out waiting for SSE frame")
                .expect("SSE stream ended unexpectedly")
                .expect("failed to read SSE frame");
            let bytes = frame.into_data().expect("expected SSE data frame");
            frames.push(std::str::from_utf8(&bytes).unwrap().to_string());
        }

        assert!(frames
            .iter()
            .any(|text| text.contains("event: ReasoningDelta")));
        assert!(frames.iter().any(|text| text.contains("Need evidence.")));
        assert!(frames
            .iter()
            .any(|text| text.contains("event: ContentDelta")));
        assert!(frames
            .iter()
            .any(|text| text.contains("Streaming route observable.")));
        assert!(frames.iter().any(|text| text.contains("event: Usage")));
        assert!(frames.iter().any(|text| text.contains("event: Done")));
        let end = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame())
            .await
            .expect("timed out waiting for SSE completion");
        assert!(
            end.is_none(),
            "stream should finish after the terminal Done event"
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn events_stream_route_resumes_from_last_event_id() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-workers-sse-resume-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let opc_id = create_company_id(&state, "SSE Resume Co").await;
        seed_company_for_worker_visibility(
            &state,
            &opc_id,
            "wo-stream-resume",
            "agent-founder-01",
            "worker-stream-resume",
            "session-stream-resume",
            "run-stream-resume",
        )
        .await;
        let now = chrono::Utc::now().timestamp_millis();
        for (seq, event_type, payload) in [
            (1_i64, "ContentDelta", "{\"delta\":\"answer\"}"),
            (2_i64, "Done", "{\"finish_reason\":\"stop\"}"),
        ] {
            sqlx::query(
                "INSERT INTO worker_events (event_id, run_id, event_seq, event_type, payload_json, created_at_ms, session_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("evt-stream-resume-{seq}"))
            .bind("run-stream-resume")
            .bind(seq)
            .bind(event_type)
            .bind(payload)
            .bind(now + seq)
            .bind("session-stream-resume")
            .execute(&pool)
            .await
            .unwrap();
        }

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers/runs/run-stream-resume/events/stream")
                    .header(LEGACY_OPC_ID_HEADER, &opc_id)
                    .header("Last-Event-ID", "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let mut body = response.into_body();
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame())
            .await
            .expect("timed out waiting for resumed SSE frame")
            .expect("SSE stream ended unexpectedly")
            .expect("failed to read SSE frame");
        let bytes = frame.into_data().expect("expected SSE data frame");
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("id: 2"));
        assert!(text.contains("event: Done"));
        assert!(!text.contains("id: 0"));
        assert!(!text.contains("id: 1"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_worker_routes_require_header_and_hide_cross_company_runs() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-workers-company-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());

        let alpha = create_company_id(&state, "Alpha Workers").await;
        let beta = create_company_id(&state, "Beta Workers").await;
        seed_company_for_worker_visibility(
            &state,
            &alpha,
            "wo-alpha-workers",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-wo-alpha-workers",
            "run-alpha-workers",
        )
        .await;

        let app = build_router(state.clone());

        let no_header = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers/runs/run-alpha-workers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(no_header.status(), StatusCode::BAD_REQUEST);
        let no_header_body = axum::body::to_bytes(no_header.into_body(), usize::MAX)
            .await
            .unwrap();
        let no_header_json: serde_json::Value = serde_json::from_slice(&no_header_body).unwrap();
        assert!(no_header_json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let wrong_company = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers/runs/run-alpha-workers")
                    .header(LEGACY_OPC_ID_HEADER, &beta)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong_company.status(), StatusCode::NOT_FOUND);

        let right_company = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers/runs/run-alpha-workers")
                    .header(LEGACY_OPC_ID_HEADER, &alpha)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(right_company.status(), StatusCode::OK);
        let right_company_body = axum::body::to_bytes(right_company.into_body(), usize::MAX)
            .await
            .unwrap();
        let right_company_json: serde_json::Value =
            serde_json::from_slice(&right_company_body).unwrap();
        assert_eq!(right_company_json["run_id"], "run-alpha-workers");

        let worker_list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers")
                    .header(LEGACY_OPC_ID_HEADER, &alpha)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(worker_list.status(), StatusCode::OK);
        let worker_list_body = axum::body::to_bytes(worker_list.into_body(), usize::MAX)
            .await
            .unwrap();
        let worker_list_json: serde_json::Value =
            serde_json::from_slice(&worker_list_body).unwrap();
        assert_eq!(worker_list_json.as_array().unwrap().len(), 1);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_worker_routes_reject_runtime_rows_with_mismatched_opc_id() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-workers-opc-mismatch-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());

        let alpha = create_company_id(&state, "Alpha Workers Opc").await;
        let beta = create_company_id(&state, "Beta Workers Opc").await;
        seed_company_for_worker_visibility(
            &state,
            &alpha,
            "wo-alpha-opc-mismatch",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-wo-alpha-opc-mismatch",
            "run-alpha-opc-mismatch",
        )
        .await;

        sqlx::query("UPDATE agent_workers SET opc_id = ? WHERE worker_id = ?")
            .bind(&beta)
            .bind("worker-agent-founder-01")
            .execute(&pool)
            .await
            .unwrap();

        let app = build_router(state.clone());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers/worker-agent-founder-01")
                    .header(LEGACY_OPC_ID_HEADER, &alpha)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let runs_response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/workers/worker-agent-founder-01/runs")
                    .header(LEGACY_OPC_ID_HEADER, &alpha)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(runs_response.status(), StatusCode::NOT_FOUND);

        std::fs::remove_dir_all(root).ok();
    }
}
