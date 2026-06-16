use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use coevo_audit::logger::AuditLogger;
use coevo_store::pool::create_pool;
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

async fn load_scoped_work_order(
    state: &AppState,
    headers: &HeaderMap,
    work_order_id: &str,
) -> Result<
    (coevo_core::opc::WorkOrder, Option<sqlx::SqlitePool>),
    (StatusCode, Json<serde_json::Value>),
> {
    let opc_id = require_legacy_opc_id(headers, "/opc/tools and /opc/workers routes")?;
    let scoped_pool = Some(company_pool(state, &opc_id).await?);
    let pool_ref = scoped_pool.as_ref().unwrap();
    let work_order = match WorkOrderRepo::get(pool_ref, work_order_id).await {
        Ok(Some(work_order)) => work_order,
        Ok(None) => return Err(err!(StatusCode::BAD_REQUEST, "WorkOrder not found")),
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

async fn resolve_legacy_tool_work_order(
    state: &AppState,
    headers: &HeaderMap,
    work_order_id: &str,
) -> Result<
    (coevo_core::opc::WorkOrder, Option<sqlx::SqlitePool>),
    (StatusCode, Json<serde_json::Value>),
> {
    let Some(location) = lookup_work_order_location(state, work_order_id).await? else {
        return Err(err!(StatusCode::BAD_REQUEST, "WorkOrder not found"));
    };

    if location == "default-opc" {
        let work_order = match WorkOrderRepo::get(&state.pool, work_order_id).await {
            Ok(Some(work_order)) => work_order,
            Ok(None) => return Err(err!(StatusCode::BAD_REQUEST, "WorkOrder not found")),
            Err(e) => return Err(err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        };
        return Ok((work_order, None));
    }

    let opc_id = require_legacy_opc_id(headers, "/opc/tools routes")?;
    if opc_id != location {
        return Err(err!(
            StatusCode::CONFLICT,
            format!(
                "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match stored opc_id={}",
                opc_id, location
            )
        ));
    }
    load_scoped_work_order(state, headers, work_order_id).await
}

async fn lookup_work_order_location(
    state: &AppState,
    work_order_id: &str,
) -> Result<Option<String>, (StatusCode, Json<serde_json::Value>)> {
    match WorkOrderRepo::get(&state.pool, work_order_id).await {
        Ok(Some(work_order)) => {
            let opc_id = if work_order.opc_id.trim().is_empty() {
                "default-opc".to_string()
            } else {
                work_order.opc_id
            };
            return Ok(Some(opc_id));
        }
        Ok(None) => {}
        Err(e) => return Err(err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }

    let companies = state
        .company_workspace
        .list_companies()
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    for company in companies {
        let pool = company_pool(state, &company.opc_id).await?;
        let found = match WorkOrderRepo::get(&pool, work_order_id).await {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(e) => {
                pool.close().await;
                return Err(err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            }
        };
        pool.close().await;
        if found {
            return Ok(Some(company.opc_id));
        }
    }

    Ok(None)
}

async fn resolve_legacy_worker_work_order(
    state: &AppState,
    headers: &HeaderMap,
    work_order_id: &str,
) -> Result<
    (coevo_core::opc::WorkOrder, Option<sqlx::SqlitePool>),
    (StatusCode, Json<serde_json::Value>),
> {
    let Some(location) = lookup_work_order_location(state, work_order_id).await? else {
        return Err(err!(StatusCode::BAD_REQUEST, "WorkOrder not found"));
    };

    if location == "default-opc" {
        let work_order = match WorkOrderRepo::get(&state.pool, work_order_id).await {
            Ok(Some(work_order)) => work_order,
            Ok(None) => return Err(err!(StatusCode::BAD_REQUEST, "WorkOrder not found")),
            Err(e) => return Err(err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        };
        return Ok((work_order, None));
    }

    let opc_id = require_legacy_opc_id(headers, "/opc/workers routes")?;
    if opc_id != location {
        return Err(err!(
            StatusCode::CONFLICT,
            format!(
                "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match stored opc_id={}",
                opc_id, location
            )
        ));
    }
    load_scoped_work_order(state, headers, work_order_id).await
}

fn worker_scope_opc_id(row: &sqlx::sqlite::SqliteRow) -> String {
    row.try_get::<Option<String>, _>("opc_id")
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "default-opc".to_string())
}

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
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ToolExecReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (wo, _scoped_pool) =
        match resolve_legacy_tool_work_order(&s, &headers, &req.work_order_id).await {
            Ok(result) => result,
            Err(err) => return err,
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
        let _ = AuditLogger::log_json(
            &s.pool,
            "tool.approval.required",
            Some(&wo.contract_hash),
            wo.selected_agents.first().map(String::as_str),
            None,
            &wo.opc_id,
            &serde_json::json!({
                "work_order_id": wo.work_order_id,
                "tool_id": id,
                "reason": decision.reason,
                "track": wo.track,
            }),
        )
        .await;
        return err!(
            StatusCode::PRECONDITION_REQUIRED,
            format!("ToolApprovalRequired: {}", decision.reason)
        );
    }
    if decision.required_approval {
        let _ = AuditLogger::log_json(
            &s.pool,
            "tool.approval.receipt.accepted",
            Some(&wo.contract_hash),
            wo.selected_agents.first().map(String::as_str),
            None,
            &wo.opc_id,
            &serde_json::json!({
                "work_order_id": wo.work_order_id,
                "tool_id": id,
                "approval_receipt": req.approval_receipt,
                "track": wo.track,
            }),
        )
        .await;
    }
    match r.execute(&id, req.input).await {
        Ok(result) => ok!(result),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}
// Worker operations — real implementations
pub async fn assign_worker(
    headers: HeaderMap,
    State(s): State<AppState>,
    Json(body): Json<AssignReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut scoped_opc_id = legacy_opc_id(&headers).unwrap_or_else(|| "default-opc".to_string());
    if let Some(work_order_id) = body.work_order_id.as_deref() {
        let (work_order, scoped_pool) =
            match resolve_legacy_worker_work_order(&s, &headers, work_order_id).await {
                Ok(result) => result,
                Err(err) => return err,
            };
        scoped_opc_id = if work_order.opc_id.trim().is_empty() {
            "default-opc".to_string()
        } else {
            work_order.opc_id.clone()
        };
        if let Some(pool) = scoped_pool.as_ref() {
            match coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo::get(
                pool,
                &body.agent_id,
            )
            .await
            {
                Ok(Some(_)) => {}
                Ok(None) => {
                    pool.close().await;
                    return err!(
                        StatusCode::FORBIDDEN,
                        format!("Agent {} not found", body.agent_id)
                    );
                }
                Err(e) => {
                    pool.close().await;
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                }
            }
            pool.close().await;
        }

        if let Ok(Some(existing)) =
            AgentWorkerRepo::get(&s.pool, &format!("worker-{}", body.agent_id)).await
        {
            let existing_work_order_id: Option<String> = existing.get("current_work_order_id");
            let existing_status: String = existing.get("status");
            if let Some(existing_work_order_id) = existing_work_order_id {
                let active_statuses = [
                    "Assigned",
                    "Planning",
                    "Executing",
                    "WaitingApproval",
                    "Reflecting",
                ];
                if existing_work_order_id != work_order.work_order_id
                    && active_statuses.contains(&existing_status.as_str())
                {
                    match lookup_work_order_location(&s, &existing_work_order_id).await {
                        Ok(Some(existing_location)) if existing_location != work_order.opc_id => {
                            return err!(
                                StatusCode::CONFLICT,
                                format!(
                                    "WORKER_BOUND_TO_OTHER_COMPANY: worker-{} is still bound to {}",
                                    body.agent_id, existing_location
                                )
                            );
                        }
                        Ok(_) => {}
                        Err(err) => return err,
                    }
                }
            }
        }
    } else if let Ok(Some(existing)) =
        AgentWorkerRepo::get(&s.pool, &format!("worker-{}", body.agent_id)).await
    {
        let existing_opc_id = worker_scope_opc_id(&existing);
        if existing_opc_id != "default-opc" {
            let header_opc_id = match require_legacy_opc_id(&headers, "/opc/workers routes") {
                Ok(opc_id) => opc_id,
                Err(err) => return err,
            };
            if header_opc_id != existing_opc_id {
                return err!(
                    StatusCode::CONFLICT,
                    format!(
                        "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match stored opc_id={}",
                        header_opc_id, existing_opc_id
                    )
                );
            }
            scoped_opc_id = header_opc_id;
        }
    }

    let now = chrono::Utc::now().timestamp_millis();
    let wid = format!("worker-{}", body.agent_id);
    if let Err(e) = AgentWorkerRepo::upsert(
        &s.pool,
        &wid,
        &scoped_opc_id,
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
    headers: HeaderMap,
    Path(worker_id): Path<String>,
    Json(body): Json<RunReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (work_order, scoped_pool) =
        match resolve_legacy_worker_work_order(&s, &headers, &body.work_order_id).await {
            Ok(result) => result,
            Err(err) => return err,
        };
    let expected_agent_id = work_order
        .selected_agents
        .first()
        .cloned()
        .unwrap_or_default();
    if expected_agent_id.is_empty() {
        return err!(
            StatusCode::BAD_REQUEST,
            "WorkOrder has no selected agent to bind legacy worker run"
        );
    }
    let expected_worker_id = format!("worker-{expected_agent_id}");
    if worker_id != expected_worker_id {
        return err!(
            StatusCode::CONFLICT,
            format!(
                "WORKER_PATH_ID_MISMATCH: path worker_id={} does not match work order worker_id={}",
                worker_id, expected_worker_id
            )
        );
    }
    if let Ok(Some(existing_worker)) = AgentWorkerRepo::get(&s.pool, &worker_id).await {
        let existing_opc_id = worker_scope_opc_id(&existing_worker);
        let expected_opc_id = if work_order.opc_id.trim().is_empty() {
            "default-opc".to_string()
        } else {
            work_order.opc_id.clone()
        };
        if existing_opc_id != "default-opc" && existing_opc_id != expected_opc_id {
            return err!(
                StatusCode::CONFLICT,
                format!(
                    "WORKER_BOUND_TO_OTHER_COMPANY: worker-{expected_agent_id} is still bound to {existing_opc_id}"
                )
            );
        }
    }
    let options = WorkerHarnessOptions {
        approval_receipt: None,
        max_runtime_ms: None,
        deterministic_mode: true,
        preferred_tool_ids: vec![],
        allow_mock_model_routing: false,
    };
    let result = match scoped_pool.as_ref() {
        Some(opc_pool) => {
            WorkerHarness::run_work_order_with_pools(
                &s.pool,
                opc_pool,
                &body.work_order_id,
                options,
            )
            .await
        }
        None => WorkerHarness::run_work_order(&s.pool, &body.work_order_id, options).await,
    };
    match result {
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
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut cancelled_run = None;
    let mut cancelled_session = None;
    let mut queue_released = false;
    if let Ok(Some(row)) = AgentWorkerRepo::get(&s.pool, &id).await {
        let worker_opc_id = worker_scope_opc_id(&row);
        let current_work_order_id: Option<String> = row.get("current_work_order_id");
        let sid: Option<String> = row.get("current_session_id");
        let scoped_work_order_id = if let Some(work_order_id) = current_work_order_id {
            Some(work_order_id)
        } else if let Some(ref session_id) = sid {
            sqlx::query("SELECT work_order_id FROM worker_sessions WHERE session_id=?")
                .bind(session_id)
                .fetch_optional(&s.pool)
                .await
                .ok()
                .flatten()
                .and_then(|r| r.get::<Option<String>, _>("work_order_id"))
        } else {
            None
        };
        if let Some(ref work_order_id) = scoped_work_order_id {
            if let Err(err) = resolve_legacy_worker_work_order(&s, &headers, work_order_id).await {
                return err;
            }
        } else if worker_opc_id != "default-opc" {
            let header_opc_id = match require_legacy_opc_id(&headers, "/opc/workers routes") {
                Ok(opc_id) => opc_id,
                Err(err) => return err,
            };
            if header_opc_id != worker_opc_id {
                return err!(
                    StatusCode::CONFLICT,
                    format!(
                        "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match stored opc_id={}",
                        header_opc_id, worker_opc_id
                    )
                );
            }
        }
        if let Some(ref session_id) = sid {
            let session_status: Option<String> =
                sqlx::query_scalar("SELECT status FROM worker_sessions WHERE session_id=?")
                    .bind(session_id)
                    .fetch_optional(&s.pool)
                    .await
                    .ok()
                    .flatten();
            let session_is_active = matches!(
                session_status.as_deref(),
                Some("Open" | "Running" | "WaitingApproval")
            );
            // Get active_run_id from queue lane
            let active_run_id: Option<String> =
                sqlx::query("SELECT active_run_id FROM worker_queue_lanes WHERE session_id=?")
                    .bind(session_id)
                    .fetch_optional(&s.pool)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|r| r.get::<Option<String>, _>("active_run_id"));
            let resolved_run = if let Some(run_id) = active_run_id {
                Some((run_id, "Running".to_string()))
            } else {
                sqlx::query(
                    "SELECT run_id,status FROM worker_runs WHERE session_id=? ORDER BY created_at_ms DESC LIMIT 1",
                )
                .bind(session_id)
                .fetch_optional(&s.pool)
                .await
                .ok()
                .flatten()
                .map(|row| {
                    (
                        row.get::<String, _>("run_id"),
                        row.get::<String, _>("status"),
                    )
                })
            };
            let active_run = resolved_run.filter(|(_, status)| {
                matches!(status.as_str(), "Queued" | "Running" | "WaitingApproval")
            });
            if let Some((ref run_id, _)) = active_run {
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
            if session_is_active || active_run.is_some() {
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
    }
    if let Some(ref run_id) = cancelled_run {
        let _ = coevo_worker::worker_cancel::cancel_run(run_id);
    }
    ok!(
        serde_json::json!({"worker_id":id,"status":"Cancelled","cancelled_run_id":cancelled_run,"session_id":cancelled_session,"queue_released":queue_released})
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::{extract::Path, http::HeaderMap};
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
    use coevo_store::migrate::run_migrations;
    use coevo_store::models::AuditEventRow;
    use coevo_store::pool::{create_pool, create_test_pool};
    use coevo_store::repos::audit_repo::AuditRepo;
    use coevo_store::repos::worker_run_repo::{WorkerQueueRepo, WorkerRunRepo};
    use coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo;
    use coevo_store::repos_opc::work_order_repo::WorkOrderRepo;
    use coevo_worker::worker_cancel;

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
            HeaderMap::new(),
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

    async fn create_company_pool(state: &AppState, name: &str) -> (String, sqlx::SqlitePool) {
        let company = state
            .company_workspace
            .create_company(name, Some("tools test"), "default-founder")
            .await
            .unwrap();
        let pool = create_pool(
            &state
                .company_workspace
                .company_db_path(&company.opc_id)
                .to_string_lossy(),
        )
        .await
        .unwrap();
        (company.opc_id, pool)
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

        insert_work_order(
            &pool,
            "wo-tools-green",
            "green",
            vec!["read", "http_get"],
            vec![],
        )
        .await;

        let (read_status, Json(read_body)) = tool_execute(
            State(state.clone()),
            HeaderMap::new(),
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
            HeaderMap::new(),
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
            HeaderMap::new(),
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
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "workspace-write-ok"
        );

        let (shell_status, Json(shell_body)) = tool_execute(
            State(state),
            HeaderMap::new(),
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
            HeaderMap::new(),
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

    #[tokio::test]
    async fn legacy_tool_execute_uses_company_scoped_work_order_and_keeps_audit_global() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-tools-legacy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Legacy Tools Co").await;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-legacy-company-tool".to_string(),
            conversation_id: None,
            contract_hash: "e".repeat(64),
            plan_hash: "f".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "legacy tool execution".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "legacy".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;

        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, opc_id.parse().unwrap());
        let denied_path = root.join("denied.txt");
        let (status, Json(body)) = tool_execute(
            State(state.clone()),
            headers,
            Path("workspace-write-file".to_string()),
            Json(ToolExecReq {
                work_order_id: work_order.work_order_id.clone(),
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

        let global_audit_rows: Vec<AuditEventRow> =
            AuditRepo::list_by_tenant(&pool, &opc_id, 20).await.unwrap();
        assert!(global_audit_rows
            .iter()
            .any(|row| row.event_type == "tool.denied"));

        let company_pool = create_pool(
            &state
                .company_workspace
                .company_db_path(&opc_id)
                .to_string_lossy(),
        )
        .await
        .unwrap();
        let company_audit_rows: Vec<AuditEventRow> =
            AuditRepo::list_by_tenant(&company_pool, &opc_id, 20)
                .await
                .unwrap();
        assert_eq!(company_audit_rows.len(), 0);
        company_pool.close().await;

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_run_worker_uses_company_scoped_work_order() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-tools-legacy-run-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Legacy Worker Co").await;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-legacy-company-run".to_string(),
            conversation_id: None,
            contract_hash: "1".repeat(64),
            plan_hash: "2".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "legacy run worker".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec!["delete".to_string()],
            risk_summary: "legacy".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;

        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, opc_id.parse().unwrap());
        let (status, Json(body)) = run_worker(
            State(state.clone()),
            headers,
            Path("worker-agent-founder-01".to_string()),
            Json(RunReq {
                work_order_id: work_order.work_order_id.clone(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("MODEL_PROVIDER_NOT_CONFIGURED"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_tool_execute_requires_opc_header_for_company_scoped_work_orders() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-tools-header-required-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Header Required Tools Co").await;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-legacy-header-required-tool".to_string(),
            conversation_id: None,
            contract_hash: "7".repeat(64),
            plan_hash: "8".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "header required tool execute".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "legacy".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;

        let (status, Json(body)) = tool_execute(
            State(state),
            HeaderMap::new(),
            Path("file-readonly".to_string()),
            Json(ToolExecReq {
                work_order_id: work_order.work_order_id.clone(),
                input: serde_json::json!({
                    "action": "ReadFile",
                    "path": root.join("notes.txt").to_string_lossy().to_string(),
                    "allowed_paths": [root.to_string_lossy().to_string()]
                }),
                approval_receipt: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_run_worker_requires_opc_header_for_company_scoped_work_orders() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-run-worker-header-required-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Header Required Worker Co").await;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-legacy-header-required-run".to_string(),
            conversation_id: None,
            contract_hash: "9".repeat(64),
            plan_hash: "a".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "header required run worker".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "legacy".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;

        let (status, Json(body)) = run_worker(
            State(state),
            HeaderMap::new(),
            Path("worker-agent-founder-01".to_string()),
            Json(RunReq {
                work_order_id: work_order.work_order_id.clone(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_run_worker_rejects_path_worker_id_mismatch() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-legacy-path-worker-mismatch".to_string(),
            conversation_id: None,
            contract_hash: "9".repeat(64),
            plan_hash: "a".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "path worker mismatch".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "legacy".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&pool, &work_order).await.unwrap();

        let (status, Json(body)) = run_worker(
            State(state),
            HeaderMap::new(),
            Path("worker-some-other-agent".to_string()),
            Json(RunReq {
                work_order_id: work_order.work_order_id.clone(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("WORKER_PATH_ID_MISMATCH"));
    }

    #[tokio::test]
    async fn legacy_assign_worker_requires_opc_header_for_company_scoped_work_orders() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-assign-worker-header-required-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Header Required Assign Co").await;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-legacy-header-required-assign".to_string(),
            conversation_id: None,
            contract_hash: "b".repeat(64),
            plan_hash: "c".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "header required assign worker".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "legacy".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;

        let (status, Json(body)) = assign_worker(
            HeaderMap::new(),
            State(state),
            Json(AssignReq {
                agent_id: "agent-founder-01".to_string(),
                work_order_id: Some(work_order.work_order_id.clone()),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_assign_worker_allows_default_opc_work_order_without_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-assign-worker-default-opc-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-default-assign".to_string(),
            conversation_id: None,
            contract_hash: "b".repeat(64),
            plan_hash: "c".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "assign default opc worker".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "default-opc".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&pool, &work_order).await.unwrap();

        let (status, Json(body)) = assign_worker(
            HeaderMap::new(),
            State(state),
            Json(AssignReq {
                agent_id: "agent-founder-01".to_string(),
                work_order_id: Some(work_order.work_order_id.clone()),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        let worker = AgentWorkerRepo::get(&pool, "worker-agent-founder-01")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            worker
                .get::<Option<String>, _>("current_work_order_id")
                .as_deref(),
            Some(work_order.work_order_id.as_str())
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_cancel_worker_requires_opc_header_for_company_scoped_work_orders() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-cancel-worker-header-required-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Header Required Cancel Co").await;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-legacy-header-required-cancel".to_string(),
            conversation_id: None,
            contract_hash: "d".repeat(64),
            plan_hash: "e".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "header required cancel worker".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "legacy".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;

        AgentWorkerRepo::upsert(
            &pool,
            "worker-agent-founder-01",
            &opc_id,
            "agent-founder-01",
            "Default",
            "Assigned",
            Some(&work_order.work_order_id),
            None,
            "[]",
            "Task",
            "[]",
            now as i64,
            now as i64,
        )
        .await
        .unwrap();

        let (status, Json(body)) = cancel_worker(
            HeaderMap::new(),
            State(state),
            Path("worker-agent-founder-01".to_string()),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_cancel_worker_allows_default_opc_work_order_without_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-cancel-worker-default-opc-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-default-cancel".to_string(),
            conversation_id: None,
            contract_hash: "d".repeat(64),
            plan_hash: "e".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "cancel default opc worker".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "default-opc".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&pool, &work_order).await.unwrap();

        AgentWorkerRepo::upsert(
            &pool,
            "worker-agent-founder-01",
            "default-opc",
            "agent-founder-01",
            "Default",
            "Assigned",
            Some(&work_order.work_order_id),
            None,
            "[]",
            "Task",
            "[]",
            now as i64,
            now as i64,
        )
        .await
        .unwrap();

        let (status, Json(body)) = cancel_worker(
            HeaderMap::new(),
            State(state),
            Path("worker-agent-founder-01".to_string()),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["status"], "Cancelled");
        let worker = AgentWorkerRepo::get(&pool, "worker-agent-founder-01")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(worker.get::<String, _>("status"), "Cancelled");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_assign_worker_rejects_cross_company_active_worker_binding() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-assign-worker-cross-company-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (alpha_opc_id, alpha_pool) = create_company_pool(&state, "Alpha Assign Co").await;
        let (beta_opc_id, beta_pool) = create_company_pool(&state, "Beta Assign Co").await;
        AgentEmployeeRepo::seed(&alpha_pool).await.unwrap();
        AgentEmployeeRepo::seed(&beta_pool).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let alpha_work_order = WorkOrder {
            work_order_id: "wo-alpha-assign".to_string(),
            conversation_id: None,
            contract_hash: "f".repeat(64),
            plan_hash: "1".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: alpha_opc_id.clone(),
            mission_intent: "alpha assign".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "alpha".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        let beta_work_order = WorkOrder {
            work_order_id: "wo-beta-assign".to_string(),
            conversation_id: None,
            contract_hash: "2".repeat(64),
            plan_hash: "3".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: beta_opc_id.clone(),
            mission_intent: "beta assign".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "beta".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&alpha_pool, &alpha_work_order)
            .await
            .unwrap();
        WorkOrderRepo::create(&beta_pool, &beta_work_order)
            .await
            .unwrap();
        alpha_pool.close().await;
        beta_pool.close().await;

        let mut alpha_headers = HeaderMap::new();
        alpha_headers.insert(LEGACY_OPC_ID_HEADER, alpha_opc_id.parse().unwrap());
        let (alpha_status, _) = assign_worker(
            alpha_headers,
            State(state.clone()),
            Json(AssignReq {
                agent_id: "agent-founder-01".to_string(),
                work_order_id: Some(alpha_work_order.work_order_id.clone()),
            }),
        )
        .await;
        assert_eq!(alpha_status, StatusCode::OK);

        let mut beta_headers = HeaderMap::new();
        beta_headers.insert(LEGACY_OPC_ID_HEADER, beta_opc_id.parse().unwrap());
        let (beta_status, Json(beta_body)) = assign_worker(
            beta_headers,
            State(state.clone()),
            Json(AssignReq {
                agent_id: "agent-founder-01".to_string(),
                work_order_id: Some(beta_work_order.work_order_id.clone()),
            }),
        )
        .await;
        assert_eq!(beta_status, StatusCode::CONFLICT);
        assert!(beta_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("WORKER_BOUND_TO_OTHER_COMPANY"));

        let worker = AgentWorkerRepo::get(&pool, "worker-agent-founder-01")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            worker
                .get::<Option<String>, _>("current_work_order_id")
                .as_deref(),
            Some(alpha_work_order.work_order_id.as_str())
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_cancel_worker_rejects_cross_company_header_mismatch() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-cancel-worker-cross-company-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (alpha_opc_id, alpha_pool) = create_company_pool(&state, "Alpha Cancel Co").await;
        let (beta_opc_id, beta_pool) = create_company_pool(&state, "Beta Cancel Co").await;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let alpha_work_order = WorkOrder {
            work_order_id: "wo-alpha-cancel".to_string(),
            conversation_id: None,
            contract_hash: "4".repeat(64),
            plan_hash: "5".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: alpha_opc_id.clone(),
            mission_intent: "alpha cancel".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "alpha".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&alpha_pool, &alpha_work_order)
            .await
            .unwrap();
        alpha_pool.close().await;
        beta_pool.close().await;

        AgentWorkerRepo::upsert(
            &pool,
            "worker-agent-founder-01",
            &alpha_opc_id,
            "agent-founder-01",
            "Default",
            "Assigned",
            Some(&alpha_work_order.work_order_id),
            None,
            "[]",
            "Task",
            "[]",
            now as i64,
            now as i64,
        )
        .await
        .unwrap();

        let mut beta_headers = HeaderMap::new();
        beta_headers.insert(LEGACY_OPC_ID_HEADER, beta_opc_id.parse().unwrap());
        let (status, Json(body)) = cancel_worker(
            beta_headers,
            State(state),
            Path("worker-agent-founder-01".to_string()),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_HEADER_BODY_MISMATCH"));

        let worker = AgentWorkerRepo::get(&pool, "worker-agent-founder-01")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(worker.get::<String, _>("status"), "Assigned");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_assign_worker_without_work_order_requires_matching_header_for_company_worker() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-assign-worker-company-scope-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Assign Scope Co").await;
        AgentEmployeeRepo::seed(&company_pool).await.unwrap();
        company_pool.close().await;

        let now = chrono::Utc::now().timestamp_millis();
        AgentWorkerRepo::upsert(
            &pool,
            "worker-agent-founder-01",
            &opc_id,
            "agent-founder-01",
            "Default",
            "Completed",
            None,
            None,
            "[]",
            "Task",
            "[]",
            now,
            now,
        )
        .await
        .unwrap();

        let (missing_status, Json(missing_body)) = assign_worker(
            HeaderMap::new(),
            State(state.clone()),
            Json(AssignReq {
                agent_id: "agent-founder-01".to_string(),
                work_order_id: None,
            }),
        )
        .await;
        assert_eq!(missing_status, StatusCode::BAD_REQUEST);
        assert!(missing_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let mut wrong_headers = HeaderMap::new();
        wrong_headers.insert(LEGACY_OPC_ID_HEADER, "default-opc".parse().unwrap());
        let (wrong_status, Json(wrong_body)) = assign_worker(
            wrong_headers,
            State(state.clone()),
            Json(AssignReq {
                agent_id: "agent-founder-01".to_string(),
                work_order_id: None,
            }),
        )
        .await;
        assert_eq!(wrong_status, StatusCode::CONFLICT);
        assert!(wrong_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_HEADER_BODY_MISMATCH"));

        let mut ok_headers = HeaderMap::new();
        ok_headers.insert(LEGACY_OPC_ID_HEADER, opc_id.parse().unwrap());
        let (ok_status, Json(ok_body)) = assign_worker(
            ok_headers,
            State(state),
            Json(AssignReq {
                agent_id: "agent-founder-01".to_string(),
                work_order_id: None,
            }),
        )
        .await;
        assert_eq!(ok_status, StatusCode::OK, "{ok_body:?}");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_cancel_worker_without_work_order_requires_matching_header_for_company_worker() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-cancel-worker-company-scope-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Cancel Scope Co").await;
        AgentEmployeeRepo::seed(&company_pool).await.unwrap();
        company_pool.close().await;

        let now = chrono::Utc::now().timestamp_millis();
        AgentWorkerRepo::upsert(
            &pool,
            "worker-agent-founder-01",
            &opc_id,
            "agent-founder-01",
            "Default",
            "Completed",
            None,
            None,
            "[]",
            "Task",
            "[]",
            now,
            now,
        )
        .await
        .unwrap();

        let (missing_status, Json(missing_body)) = cancel_worker(
            HeaderMap::new(),
            State(state.clone()),
            Path("worker-agent-founder-01".to_string()),
        )
        .await;
        assert_eq!(missing_status, StatusCode::BAD_REQUEST);
        assert!(missing_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let mut wrong_headers = HeaderMap::new();
        wrong_headers.insert(LEGACY_OPC_ID_HEADER, "default-opc".parse().unwrap());
        let (wrong_status, Json(wrong_body)) = cancel_worker(
            wrong_headers,
            State(state.clone()),
            Path("worker-agent-founder-01".to_string()),
        )
        .await;
        assert_eq!(wrong_status, StatusCode::CONFLICT);
        assert!(wrong_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_HEADER_BODY_MISMATCH"));

        let mut ok_headers = HeaderMap::new();
        ok_headers.insert(LEGACY_OPC_ID_HEADER, opc_id.parse().unwrap());
        let (ok_status, Json(ok_body)) = cancel_worker(
            ok_headers,
            State(state.clone()),
            Path("worker-agent-founder-01".to_string()),
        )
        .await;
        assert_eq!(ok_status, StatusCode::OK, "{ok_body:?}");

        let worker = AgentWorkerRepo::get(&pool, "worker-agent-founder-01")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(worker.get::<String, _>("status"), "Cancelled");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn cancel_worker_triggers_in_process_cancellation_signal() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-cancel-worker-signal-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());

        let now = chrono::Utc::now().timestamp_millis();
        let worker_id = "worker-agent-founder-01";
        let session_id = "session-cancel-signal";
        let run_id = "run-cancel-signal";
        let work_order_id = "wo-cancel-signal";
        let work_order = WorkOrder {
            work_order_id: work_order_id.to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "cancel signal test".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "signal".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now as u64,
            updated_at_ms: now as u64,
        };
        WorkOrderRepo::create(&pool, &work_order).await.unwrap();
        AgentWorkerRepo::upsert(
            &pool,
            worker_id,
            "default-opc",
            "agent-founder-01",
            "Default",
            "Executing",
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
        sqlx::query(
            "INSERT INTO worker_sessions (
                session_id,
                opc_id,
                worker_id,
                work_order_id,
                agent_id,
                channel,
                messages_json,
                context_memory_ids_json,
                loaded_skill_ids_json,
                tool_call_ids_json,
                status,
                created_at_ms,
                updated_at_ms
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(session_id)
        .bind("default-opc")
        .bind(worker_id)
        .bind(Some(work_order_id.to_string()))
        .bind("agent-founder-01")
        .bind("MissionChat")
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("Running")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
        WorkerQueueRepo::acquire(&pool, session_id, run_id, 120_000)
            .await
            .unwrap();
        WorkerRunRepo::create(
            &pool,
            "default-opc",
            run_id,
            "wo-cancel-signal",
            "agent-founder-01",
            worker_id,
            session_id,
            "Running",
            "{}",
            "[]",
            "[]",
            None,
            now,
            None,
        )
        .await
        .unwrap();

        let cancellation = worker_cancel::register_run(run_id);
        let live_token = cancellation.token();
        assert!(!live_token.is_cancelled());

        let (status, Json(body)) = cancel_worker(
            HeaderMap::new(),
            State(state),
            Path(worker_id.to_string()),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["cancelled_run_id"], run_id);
        assert_eq!(body["queue_released"], true);
        assert!(live_token.is_cancelled());

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn cancel_worker_does_not_rewrite_completed_run_when_queue_lane_is_missing() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-cancel-worker-late-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool.clone(), root.clone());

        let now = chrono::Utc::now().timestamp_millis();
        let worker_id = "worker-agent-founder-01";
        let session_id = "session-cancel-late";
        let run_id = "run-cancel-late";
        let work_order_id = "wo-cancel-late";
        let work_order = WorkOrder {
            work_order_id: work_order_id.to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "late cancel should be ignored".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Completed,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "late-cancel".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now as u64,
            updated_at_ms: now as u64,
        };
        WorkOrderRepo::create(&pool, &work_order).await.unwrap();
        AgentWorkerRepo::upsert(
            &pool,
            worker_id,
            "default-opc",
            "agent-founder-01",
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
        sqlx::query(
            "INSERT INTO worker_sessions (
                session_id,
                opc_id,
                worker_id,
                work_order_id,
                agent_id,
                channel,
                messages_json,
                context_memory_ids_json,
                loaded_skill_ids_json,
                tool_call_ids_json,
                status,
                created_at_ms,
                updated_at_ms
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(session_id)
        .bind("default-opc")
        .bind(worker_id)
        .bind(Some(work_order_id.to_string()))
        .bind("agent-founder-01")
        .bind("MissionChat")
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("Completed")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
        WorkerRunRepo::create(
            &pool,
            "default-opc",
            run_id,
            work_order_id,
            "agent-founder-01",
            worker_id,
            session_id,
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            now,
            Some(now),
        )
        .await
        .unwrap();

        let (status, Json(body)) = cancel_worker(
            HeaderMap::new(),
            State(state),
            Path(worker_id.to_string()),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert!(body["cancelled_run_id"].is_null(), "{body:?}");
        assert!(body["session_id"].is_null(), "{body:?}");
        assert_eq!(body["queue_released"], false);

        let run_status: String = sqlx::query_scalar("SELECT status FROM worker_runs WHERE run_id=?")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let session_status: String =
            sqlx::query_scalar("SELECT status FROM worker_sessions WHERE session_id=?")
                .bind(session_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let worker_status: String =
            sqlx::query_scalar("SELECT status FROM agent_workers WHERE worker_id=?")
                .bind(worker_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(run_status, "Completed");
        assert_eq!(session_status, "Completed");
        assert_eq!(worker_status, "Cancelled");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_worker_routes_reject_malformed_opc_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-tools-bad-header-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::new(pool, root.clone());
        let (opc_id, company_pool) = create_company_pool(&state, "Bad Header Co").await;
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-bad-header".to_string(),
            conversation_id: None,
            contract_hash: "1".repeat(64),
            plan_hash: "2".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "validate malformed header".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "low".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, "../escape".parse().unwrap());

        let (status, Json(body)) = assign_worker(
            headers,
            State(state),
            Json(AssignReq {
                agent_id: "agent-founder-01".to_string(),
                work_order_id: Some(work_order.work_order_id),
            }),
        )
        .await;

        std::fs::remove_dir_all(root).ok();

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));
    }
}
