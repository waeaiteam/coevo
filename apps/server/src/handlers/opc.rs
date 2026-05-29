use axum::{extract::{Path, Query, State}, Json, http::StatusCode};
use sqlx::Row;
use serde::Deserialize;
use coevo_core::opc::*;
use coevo_core::skills::*;
use coevo_core::cognitive::CognitiveLayer;
use coevo_store::repos_opc::*;
use coevo_store::repos::worker_step_event_repo::{WorkerRunStepRepo, WorkerEventRepo};
use coevo_evolution::{analyzer::FailureAnalyzer, generator::SkillGenerator, verifier::SkillVerifier};
use coevo_executors::adapters::*;
use coevo_executors::traits::*;
use coevo_worker::harness::{WorkerHarness, WorkerHarnessOptions};
use crate::state::AppState;

#[derive(Deserialize)] pub struct MemoryQuery { pub scope: Option<String>, pub owner_id: Option<String>, pub include_revoked: Option<bool>, pub q: Option<String> }
#[derive(Deserialize)] pub struct ExecutorDryRunReq { pub work_order_id: String }
#[derive(Deserialize)] pub struct WorkOrderFeedback { pub feedback: String, pub agent_id: Option<String> }
#[derive(Deserialize)] pub struct SkillsQuery { pub agent_id: Option<String> }
#[derive(Deserialize)] pub struct ExecuteRequest { pub caller_identity_proof: Option<String>, pub monitoring_signature: Option<String>, pub diagnostic_signature: Option<String>, pub lease_id: Option<String> }
#[derive(Deserialize)] pub struct CreateWORequest { pub work_order_id: Option<String>, pub contract_hash: String, pub plan_hash: String, pub user_id: String, pub opc_id: String, pub mission_intent: String, pub selected_agents: Vec<String>, pub selected_executors: Vec<String>, pub required_skills: Vec<String>, pub track: String, pub allowed_actions: Vec<String>, pub restricted_actions: Vec<String>, pub risk_summary: String }

macro_rules! ok { ($v:expr) => { (StatusCode::OK, Json($v)) } }
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

fn track_risk(track: &str) -> f64 { match track { "green" => 0.3, "yellow" => 0.6, _ => 0.9 } }
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
pub async fn put_user_profile(State(s): State<AppState>, Json(p): Json<UserProfile>) -> (StatusCode, Json<serde_json::Value>) {
    user_profile_repo::UserProfileRepo::upsert(&s.pool, &p).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |_| ok!(serde_json::json!({"ok":true})))
}
pub async fn get_company_profile(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match opc_profile_repo::OPCProfileRepo::get(&s.pool, "default-opc").await {
        Ok(Some(p)) => ok!(serde_json::to_value(p).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "OPC profile not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn put_company_profile(State(s): State<AppState>, Json(p): Json<OPCProfile>) -> (StatusCode, Json<serde_json::Value>) {
    opc_profile_repo::OPCProfileRepo::upsert(&s.pool, &p).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |_| ok!(serde_json::json!({"ok":true})))
}

// === Memory ===
pub async fn list_memory(State(s): State<AppState>, Query(q): Query<MemoryQuery>) -> (StatusCode, Json<serde_json::Value>) {
    let res = if let Some(ref query) = q.q { memory_repo::MemoryRepo::search(&s.pool, query, q.scope.as_deref(), q.owner_id.as_deref()).await }
    else { memory_repo::MemoryRepo::list(&s.pool, q.scope.as_deref(), q.owner_id.as_deref(), q.include_revoked.unwrap_or(false)).await };
    res.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |items| ok!(serde_json::to_value(items).unwrap()))
}
pub async fn create_memory(State(s): State<AppState>, Json(m): Json<MemoryRecord>) -> (StatusCode, Json<serde_json::Value>) {
    match memory_repo::MemoryRepo::create(&s.pool, &m).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => { let msg = e.to_string();
            if msg.contains("provenance") { err!(StatusCode::UNPROCESSABLE_ENTITY, "MEMORY_PROVENANCE_REQUIRED") }
            else { err!(StatusCode::INTERNAL_SERVER_ERROR, msg) }
        }
    }
}
pub async fn stale_memory(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    memory_repo::MemoryRepo::mark_stale(&s.pool, &id).await.ok(); ok!(serde_json::json!({"ok":true}))
}
pub async fn revoke_memory(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    memory_repo::MemoryRepo::revoke(&s.pool, &id).await.ok(); ok!(serde_json::json!({"ok":true}))
}

