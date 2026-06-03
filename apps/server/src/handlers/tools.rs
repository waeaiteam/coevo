use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_store::repos::agent_worker_repo::AgentWorkerRepo;
use coevo_store::repos::audit_repo::AuditRepo;
use coevo_store::repos::worker_run_repo::WorkerRunRepo;
use coevo_store::repos_opc::work_order_repo::WorkOrderRepo;
use coevo_worker::harness::{WorkerHarness, WorkerHarnessOptions};
use coevo_worker::queue::WorkerQueueService;
use coevo_worker::tool_policy::ToolPolicyEngine;
use coevo_worker::tool_registry::ToolRegistry;
use serde::Deserialize;
use sqlx::Row;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

fn tool_policy_allowed_actions(tool_id: &str, input: &serde_json::Value) -> Vec<String> {
    match tool_id {
        "http-get" => vec!["http_get".to_string()],
        "workspace-write-file" => vec!["write".to_string()],
        "workspace-shell" => vec!["shell".to_string()],
        "file-readonly" => match input["action"].as_str().unwrap_or("ReadFile") {
            "ListDirectory" => vec!["list".to_string()],
            _ => vec!["read".to_string()],
        },
        _ => vec![],
    }
}

async fn audit_tool_denied(
    pool: &sqlx::SqlitePool,
    work_order: &coevo_core::opc::WorkOrder,
    tool_id: &str,
    reason: &str,
) {
    let _ = AuditRepo::insert(
        pool,
        "tool.denied",
        Some(&work_order.contract_hash),
        work_order.selected_agents.first().map(String::as_str),
        None,
        &work_order.opc_id,
        &serde_json::json!({
            "work_order_id": work_order.work_order_id,
            "tool_id": tool_id,
            "reason": reason,
            "track": work_order.track,
        })
        .to_string(),
    )
    .await;
}

#[derive(Deserialize)]
pub struct ToolExecReq {
    pub work_order_id: String,
    pub input: serde_json::Value,
    pub approval_receipt: Option<String>,
}
#[derive(Deserialize)]
pub struct AssignReq {
    pub agent_id: String,
    pub work_order_id: Option<String>,
}
#[derive(Deserialize)]
pub struct RunReq {
    pub work_order_id: String,
}

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
pub async fn tool_dry_run(
    Path(id): Path<String>,
    Json(_input): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let r = ToolRegistry::default_registry();
    match r.get(&id) {
        Some(_) => ok!(serde_json::json!({"dry_run":true,"tool_id":id})),
        None => err!(StatusCode::NOT_FOUND, "Tool not found"),
    }
}
pub async fn tool_execute(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ToolExecReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let wo = match WorkOrderRepo::get(&s.pool, &req.work_order_id).await {
        Ok(Some(w)) => w,
        _ => return err!(StatusCode::BAD_REQUEST, "WorkOrder not found"),
    };
    let r = ToolRegistry::default_registry();
    let tool = match r.list().iter().find(|t| t.tool_id == id) {
        Some(t) => t,
        None => return err!(StatusCode::NOT_FOUND, "Tool not found"),
    };
    let requested_actions = tool_policy_allowed_actions(&id, &req.input);
    if !requested_actions.is_empty()
        && !requested_actions.iter().all(|action| {
            wo.allowed_actions
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(action))
        })
    {
        let reason = format!(
            "ToolDeniedByPolicy: requested actions {:?} exceed work order allowed actions {:?}",
            requested_actions, wo.allowed_actions
        );
        audit_tool_denied(&s.pool, &wo, &id, &reason).await;
        return err!(StatusCode::FORBIDDEN, reason);
    }
    let decision =
        ToolPolicyEngine::evaluate(tool, &wo.track, &wo.allowed_actions, &wo.restricted_actions);
    if !decision.allowed {
        let reason = format!("ToolDeniedByPolicy: {}", decision.reason);
        audit_tool_denied(&s.pool, &wo, &id, &reason).await;
        return err!(StatusCode::FORBIDDEN, reason);
    }
    if decision.required_approval && req.approval_receipt.is_none() {
        return err!(
            StatusCode::PRECONDITION_REQUIRED,
            format!("ToolApprovalRequired: {}", decision.reason)
        );
    }
    match r.execute(&id, req.input).await {
        Ok(result) => ok!(result),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}
