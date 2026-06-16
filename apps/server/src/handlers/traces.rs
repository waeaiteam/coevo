use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_store::pool::create_pool;
use sqlx::Row;

use crate::state::AppState;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

fn step_kind(step_type: &str) -> &'static str {
    match step_type {
        "Think" => "plan",
        "ModelCall" => "model_call",
        "CallTool" | "SelectTool" | "CallExecutor" => "tool_call",
        "AskHuman" => "governance",
        "Reflect" => "reflection",
        _ => "mission",
    }
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
    state: &AppState,
    opc_id: &str,
) -> Result<Vec<String>, (StatusCode, Json<serde_json::Value>)> {
    let company_db = company_pool(state, opc_id).await?;
    let company_rows =
        sqlx::query("SELECT work_order_id FROM work_orders ORDER BY created_at_ms DESC")
            .fetch_all(&company_db)
            .await
            .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    company_db.close().await;

    let global_rows = sqlx::query(
        "SELECT work_order_id FROM work_orders WHERE opc_id = ? ORDER BY created_at_ms DESC",
    )
    .bind(opc_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in company_rows.into_iter().chain(global_rows.into_iter()) {
        let work_order_id: String = row.get("work_order_id");
        if seen.insert(work_order_id.clone()) {
            ids.push(work_order_id);
        }
    }
    Ok(ids)
}

pub async fn list_company_traces(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let work_order_ids = match company_work_order_ids(&s, &opc_id).await {
        Ok(ids) => ids,
        Err(err) => return err,
    };
    let mut items = Vec::new();
    for work_order_id in work_order_ids {
        let rows = match sqlx::query(
            "SELECT run_id, work_order_id, agent_id, status, started_at_ms, ended_at_ms, total_tokens, total_cost_usd
             FROM worker_runs
             WHERE opc_id = ? AND work_order_id = ?
             ORDER BY started_at_ms DESC",
        )
        .bind(&opc_id)
        .bind(&work_order_id)
        .fetch_all(&s.pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        items.extend(rows.into_iter().map(|row| {
            serde_json::json!({
                "trace_id": row.get::<String, _>("run_id"),
                "work_order_id": row.get::<String, _>("work_order_id"),
                "agent_id": row.get::<String, _>("agent_id"),
                "status": row.get::<String, _>("status").to_lowercase(),
                "started_at_ms": row.get::<i64, _>("started_at_ms"),
                "ended_at_ms": row.try_get::<Option<i64>, _>("ended_at_ms").ok().flatten(),
                "total_tokens": row.try_get::<Option<i64>, _>("total_tokens").ok().flatten().unwrap_or(0),
                "total_cost_usd": row.try_get::<Option<f64>, _>("total_cost_usd").ok().flatten().unwrap_or(0.0),
            })
        }));
    }
    items.sort_by_key(|item| std::cmp::Reverse(item["started_at_ms"].as_i64().unwrap_or_default()));
    ok!(serde_json::Value::Array(items))
}

pub async fn get_company_trace_spans(
    State(s): State<AppState>,
    Path((opc_id, trace_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let run = match sqlx::query("SELECT * FROM worker_runs WHERE run_id = ? AND opc_id = ?")
        .bind(&trace_id)
        .bind(&opc_id)
        .fetch_optional(&s.pool)
        .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "Trace not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let steps = match sqlx::query("SELECT * FROM worker_steps WHERE run_id=? ORDER BY step_index")
        .bind(&trace_id)
        .fetch_all(&s.pool)
        .await
    {
        Ok(rows) => rows,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let tool_calls =
        match sqlx::query("SELECT * FROM worker_tool_calls WHERE run_id=? ORDER BY started_at_ms")
            .bind(&trace_id)
            .fetch_all(&s.pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };

    let root_span_id = format!("run:{trace_id}");
    let mut spans = Vec::new();
    for step in &steps {
        let step_id: String = step.get("step_id");
        spans.push(serde_json::json!({
            "span_id": step_id,
            "parent_span_id": root_span_id,
            "name": step.get::<String, _>("step_type"),
            "kind": step_kind(&step.get::<String, _>("step_type")),
            "status": step.get::<String, _>("status").to_lowercase(),
            "input": step.try_get::<String, _>("input_json").unwrap_or_else(|_| "{}".to_string()),
            "output": step.try_get::<Option<String>, _>("output_json").ok().flatten().unwrap_or_default(),
            "tokens": 0,
            "cost_usd": 0.0,
            "started_at_ms": step.get::<i64, _>("started_at_ms"),
            "ended_at_ms": step.try_get::<Option<i64>, _>("ended_at_ms").ok().flatten(),
        }));
    }
    for call in &tool_calls {
        spans.push(serde_json::json!({
            "span_id": call.get::<String, _>("tool_call_id"),
            "parent_span_id": root_span_id,
            "name": call.get::<String, _>("tool_id"),
            "kind": "tool_call",
            "status": if call.get::<i64, _>("success") == 1 { "ok" } else { "error" },
            "input": call.get::<String, _>("input_summary"),
            "output": call.get::<String, _>("output_summary"),
            "tokens": 0,
            "cost_usd": 0.0,
            "started_at_ms": call.get::<i64, _>("started_at_ms"),
            "ended_at_ms": call.try_get::<Option<i64>, _>("ended_at_ms").ok().flatten(),
        }));
    }

    ok!(serde_json::json!({
        "trace_id": trace_id,
        "work_order_id": run.get::<String, _>("work_order_id"),
        "agent_id": run.get::<String, _>("agent_id"),
        "status": run.get::<String, _>("status").to_lowercase(),
        "started_at_ms": run.get::<i64, _>("started_at_ms"),
        "ended_at_ms": run.try_get::<Option<i64>, _>("ended_at_ms").ok().flatten(),
        "total_tokens": run.try_get::<Option<i64>, _>("total_tokens").ok().flatten().unwrap_or(0),
        "total_cost_usd": run.try_get::<Option<f64>, _>("total_cost_usd").ok().flatten().unwrap_or(0.0),
        "spans": spans
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{router::build_router, state::AppState};
    use axum::{body::Body, http::Request};
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
    use coevo_store::{
        migrate::run_migrations,
        pool::{create_pool, create_test_pool},
        repos::worker_run_repo::{
            WorkerEventRepo, WorkerRunRepo, WorkerStepRepo, WorkerToolCallRepo,
        },
        repos_opc::work_order_repo::WorkOrderRepo,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn company_trace_endpoints_return_real_run_tree() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-trace-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool.clone(), root.clone()));

        let create_company = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Trace Co","mission":"Trace tests"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let company: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_company.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company["opc_id"].as_str().unwrap().to_string();

        let now = chrono::Utc::now().timestamp_millis() as u64;
        WorkOrderRepo::create(
            &pool,
            &WorkOrder {
                work_order_id: "wo-trace".to_string(),
                conversation_id: None,
                contract_hash: "a".repeat(64),
                plan_hash: "b".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: opc_id.clone(),
                mission_intent: "Trace replay".to_string(),
                selected_agents: vec!["agent-founder-01".to_string()],
                selected_executors: vec![],
                required_skills: vec![],
                track: "green".to_string(),
                status: WorkOrderStatus::Completed,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec!["delete".to_string()],
                risk_summary: "trace".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .await
        .unwrap();
        WorkerRunRepo::create(
            &pool,
            &opc_id,
            "run-trace",
            "wo-trace",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-trace",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            now as i64,
            Some(now as i64 + 10),
        )
        .await
        .unwrap();
        WorkerStepRepo::create(
            &pool,
            "step-trace",
            "run-trace",
            0,
            "ModelCall",
            "{\"input\":\"hello\"}",
            Some("{\"output\":\"world\"}"),
            "Completed",
            now as i64,
            Some(now as i64 + 5),
            None,
        )
        .await
        .unwrap();
        WorkerEventRepo::append(&pool, "run-trace", "ToolEnd", "{\"ok\":true}")
            .await
            .unwrap();
        WorkerToolCallRepo::create(
            &pool,
            "tool-trace",
            "run-trace",
            "file-readonly",
            "FileReadonly",
            "input",
            "output",
            true,
            0.3,
            None,
            now as i64,
            Some(now as i64 + 7),
        )
        .await
        .unwrap();

        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/traces"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);

        let detail = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/traces/run-trace/spans"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(detail.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["trace_id"], "run-trace");
        assert!(body["spans"].as_array().unwrap().len() >= 2);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_trace_endpoints_accept_company_local_work_order() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-trace-local-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool.clone(), root.clone()));

        let create_company = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Trace Local Co","mission":"Trace local tests"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let company: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_company.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company["opc_id"].as_str().unwrap().to_string();

        let company_pool = create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
            .await
            .unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        WorkOrderRepo::create(
            &company_pool,
            &WorkOrder {
                work_order_id: "wo-trace-local".to_string(),
                conversation_id: None,
                contract_hash: "c".repeat(64),
                plan_hash: "d".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: opc_id.clone(),
                mission_intent: "Trace replay local".to_string(),
                selected_agents: vec!["agent-founder-01".to_string()],
                selected_executors: vec![],
                required_skills: vec![],
                track: "green".to_string(),
                status: WorkOrderStatus::Completed,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec!["delete".to_string()],
                risk_summary: "trace".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .await
        .unwrap();
        company_pool.close().await;

        WorkerRunRepo::create(
            &pool,
            &opc_id,
            "run-trace-local",
            "wo-trace-local",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-trace-local",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            now as i64,
            Some(now as i64 + 10),
        )
        .await
        .unwrap();
        WorkerStepRepo::create(
            &pool,
            "step-trace-local",
            "run-trace-local",
            0,
            "ModelCall",
            "{\"input\":\"hello\"}",
            Some("{\"output\":\"world\"}"),
            "Completed",
            now as i64,
            Some(now as i64 + 5),
            None,
        )
        .await
        .unwrap();
        WorkerEventRepo::append(&pool, "run-trace-local", "ToolEnd", "{\"ok\":true}")
            .await
            .unwrap();
        WorkerToolCallRepo::create(
            &pool,
            "tool-trace-local",
            "run-trace-local",
            "file-readonly",
            "FileReadonly",
            "input",
            "output",
            true,
            0.3,
            None,
            now as i64,
            Some(now as i64 + 7),
        )
        .await
        .unwrap();

        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/traces"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(list_body
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["trace_id"] == "run-trace-local"));

        let detail = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/traces/run-trace-local/spans"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(detail.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["trace_id"], "run-trace-local");
        assert!(body["spans"].as_array().unwrap().len() >= 2);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_trace_routes_hide_foreign_runs_when_work_order_ids_collide() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-trace-collision-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool.clone(), root.clone()));

        let alpha_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Trace Alpha","mission":"Alpha"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let alpha: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(alpha_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let alpha_opc = alpha["opc_id"].as_str().unwrap().to_string();

        let beta_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Trace Beta","mission":"Beta"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let beta: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(beta_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let beta_opc = beta["opc_id"].as_str().unwrap().to_string();

        let now = chrono::Utc::now().timestamp_millis() as u64;
        for opc_id in [&alpha_opc, &beta_opc] {
            let company_pool = create_pool(&root.join(opc_id).join("data.db").to_string_lossy())
                .await
                .unwrap();
            WorkOrderRepo::create(
                &company_pool,
                &WorkOrder {
                    work_order_id: "wo-shared-trace".to_string(),
                    conversation_id: None,
                    contract_hash: "a".repeat(64),
                    plan_hash: "b".repeat(64),
                    user_id: "default-founder".to_string(),
                    opc_id: opc_id.to_string(),
                    mission_intent: "shared trace id".to_string(),
                    selected_agents: vec!["agent-founder-01".to_string()],
                    selected_executors: vec![],
                    required_skills: vec![],
                    track: "green".to_string(),
                    status: WorkOrderStatus::Completed,
                    allowed_actions: vec!["read".to_string()],
                    restricted_actions: vec![],
                    risk_summary: "trace".to_string(),
                    governance_proposal: None,
                    governance_verdict: None,
                    created_at_ms: now,
                    updated_at_ms: now,
                },
            )
            .await
            .unwrap();
            company_pool.close().await;
        }

        WorkerRunRepo::create(
            &pool,
            &alpha_opc,
            "run-trace-alpha",
            "wo-shared-trace",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-trace-alpha",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            now as i64,
            Some(now as i64 + 5),
        )
        .await
        .unwrap();
        WorkerRunRepo::create(
            &pool,
            &beta_opc,
            "run-trace-beta",
            "wo-shared-trace",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-trace-beta",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            now as i64 + 10,
            Some(now as i64 + 15),
        )
        .await
        .unwrap();

        let alpha_list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{alpha_opc}/traces"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(alpha_list.status(), StatusCode::OK);
        let alpha_list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(alpha_list.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let trace_ids = alpha_list_body
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["trace_id"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        assert_eq!(trace_ids, vec!["run-trace-alpha"]);

        let alpha_detail = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{alpha_opc}/traces/run-trace-beta/spans"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(alpha_detail.status(), StatusCode::NOT_FOUND);

        std::fs::remove_dir_all(root).ok();
    }
}
