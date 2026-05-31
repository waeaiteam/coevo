use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use coevo_core::opc::*;
use coevo_core::skills::*;
use coevo_evolution::{
    analyzer::FailureAnalyzer, generator::SkillGenerator, verifier::SkillVerifier,
};
use coevo_executors::adapters::*;
use coevo_executors::traits::*;
use coevo_store::repos_opc::*;
use coevo_store::repos::{approval_repo::ApprovalRepo, contract_repo::ContractRepo};
use coevo_worker::harness::{WorkerHarness, WorkerHarnessOptions};
use serde::Deserialize;
use sqlx::Row;

#[derive(Deserialize)]
pub struct MemoryQuery {
    pub scope: Option<String>,
    pub owner_id: Option<String>,
    pub include_revoked: Option<bool>,
    pub q: Option<String>,
}
#[derive(Deserialize)]
pub struct ExecutorDryRunReq {
    pub work_order_id: String,
}
#[derive(Deserialize)]
pub struct WorkOrderFeedback {
    pub feedback: String,
    pub agent_id: Option<String>,
}
#[derive(Deserialize)]
pub struct SkillsQuery {
    pub agent_id: Option<String>,
}
#[derive(Deserialize)]
pub struct ExecuteRequest {
    pub caller_identity_proof: Option<String>,
    pub monitoring_signature: Option<String>,
    pub diagnostic_signature: Option<String>,
    pub lease_id: Option<String>,
}
#[derive(Deserialize)]
pub struct CreateWORequest {
    pub work_order_id: Option<String>,
    pub conversation_id: Option<String>,
    pub contract_hash: String,
    pub plan_hash: String,
    pub user_id: String,
    pub opc_id: String,
    pub mission_intent: String,
    pub selected_agents: Vec<String>,
    pub selected_executors: Vec<String>,
    pub required_skills: Vec<String>,
}

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

fn track_risk(track: &str) -> f64 {
    match track {
        "green" => 0.3,
        "yellow" => 0.6,
        _ => 0.9,
    }
}

struct TrackDecision {
    track: &'static str,
    risk_summary: String,
    allowed_actions: Vec<String>,
    restricted_actions: Vec<String>,
}

const RED_TRIGGERS: [&str; 11] = [
    "production",
    "prod",
    "critical",
    "database mutation",
    "rollback",
    "payment",
    "delete",
    "p1",
    "emergency",
    "financial",
    "customer data",
];
const YELLOW_TRIGGERS: [&str; 10] = [
    "deploy",
    "notification",
    "staging",
    "send",
    "create ticket",
    "write",
    "update",
    "changelog",
    "internal",
    "modify",
];

fn classify_mission_track(intent: &str) -> TrackDecision {
    let lower = intent.to_lowercase();
    for trigger in RED_TRIGGERS {
        if lower.contains(trigger) {
            return TrackDecision {
                track: "red",
                risk_summary: format!(
                    "Server RiskGate: intent matches high-risk trigger \"{}\". Production/critical operations require Red Track.",
                    trigger
                ),
                allowed_actions: vec!["read".to_string(), "draft".to_string()],
                restricted_actions: vec![
                    "write".to_string(),
                    "delete".to_string(),
                    "deploy".to_string(),
                    "payment".to_string(),
                    "production".to_string(),
                ],
            };
        }
    }
    for trigger in YELLOW_TRIGGERS {
        if lower.contains(trigger) {
            return TrackDecision {
                track: "yellow",
                risk_summary: format!(
                    "Server RiskGate: intent matches moderate-risk trigger \"{}\". Internal writes require Yellow Track approval.",
                    trigger
                ),
                allowed_actions: vec!["read".to_string(), "draft".to_string()],
                restricted_actions: vec![
                    "delete".to_string(),
                    "payment".to_string(),
                    "production".to_string(),
                ],
            };
        }
    }
    TrackDecision {
        track: "green",
        risk_summary: "Server RiskGate: low-risk read/analyze intent. Green Track auto-execution is allowed.".to_string(),
        allowed_actions: vec!["read".to_string(), "analyze".to_string()],
        restricted_actions: vec![
            "delete".to_string(),
            "payment".to_string(),
            "production".to_string(),
        ],
    }
}

fn yellow_approval_receipt(req: &ExecuteRequest) -> Option<&str> {
    // Alpha compatibility: Yellow approval receipts may arrive through the legacy
    // caller_identity_proof field or lease_id. Red Track leases need a separate
    // verifier before Red execution is enabled.
    req.caller_identity_proof
        .as_deref()
        .or(req.lease_id.as_deref())
        .filter(|s| !s.trim().is_empty())
}

fn make_executor(source_type: &ExecutorSourceType) -> Option<Box<dyn ExternalExecutorAdapter>> {
    match source_type {
        ExecutorSourceType::Hermes => Some(Box::new(MockHermesAdapter::new())),
        ExecutorSourceType::OpenClaw => Some(Box::new(MockOpenClawAdapter::new())),
        ExecutorSourceType::MCP => Some(Box::new(MockMcpAdapter::new())),
        ExecutorSourceType::Local302AI => Some(Box::new(MockLocal302AIAdapter::new())),
        ExecutorSourceType::Browser => Some(Box::new(MockBrowserAdapter::new())),
        ExecutorSourceType::LocalProcess => Some(Box::new(MockLocalProcessAdapter::new())),
        ExecutorSourceType::Docker => Some(Box::new(MockDockerAdapter::new())),
        ExecutorSourceType::Custom => None,
    }
}