// Worker operations — real implementations
pub async fn assign_worker(
    State(s): State<AppState>,
    Json(body): Json<AssignReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let now = chrono::Utc::now().timestamp_millis();
    let wid = format!("worker-{}", body.agent_id);
    if let Err(e) = AgentWorkerRepo::upsert(
        &s.pool,
        &wid,
        &body.agent_id,
        "Default",
        "Assigned",
        body.work_order_id.as_deref(),
        None,
        "[]",
        "Task",
        "[]",
        now,
        now,
    )
    .await
    {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    match AgentWorkerRepo::get(&s.pool, &wid).await {
        Ok(Some(row)) => ok!(
            serde_json::json!({"worker_id":row.get::<String,_>("worker_id"),"agent_id":row.get::<String,_>("agent_id"),"status":row.get::<String,_>("status")})
        ),
        _ => ok!(serde_json::json!({"worker_id":wid})),
    }
}
pub async fn run_worker(
    State(s): State<AppState>,
    Path(_id): Path<String>,
    Json(body): Json<RunReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    match WorkerHarness::run_work_order(
        &s.pool,
        &body.work_order_id,
        WorkerHarnessOptions {
            approval_receipt: None,
            max_runtime_ms: None,
            deterministic_mode: true,
            preferred_tool_ids: vec![],
            allow_mock_model_routing: false,
        },
    )
    .await
    {
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
pub async fn cancel_worker(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut cancelled_run = None;
    let mut cancelled_session = None;
    let mut queue_released = false;
    if let Ok(Some(row)) = AgentWorkerRepo::get(&s.pool, &id).await {
        let sid: Option<String> = row.get("current_session_id");
        if let Some(ref session_id) = sid {
            // Get active_run_id from queue lane
            let active_run_id: Option<String> =
                sqlx::query("SELECT active_run_id FROM worker_queue_lanes WHERE session_id=?")
                    .bind(session_id)
                    .fetch_optional(&s.pool)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|r| r.get::<Option<String>, _>("active_run_id"));
            if let Some(ref run_id) = active_run_id {
                WorkerRunRepo::set_status(&s.pool, run_id, "Cancelled")
                    .await
                    .ok();
                cancelled_run = Some(run_id.clone());
                // Release queue
                WorkerQueueService::release(&s.pool, session_id, run_id)
                    .await
                    .ok();
                queue_released = true;
                // Emit cancel event
                let _ = sqlx::query("INSERT INTO worker_events VALUES (?,?,?,?,?,?)")
                    .bind(format!("ev-{}-cancel", run_id))
                    .bind(run_id)
                    .bind(999i64)
                    .bind("LifecycleEnd")
                    .bind(
                        serde_json::to_string(&serde_json::json!({"reason":"CancelledByUser"}))
                            .unwrap(),
                    )
                    .bind(chrono::Utc::now().timestamp_millis())
                    .execute(&s.pool)
                    .await;
            }
            // Update session
            sqlx::query(
                "UPDATE worker_sessions SET status='Cancelled',updated_at_ms=? WHERE session_id=?",
            )
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(session_id)
            .execute(&s.pool)
            .await
            .ok();
            cancelled_session = Some(session_id.clone());
        }
    }
    AgentWorkerRepo::set_status(&s.pool, &id, "Cancelled")
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        .ok();
    ok!(
        serde_json::json!({"worker_id":id,"status":"Cancelled","cancelled_run_id":cancelled_run,"session_id":cancelled_session,"queue_released":queue_released})
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::extract::Path;
    use coevo_store::models::AuditEventRow;
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
    use coevo_store::migrate::run_migrations;
    use coevo_store::pool::create_test_pool;
    use coevo_store::repos::audit_repo::AuditRepo;
    use coevo_store::repos_opc::work_order_repo::WorkOrderRepo;

    #[tokio::test]
    async fn run_worker_rejects_public_mock_routing_when_no_model_provider() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let wo = WorkOrder {
            work_order_id: "wo-run-worker-provider-required".to_string(),
            conversation_id: None,
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
            governance_proposal: None,
            governance_verdict: None,
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

    async fn insert_work_order(
        pool: &sqlx::SqlitePool,
        work_order_id: &str,
        track: &str,
        allowed_actions: Vec<&str>,
        restricted_actions: Vec<&str>,
    ) {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let wo = WorkOrder {
            work_order_id: work_order_id.to_string(),
            conversation_id: None,
            contract_hash: "c".repeat(64),
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Tool execution test".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: track.to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: allowed_actions.into_iter().map(str::to_string).collect(),
            restricted_actions: restricted_actions.into_iter().map(str::to_string).collect(),
            risk_summary: "test".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(pool, &wo).await.unwrap();
    }

    #[tokio::test]
    async fn green_tool_execute_allows_file_read_and_http_get() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-tools-green-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let readme = root.join("notes.txt");
        std::fs::write(&readme, "green-track-readable").unwrap();
        let state = AppState::new(pool.clone(), root.clone());

        insert_work_order(&pool, "wo-tools-green", "green", vec!["read", "http_get"], vec![]).await;

        let (read_status, Json(read_body)) = tool_execute(
            State(state.clone()),
            Path("file-readonly".to_string()),
            Json(ToolExecReq {
                work_order_id: "wo-tools-green".to_string(),
                input: serde_json::json!({
                    "action": "ReadFile",
                    "path": readme.to_string_lossy().to_string(),
                    "allowed_paths": [root.to_string_lossy().to_string()]
                }),
                approval_receipt: None,
            }),
        )
        .await;
        assert_eq!(read_status, StatusCode::OK);
        assert_eq!(read_body["content"], "green-track-readable");

        let (http_status, Json(http_body)) = tool_execute(
            State(state),
            Path("http-get".to_string()),
            Json(ToolExecReq {
                work_order_id: "wo-tools-green".to_string(),
                input: serde_json::json!({
                    "url": "https://example.com"
                }),
                approval_receipt: None,
            }),
        )
        .await;
        assert_eq!(http_status, StatusCode::OK);
        assert_eq!(http_body["status_code"], 200);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn workspace_write_allows_file_write_and_shell() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-tools-write-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());

        insert_work_order(
            &pool,
            "wo-tools-write",
            "yellow",
            vec!["write", "shell"],
            vec![],
        )
        .await;

        let file_path = root.join("out.txt");
        let (write_status, Json(write_body)) = tool_execute(
            State(state.clone()),
            Path("workspace-write-file".to_string()),
            Json(ToolExecReq {
                work_order_id: "wo-tools-write".to_string(),
                input: serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "content": "workspace-write-ok",
                    "workspace_root": root.to_string_lossy().to_string()
                }),
                approval_receipt: None,
            }),
        )
        .await;
        assert_eq!(write_status, StatusCode::OK);
        assert_eq!(write_body["bytes_written"], 18);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "workspace-write-ok");

        let (shell_status, Json(shell_body)) = tool_execute(
            State(state),
            Path("workspace-shell".to_string()),
            Json(ToolExecReq {
                work_order_id: "wo-tools-write".to_string(),
                input: serde_json::json!({
                    "command": "Write-Output 'workspace-shell-ok'",
                    "workspace_root": root.to_string_lossy().to_string()
                }),
                approval_receipt: None,
            }),
        )
        .await;
        assert_eq!(shell_status, StatusCode::OK);
        assert!(shell_body["stdout"]
            .as_str()
            .unwrap_or_default()
            .contains("workspace-shell-ok"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn green_write_attempt_is_blocked_and_audited() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-tools-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());

        insert_work_order(&pool, "wo-tools-denied", "green", vec!["read"], vec![]).await;

        let denied_path = root.join("denied.txt");
        let (status, Json(body)) = tool_execute(
            State(state),
            Path("workspace-write-file".to_string()),
            Json(ToolExecReq {
                work_order_id: "wo-tools-denied".to_string(),
                input: serde_json::json!({
                    "path": denied_path.to_string_lossy().to_string(),
                    "content": "should-not-write",
                    "workspace_root": root.to_string_lossy().to_string()
                }),
                approval_receipt: None,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("ToolDeniedByPolicy"));
        assert!(!denied_path.exists());

        let audit_rows: Vec<AuditEventRow> = AuditRepo::list_by_tenant(&pool, "default-opc", 20)
            .await
            .unwrap();
        assert!(audit_rows.iter().any(|row| row.event_type == "tool.denied"));

        std::fs::remove_dir_all(root).ok();
    }
}
