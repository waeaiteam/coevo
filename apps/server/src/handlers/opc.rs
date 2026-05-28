use axum::{extract::{Path, Query, State}, Json, http::StatusCode};
use serde::Deserialize;
use coevo_core::opc::*;
use coevo_core::skills::*;
use coevo_core::cognitive::CognitiveLayer;
use coevo_store::repos_opc::*;
use coevo_evolution::{analyzer::FailureAnalyzer, generator::SkillGenerator, verifier::SkillVerifier};
use crate::state::AppState;

#[derive(Deserialize)] pub struct MemoryQuery { pub scope: Option<String>, pub owner_id: Option<String>, pub include_revoked: Option<bool>, pub q: Option<String> }
#[derive(Deserialize)] pub struct ExecutorDryRunReq { pub work_order_id: String }
#[derive(Deserialize)] pub struct WorkOrderFeedback { pub feedback: String, pub agent_id: Option<String> }

macro_rules! ok { ($v:expr) => { (StatusCode::OK, Json($v)) } }
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

// === User Profile ===
pub async fn get_user_profile(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match user_profile_repo::UserProfileRepo::get(&s.pool, "default-founder").await {
        Ok(Some(p)) => ok!(serde_json::to_value(p).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "User profile not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn put_user_profile(State(s): State<AppState>, Json(p): Json<UserProfile>) -> (StatusCode, Json<serde_json::Value>) {
    match user_profile_repo::UserProfileRepo::upsert(&s.pool, &p).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// === Company Profile ===
pub async fn get_company_profile(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match opc_profile_repo::OPCProfileRepo::get(&s.pool, "default-opc").await {
        Ok(Some(p)) => ok!(serde_json::to_value(p).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "OPC profile not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn put_company_profile(State(s): State<AppState>, Json(p): Json<OPCProfile>) -> (StatusCode, Json<serde_json::Value>) {
    match opc_profile_repo::OPCProfileRepo::upsert(&s.pool, &p).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// === Memory ===
pub async fn list_memory(State(s): State<AppState>, Query(q): Query<MemoryQuery>) -> (StatusCode, Json<serde_json::Value>) {
    let res = if let Some(ref query) = q.q {
        memory_repo::MemoryRepo::search(&s.pool, query, q.scope.as_deref(), q.owner_id.as_deref()).await
    } else {
        memory_repo::MemoryRepo::list(&s.pool, q.scope.as_deref(), q.owner_id.as_deref(), q.include_revoked.unwrap_or(false)).await
    };
    match res { Ok(items) => ok!(serde_json::to_value(items).unwrap()), Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()) }
}
pub async fn create_memory(State(s): State<AppState>, Json(m): Json<MemoryRecord>) -> (StatusCode, Json<serde_json::Value>) {
    match memory_repo::MemoryRepo::create(&s.pool, &m).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("provenance") { err!(StatusCode::UNPROCESSABLE_ENTITY, "MEMORY_PROVENANCE_REQUIRED: Fact memory requires provenance") }
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
    match agent_employee_repo::AgentEmployeeRepo::list(&s.pool).await {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn seed_employees_handler(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    agent_employee_repo::AgentEmployeeRepo::seed(&s.pool).await.ok(); ok!(serde_json::json!({"ok":true}))
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
    match executor_repo::ExecutorRepo::list(&s.pool).await {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn register_executor(State(s): State<AppState>, Json(p): Json<ExternalExecutorPassport>) -> (StatusCode, Json<serde_json::Value>) {
    match executor_repo::ExecutorRepo::register(&s.pool, &p).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
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
    let wo_risk = match wo.track.as_str() { "green" => 0.3, "yellow" => 0.6, _ => 0.9 };
    if ex.risk_ceiling < wo_risk { return err!(StatusCode::FORBIDDEN, format!("Risk ceiling {} < work order risk {}", ex.risk_ceiling, wo_risk)); }
    ok!(serde_json::json!({"passed":true,"estimated_cost_usd":0.01,"estimated_duration_ms":100,"warnings":[]}))
}

// === Work Orders ===
pub async fn create_work_order(State(s): State<AppState>, Json(wo): Json<WorkOrder>) -> (StatusCode, Json<serde_json::Value>) {
    match work_order_repo::WorkOrderRepo::create(&s.pool, &wo).await {
        Ok(()) => ok!(serde_json::json!({"ok":true,"work_order_id":wo.work_order_id})),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("contract_hash") || msg.contains("plan_hash") { err!(StatusCode::UNPROCESSABLE_ENTITY, msg) }
            else { err!(StatusCode::INTERNAL_SERVER_ERROR, msg) }
        }
    }
}
pub async fn list_work_orders(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match work_order_repo::WorkOrderRepo::list(&s.pool).await {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn execute_work_order(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let wo = match work_order_repo::WorkOrderRepo::get(&s.pool, &id).await {
        Ok(Some(w)) => w, _ => return err!(StatusCode::NOT_FOUND, "Work order not found"),
    };
    if wo.contract_hash.is_empty() || wo.plan_hash.is_empty() { return err!(StatusCode::UNPROCESSABLE_ENTITY, "Missing contract_hash/plan_hash"); }
    // Red track guard
    if wo.track == "red" {
        return err!(StatusCode::FORBIDDEN, "Red Track requires caller_identity_proof, dual-sign, and emergency lease");
    }
    // Write task memory
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let mem = MemoryRecord{memory_id:format!("task-mem-{}",uuid::Uuid::new_v4()),scope:MemoryScope::Task,owner_id:id.clone(),
        title:format!("WorkOrder {} execution", &wo.work_order_id),content:format!("Executed: {}", wo.mission_intent),
        tags:vec![],source:"work-order-executor".into(),provenance:"mock-execution".into(),confidence:0.9,
        ttl_seconds:86400,created_at_ms:now,updated_at_ms:now,access_policy:"".into(),
        status:MemoryStatus::Active,cognitive_layer:CognitiveLayer::Hypothesis,
        linked_contract_hash:Some(wo.contract_hash.clone()),linked_plan_hash:Some(wo.plan_hash.clone()),linked_adr_id:None};
    memory_repo::MemoryRepo::create(&s.pool, &mem).await.ok();
    work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Completed").await.ok();
    ok!(serde_json::json!({"ok":true,"status":"Completed","memory_id":mem.memory_id}))
}
pub async fn cancel_work_order(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Cancelled").await.ok(); ok!(serde_json::json!({"ok":true}))
}
pub async fn work_order_feedback(State(s): State<AppState>, Path(id): Path<String>, Json(req): Json<WorkOrderFeedback>) -> (StatusCode, Json<serde_json::Value>) {
    let analysis = FailureAnalyzer::analyze(&req.feedback, false, false, None);
    let proposal = SkillGenerator::generate_from_failure(&analysis, "skill-mission-draft", req.agent_id.as_deref().unwrap_or("system"));
    skill_evolution_repo::SkillEvolutionRepo::create_proposal(&s.pool, &proposal).await.ok();
    work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Failed").await.ok();
    ok!(serde_json::json!({"ok":true,"proposal_id":proposal.proposal_id}))
}

// === Skills ===
pub async fn list_skills(State(s): State<AppState>, Query(agent_id): Query<Option<String>>) -> (StatusCode, Json<serde_json::Value>) {
    match skill_repo::SkillRepo::list(&s.pool, agent_id.as_deref()).await {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
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
    skill_repo::SkillRepo::rollback(&s.pool, &id, &ver).await.ok();
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let vr = SkillVersionRecord{skill_id:id.clone(),version:ver.clone(),parent_version:"active".into(),diff_summary:"rollback".into(),
        change_reason:"manual rollback".into(),verifier_result:None,approved_by:Some("api".into()),rollback_available:true,created_at_ms:now};
    skill_evolution_repo::SkillEvolutionRepo::record_version(&s.pool, &vr).await.ok();
    ok!(serde_json::json!({"ok":true}))
}

// === Skill Evolution ===
pub async fn list_proposals(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None).await {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn run_evolution(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let proposal = SkillEvolutionProposal{proposal_id:format!("evol-{}",uuid::Uuid::new_v4()),
        source_type:EvolutionSourceType::AgentReflection,source_refs:vec![],target_skill_id:"skill-mission-draft".into(),
        proposal_type:EvolutionProposalType::PatchSkill,diagnosis:"scheduled evolution run".into(),
        proposed_changes:"auto-patch".into(),expected_benefit:"improve".into(),risk_assessment:"LOW".into(),
        generated_tests:vec![],status:EvolutionProposalStatus::Draft,created_by_agent:"scheduler".into(),created_at_ms:now};
    skill_evolution_repo::SkillEvolutionRepo::create_proposal(&s.pool, &proposal).await.ok();
    ok!(serde_json::to_value(&proposal).unwrap())
}
pub async fn verify_proposal(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None).await.unwrap_or_default();
    let proposal = proposals.into_iter().find(|p| p.proposal_id == id);
    let proposal = match proposal { Some(p) => p, None => return err!(StatusCode::NOT_FOUND, "Proposal not found") };
    let skill = skill_repo::SkillRepo::get(&s.pool, &proposal.target_skill_id, None).await.ok().flatten();
    let eval = SkillVerifier::verify(&proposal, skill.as_ref());
    skill_evolution_repo::SkillEvolutionRepo::append_eval(&s.pool, &eval).await.ok();
    let new_status = if eval.passed && !proposal.risk_assessment.contains("HIGH") { "Approved" } else { "NeedsHumanReview" };
    skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, new_status).await.ok();
    ok!(serde_json::to_value(&eval).unwrap())
}
pub async fn approve_proposal(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, "Applied").await.ok();
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let vr = SkillVersionRecord{skill_id:"skill-mission-draft".into(),version:"1.1.0".into(),parent_version:"1.0.0".into(),
        diff_summary:"approved evolution".into(),change_reason:"human approved".into(),verifier_result:None,
        approved_by:Some("api".into()),rollback_available:true,created_at_ms:now};
    skill_evolution_repo::SkillEvolutionRepo::record_version(&s.pool, &vr).await.ok();
    ok!(serde_json::json!({"ok":true}))
}
pub async fn reject_proposal(State(s): State<AppState>, Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    skill_evolution_repo::SkillEvolutionRepo::update_status(&s.pool, &id, "Rejected").await.ok(); ok!(serde_json::json!({"ok":true}))
}