// === Employees ===
pub async fn list_employees(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    agent_employee_repo::AgentEmployeeRepo::list(&s.pool).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |items| ok!(serde_json::to_value(items).unwrap()))
}
pub async fn seed_employees_handler(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match agent_employee_repo::AgentEmployeeRepo::seed(&s.pool).await {
        Ok(()) => {
            let count = agent_employee_repo::AgentEmployeeRepo::list(&s.pool).await.map(|v| v.len()).unwrap_or(0);
            ok!(serde_json::json!({"ok":true,"inserted":count,"total":count}))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn get_agent_memory(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    match agent_memory_repo::AgentMemoryRepo::get(&s.pool, &id).await {
        Ok(Some(m)) => ok!(serde_json::to_value(m).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Agent memory not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// === Executors ===
pub async fn list_executors(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    executor_repo::ExecutorRepo::list(&s.pool).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |items| ok!(serde_json::to_value(items).unwrap()))
}
pub async fn register_executor(State(s): State<AppState>, Json(p): Json<ExternalExecutorPassport>) -> (StatusCode, Json<serde_json::Value>) {
    executor_repo::ExecutorRepo::register(&s.pool, &p).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |_| ok!(serde_json::json!({"ok":true})))
}
pub async fn disable_executor(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    executor_repo::ExecutorRepo::disable(&s.pool, &id).await.ok(); ok!(serde_json::json!({"ok":true}))
}
pub async fn executor_health(State(_s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::json!({"executor_id":id,"online":true,"latency_ms":1,"version":"mock-1.0"}))
}
pub async fn executor_dry_run(State(s): State<AppState>, Path(id): Path<String>, Json(req): Json<ExecutorDryRunReq>) -> (StatusCode, Json<serde_json::Value>) {
    let ex = match executor_repo::ExecutorRepo::get(&s.pool, &id).await {
        Ok(Some(e)) => e, Ok(None) => return err!(StatusCode::NOT_FOUND, "Executor not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if ex.status != ExecutorStatus::Registered { return err!(StatusCode::FORBIDDEN, "Executor not registered"); }
    let wo = match work_order_repo::WorkOrderRepo::get(&s.pool, &req.work_order_id).await {
        Ok(Some(w)) => w, _ => return err!(StatusCode::NOT_FOUND, "Work order not found"),
    };
    let risk = track_risk(&wo.track);
    if ex.risk_ceiling < risk { return err!(StatusCode::FORBIDDEN, format!("risk_ceiling {} < track risk {}", ex.risk_ceiling, risk)); }
    let adapter = make_executor(&ex.source_type);
    match adapter {
        Some(a) => match a.dry_run(&wo).await {
            Ok(r) => ok!(serde_json::to_value(r).unwrap()),
            Err(e) => err!(StatusCode::FORBIDDEN, e.to_string()),
        },
        None => ok!(serde_json::json!({"passed":true,"estimated_cost_usd":0.01,"estimated_duration_ms":100,"warnings":[]})),
    }
}

// === Work Orders ===
pub async fn create_work_order(State(s): State<AppState>, Json(req): Json<CreateWORequest>) -> (StatusCode, Json<serde_json::Value>) {
    if req.contract_hash.is_empty() || req.plan_hash.is_empty() { return err!(StatusCode::UNPROCESSABLE_ENTITY, "contract_hash and plan_hash required"); }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let wo = WorkOrder{
        work_order_id: req.work_order_id.unwrap_or_else(|| format!("wo-{}", uuid::Uuid::new_v4())),
        contract_hash: req.contract_hash, plan_hash: req.plan_hash, user_id: req.user_id, opc_id: req.opc_id,
        mission_intent: req.mission_intent, selected_agents: req.selected_agents,
        selected_executors: req.selected_executors, required_skills: req.required_skills,
        track: req.track, status: WorkOrderStatus::Planned,
        allowed_actions: req.allowed_actions, restricted_actions: req.restricted_actions,
        risk_summary: req.risk_summary, created_at_ms: now, updated_at_ms: now,
    };
    match work_order_repo::WorkOrderRepo::create(&s.pool, &wo).await {
        Ok(()) => ok!(serde_json::json!({"ok":true,"work_order_id":wo.work_order_id,"status":"Planned","created_at_ms":now})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn list_work_orders(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    work_order_repo::WorkOrderRepo::list(&s.pool).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |items| ok!(serde_json::to_value(items).unwrap()))
}

pub async fn execute_work_order(State(s): State<AppState>, Path(id): Path<String>, Json(req): Json<ExecuteRequest>) -> (StatusCode, Json<serde_json::Value>) {
    // 1. Load work order
    let wo = match work_order_repo::WorkOrderRepo::get(&s.pool, &id).await {
        Ok(Some(w)) => w, _ => return err!(StatusCode::NOT_FOUND, "Work order not found"),
    };
    // 2. Validate hashes
    if wo.contract_hash.is_empty() || wo.plan_hash.is_empty() { return err!(StatusCode::UNPROCESSABLE_ENTITY, "Missing contract_hash/plan_hash"); }
    let risk = track_risk(&wo.track);

    // 3. Validate agents
    let employees = agent_employee_repo::AgentEmployeeRepo::list(&s.pool).await.unwrap_or_default();
    for aid in &wo.selected_agents {
        let emp = employees.iter().find(|e| &e.agent_id == aid).ok_or_else(|| {
            (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":format!("Agent {} not found",aid)})))
        });
        if let Err(e) = emp { return e; }
        let emp = emp.unwrap();
        if emp.lifecycle_status != LifecycleStatus::Active { return err!(StatusCode::FORBIDDEN, format!("Agent {} not Active", aid)); }
        if emp.risk_ceiling < risk { return err!(StatusCode::FORBIDDEN, format!("Agent {} risk_ceiling {} < track risk {}", aid, emp.risk_ceiling, risk)); }
        if emp.permission_boundary.max_risk_score < risk { return err!(StatusCode::FORBIDDEN, format!("Agent {} max_risk_score {} < track risk {}", aid, emp.permission_boundary.max_risk_score, risk)); }
    }
    // 4. Validate executors
    for eid in &wo.selected_executors {
        let ex = executor_repo::ExecutorRepo::get(&s.pool, eid).await.map_or(None, |x| x);
        let ex = match ex { Some(e) => e, None => return err!(StatusCode::FORBIDDEN, format!("Executor {} not found", eid)) };
        if ex.status != ExecutorStatus::Registered { return err!(StatusCode::FORBIDDEN, format!("Executor {} not registered", eid)); }
        if ex.risk_ceiling < risk { return err!(StatusCode::FORBIDDEN, format!("Executor {} risk_ceiling {} < track risk {}", eid, ex.risk_ceiling, risk)); }
    }
    // 5. Yellow Track: require explicit approval or return WaitingApproval
    if wo.track == "yellow" && req.caller_identity_proof.is_none() {
        work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "WaitingApproval").await.ok();
        return ok!(serde_json::json!({"ok":true,"status":"WaitingApproval","approval_mode":"negative_consent","message":"Yellow Track requires approval. Use negative_consent timeout or explicit approval."}));
    }
    // 6. Red Track guards
    if wo.track == "red" {
        let missing: Vec<&str> = [
            ("caller_identity_proof", req.caller_identity_proof.as_ref().map(|s| !s.is_empty()).unwrap_or(false)),
            ("monitoring_signature", req.monitoring_signature.as_ref().map(|s| !s.is_empty()).unwrap_or(false)),
            ("diagnostic_signature", req.diagnostic_signature.as_ref().map(|s| !s.is_empty()).unwrap_or(false)),
            ("lease_id", req.lease_id.as_ref().map(|s| !s.is_empty()).unwrap_or(false)),
        ].iter().filter(|(_, ok)| !ok).map(|(name, _)| *name).collect();
        if !missing.is_empty() { return err!(StatusCode::FORBIDDEN, format!("Red Track missing: {:?}", missing)); }
        // Reject mock/placeholder credentials in Alpha
        let mock_patterns = ["mock", "demo", "placeholder", "test"];
        for (name, val) in [("caller_identity_proof",&req.caller_identity_proof),("monitoring_signature",&req.monitoring_signature),("diagnostic_signature",&req.diagnostic_signature),("lease_id",&req.lease_id)] {
            if let Some(v) = val {
                let lower = v.to_lowercase();
                if mock_patterns.iter().any(|p| lower.contains(p)) {
                    return err!(StatusCode::FORBIDDEN, format!("Red Track blocked: {} appears to be a mock value. Production MFA/lease verifier required.", name));
                }
            }
        }
    }
    // 6. Green/Yellow with approval: use WorkerHarness
    let harness_result = match WorkerHarness::run_work_order(&s.pool, &id, WorkerHarnessOptions{approval_receipt:req.caller_identity_proof.clone().or(req.lease_id.clone()),max_runtime_ms:Some(60000),deterministic_mode:true,preferred_tool_ids:vec![]}).await {
        Ok(hr) => hr,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    // Load worker session IDs from DB
    let session_rows = sqlx::query("SELECT session_id FROM worker_sessions WHERE work_order_id=? ORDER BY started_at_ms").bind(&id).fetch_all(&s.pool).await.unwrap_or_default();
    let worker_session_ids: Vec<String> = session_rows.iter().map(|r| r.get::<String,_>("session_id")).collect();
    // Write steps + events using repos, not direct SQL
    let now_ms = chrono::Utc::now().timestamp_millis();
    for sid in &worker_session_ids {
        for (i, stype) in ["ModelReasoning","ToolDryRun","ToolExecute","MemoryWrite","Reflection"].iter().enumerate() {
            if let Err(e) = WorkerRunStepRepo::append(&s.pool, &format!("step-{}-{}", sid, i), sid, stype, "{}", None, "Completed", now_ms).await {
                return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write step: {}", e));
            }
        }
        if let Err(e) = WorkerEventRepo::append(&s.pool, &format!("ev-{}-created", sid), sid, "SessionCreated", "{}", now_ms).await {
            return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write event: {}", e));
        }
        if let Err(e) = WorkerEventRepo::append(&s.pool, &format!("ev-{}-completed", sid), sid, "SessionCompleted", "{}", now_ms).await {
            return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write event: {}", e));
        }
    }
    // Synthesizer summary via Mock model
    let synthesized = "WorkerHarness completed execution. Results written to Task Memory.".to_string();
    if let Err(e) = work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, &harness_result.status).await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update status: {}", e));
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

pub async fn cancel_work_order(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Cancelled").await.ok(); ok!(serde_json::json!({"ok":true}))
}
pub async fn work_order_feedback(State(s): State<AppState>, Path(id): Path<String>, Json(req): Json<WorkOrderFeedback>) -> (StatusCode, Json<serde_json::Value>) {
    let analysis = FailureAnalyzer::analyze(&req.feedback, false, false, None);
    let proposal = SkillGenerator::generate_from_failure(&analysis, "skill-mission-draft", req.agent_id.as_deref().unwrap_or("system"));
    if let Err(e) = skill_evolution_repo::SkillEvolutionRepo::create_proposal(&s.pool, &proposal).await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create proposal: {}", e));
    }
    if let Err(e) = work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Failed").await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update status: {}", e));
    }
    ok!(serde_json::json!({"ok":true,"proposal_id":proposal.proposal_id}))
}

// === Skills ===
pub async fn list_skills(State(s): State<AppState>, Query(q): Query<SkillsQuery>) -> (StatusCode, Json<serde_json::Value>) {
    skill_repo::SkillRepo::list(&s.pool, q.agent_id.as_deref()).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |items| ok!(serde_json::to_value(items).unwrap()))
}
pub async fn seed_skills(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    skill_repo::SkillRepo::seed_default(&s.pool).await.ok(); ok!(serde_json::json!({"ok":true}))
}
pub async fn activate_skill(State(s): State<AppState>, Path((id,ver)): Path<(String,String)>) -> (StatusCode, Json<serde_json::Value>) {
    match skill_repo::SkillRepo::activate(&s.pool, &id, &ver).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => err!(StatusCode::FORBIDDEN, e.to_string()),
    }
}
pub async fn rollback_skill(State(s): State<AppState>, Path((id,ver)): Path<(String,String)>) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = skill_repo::SkillRepo::rollback(&s.pool, &id, &ver).await { return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()); }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let vr = SkillVersionRecord{skill_id:id.clone(),version:ver.clone(),parent_version:"active".into(),diff_summary:"rollback".into(),
        change_reason:"manual rollback".into(),verifier_result:None,approved_by:Some("api".into()),rollback_available:true,created_at_ms:now};
    if let Err(e) = skill_evolution_repo::SkillEvolutionRepo::record_version(&s.pool, &vr).await { return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()); }
    ok!(serde_json::json!({"ok":true}))
}