// === Profiles ===
pub async fn get_user_profile(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match user_profile_repo::UserProfileRepo::get(&s.pool, "default-founder").await {
        Ok(Some(p)) => ok!(serde_json::to_value(p).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "User profile not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn put_user_profile(
    State(s): State<AppState>,
    Json(p): Json<UserProfile>,
) -> (StatusCode, Json<serde_json::Value>) {
    user_profile_repo::UserProfileRepo::upsert(&s.pool, &p)
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |_| ok!(serde_json::json!({"ok":true})),
        )
}
pub async fn get_company_profile(
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match opc_profile_repo::OPCProfileRepo::get(&s.pool, "default-opc").await {
        Ok(Some(p)) => ok!(serde_json::to_value(p).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "OPC profile not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn put_company_profile(
    State(s): State<AppState>,
    Json(p): Json<OPCProfile>,
) -> (StatusCode, Json<serde_json::Value>) {
    opc_profile_repo::OPCProfileRepo::upsert(&s.pool, &p)
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |_| ok!(serde_json::json!({"ok":true})),
        )
}

// === Memory ===
pub async fn list_memory(
    State(s): State<AppState>,
    Query(q): Query<MemoryQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let res = if let Some(ref query) = q.q {
        memory_repo::MemoryRepo::search(&s.pool, query, q.scope.as_deref(), q.owner_id.as_deref())
            .await
    } else {
        memory_repo::MemoryRepo::list(
            &s.pool,
            q.scope.as_deref(),
            q.owner_id.as_deref(),
            q.include_revoked.unwrap_or(false),
        )
        .await
    };
    res.map_or_else(
        |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        |items| ok!(serde_json::to_value(items).unwrap()),
    )
}
pub async fn create_memory(
    State(s): State<AppState>,
    Json(m): Json<MemoryRecord>,
) -> (StatusCode, Json<serde_json::Value>) {
    match memory_repo::MemoryRepo::create(&s.pool, &m).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("provenance") {
                err!(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "MEMORY_PROVENANCE_REQUIRED"
                )
            } else {
                err!(StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
        }
    }
}
pub async fn stale_memory(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    memory_repo::MemoryRepo::mark_stale(&s.pool, &id).await.ok();
    ok!(serde_json::json!({"ok":true}))
}
pub async fn revoke_memory(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    memory_repo::MemoryRepo::revoke(&s.pool, &id).await.ok();
    ok!(serde_json::json!({"ok":true}))
}

// === Employees ===
pub async fn list_employees(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    agent_employee_repo::AgentEmployeeRepo::list(&s.pool)
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |items| ok!(serde_json::to_value(items).unwrap()),
        )
}
pub async fn seed_employees_handler(
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match agent_employee_repo::AgentEmployeeRepo::seed(&s.pool).await {
        Ok(()) => {
            let count = agent_employee_repo::AgentEmployeeRepo::list(&s.pool)
                .await
                .map(|v| v.len())
                .unwrap_or(0);
            ok!(serde_json::json!({"ok":true,"inserted":count,"total":count}))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn get_agent_memory(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match agent_memory_repo::AgentMemoryRepo::get(&s.pool, &id).await {
        Ok(Some(m)) => ok!(serde_json::to_value(m).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Agent memory not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// === Executors ===
pub async fn list_executors(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    executor_repo::ExecutorRepo::list(&s.pool)
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |items| ok!(serde_json::to_value(items).unwrap()),
        )
}
pub async fn register_executor(
    State(s): State<AppState>,
    Json(p): Json<ExternalExecutorPassport>,
) -> (StatusCode, Json<serde_json::Value>) {
    executor_repo::ExecutorRepo::register(&s.pool, &p)
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |_| ok!(serde_json::json!({"ok":true})),
        )
}
pub async fn disable_executor(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    executor_repo::ExecutorRepo::disable(&s.pool, &id)
        .await
        .ok();
    ok!(serde_json::json!({"ok":true}))
}
pub async fn executor_health(
    State(_s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::json!({"executor_id":id,"online":true,"latency_ms":1,"version":"mock-1.0"}))
}
pub async fn executor_dry_run(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecutorDryRunReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let ex = match executor_repo::ExecutorRepo::get(&s.pool, &id).await {
        Ok(Some(e)) => e,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "Executor not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if ex.status != ExecutorStatus::Registered {
        return err!(StatusCode::FORBIDDEN, "Executor not registered");
    }
    let wo = match work_order_repo::WorkOrderRepo::get(&s.pool, &req.work_order_id).await {
        Ok(Some(w)) => w,
        _ => return err!(StatusCode::NOT_FOUND, "Work order not found"),
    };
    let risk = track_risk(&wo.track);
    if ex.risk_ceiling < risk {
        return err!(
            StatusCode::FORBIDDEN,
            format!("risk_ceiling {} < track risk {}", ex.risk_ceiling, risk)
        );
    }
    let adapter = make_executor(&ex.source_type);
    match adapter {
        Some(a) => match a.dry_run(&wo).await {
            Ok(r) => ok!(serde_json::to_value(r).unwrap()),
            Err(e) => err!(StatusCode::FORBIDDEN, e.to_string()),
        },
        None => ok!(
            serde_json::json!({"passed":true,"estimated_cost_usd":0.01,"estimated_duration_ms":100,"warnings":[]})
        ),
    }
}

// === Work Orders ===
pub async fn create_work_order(
    State(s): State<AppState>,
    Json(req): Json<CreateWORequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.contract_hash.is_empty() || req.plan_hash.is_empty() {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "contract_hash and plan_hash required"
        );
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let track_decision = classify_mission_track(&req.mission_intent);
    let wo = WorkOrder {
        work_order_id: req
            .work_order_id
            .unwrap_or_else(|| format!("wo-{}", uuid::Uuid::new_v4())),
        conversation_id: req.conversation_id,
        contract_hash: req.contract_hash,
        plan_hash: req.plan_hash,
        user_id: req.user_id,
        opc_id: req.opc_id,
        mission_intent: req.mission_intent,
        selected_agents: req.selected_agents,
        selected_executors: req.selected_executors,
        required_skills: req.required_skills,
        track: track_decision.track.to_string(),
        status: WorkOrderStatus::Planned,
        allowed_actions: track_decision.allowed_actions,
        restricted_actions: track_decision.restricted_actions,
        risk_summary: track_decision.risk_summary,
        created_at_ms: now,
        updated_at_ms: now,
    };
    match work_order_repo::WorkOrderRepo::create(&s.pool, &wo).await {
        Ok(()) => ok!(
            serde_json::json!({"ok":true,"work_order_id":wo.work_order_id,"status":"Planned","track":wo.track,"risk_summary":wo.risk_summary,"allowed_actions":wo.allowed_actions,"restricted_actions":wo.restricted_actions,"created_at_ms":now})
        ),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn list_work_orders(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    work_order_repo::WorkOrderRepo::list(&s.pool)
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |items| ok!(serde_json::to_value(items).unwrap()),
        )
}

pub async fn execute_work_order(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // 1. Load work order
    let wo = match work_order_repo::WorkOrderRepo::get(&s.pool, &id).await {
        Ok(Some(w)) => w,
        _ => return err!(StatusCode::NOT_FOUND, "Work order not found"),
    };
    // 2. Validate hashes
    if wo.contract_hash.is_empty() || wo.plan_hash.is_empty() {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Missing contract_hash/plan_hash"
        );
    }
    // Alpha: Red Track always blocks before any worker, agent, or executor validation can proceed.
    if wo.track == "red" {
        return err!(StatusCode::FORBIDDEN, "RED_TRACK_BLOCKED_UNTIL_PRODUCTION_VERIFIER: Alpha does not support Red Track execution. Requires production MFA, dual-sign, and emergency lease verifier.");
    }
    let risk = track_risk(&wo.track);

    // 3. Validate agents
    let employees = agent_employee_repo::AgentEmployeeRepo::list(&s.pool)
        .await
        .unwrap_or_default();
    for aid in &wo.selected_agents {
        let emp = employees
            .iter()
            .find(|e| &e.agent_id == aid)
            .ok_or_else(|| {
                (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({"error":format!("Agent {} not found",aid)})),
                )
            });
        if let Err(e) = emp {
            return e;
        }
        let emp = emp.unwrap();
        if emp.lifecycle_status != LifecycleStatus::Active {
            return err!(StatusCode::FORBIDDEN, format!("Agent {} not Active", aid));
        }
        if emp.risk_ceiling < risk {
            return err!(
                StatusCode::FORBIDDEN,
                format!(
                    "Agent {} risk_ceiling {} < track risk {}",
                    aid, emp.risk_ceiling, risk
                )
            );
        }
        if emp.permission_boundary.max_risk_score < risk {
            return err!(
                StatusCode::FORBIDDEN,
                format!(
                    "Agent {} max_risk_score {} < track risk {}",
                    aid, emp.permission_boundary.max_risk_score, risk
                )
            );
        }
    }
    // 4. Validate executors
    for eid in &wo.selected_executors {
        let ex = executor_repo::ExecutorRepo::get(&s.pool, eid)
            .await
            .map_or(None, |x| x);
        let ex = match ex {
            Some(e) => e,
            None => return err!(StatusCode::FORBIDDEN, format!("Executor {} not found", eid)),
        };
        if ex.status != ExecutorStatus::Registered {
            return err!(
                StatusCode::FORBIDDEN,
                format!("Executor {} not registered", eid)
            );
        }
        if ex.risk_ceiling < risk {
            return err!(
                StatusCode::FORBIDDEN,
                format!(
                    "Executor {} risk_ceiling {} < track risk {}",
                    eid, ex.risk_ceiling, risk
                )
            );
        }
    }
    // 5. Yellow Track: require a real approval receipt anchored to a structurally valid WorkOrder.
    if wo.track == "yellow" {
        let contract = match ContractRepo::find_by_hash(&s.pool, &wo.contract_hash).await {
            Ok(Some(c)) => c,
            Ok(None) => {
                return err!(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "CONTRACT_ANCHOR_REQUIRED_FOR_APPROVAL: compile and persist the contract before Yellow Track execution"
                )
            }
            Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        let receipt = yellow_approval_receipt(&req);
        if receipt.is_none() {
            let approval_id = match ApprovalRepo::create(
                &s.pool,
                &wo.contract_hash,
                &format!("urn:coevo:work-order:{}:execute", id),
                "NEGATIVE_CONSENT",
                &wo.user_id,
                300_000,
            )
            .await
            {
                Ok(id) => id,
                Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            };
            work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "WaitingApproval")
                .await
                .ok();
            return ok!(serde_json::json!({
                "ok":true,
                "status":"WaitingApproval",
                "approval_id":approval_id,
                "approval_mode":"NEGATIVE_CONSENT",
                "contract_hash":contract.contract_hash,
                "action_urn":format!("urn:coevo:work-order:{}:execute", id),
                "message":"Yellow Track requires an approved approval receipt before execution."
            }));
        }
        let receipt_id = receipt.unwrap();
        let approval = match ApprovalRepo::find_by_id(&s.pool, receipt_id).await {
            Ok(Some(a)) => a,
            Ok(None) => return err!(StatusCode::FORBIDDEN, "APPROVAL_RECEIPT_NOT_FOUND"),
            Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        if approval.contract_hash != wo.contract_hash {
            return err!(StatusCode::FORBIDDEN, "APPROVAL_RECEIPT_CONTRACT_MISMATCH");
        }
        let action_urn = format!("urn:coevo:work-order:{}:execute", id);
        if approval.action_urn != action_urn {
            return err!(StatusCode::FORBIDDEN, "APPROVAL_RECEIPT_ACTION_MISMATCH");
        }
        if approval.expires_at_ms < chrono::Utc::now().timestamp_millis() {
            return err!(StatusCode::FORBIDDEN, "APPROVAL_RECEIPT_EXPIRED");
        }
        if approval.status != "approved" {
            return err!(StatusCode::FORBIDDEN, "APPROVAL_RECEIPT_NOT_APPROVED");
        }
    }
    // 6. Green/Yellow with approval: use WorkerHarness
    let harness_result = match WorkerHarness::run_work_order(
        &s.pool,
        &id,
        WorkerHarnessOptions {
            approval_receipt: req.caller_identity_proof.clone().or(req.lease_id.clone()),
            max_runtime_ms: Some(60000),
            deterministic_mode: true,
            preferred_tool_ids: vec![],
            allow_mock_model_routing: false,
        },
    )
    .await
    {
        Ok(hr) => hr,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("MODEL_PROVIDER_NOT_CONFIGURED") {
                return err!(StatusCode::CONFLICT, msg);
            }
            return err!(StatusCode::INTERNAL_SERVER_ERROR, msg);
        }
    };
    // Load worker session IDs from DB
    let session_rows = sqlx::query(
        "SELECT session_id FROM worker_sessions WHERE work_order_id=? ORDER BY created_at_ms",
    )
    .bind(&id)
    .fetch_all(&s.pool)
    .await
    .unwrap_or_default();
    let worker_session_ids: Vec<String> = session_rows
        .iter()
        .map(|r| r.get::<String, _>("session_id"))
        .collect();
    let synthesized =
        "WorkerHarness completed execution. Results were persisted to Task Memory.".to_string();
    if let Err(e) =
        work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, &harness_result.status).await
    {
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update status: {}", e)
        );
    }
    ok!(serde_json::json!({
        "ok":true,"status":harness_result.status,"summary":harness_result.summary,
        "worker_session_ids":worker_session_ids,"synthesized_summary":synthesized,
        "worker_runs":harness_result.worker_runs,"worker_steps":harness_result.worker_steps,
        "worker_events":harness_result.worker_events,"skill_usage":harness_result.skill_usage,
        "tool_calls":harness_result.tool_calls,"memory_ids":harness_result.memory_ids,
        "reflection_id":harness_result.reflection_id,"proposal_id":harness_result.proposal_id
    }))
}

pub async fn cancel_work_order(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Cancelled")
        .await
        .ok();
    ok!(serde_json::json!({"ok":true}))
}
pub async fn work_order_feedback(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<WorkOrderFeedback>,
) -> (StatusCode, Json<serde_json::Value>) {
    let analysis = FailureAnalyzer::analyze(&req.feedback, false, false, None);
    let proposal = SkillGenerator::generate_from_failure(
        &analysis,
        "skill-mission-draft",
        req.agent_id.as_deref().unwrap_or("system"),
    );
    if let Err(e) =
        skill_evolution_repo::SkillEvolutionRepo::create_proposal(&s.pool, &proposal).await
    {
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create proposal: {}", e)
        );
    }
    if let Err(e) = work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Failed").await {
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update status: {}", e)
        );
    }
    ok!(serde_json::json!({"ok":true,"proposal_id":proposal.proposal_id}))
}

// === Skills ===
pub async fn list_skills(
    State(s): State<AppState>,
    Query(q): Query<SkillsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    skill_repo::SkillRepo::list(&s.pool, q.agent_id.as_deref())
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |items| ok!(serde_json::to_value(items).unwrap()),
        )
}
pub async fn seed_skills(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    skill_repo::SkillRepo::seed_default(&s.pool).await.ok();
    ok!(serde_json::json!({"ok":true}))
}
pub async fn activate_skill(
    State(s): State<AppState>,
    Path((id, ver)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    match skill_repo::SkillRepo::activate(&s.pool, &id, &ver).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => err!(StatusCode::FORBIDDEN, e.to_string()),
    }
}
pub async fn rollback_skill(
    State(s): State<AppState>,
    Path((id, ver)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = skill_repo::SkillRepo::rollback(&s.pool, &id, &ver).await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let vr = SkillVersionRecord {
        skill_id: id.clone(),
        version: ver.clone(),
        parent_version: "active".into(),
        diff_summary: "rollback".into(),
        change_reason: "manual rollback".into(),
        verifier_result: None,
        approved_by: Some("api".into()),
        rollback_available: true,
        created_at_ms: now,
    };
    if let Err(e) = skill_evolution_repo::SkillEvolutionRepo::record_version(&s.pool, &vr).await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    ok!(serde_json::json!({"ok":true}))
}

// === Skill Evolution ===
pub async fn list_proposals(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None)
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |items| ok!(serde_json::to_value(items).unwrap()),
        )
}
pub async fn run_evolution(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let p = SkillEvolutionProposal {
        proposal_id: format!("evol-{}", uuid::Uuid::new_v4()),
        source_type: EvolutionSourceType::AgentReflection,
        source_refs: vec![],
        target_skill_id: "skill-mission-draft".into(),
        proposal_type: EvolutionProposalType::PatchSkill,
        diagnosis: "scheduled evolution run".into(),
        proposed_changes: "auto-patch".into(),
        expected_benefit: "improve".into(),
        risk_assessment: "LOW".into(),
        generated_tests: vec![],
        status: EvolutionProposalStatus::Draft,
        created_by_agent: "scheduler".into(),
        created_at_ms: now,
    };
    skill_evolution_repo::SkillEvolutionRepo::create_proposal(&s.pool, &p)
        .await
        .ok();
    ok!(serde_json::to_value(&p).unwrap())
}
pub async fn verify_proposal(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None)
        .await
        .unwrap_or_default();
    let proposal = match proposals.into_iter().find(|p| p.proposal_id == id) {
        Some(p) => p,
        None => return err!(StatusCode::NOT_FOUND, "Proposal not found"),
    };
    let skill = skill_repo::SkillRepo::get(&s.pool, &proposal.target_skill_id, None)
        .await
        .ok()
        .flatten();
    let eval = SkillVerifier::verify(&proposal, skill.as_ref());
    skill_evolution_repo::SkillEvolutionRepo::append_eval(&s.pool, &eval)
        .await
        .ok();
    let new_status = if eval.passed && !proposal.risk_assessment.contains("HIGH") {
        "Approved"
    } else {
        "NeedsHumanReview"
    };
    skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, new_status)
        .await
        .ok();
    ok!(serde_json::to_value(&eval).unwrap())
}
pub async fn approve_proposal(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None)
        .await
        .unwrap_or_default();
    let proposal = match proposals.into_iter().find(|p| p.proposal_id == id) {
        Some(p) => p,
        None => return err!(StatusCode::NOT_FOUND, "Proposal not found"),
    };
    // HIGH risk requires human
    if proposal.risk_assessment.to_uppercase().contains("HIGH")
        || proposal.risk_assessment.to_uppercase().contains("RED")
    {
        return err!(
            StatusCode::FORBIDDEN,
            "HIGH/RED risk skill proposal requires explicit human approval marker"
        );
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    match proposal.proposal_type {
        EvolutionProposalType::CreateNewSkill => {
            let sk = AgentSkillPackage {
                skill_id: proposal.target_skill_id.clone(),
                version: "1.0.0".into(),
                name: proposal.target_skill_id.clone(),
                owner_agent_id: proposal.created_by_agent.clone(),
                department: "Custom".into(),
                description: proposal.diagnosis.clone(),
                trigger_patterns: vec![],
                applicable_domains: vec![],
                required_tools: vec![],
                required_model_profile: None,
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                prompt_template: proposal.proposed_changes.clone(),
                procedure_steps: vec![],
                guardrails: vec!["no escalation".into()],
                examples: vec![],
                tests: proposal.generated_tests.clone(),
                evals: vec![],
                permissions_required: vec![],
                allowed_cognitive_layers: vec!["Hypothesis".into()],
                allowed_action_modes: vec!["DRAFT_ONLY".into()],
                risk_ceiling: 0.3,
                provenance: format!("skill-evolution-{}", proposal.proposal_id),
                status: SkillStatus::Active,
                created_at_ms: now,
                updated_at_ms: now,
            };
            skill_repo::SkillRepo::upsert(&s.pool, &sk).await.ok();
        }
        EvolutionProposalType::PatchSkill => {
            let existing = skill_repo::SkillRepo::get(&s.pool, &proposal.target_skill_id, None)
                .await
                .ok()
                .flatten();
            let mut patched = existing.unwrap_or_else(|| AgentSkillPackage {
                skill_id: proposal.target_skill_id.clone(),
                version: "1.0.0".into(),
                name: proposal.target_skill_id.clone(),
                owner_agent_id: proposal.created_by_agent.clone(),
                department: "Custom".into(),
                description: String::new(),
                trigger_patterns: vec![],
                applicable_domains: vec![],
                required_tools: vec![],
                required_model_profile: None,
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                prompt_template: String::new(),
                procedure_steps: vec![],
                guardrails: vec![],
                examples: vec![],
                tests: vec![],
                evals: vec![],
                permissions_required: vec![],
                allowed_cognitive_layers: vec![],
                allowed_action_modes: vec![],
                risk_ceiling: 0.3,
                provenance: String::new(),
                status: SkillStatus::Draft,
                created_at_ms: now,
                updated_at_ms: now,
            });
            // Bump version, apply patch
            let parts: Vec<u32> = patched
                .version
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();
            let new_ver = if parts.len() >= 3 {
                format!("{}.{}.{}", parts[0], parts[1], parts[2] + 1)
            } else {
                "1.1.0".to_string()
            };
            patched.version = new_ver.clone();
            patched.prompt_template = proposal.proposed_changes.clone();
            patched.status = SkillStatus::Active;
            patched.updated_at_ms = now;
            patched.provenance = format!("skill-evolution-{}", proposal.proposal_id);
            skill_repo::SkillRepo::upsert(&s.pool, &patched).await.ok();
        }
        EvolutionProposalType::DeprecateSkill => {
            if let Ok(Some(mut sk)) =
                skill_repo::SkillRepo::get(&s.pool, &proposal.target_skill_id, None).await
            {
                sk.status = SkillStatus::Deprecated;
                sk.updated_at_ms = now;
                skill_repo::SkillRepo::upsert(&s.pool, &sk).await.ok();
            }
        }
        EvolutionProposalType::SplitSkill | EvolutionProposalType::MergeSkills => {
            return err!(
                StatusCode::UNPROCESSABLE_ENTITY,
                "NOT_IMPLEMENTED: SplitSkill/MergeSkills not supported yet"
            );
        }
    }
    let vr = SkillVersionRecord {
        skill_id: proposal.target_skill_id.clone(),
        version: "1.1.0".into(),
        parent_version: "1.0.0".into(),
        diff_summary: proposal.proposed_changes.clone(),
        change_reason: proposal.diagnosis.clone(),
        verifier_result: None,
        approved_by: Some("api".into()),
        rollback_available: true,
        created_at_ms: now,
    };
    if let Err(e) = skill_evolution_repo::SkillEvolutionRepo::record_version(&s.pool, &vr).await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    if let Err(e) =
        skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, "Applied").await
    {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    ok!(serde_json::json!({"ok":true,"proposal_id":id}))
}
pub async fn reject_proposal(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, "Rejected")
        .await
        .ok();
    ok!(serde_json::json!({"ok":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::timeline::work_order_audit_export;
    use crate::state::AppState;
    use coevo_core::contract::*;
    use coevo_store::repos::{approval_repo::ApprovalRepo, contract_repo::ContractRepo};
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    async fn count_rows(pool: &sqlx::SqlitePool, table: &str, work_order_id: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) AS n FROM {} WHERE work_order_id=?", table);
        sqlx::query_scalar::<_, i64>(&sql)
            .bind(work_order_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn create_yellow_work_order(
        state: AppState,
        work_order_id: &str,
        contract_hash: &str,
    ) {
        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: contract_hash.to_string(),
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Draft an internal update".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };
        let (create_status, Json(created)) = create_work_order(State(state), Json(create)).await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        assert_eq!(created["track"], "yellow");
    }

    async fn configure_active_openai_compatible(pool: &sqlx::SqlitePool) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("desktop-test")
            .bind("OpenAICompatible")
            .bind("https://api.openai.com/v1")
            .bind("sk-test")
            .bind("sk-t****test")
            .bind("gpt-4o")
            .bind("gpt-4o-mini")
            .bind("o3-mini")
            .bind("gpt-4o")
            .bind(16384)
            .bind(0.2)
            .bind(30000)
            .bind(5.0)
            .bind(1)
            .bind(now)
            .bind(now)
            .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn classify_mission_track_is_server_authoritative_and_red_takes_priority() {
        let cases = [
            ("production database rollback", "red", "high-risk trigger"),
            ("critical P1 emergency", "red", "high-risk trigger"),
            ("customer data delete request", "red", "high-risk trigger"),
            ("draft a changelog and send it internally", "yellow", "moderate-risk trigger"),
            ("update staging release notes", "yellow", "moderate-risk trigger"),
            ("read metrics and analyze logs", "green", "Green Track"),
        ];

        for (intent, expected_track, summary_fragment) in cases {
            let decision = classify_mission_track(intent);
            assert_eq!(decision.track, expected_track, "{intent}");
            assert!(decision.risk_summary.contains(summary_fragment), "{intent}");
        }

        let priority = classify_mission_track("send a production notification");
        assert_eq!(priority.track, "red");
        assert!(priority.restricted_actions.contains(&"production".to_string()));
    }

    #[tokio::test]
    async fn red_execute_returns_alpha_block_without_worker_audit_rows() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let work_order_id = "wo-red-alpha-block";

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Delete production customer data".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };
        let (create_status, _) = create_work_order(State(state.clone()), Json(create)).await;
        assert_eq!(create_status, StatusCode::OK);

        let (status, Json(body)) = execute_work_order(
            State(state),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: Some("real-looking-proof".to_string()),
                monitoring_signature: Some("real-looking-monitoring".to_string()),
                diagnostic_signature: Some("real-looking-diagnostic".to_string()),
                lease_id: Some("lease-real-looking".to_string()),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("RED_TRACK_BLOCKED_UNTIL_PRODUCTION_VERIFIER"));
        assert_eq!(count_rows(&pool, "worker_sessions", work_order_id).await, 0);
        assert_eq!(count_rows(&pool, "worker_runs", work_order_id).await, 0);
    }

    #[tokio::test]
    async fn create_work_order_overrides_client_track_with_server_classification() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let work_order_id = "wo-server-classifies-red";

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Delete production customer data".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };

        let (status, Json(body)) = create_work_order(State(state), Json(create)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["track"], "red");
        assert!(body["risk_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("high-risk trigger"));
        let stored = work_order_repo::WorkOrderRepo::get(&pool, work_order_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.track, "red");
        assert!(stored.restricted_actions.contains(&"delete".to_string()));
        assert!(stored.restricted_actions.contains(&"production".to_string()));
    }

    #[tokio::test]
    async fn green_execute_uses_scoped_file_readonly_tool() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        agent_employee_repo::AgentEmployeeRepo::seed(&pool).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let work_order_id = "wo-green-file-readonly";
        let root = std::env::temp_dir().join(format!("coevo-readonly-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mission-notes.md"), "launch readiness evidence").unwrap();
        std::env::set_var("COEVO_WORKSPACE_DIR", &root);

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Analyze mission-notes.md for launch readiness".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };
        let (create_status, _) = create_work_order(State(state.clone()), Json(create)).await;
        assert_eq!(create_status, StatusCode::OK);

        let (status, Json(body)) = execute_work_order(
            State(state),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;

        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["status"], "Completed");
        let tool_row = sqlx::query(
            "SELECT tool_id, success, output_summary FROM worker_tool_calls WHERE run_id IN (SELECT run_id FROM worker_runs WHERE work_order_id=?)",
        )
        .bind(work_order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_row.get::<String, _>("tool_id"), "file-readonly");
        assert_eq!(tool_row.get::<i64, _>("success"), 1);
        assert!(tool_row
            .get::<String, _>("output_summary")
            .contains("launch readiness evidence"));
    }

    #[tokio::test]
    async fn green_execute_requires_active_model_provider_config() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let work_order_id = "wo-green-provider-required";

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Analyze README.md".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };
        let (create_status, Json(created)) = create_work_order(State(state.clone()), Json(create)).await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");

        let (status, Json(body)) = execute_work_order(
            State(state),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("MODEL_PROVIDER_NOT_CONFIGURED"));
        assert_eq!(count_rows(&pool, "worker_runs", work_order_id).await, 0);
    }

    #[tokio::test]
    async fn green_execute_routes_model_calls_to_active_provider_config_not_mock() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        agent_employee_repo::AgentEmployeeRepo::seed(&pool).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let work_order_id = "wo-green-active-model-routing";
        let root = std::env::temp_dir().join(format!("coevo-active-model-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mission-notes.md"), "active model routing evidence").unwrap();
        std::env::set_var("COEVO_WORKSPACE_DIR", &root);

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Analyze mission-notes.md for model routing".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };
        let (create_status, Json(created)) = create_work_order(State(state.clone()), Json(create)).await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");

        let (status, Json(body)) = execute_work_order(
            State(state),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;

        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(status, StatusCode::OK, "{body:?}");
        let step_rows = sqlx::query(
            "SELECT output_json FROM worker_steps WHERE step_type='ModelCall' AND run_id IN (SELECT run_id FROM worker_runs WHERE work_order_id=?)",
        )
        .bind(work_order_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(!step_rows.is_empty());
        for row in step_rows {
            let output: String = row.get("output_json");
            let decision: serde_json::Value = serde_json::from_str(&output).unwrap();
            assert_eq!(decision["selected_provider_id"], "desktop-test");
            assert_ne!(decision["selected_model_id"], "mock-fast");
            assert_ne!(decision["selected_model_id"], "mock-reasoning");
        }
    }

    #[tokio::test]
    async fn audit_export_includes_work_order_execution_and_memory_evidence() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        agent_employee_repo::AgentEmployeeRepo::seed(&pool).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let work_order_id = "wo-audit-export";
        let root = std::env::temp_dir().join(format!("coevo-audit-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mission-notes.md"), "audit export evidence").unwrap();
        std::env::set_var("COEVO_WORKSPACE_DIR", &root);

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Analyze mission-notes.md for audit export".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };
        let (create_status, _) = create_work_order(State(state.clone()), Json(create)).await;
        assert_eq!(create_status, StatusCode::OK);
        let (execute_status, _) = execute_work_order(
            State(state.clone()),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(execute_status, StatusCode::OK);

        let (export_status, Json(export)) =
            work_order_audit_export(State(state), Path(work_order_id.to_string())).await;

        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(export_status, StatusCode::OK, "{export:?}");
        assert_eq!(export["schema_version"], "coevo.audit_export.v1");
        assert_eq!(export["work_order"]["work_order_id"], work_order_id);
        assert_eq!(export["governance"]["track"], "green");
        assert!(export["worker_runs"].as_array().unwrap().len() >= 1);
        assert!(export["worker_steps"].as_array().unwrap().len() >= 1);
        assert!(export["worker_events"].as_array().unwrap().len() >= 1);
        assert!(export["tool_calls"].as_array().unwrap().iter().any(|tc| tc["tool_id"] == "file-readonly"));
        assert!(export["memory_records"].as_array().unwrap().iter().any(|m| {
            m["provenance"]
                .as_str()
                .unwrap_or_default()
                .starts_with("worker-run-")
        }));
    }

    async fn insert_contract(pool: &sqlx::SqlitePool, hash: &str) {
        let contract = MCLSpec {
            mcl_version: "1.0".to_string(),
            mcl_state: ContractState::DraftContract,
            parent_contract_hash: "0".repeat(64),
            goal_tree: GoalTree {
                root: GoalNode {
                    id: "root".to_string(),
                    description: "test yellow approval".to_string(),
                    status: GoalStatus::Pending,
                    children: vec![],
                    depends_on: vec![],
                },
            },
            institution_policy_hash: "0".repeat(64),
            data_boundary: vec![],
            allowed_action_modes: vec![ActionMode::DraftOnly],
            human_approval_policy: HumanApprovalPolicy {
                approval_mode: ApprovalMode::NegativeConsent,
                authorized_roles: vec!["Admin".to_string()],
                negative_consent_timeout_secs: 300,
                mfa_auth_url: None,
            },
            evidence_requirement: EvidenceRequirement {
                minimum_level: "none".to_string(),
                require_json_report: false,
            },
            risk_tolerance_profile: RiskToleranceProfile {
                max_risk_score: 0.6,
                allow_emergency_lease: false,
            },
            termination_policy: TerminationPolicy {
                max_token_budget: 10000,
                max_hops: 3,
                max_latency_ms: 60000,
                max_stance_rounds: 3,
            },
            responsibility_anchor_policy: ResponsibilityAnchorPolicy {
                required_human_roles: vec!["Admin".to_string()],
                agent_forbidden_actions: vec![],
            },
        };
        ContractRepo::insert(pool, &contract, hash).await.unwrap();
    }

    #[tokio::test]
    async fn yellow_execute_creates_approval_request_and_rejects_unapproved_receipts() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        agent_employee_repo::AgentEmployeeRepo::seed(&pool).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let work_order_id = "wo-yellow-approval";
        let contract_hash = "c".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: contract_hash.clone(),
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Draft a changelog update for internal release".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };
        let (create_status, Json(created)) = create_work_order(State(state.clone()), Json(create)).await;
        assert_eq!(create_status, StatusCode::OK);
        assert_eq!(created["track"], "yellow");

        let (wait_status, Json(wait_body)) = execute_work_order(
            State(state.clone()),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(wait_status, StatusCode::OK);
        assert_eq!(wait_body["status"], "WaitingApproval");
        let approval_id = wait_body["approval_id"].as_str().unwrap();
        let approval = ApprovalRepo::find_by_id(&pool, approval_id).await.unwrap().unwrap();
        assert_eq!(approval.status, "pending");
        assert_eq!(approval.contract_hash, contract_hash);

        let (blocked_status, Json(blocked_body)) = execute_work_order(
            State(state.clone()),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: Some(approval_id.to_string()),
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(blocked_status, StatusCode::FORBIDDEN);
        assert!(blocked_body["error"].as_str().unwrap_or_default().contains("APPROVAL_RECEIPT_NOT_APPROVED"));
        assert_eq!(count_rows(&pool, "worker_sessions", work_order_id).await, 0);

        ApprovalRepo::approve(&pool, approval_id, "default-founder").await.unwrap();
        let (execute_status, Json(execute_body)) = execute_work_order(
            State(state),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: Some(approval_id.to_string()),
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(execute_status, StatusCode::OK, "{execute_body:?}");
        assert_eq!(execute_body["status"], "Completed");
        assert!(count_rows(&pool, "worker_sessions", work_order_id).await >= 1);
    }

    #[tokio::test]
    async fn yellow_execute_rejects_arbitrary_identity_string_as_approval_receipt() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let work_order_id = "wo-yellow-no-freeform-proof";
        let contract_hash = "f".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash,
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Draft an internal update".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
        };
        let (create_status, Json(created)) =
            create_work_order(State(state.clone()), Json(create)).await;
        assert_eq!(create_status, StatusCode::OK);
        assert_eq!(created["track"], "yellow");

        let (status, Json(body)) = execute_work_order(
            State(state),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: Some("yes".to_string()),
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("APPROVAL_RECEIPT_NOT_FOUND"));
        assert_eq!(count_rows(&pool, "worker_sessions", work_order_id).await, 0);
    }

    #[tokio::test]
    async fn yellow_execute_rejects_expired_denied_or_wrong_action_receipts() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone());
        let contract_hash = "e".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let expired_wo = "wo-yellow-expired-receipt";
        create_yellow_work_order(state.clone(), expired_wo, &contract_hash).await;
        let expired_id = ApprovalRepo::create(
            &pool,
            &contract_hash,
            &format!("urn:coevo:work-order:{}:execute", expired_wo),
            "NEGATIVE_CONSENT",
            "default-founder",
            -1,
        )
        .await
        .unwrap();
        ApprovalRepo::approve(&pool, &expired_id, "default-founder")
            .await
            .unwrap();
        let (expired_status, Json(expired_body)) = execute_work_order(
            State(state.clone()),
            Path(expired_wo.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: Some(expired_id),
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(expired_status, StatusCode::FORBIDDEN);
        assert!(expired_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("APPROVAL_RECEIPT_EXPIRED"));
        assert_eq!(count_rows(&pool, "worker_sessions", expired_wo).await, 0);

        let denied_wo = "wo-yellow-denied-receipt";
        create_yellow_work_order(state.clone(), denied_wo, &contract_hash).await;
        let denied_id = ApprovalRepo::create(
            &pool,
            &contract_hash,
            &format!("urn:coevo:work-order:{}:execute", denied_wo),
            "NEGATIVE_CONSENT",
            "default-founder",
            300_000,
        )
        .await
        .unwrap();
        ApprovalRepo::deny(&pool, &denied_id, "default-founder")
            .await
            .unwrap();
        let (denied_status, Json(denied_body)) = execute_work_order(
            State(state.clone()),
            Path(denied_wo.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: Some(denied_id),
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(denied_status, StatusCode::FORBIDDEN);
        assert!(denied_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("APPROVAL_RECEIPT_NOT_APPROVED"));
        assert_eq!(count_rows(&pool, "worker_sessions", denied_wo).await, 0);

        let action_mismatch_wo = "wo-yellow-wrong-action-receipt";
        create_yellow_work_order(state.clone(), action_mismatch_wo, &contract_hash).await;
        let wrong_action_id = ApprovalRepo::create(
            &pool,
            &contract_hash,
            "urn:coevo:work-order:other-work-order:execute",
            "NEGATIVE_CONSENT",
            "default-founder",
            300_000,
        )
        .await
        .unwrap();
        ApprovalRepo::approve(&pool, &wrong_action_id, "default-founder")
            .await
            .unwrap();
        let (mismatch_status, Json(mismatch_body)) = execute_work_order(
            State(state),
            Path(action_mismatch_wo.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: Some(wrong_action_id),
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(mismatch_status, StatusCode::FORBIDDEN);
        assert!(mismatch_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("APPROVAL_RECEIPT_ACTION_MISMATCH"));
        assert_eq!(count_rows(&pool, "worker_sessions", action_mismatch_wo).await, 0);
    }
}