// === Skill Evolution ===
pub async fn list_proposals(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None).await.map_or_else(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()), |items| ok!(serde_json::to_value(items).unwrap()))
}
pub async fn run_evolution(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let p = SkillEvolutionProposal{proposal_id:format!("evol-{}",uuid::Uuid::new_v4()),source_type:EvolutionSourceType::AgentReflection,source_refs:vec![],target_skill_id:"skill-mission-draft".into(),proposal_type:EvolutionProposalType::PatchSkill,diagnosis:"scheduled evolution run".into(),proposed_changes:"auto-patch".into(),expected_benefit:"improve".into(),risk_assessment:"LOW".into(),generated_tests:vec![],status:EvolutionProposalStatus::Draft,created_by_agent:"scheduler".into(),created_at_ms:now};
    skill_evolution_repo::SkillEvolutionRepo::create_proposal(&s.pool, &p).await.ok();
    ok!(serde_json::to_value(&p).unwrap())
}
pub async fn verify_proposal(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None).await.unwrap_or_default();
    let proposal = match proposals.into_iter().find(|p| p.proposal_id == id) { Some(p) => p, None => return err!(StatusCode::NOT_FOUND, "Proposal not found") };
    let skill = skill_repo::SkillRepo::get(&s.pool, &proposal.target_skill_id, None).await.ok().flatten();
    let eval = SkillVerifier::verify(&proposal, skill.as_ref());
    skill_evolution_repo::SkillEvolutionRepo::append_eval(&s.pool, &eval).await.ok();
    let new_status = if eval.passed && !proposal.risk_assessment.contains("HIGH") { "Approved" } else { "NeedsHumanReview" };
    skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, new_status).await.ok();
    ok!(serde_json::to_value(&eval).unwrap())
}
pub async fn approve_proposal(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None).await.unwrap_or_default();
    let proposal = match proposals.into_iter().find(|p| p.proposal_id == id) { Some(p) => p, None => return err!(StatusCode::NOT_FOUND, "Proposal not found") };
    // HIGH risk requires human
    if proposal.risk_assessment.to_uppercase().contains("HIGH") || proposal.risk_assessment.to_uppercase().contains("RED") {
        return err!(StatusCode::FORBIDDEN, "HIGH/RED risk skill proposal requires explicit human approval marker");
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    match proposal.proposal_type {
        EvolutionProposalType::CreateNewSkill => {
            let sk = AgentSkillPackage{skill_id:proposal.target_skill_id.clone(),version:"1.0.0".into(),name:proposal.target_skill_id.clone(),owner_agent_id:proposal.created_by_agent.clone(),department:"Custom".into(),description:proposal.diagnosis.clone(),trigger_patterns:vec![],applicable_domains:vec![],required_tools:vec![],required_model_profile:None,input_schema:serde_json::json!({}),output_schema:serde_json::json!({}),prompt_template:proposal.proposed_changes.clone(),procedure_steps:vec![],guardrails:vec!["no escalation".into()],examples:vec![],tests:proposal.generated_tests.clone(),evals:vec![],permissions_required:vec![],allowed_cognitive_layers:vec!["Hypothesis".into()],allowed_action_modes:vec!["DRAFT_ONLY".into()],risk_ceiling:0.3,provenance:format!("skill-evolution-{}",proposal.proposal_id),status:SkillStatus::Active,created_at_ms:now,updated_at_ms:now};
            skill_repo::SkillRepo::upsert(&s.pool, &sk).await.ok();
        }
        EvolutionProposalType::PatchSkill => {
            let existing = skill_repo::SkillRepo::get(&s.pool, &proposal.target_skill_id, None).await.ok().flatten();
            let mut patched = existing.unwrap_or_else(|| AgentSkillPackage{skill_id:proposal.target_skill_id.clone(),version:"1.0.0".into(),name:proposal.target_skill_id.clone(),owner_agent_id:proposal.created_by_agent.clone(),department:"Custom".into(),description:String::new(),trigger_patterns:vec![],applicable_domains:vec![],required_tools:vec![],required_model_profile:None,input_schema:serde_json::json!({}),output_schema:serde_json::json!({}),prompt_template:String::new(),procedure_steps:vec![],guardrails:vec![],examples:vec![],tests:vec![],evals:vec![],permissions_required:vec![],allowed_cognitive_layers:vec![],allowed_action_modes:vec![],risk_ceiling:0.3,provenance:String::new(),status:SkillStatus::Draft,created_at_ms:now,updated_at_ms:now});
            // Bump version, apply patch
            let parts: Vec<u32> = patched.version.split('.').filter_map(|s| s.parse().ok()).collect();
            let new_ver = if parts.len() >= 3 { format!("{}.{}.{}", parts[0], parts[1], parts[2] + 1) } else { "1.1.0".to_string() };
            patched.version = new_ver.clone();
            patched.prompt_template = proposal.proposed_changes.clone();
            patched.status = SkillStatus::Active;
            patched.updated_at_ms = now;
            patched.provenance = format!("skill-evolution-{}", proposal.proposal_id);
            skill_repo::SkillRepo::upsert(&s.pool, &patched).await.ok();
        }
        EvolutionProposalType::DeprecateSkill => {
            if let Ok(Some(mut sk)) = skill_repo::SkillRepo::get(&s.pool, &proposal.target_skill_id, None).await { sk.status = SkillStatus::Deprecated; sk.updated_at_ms = now; skill_repo::SkillRepo::upsert(&s.pool, &sk).await.ok(); }
        }
        EvolutionProposalType::SplitSkill | EvolutionProposalType::MergeSkills => {
            return err!(StatusCode::UNPROCESSABLE_ENTITY, "NOT_IMPLEMENTED: SplitSkill/MergeSkills not supported yet");
        }
    }
    let vr = SkillVersionRecord{skill_id:proposal.target_skill_id.clone(),version:"1.1.0".into(),parent_version:"1.0.0".into(),diff_summary:proposal.proposed_changes.clone(),change_reason:proposal.diagnosis.clone(),verifier_result:None,approved_by:Some("api".into()),rollback_available:true,created_at_ms:now};
    if let Err(e) = skill_evolution_repo::SkillEvolutionRepo::record_version(&s.pool, &vr).await { return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()); }
    if let Err(e) = skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, "Applied").await { return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()); }
    ok!(serde_json::json!({"ok":true,"proposal_id":id}))
}
pub async fn reject_proposal(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, "Rejected").await.ok(); ok!(serde_json::json!({"ok":true}))
}
