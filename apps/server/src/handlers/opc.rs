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
use coevo_store::pool::create_pool;
use coevo_store::repos::{approval_repo::ApprovalRepo, contract_repo::ContractRepo};
use coevo_store::repos_opc::*;
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
pub struct ApprovalDecisionRequest {
    pub approval_id: String,
    pub decision: String,
    pub comment: Option<String>,
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
    pub governance_proposal: Option<GovernanceProposal>,
}

#[derive(Deserialize)]
pub struct UpdateFounderRequest {
    pub display_name: Option<String>,
    pub preferences: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct CreateCompanyRequest {
    pub name: String,
    pub mission: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateCompanyRequest {
    pub name: Option<String>,
    pub mission: Option<String>,
    pub charter: Option<String>,
}

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

fn founder_placeholder(profile: Option<UserProfile>) -> serde_json::Value {
    if let Some(profile) = profile {
        serde_json::json!({
            "founder_id": profile.user_id,
            "display_name": profile.display_name,
            "preferences": {
                "preferred_language": profile.preferred_language,
                "timezone": profile.timezone,
                "risk_preference": profile.risk_preference,
                "default_mission_mode": profile.default_mission_mode,
                "communication_style": profile.communication_style,
            }
        })
    } else {
        serde_json::json!({
            "founder_id": "default-founder",
            "display_name": "Founder",
            "preferences": {}
        })
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

fn company_employee_dir(state: &AppState, opc_id: &str, agent_id: &str) -> std::path::PathBuf {
    state
        .company_workspace
        .company_dir(opc_id)
        .join("employees")
        .join(agent_id)
}

fn company_employee_passport_path(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
) -> std::path::PathBuf {
    company_employee_dir(state, opc_id, agent_id).join("passport.json")
}

fn company_employee_prompt_path(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
) -> std::path::PathBuf {
    company_employee_dir(state, opc_id, agent_id).join("prompt.md")
}

fn company_employee_prompt_versions_dir(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
) -> std::path::PathBuf {
    company_employee_dir(state, opc_id, agent_id).join("prompt_versions")
}

fn company_employee_prompt_version_path(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
    version: i32,
) -> std::path::PathBuf {
    company_employee_prompt_versions_dir(state, opc_id, agent_id).join(format!("v{version}.md"))
}

fn company_employee_prompt_current_version_path(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
) -> std::path::PathBuf {
    company_employee_prompt_versions_dir(state, opc_id, agent_id).join("current.txt")
}

fn ensure_company_employee_files(
    state: &AppState,
    opc_id: &str,
    employee: &AgentEmployee,
) -> Result<(), String> {
    let employee_dir = company_employee_dir(state, opc_id, &employee.agent_id);
    std::fs::create_dir_all(&employee_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(company_employee_prompt_versions_dir(
        state,
        opc_id,
        &employee.agent_id,
    ))
    .map_err(|e| e.to_string())?;
    std::fs::write(
        company_employee_passport_path(state, opc_id, &employee.agent_id),
        serde_json::to_string_pretty(&employee.passport).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let prompt_path = company_employee_prompt_path(state, opc_id, &employee.agent_id);
    if !prompt_path.exists() {
        std::fs::write(prompt_path, &employee.system_prompt).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn read_company_employee_current_prompt_version(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
) -> Result<Option<i32>, String> {
    let path = company_employee_prompt_current_version_path(state, opc_id, agent_id);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    raw.trim()
        .parse::<i32>()
        .map(Some)
        .map_err(|e| e.to_string())
}

fn write_company_employee_prompt_version(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
    version: i32,
    content: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(company_employee_prompt_versions_dir(state, opc_id, agent_id))
        .map_err(|e| e.to_string())?;
    std::fs::write(
        company_employee_prompt_version_path(state, opc_id, agent_id, version),
        content,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(company_employee_prompt_path(state, opc_id, agent_id), content)
        .map_err(|e| e.to_string())?;
    std::fs::write(
        company_employee_prompt_current_version_path(state, opc_id, agent_id),
        version.to_string(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn memory_scope_query_to_db(scope: Option<&str>) -> Option<&'static str> {
    match scope?.trim() {
        "User" | "user" => Some("User"),
        "Company" | "company" => Some("Company"),
        "Agent" | "agent" => Some("Agent"),
        "Task" | "task" => Some("Task"),
        "Skill" | "skill" => Some("Skill"),
        "Executor" | "executor" => Some("Executor"),
        "Audit" | "audit" => Some("Audit"),
        _ => None,
    }
}

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

fn tier_rank(tier: AutonomyCeiling) -> u8 {
    match tier {
        AutonomyCeiling::ReadOnly => 0,
        AutonomyCeiling::WorkspaceWrite => 1,
        AutonomyCeiling::FullAccess => 2,
    }
}

fn min_tier(requested: AutonomyCeiling, ceiling: AutonomyCeiling) -> AutonomyCeiling {
    if tier_rank(requested) <= tier_rank(ceiling) {
        requested
    } else {
        ceiling
    }
}

fn track_tier_ceiling(track: &str) -> AutonomyCeiling {
    match track {
        "yellow" => AutonomyCeiling::WorkspaceWrite,
        "red" => AutonomyCeiling::ReadOnly,
        _ => AutonomyCeiling::ReadOnly,
    }
}

fn default_governance_proposal(req: &CreateWORequest) -> GovernanceProposal {
    GovernanceProposal {
        autonomy_ceiling: AutonomyCeiling::ReadOnly,
        model_preference: ModelPreference::Standard,
        assigned_agent_id: req
            .selected_agents
            .first()
            .filter(|id| !id.trim().is_empty())
            .cloned(),
    }
}

fn choose_agent_for_track(employees: &[AgentEmployee], track: &str) -> Option<String> {
    let risk = if track == "red" {
        track_risk("yellow")
    } else {
        track_risk(track)
    };
    let qualified = |employee: &&AgentEmployee| {
        employee.lifecycle_status == LifecycleStatus::Active
            && employee.risk_ceiling >= risk
            && employee.permission_boundary.max_risk_score >= risk
    };
    employees
        .iter()
        .filter(qualified)
        .find(|employee| employee.agent_id == "agent-founder-01")
        .or_else(|| {
            employees
                .iter()
                .filter(qualified)
                .find(|employee| employee.agent_id == "agent-risk-01")
        })
        .or_else(|| employees.iter().find(qualified))
        .map(|employee| employee.agent_id.clone())
}

fn resolve_governance_verdict(
    proposal: &GovernanceProposal,
    track_decision: &TrackDecision,
    employees: &[AgentEmployee],
    client_selected_agents: &[String],
) -> GovernanceVerdict {
    let risk_ceiling = track_tier_ceiling(track_decision.track);
    let effective_tier = min_tier(proposal.autonomy_ceiling, risk_ceiling);
    let downgraded = effective_tier != proposal.autonomy_ceiling;
    let requested_agent = proposal
        .assigned_agent_id
        .as_ref()
        .filter(|id| !id.trim().is_empty() && id.as_str() != "auto");
    let requested_agent_is_active = requested_agent.and_then(|id| {
        employees.iter().find(|employee| {
            employee.agent_id == *id && employee.lifecycle_status == LifecycleStatus::Active
        })
    });
    let resolved_agent_id = requested_agent_is_active
        .map(|employee| employee.agent_id.clone())
        .or_else(|| choose_agent_for_track(employees, track_decision.track))
        .or_else(|| {
            client_selected_agents
                .first()
                .filter(|id| !id.is_empty())
                .cloned()
        });
    let blocked = track_decision.track == "red";

    GovernanceVerdict {
        effective_track: track_decision.track.to_string(),
        effective_tier,
        requested_ceiling: proposal.autonomy_ceiling,
        downgraded,
        downgrade_reason: downgraded.then(|| {
            "Requested autonomy exceeds the server RiskGate ceiling for this task.".to_string()
        }),
        blocked,
        block_reason: blocked.then(|| {
            "Red Track execution is blocked in Alpha until the production verifier is available."
                .to_string()
        }),
        resolved_agent_id,
    }
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

fn is_trigger_boundary(ch: Option<char>) -> bool {
    match ch {
        None => true,
        Some(c) => !c.is_ascii_alphanumeric(),
    }
}

fn contains_governance_trigger(intent: &str, trigger: &str) -> bool {
    let mut search_start = 0;
    while let Some(relative_start) = intent[search_start..].find(trigger) {
        let start = search_start + relative_start;
        let end = start + trigger.len();
        let before = intent[..start].chars().next_back();
        let after = intent[end..].chars().next();
        if is_trigger_boundary(before) && is_trigger_boundary(after) {
            return true;
        }
        search_start = end;
    }
    false
}

fn classify_mission_track(intent: &str) -> TrackDecision {
    let lower = intent.to_lowercase();
    for trigger in RED_TRIGGERS {
        if contains_governance_trigger(&lower, trigger) {
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
        if contains_governance_trigger(&lower, trigger) {
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
        risk_summary:
            "Server RiskGate: low-risk read/analyze intent. Green Track auto-execution is allowed."
                .to_string(),
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
pub async fn get_founder(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match user_profile_repo::UserProfileRepo::get(&s.pool, "default-founder").await {
        Ok(profile) => ok!(founder_placeholder(profile)),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn put_founder(
    State(s): State<AppState>,
    Json(req): Json<UpdateFounderRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let existing = user_profile_repo::UserProfileRepo::get(&s.pool, "default-founder")
        .await
        .ok()
        .flatten();
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let display_name = req
        .display_name
        .unwrap_or_else(|| existing.as_ref().map(|p| p.display_name.clone()).unwrap_or_else(|| "Founder".to_string()));
    let preferred_language = existing
        .as_ref()
        .map(|p| p.preferred_language.clone())
        .unwrap_or_else(|| "zh-CN".to_string());
    let timezone = existing
        .as_ref()
        .map(|p| p.timezone.clone())
        .unwrap_or_else(|| "Asia/Shanghai".to_string());
    let communication_style = existing
        .as_ref()
        .map(|p| p.communication_style.clone())
        .unwrap_or_else(|| "concise".to_string());

    let profile = UserProfile {
        user_id: "default-founder".to_string(),
        display_name,
        preferred_language,
        timezone,
        risk_preference: existing
            .as_ref()
            .map(|p| p.risk_preference)
            .unwrap_or(RiskPreference::Balanced),
        default_mission_mode: existing
            .as_ref()
            .map(|p| p.default_mission_mode)
            .unwrap_or(MissionMode::Collaborative),
        long_term_goals: existing
            .as_ref()
            .map(|p| p.long_term_goals.clone())
            .unwrap_or_default(),
        business_domains: existing
            .as_ref()
            .map(|p| p.business_domains.clone())
            .unwrap_or_default(),
        communication_style,
        approval_preferences: existing.as_ref().map(|p| p.approval_preferences.clone()).unwrap_or(
            ApprovalPreferences {
                auto_approve_below_risk: 0.3,
                require_explicit_for_yellow: true,
                require_mfa_for_red: true,
                negative_consent_timeout_secs: 300,
            },
        ),
        data_boundaries: existing
            .as_ref()
            .map(|p| p.data_boundaries.clone())
            .unwrap_or_default(),
        budget_limits: existing.as_ref().map(|p| p.budget_limits.clone()).unwrap_or(
            BudgetLimits {
                max_cost_per_task_usd: 50.0,
                max_cost_per_day_usd: 500.0,
                max_agents_per_task: 5,
            },
        ),
        favorite_tools: existing
            .as_ref()
            .map(|p| p.favorite_tools.clone())
            .unwrap_or_default(),
        active_projects: existing
            .as_ref()
            .map(|p| p.active_projects.clone())
            .unwrap_or_default(),
        created_at_ms: existing.as_ref().map(|p| p.created_at_ms).unwrap_or(now),
        updated_at_ms: now,
    };

    match user_profile_repo::UserProfileRepo::upsert(&s.pool, &profile).await {
        Ok(()) => ok!(founder_placeholder(Some(profile))),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_companies(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match s.company_workspace.list_companies().await {
        Ok(items) => {
            let enriched: Vec<_> = items
                .into_iter()
                .map(|company| {
                    let employee_count = {
                        let db_path = s.company_workspace.company_db_path(&company.opc_id);
                        if db_path.exists() {
                            0
                        } else {
                            0
                        }
                    };
                    serde_json::json!({
                        "opc_id": company.opc_id,
                        "name": company.name,
                        "mission": "",
                        "employee_count": employee_count,
                        "created_at_ms": company.created_at_ms,
                        "dir": company.dir,
                    })
                })
                .collect();
            ok!(serde_json::Value::Array(enriched))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_company(
    State(s): State<AppState>,
    Json(req): Json<CreateCompanyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.name.trim().is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "name is required");
    }
    match s
        .company_workspace
        .create_company(req.name.trim(), req.mission.as_deref(), "default-founder")
        .await
    {
        Ok(company) => ok!(serde_json::json!({
            "opc_id": company.opc_id,
            "name": company.name,
            "mission": req.mission.unwrap_or_default(),
            "employee_count": 0,
            "created_at_ms": company.created_at_ms,
            "dir": company.dir,
        })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_company(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let listed = match s.company_workspace.list_companies().await {
        Ok(items) => items,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let Some(company) = listed.into_iter().find(|company| company.opc_id == opc_id) else {
        return err!(StatusCode::NOT_FOUND, "company not found");
    };
    let charter_path = s.company_workspace.company_dir(&company.opc_id).join("charter.md");
    let charter_md = std::fs::read_to_string(charter_path).unwrap_or_default();
    ok!(serde_json::json!({
        "opc_id": company.opc_id,
        "name": company.name,
        "mission": "",
        "employee_count": 0,
        "created_at_ms": company.created_at_ms,
        "dir": company.dir,
        "charter_md": charter_md,
        "goals": [],
        "departments": [],
        "shared_files_count": 0,
        "memory_count": 0,
        "report_count": 0,
    }))
}

pub async fn put_company(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<UpdateCompanyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_dir = s.company_workspace.company_dir(&opc_id);
    if !company_dir.exists() {
        return err!(StatusCode::NOT_FOUND, "company not found");
    }

    let company_json = company_dir.join("company.json");
    let raw = match std::fs::read_to_string(&company_json) {
        Ok(raw) => raw,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let mut identity: coevo_store::company_workspace::CompanyIdentity =
        match serde_json::from_str(&raw) {
            Ok(identity) => identity,
            Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
    if let Some(name) = req.name.as_ref().filter(|name| !name.trim().is_empty()) {
        identity.name = name.trim().to_string();
    }
    if let Some(mission) = req.mission.as_ref() {
        identity.mission = mission.clone();
    }
    identity.updated_at_ms = chrono::Utc::now().timestamp_millis() as u64;
    if let Err(e) = std::fs::write(&company_json, serde_json::to_string_pretty(&identity).unwrap())
    {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    if let Some(charter) = req.charter {
        if let Err(e) = std::fs::write(company_dir.join("charter.md"), charter) {
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    }

    let companies = match s.company_workspace.list_companies().await {
        Ok(companies) => companies,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let updated_index: Vec<_> = companies
        .into_iter()
        .map(|company| {
            if company.opc_id == opc_id {
                coevo_store::company_workspace::CompanyIndexEntry {
                    name: identity.name.clone(),
                    ..company
                }
            } else {
                company
            }
        })
        .collect();
    let index_path = s.company_workspace.companies_index_path();
    if let Err(e) = std::fs::write(index_path, serde_json::to_string_pretty(&updated_index).unwrap())
    {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    ok!(serde_json::json!({
        "opc_id": identity.opc_id,
        "name": identity.name,
        "mission": identity.mission,
        "employee_count": 0,
        "created_at_ms": identity.created_at_ms,
        "dir": company_dir.to_string_lossy().to_string(),
    }))
}

pub async fn delete_company(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match s.company_workspace.delete_company(&opc_id).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

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
    let scope = memory_scope_query_to_db(q.scope.as_deref());
    let res = if let Some(ref query) = q.q {
        memory_repo::MemoryRepo::search(&s.pool, query, scope, q.owner_id.as_deref()).await
    } else {
        memory_repo::MemoryRepo::list(
            &s.pool,
            scope,
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

pub async fn list_company_employees(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = agent_employee_repo::AgentEmployeeRepo::list(&pool).await.map(|employees| {
        for employee in &employees {
            let _ = ensure_company_employee_files(&s, &opc_id, employee);
        }
        employees
    });
    pool.close().await;
    result.map_or_else(
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

pub async fn seed_company_employees_handler(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let response = match agent_employee_repo::AgentEmployeeRepo::seed(&pool).await {
        Ok(()) => {
            let count = agent_employee_repo::AgentEmployeeRepo::list(&pool)
                .await
                .map(|employees| {
                    for employee in &employees {
                        let _ = ensure_company_employee_files(&s, &opc_id, employee);
                    }
                    employees.len()
                })
                .unwrap_or(0);
            ok!(serde_json::json!({"ok":true,"inserted":count,"total":count}))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    pool.close().await;
    response
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

/// Employee growth: a plain-language view of how an AI employee is performing
/// over time — aggregated run stats, a reputation trend, and any pending
/// improvement suggestions awaiting the founder's approval.
pub async fn get_agent_growth(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    use coevo_store::repos::reputation_repo::ReputationHistoryRepo;
    use coevo_store::repos::worker_run_repo::WorkerRunRepo;

    let (total, completed, failed, avg_latency, tokens, cost) =
        WorkerRunRepo::agent_run_stats(&s.pool, &id)
            .await
            .unwrap_or((0, 0, 0, 0.0, 0, 0.0));

    let history = ReputationHistoryRepo::list_by_agent(&s.pool, &id, 100)
        .await
        .unwrap_or_default();
    let trend: Vec<serde_json::Value> = history
        .iter()
        .map(|h| {
            serde_json::json!({
                "at": h.created_at_ms,
                "score": (h.overall_score * 100.0).round(),
                "task_count": h.task_count,
            })
        })
        .collect();

    // Direction: compare the latest snapshot to the earliest available.
    let direction = match (history.first(), history.last()) {
        (Some(first), Some(last)) if history.len() >= 2 => {
            let delta = last.overall_score - first.overall_score;
            if delta > 0.02 {
                "improving"
            } else if delta < -0.02 {
                "declining"
            } else {
                "steady"
            }
        }
        _ => "new",
    };

    let current_score = history
        .last()
        .map(|h| (h.overall_score * 100.0).round())
        .unwrap_or(50.0);
    let success_rate = if total > 0 {
        ((completed as f64 / total as f64) * 100.0).round()
    } else {
        0.0
    };

    // Pending improvement proposals awaiting the founder's approval.
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&s.pool, None)
        .await
        .unwrap_or_default();
    let pending: Vec<serde_json::Value> = proposals
        .iter()
        .filter_map(|p| {
            let v = serde_json::to_value(p).ok()?;
            let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("");
            if status == "Proposed" || status == "Verified" || status == "NeedsHumanReview" {
                Some(serde_json::json!({
                    "proposal_id": p.proposal_id,
                    "diagnosis": p.diagnosis,
                    "status": v.get("status"),
                    "risk": p.risk_assessment,
                }))
            } else {
                None
            }
        })
        .collect();

    ok!(serde_json::json!({
        "agent_id": id,
        "current_score": current_score,
        "direction": direction,
        "total_tasks": total,
        "completed_tasks": completed,
        "failed_tasks": failed,
        "success_rate": success_rate,
        "avg_latency_ms": avg_latency.round(),
        "total_usage": tokens,
        "total_cost_usd": cost,
        "trend": trend,
        "pending_improvements": pending,
    }))
}

// === Agent Workbench: employee CRUD + prompt management ===
pub async fn get_employee(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match agent_employee_repo::AgentEmployeeRepo::get(&s.pool, &id).await {
        Ok(Some(e)) => ok!(serde_json::to_value(e).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Employee not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_company_employee(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = agent_employee_repo::AgentEmployeeRepo::get(&pool, &id).await.map(|employee| {
        if let Some(ref employee) = employee {
            let _ = ensure_company_employee_files(&s, &opc_id, employee);
        }
        employee
    });
    pool.close().await;
    match result {
        Ok(Some(e)) => ok!(serde_json::to_value(e).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Employee not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_employee(
    State(s): State<AppState>,
    Json(mut employee): Json<AgentEmployee>,
) -> (StatusCode, Json<serde_json::Value>) {
    if employee.agent_id.trim().is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "agent_id is required");
    }
    match agent_employee_repo::AgentEmployeeRepo::exists(&s.pool, &employee.agent_id).await {
        Ok(true) => return err!(StatusCode::CONFLICT, "agent_id already exists"),
        Ok(false) => {}
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    employee.created_at_ms = now;
    employee.updated_at_ms = now;
    match agent_employee_repo::AgentEmployeeRepo::upsert(&s.pool, &employee).await {
        Ok(()) => ok!(serde_json::to_value(employee).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_company_employee(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(mut employee): Json<AgentEmployee>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    if employee.agent_id.trim().is_empty() {
        pool.close().await;
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "agent_id is required");
    }
    match agent_employee_repo::AgentEmployeeRepo::exists(&pool, &employee.agent_id).await {
        Ok(true) => {
            pool.close().await;
            return err!(StatusCode::CONFLICT, "agent_id already exists");
        }
        Ok(false) => {}
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    employee.created_at_ms = now;
    employee.updated_at_ms = now;
    let result = agent_employee_repo::AgentEmployeeRepo::upsert(&pool, &employee)
        .await
        .map_err(|e| e.to_string())
        .and_then(|_| ensure_company_employee_files(&s, &opc_id, &employee).map(|_| ()));
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::to_value(employee).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn update_employee(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(mut employee): Json<AgentEmployee>,
) -> (StatusCode, Json<serde_json::Value>) {
    employee.agent_id = id;
    employee.updated_at_ms = chrono::Utc::now().timestamp_millis() as u64;
    match agent_employee_repo::AgentEmployeeRepo::upsert(&s.pool, &employee).await {
        Ok(()) => ok!(serde_json::to_value(employee).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn update_company_employee(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
    Json(mut employee): Json<AgentEmployee>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let existing = match agent_employee_repo::AgentEmployeeRepo::get(&pool, &id).await {
        Ok(Some(existing)) => existing,
        Ok(None) => {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Employee not found");
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    employee.agent_id = id;
    employee.passport = existing.passport;
    employee.created_at_ms = existing.created_at_ms;
    employee.updated_at_ms = chrono::Utc::now().timestamp_millis() as u64;
    let result = agent_employee_repo::AgentEmployeeRepo::upsert(&pool, &employee)
        .await
        .map_err(|e| e.to_string())
        .and_then(|_| ensure_company_employee_files(&s, &opc_id, &employee).map(|_| ()));
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::to_value(employee).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn delete_employee(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match agent_employee_repo::AgentEmployeeRepo::delete(&s.pool, &id).await {
        Ok(()) => ok!(serde_json::json!({"ok": true, "deleted": id})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn delete_company_employee(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = agent_employee_repo::AgentEmployeeRepo::delete(&pool, &id).await;
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::json!({"ok": true, "deleted": id})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(serde::Deserialize)]
pub struct UpdatePromptRequest {
    pub system_prompt: String,
    pub change_summary: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct PromptRollbackRequest {
    pub version: i32,
}

pub async fn update_employee_prompt(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePromptRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Save the prompt on the employee record.
    if let Err(e) =
        agent_employee_repo::AgentEmployeeRepo::update_system_prompt(&s.pool, &id, &req.system_prompt)
            .await
    {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    // Version it through the shared prompt-version history (prompt_id = agent_id).
    let version_id = crate::handlers::prompts::record_and_publish_version(
        &s.pool,
        &id,
        &req.system_prompt,
        "workbench",
        req.change_summary.as_deref(),
    )
    .await
    .ok();
    ok!(serde_json::json!({"ok": true, "agent_id": id, "version_id": version_id}))
}

pub async fn get_company_employee_prompt(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let prompt_path = company_employee_prompt_path(&s, &opc_id, &id);
    if !prompt_path.exists() {
        let pool = match company_pool(&s, &opc_id).await {
            Ok(pool) => pool,
            Err(err) => return err,
        };
        let employee = match agent_employee_repo::AgentEmployeeRepo::get(&pool, &id).await {
            Ok(Some(employee)) => employee,
            Ok(None) => {
                pool.close().await;
                return err!(StatusCode::NOT_FOUND, "Employee not found");
            }
            Err(e) => {
                pool.close().await;
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
        };
        let _ = ensure_company_employee_files(&s, &opc_id, &employee);
        pool.close().await;
    }
    let content = match std::fs::read_to_string(&prompt_path) {
        Ok(content) => content,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let version = match read_company_employee_current_prompt_version(&s, &opc_id, &id) {
        Ok(Some(version)) => version,
        Ok(None) => 0,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    ok!(serde_json::json!({"content_md": content, "version": version}))
}

pub async fn update_company_employee_prompt(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
    Json(req): Json<UpdatePromptRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let employee = match agent_employee_repo::AgentEmployeeRepo::get(&pool, &id).await {
        Ok(Some(employee)) => employee,
        Ok(None) => {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Employee not found");
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    let _ = ensure_company_employee_files(&s, &opc_id, &employee);
    let current_version = match read_company_employee_current_prompt_version(&s, &opc_id, &id) {
        Ok(Some(version)) => version,
        Ok(None) => 0,
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    };
    let next_version = current_version + 1;
    if let Err(e) = write_company_employee_prompt_version(&s, &opc_id, &id, next_version, &req.system_prompt) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
    }

    if let Err(e) =
        agent_employee_repo::AgentEmployeeRepo::update_system_prompt(&pool, &id, &req.system_prompt).await
    {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    pool.close().await;
    ok!(serde_json::json!({"version": next_version, "change_summary": req.change_summary}))
}

pub async fn list_company_employee_prompt_versions(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let dir = company_employee_prompt_versions_dir(&s, &opc_id, &id);
    if !dir.exists() {
        return ok!(serde_json::json!([]));
    }
    let current = match read_company_employee_current_prompt_version(&s, &opc_id, &id) {
        Ok(version) => version,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let mut versions = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(raw_version) = stem.strip_prefix('v') else {
            continue;
        };
        let Ok(version) = raw_version.parse::<i32>() else {
            continue;
        };
        versions.push(serde_json::json!({
            "version": version,
            "current": current == Some(version),
            "path": path.to_string_lossy().to_string(),
        }));
    }
    versions.sort_by(|a, b| {
        b["version"]
            .as_i64()
            .unwrap_or_default()
            .cmp(&a["version"].as_i64().unwrap_or_default())
    });
    ok!(serde_json::Value::Array(versions))
}

pub async fn get_company_employee_prompt_version(
    State(s): State<AppState>,
    Path((opc_id, id, version)): Path<(String, String, i32)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let version_path = company_employee_prompt_version_path(&s, &opc_id, &id, version);
    if !version_path.exists() {
        return err!(StatusCode::NOT_FOUND, "Prompt version not found");
    }
    match std::fs::read_to_string(&version_path) {
        Ok(content) => ok!(serde_json::json!({"content_md": content, "version": version})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn rollback_company_employee_prompt(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
    Json(req): Json<PromptRollbackRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let version_path = company_employee_prompt_version_path(&s, &opc_id, &id, req.version);
    if !version_path.exists() {
        return err!(StatusCode::NOT_FOUND, "Prompt version not found");
    }
    let content = match std::fs::read_to_string(&version_path) {
        Ok(content) => content,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if let Err(e) = write_company_employee_prompt_version(&s, &opc_id, &id, req.version, &content) {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let update_result =
        agent_employee_repo::AgentEmployeeRepo::update_system_prompt(&pool, &id, &content).await;
    pool.close().await;
    match update_result {
        Ok(()) => ok!(serde_json::json!({"version": req.version, "ok": true})),
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
    let employees = agent_employee_repo::AgentEmployeeRepo::list(&s.pool)
        .await
        .unwrap_or_default();
    let proposal = req
        .governance_proposal
        .clone()
        .unwrap_or_else(|| default_governance_proposal(&req));
    let verdict =
        resolve_governance_verdict(&proposal, &track_decision, &employees, &req.selected_agents);
    let selected_agents = verdict
        .resolved_agent_id
        .clone()
        .map(|id| vec![id])
        .unwrap_or_else(|| req.selected_agents.clone());
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
        selected_agents,
        selected_executors: req.selected_executors,
        required_skills: req.required_skills,
        track: verdict.effective_track.clone(),
        status: WorkOrderStatus::Planned,
        allowed_actions: track_decision.allowed_actions,
        restricted_actions: track_decision.restricted_actions,
        risk_summary: track_decision.risk_summary,
        governance_proposal: Some(proposal.clone()),
        governance_verdict: Some(verdict.clone()),
        created_at_ms: now,
        updated_at_ms: now,
    };
    match work_order_repo::WorkOrderRepo::create(&s.pool, &wo).await {
        Ok(()) => ok!(
            serde_json::json!({"ok":true,"work_order_id":wo.work_order_id,"status":"Planned","track":wo.track,"risk_summary":wo.risk_summary,"allowed_actions":wo.allowed_actions,"restricted_actions":wo.restricted_actions,"governance_proposal":proposal,"governance_verdict":verdict,"created_at_ms":now})
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
            let _ = work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Failed").await;
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

pub async fn decide_work_order_approval(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ApprovalDecisionRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let approval = match ApprovalRepo::find_by_id(&s.pool, &req.approval_id).await {
        Ok(Some(approval)) => approval,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "Approval request not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let expected_action_urn = format!("urn:coevo:work-order:{}:execute", id);
    if approval.action_urn != expected_action_urn {
        return err!(StatusCode::FORBIDDEN, "APPROVAL_ACTION_MISMATCH");
    }

    let actor = "default-founder";
    match req.decision.as_str() {
        "approve" | "approved" => {
            if let Err(e) = ApprovalRepo::approve(&s.pool, &req.approval_id, actor).await {
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
            let (status, Json(mut body)) = execute_work_order(
                State(s.clone()),
                Path(id.clone()),
                Json(ExecuteRequest {
                    caller_identity_proof: Some(req.approval_id.clone()),
                    monitoring_signature: None,
                    diagnostic_signature: None,
                    lease_id: None,
                }),
            )
            .await;
            if let Some(obj) = body.as_object_mut() {
                obj.insert(
                    "approval_receipt".to_string(),
                    serde_json::json!(req.approval_id),
                );
                if let Some(comment) = req.comment {
                    obj.insert("approval_comment".to_string(), serde_json::json!(comment));
                }
                if let Some(run_id) = obj
                    .get("worker_runs")
                    .and_then(|runs| runs.as_array())
                    .and_then(|runs| runs.first())
                    .and_then(|run| run.get("run_id"))
                    .cloned()
                {
                    obj.insert("run_id".to_string(), run_id);
                }
            }
            (status, Json(body))
        }
        "reject" | "rejected" | "deny" | "denied" => {
            if let Err(e) = ApprovalRepo::deny(&s.pool, &req.approval_id, actor).await {
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
            let _ = work_order_repo::WorkOrderRepo::update_status(&s.pool, &id, "Failed").await;
            ok!(serde_json::json!({
                "ok":true,
                "status":"ApprovalDenied",
                "approval_receipt":req.approval_id,
                "approval_comment":req.comment,
                "message":"Approval denied; task execution was not resumed."
            }))
        }
        _ => err!(
            StatusCode::BAD_REQUEST,
            "decision must be approve or reject"
        ),
    }
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
    // Converge the evolution upgrade into the prompt-version history so the
    // approved prompt becomes the published version the runtime + UI both read.
    let _ = crate::handlers::prompts::record_and_publish_version(
        &s.pool,
        &proposal.target_skill_id,
        &proposal.proposed_changes,
        "skill-evolution",
        Some(&proposal.diagnosis),
    )
    .await;
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
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use coevo_store::repos::{approval_repo::ApprovalRepo, contract_repo::ContractRepo};
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};
    use std::sync::Mutex;
    use tower::ServiceExt;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    async fn count_rows(pool: &sqlx::SqlitePool, table: &str, work_order_id: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) AS n FROM {} WHERE work_order_id=?", table);
        sqlx::query_scalar::<_, i64>(&sql)
            .bind(work_order_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn create_yellow_work_order(state: AppState, work_order_id: &str, contract_hash: &str) {
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
            governance_proposal: None,
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

    async fn company_test_state() -> (AppState, std::path::PathBuf) {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-company-handler-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool, root.clone());
        (state, root)
    }

    #[tokio::test]
    async fn company_routes_create_fetch_and_delete_real_companies() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let app = Router::new()
            .route("/companies", get(list_companies).post(create_company))
            .route("/companies/{opc_id}", get(get_company).delete(delete_company))
            .with_state(state.clone());

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Alpha Labs",
                            "mission": "Build alpha"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap().to_string();
        assert!(opc_id.starts_with("opc-"));
        assert_eq!(created["name"], "Alpha Labs");
        assert_eq!(created["mission"], "Build alpha");

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/companies")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let listed: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);
        assert_eq!(listed[0]["opc_id"], opc_id);
        assert_eq!(listed[0]["name"], "Alpha Labs");

        let detail_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail_response.status(), StatusCode::OK);
        let detail: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(detail_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(detail["opc_id"], opc_id);
        assert_eq!(detail["name"], "Alpha Labs");
        assert_eq!(detail["charter_md"], "# Alpha Labs\n\nBuild alpha");

        let delete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/companies/{opc_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);

        let after_delete_response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/companies")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(after_delete_response.status(), StatusCode::OK);
        let after_delete: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(after_delete_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(after_delete.as_array().unwrap().len(), 0);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_detail_returns_not_found_after_company_is_deleted() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company("Beta Works", Some("Build beta"), "default-founder")
            .await
            .unwrap();

        state
            .company_workspace
            .delete_company(&company.opc_id)
            .await
            .unwrap();

        let (status, Json(body)) = get_company(State(state), Path(company.opc_id)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "company not found");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_employee_handlers_isolate_seeded_data_per_company() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let alpha = state
            .company_workspace
            .create_company("Alpha", Some("Alpha mission"), "default-founder")
            .await
            .unwrap();
        let beta = state
            .company_workspace
            .create_company("Beta", Some("Beta mission"), "default-founder")
            .await
            .unwrap();

        let (alpha_empty_status, Json(alpha_empty)) =
            list_company_employees(State(state.clone()), Path(alpha.opc_id.clone())).await;
        assert_eq!(alpha_empty_status, StatusCode::OK);
        assert_eq!(alpha_empty.as_array().unwrap().len(), 0);

        let (beta_empty_status, Json(beta_empty)) =
            list_company_employees(State(state.clone()), Path(beta.opc_id.clone())).await;
        assert_eq!(beta_empty_status, StatusCode::OK);
        assert_eq!(beta_empty.as_array().unwrap().len(), 0);

        let (seed_status, Json(seed_body)) =
            seed_company_employees_handler(State(state.clone()), Path(alpha.opc_id.clone())).await;
        assert_eq!(seed_status, StatusCode::OK);
        assert!(seed_body["total"].as_u64().unwrap() > 0);

        let (alpha_seeded_status, Json(alpha_seeded)) =
            list_company_employees(State(state.clone()), Path(alpha.opc_id.clone())).await;
        assert_eq!(alpha_seeded_status, StatusCode::OK);
        assert!(alpha_seeded.as_array().unwrap().len() > 0);

        let (beta_after_alpha_status, Json(beta_after_alpha)) =
            list_company_employees(State(state.clone()), Path(beta.opc_id.clone())).await;
        assert_eq!(beta_after_alpha_status, StatusCode::OK);
        assert_eq!(beta_after_alpha.as_array().unwrap().len(), 0);

        let delete_beta = delete_company(State(state.clone()), Path(beta.opc_id.clone())).await;
        assert_eq!(delete_beta.0, StatusCode::OK);

        let (alpha_after_delete_status, Json(alpha_after_delete)) =
            list_company_employees(State(state), Path(alpha.opc_id)).await;
        assert_eq!(alpha_after_delete_status, StatusCode::OK);
        assert_eq!(
            alpha_after_delete.as_array().unwrap().len(),
            alpha_seeded.as_array().unwrap().len()
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_employee_passport_is_read_only_on_update() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company("Passport Co", Some("Read only passport"), "default-founder")
            .await
            .unwrap();

        let (seed_status, _) =
            seed_company_employees_handler(State(state.clone()), Path(company.opc_id.clone())).await;
        assert_eq!(seed_status, StatusCode::OK);

        let (get_status, Json(before)) = get_company_employee(
            State(state.clone()),
            Path((company.opc_id.clone(), "agent-pm-01".to_string())),
        )
        .await;
        assert_eq!(get_status, StatusCode::OK);
        let mut employee: AgentEmployee = serde_json::from_value(before.clone()).unwrap();
        let original_passport_id = employee.passport.passport_id.clone();
        employee.display_name = "Updated PM".to_string();
        employee.passport.passport_id = "tampered-passport".to_string();

        let (update_status, Json(updated)) = update_company_employee(
            State(state.clone()),
            Path((company.opc_id.clone(), "agent-pm-01".to_string())),
            Json(employee),
        )
        .await;
        assert_eq!(update_status, StatusCode::OK);
        assert_eq!(updated["display_name"], "Updated PM");

        let (after_status, Json(after)) = get_company_employee(
            State(state),
            Path((company.opc_id.clone(), "agent-pm-01".to_string())),
        )
        .await;
        assert_eq!(after_status, StatusCode::OK);
        assert_eq!(after["passport"]["passport_id"], original_passport_id);

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn classify_mission_track_is_server_authoritative_and_red_takes_priority() {
        let cases = [
            ("production database rollback", "red", "high-risk trigger"),
            ("critical P1 emergency", "red", "high-risk trigger"),
            ("customer data delete request", "red", "high-risk trigger"),
            (
                "draft a changelog and send it internally",
                "yellow",
                "moderate-risk trigger",
            ),
            (
                "update staging release notes",
                "yellow",
                "moderate-risk trigger",
            ),
            ("read metrics and analyze logs", "green", "Green Track"),
            (
                "produce a concise checklist for tomorrow",
                "green",
                "Green Track",
            ),
            ("review product positioning notes", "green", "Green Track"),
            (
                "summarize international market standards",
                "green",
                "Green Track",
            ),
            ("review api1 integration notes", "green", "Green Track"),
            (
                "step1 progress update summary",
                "yellow",
                "moderate-risk trigger",
            ),
        ];

        for (intent, expected_track, summary_fragment) in cases {
            let decision = classify_mission_track(intent);
            assert_eq!(decision.track, expected_track, "{intent}");
            assert!(decision.risk_summary.contains(summary_fragment), "{intent}");
        }

        let priority = classify_mission_track("send a production notification");
        assert_eq!(priority.track, "red");
        assert!(priority
            .restricted_actions
            .contains(&"production".to_string()));
    }

    #[test]
    fn memory_record_json_contract_accepts_snake_case_and_rejects_pascal_case() {
        let payload = serde_json::json!({
            "memory_id": "mem-contract-ok",
            "scope": "company",
            "owner_id": "default-opc",
            "title": "Operating Principles",
            "content": "Company rules",
            "tags": ["company-foundation"],
            "source": "first-run",
            "provenance": "first-run:default-opc:company-foundation",
            "confidence": 0.9,
            "ttl_seconds": 2592000,
            "created_at_ms": 1,
            "updated_at_ms": 1,
            "access_policy": "opc-local",
            "status": "active",
            "cognitive_layer": "Suggestion",
            "linked_contract_hash": null,
            "linked_plan_hash": null,
            "linked_adr_id": null
        });

        let parsed: MemoryRecord = serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(parsed.scope, MemoryScope::Company);
        assert_eq!(parsed.status, MemoryStatus::Active);

        let mut pascal_scope = payload.clone();
        pascal_scope["scope"] = serde_json::json!("Company");
        assert!(serde_json::from_value::<MemoryRecord>(pascal_scope).is_err());

        let mut pascal_status = payload;
        pascal_status["status"] = serde_json::json!("Active");
        assert!(serde_json::from_value::<MemoryRecord>(pascal_status).is_err());
    }

    #[tokio::test]
    async fn memory_scope_query_accepts_snake_case_after_snake_case_create_payload() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());
        let memory = MemoryRecord {
            memory_id: "mem-snake-query".to_string(),
            scope: MemoryScope::Company,
            owner_id: "default-opc".to_string(),
            title: "Operating Principles".to_string(),
            content: "Company rules".to_string(),
            tags: vec!["company-foundation".to_string()],
            source: "first-run".to_string(),
            provenance: "first-run:default-opc:company-foundation".to_string(),
            confidence: 0.9,
            ttl_seconds: 2592000,
            created_at_ms: 1,
            updated_at_ms: 1,
            access_policy: "opc-local".to_string(),
            status: MemoryStatus::Active,
            cognitive_layer: coevo_core::cognitive::CognitiveLayer::Suggestion,
            linked_contract_hash: None,
            linked_plan_hash: None,
            linked_adr_id: None,
        };

        let (create_status, Json(create_body)) =
            create_memory(State(state.clone()), Json(memory)).await;
        assert_eq!(create_status, StatusCode::OK, "{create_body:?}");

        let (list_status, Json(list_body)) = list_memory(
            State(state),
            Query(MemoryQuery {
                scope: Some("company".to_string()),
                owner_id: Some("default-opc".to_string()),
                include_revoked: None,
                q: None,
            }),
        )
        .await;

        assert_eq!(list_status, StatusCode::OK, "{list_body:?}");
        assert_eq!(list_body.as_array().unwrap().len(), 1);
        assert_eq!(list_body[0]["memory_id"], "mem-snake-query");
        assert_eq!(list_body[0]["scope"], "company");
        assert_eq!(list_body[0]["status"], "active");
    }

    #[tokio::test]
    async fn red_execute_returns_alpha_block_without_worker_audit_rows() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
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
            governance_proposal: None,
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
        let state = AppState::new(pool.clone(), std::env::temp_dir());
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
            governance_proposal: None,
        };

        let (status, Json(body)) = create_work_order(State(state), Json(create)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["track"], "red");
        assert_eq!(body["governance_verdict"]["effective_track"], "red");
        assert_eq!(body["governance_verdict"]["blocked"], true);
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
        assert!(stored
            .restricted_actions
            .contains(&"production".to_string()));
    }

    #[tokio::test]
    async fn governance_verdict_downgrades_requested_tier_on_server() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
        let work_order_id = "wo-verdict-downgrade";

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Read metrics and summarize customer trends".to_string(),
            selected_agents: vec![],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: Some(GovernanceProposal {
                autonomy_ceiling: AutonomyCeiling::FullAccess,
                model_preference: ModelPreference::Reasoning,
                assigned_agent_id: Some("agent-risk-01".to_string()),
            }),
        };

        let (status, Json(body)) = create_work_order(State(state), Json(create)).await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(
            body["governance_proposal"]["autonomy_ceiling"],
            "full_access"
        );
        assert_eq!(body["governance_verdict"]["effective_track"], "green");
        assert_eq!(
            body["governance_verdict"]["requested_ceiling"],
            "full_access"
        );
        assert_eq!(body["governance_verdict"]["effective_tier"], "read_only");
        assert_eq!(body["governance_verdict"]["downgraded"], true);
        assert_eq!(
            body["governance_verdict"]["resolved_agent_id"],
            "agent-risk-01"
        );
        let stored = work_order_repo::WorkOrderRepo::get(&pool, work_order_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.governance_verdict.unwrap().effective_tier,
            AutonomyCeiling::ReadOnly
        );
    }

    #[tokio::test]
    async fn green_execute_uses_scoped_file_readonly_tool() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
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
            governance_proposal: None,
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
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
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
            governance_proposal: None,
        };
        let (create_status, Json(created)) =
            create_work_order(State(state.clone()), Json(create)).await;
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
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
        let work_order_id = "wo-green-active-model-routing";
        let root =
            std::env::temp_dir().join(format!("coevo-active-model-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("mission-notes.md"),
            "active model routing evidence",
        )
        .unwrap();
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
            governance_proposal: None,
        };
        let (create_status, Json(created)) =
            create_work_order(State(state.clone()), Json(create)).await;
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
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
        let work_order_id = "wo-audit-export";
        let root =
            std::env::temp_dir().join(format!("coevo-audit-export-{}", uuid::Uuid::new_v4()));
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
            governance_proposal: None,
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
        assert!(export["tool_calls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tc| tc["tool_id"] == "file-readonly"));
        assert!(export["memory_records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| {
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
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
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
            governance_proposal: None,
        };
        let (create_status, Json(created)) =
            create_work_order(State(state.clone()), Json(create)).await;
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
        let approval = ApprovalRepo::find_by_id(&pool, approval_id)
            .await
            .unwrap()
            .unwrap();
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
        assert!(blocked_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("APPROVAL_RECEIPT_NOT_APPROVED"));
        assert_eq!(count_rows(&pool, "worker_sessions", work_order_id).await, 0);

        ApprovalRepo::approve(&pool, approval_id, "default-founder")
            .await
            .unwrap();
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
    async fn founder_and_company_handlers_manage_multi_company_workspace() {
        let root =
            std::env::temp_dir().join(format!("coevo-server-company-{}", uuid::Uuid::new_v4()));
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), root.clone());

        let (founder_status, Json(founder)) = get_founder(State(state.clone())).await;
        assert_eq!(founder_status, StatusCode::OK);
        assert_eq!(founder["founder_id"], "default-founder");

        let (update_founder_status, Json(updated_founder)) = put_founder(
            State(state.clone()),
            Json(UpdateFounderRequest {
                display_name: Some("WAE Founder".to_string()),
                preferences: None,
            }),
        )
        .await;
        assert_eq!(update_founder_status, StatusCode::OK);
        assert_eq!(updated_founder["display_name"], "WAE Founder");

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Alpha Labs".to_string(),
                mission: Some("Build alpha".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);
        let opc_id = created["opc_id"].as_str().unwrap().to_string();
        assert!(root.join("companies.json").exists());
        assert!(root.join(&opc_id).join("company.json").exists());
        assert!(root.join(&opc_id).join("data.db").exists());

        let (list_status, Json(companies)) = list_companies(State(state.clone())).await;
        assert_eq!(list_status, StatusCode::OK);
        assert_eq!(companies.as_array().unwrap().len(), 1);
        assert_eq!(companies[0]["opc_id"], opc_id);

        let (detail_status, Json(detail)) =
            get_company(State(state.clone()), Path(opc_id.clone())).await;
        assert_eq!(detail_status, StatusCode::OK);
        assert_eq!(detail["opc_id"], opc_id);
        assert!(detail["charter_md"].as_str().unwrap_or_default().contains("Alpha Labs"));

        let (update_status, Json(updated_company)) = put_company(
            State(state.clone()),
            Path(opc_id.clone()),
            Json(UpdateCompanyRequest {
                name: Some("Alpha Labs Renamed".to_string()),
                mission: Some("Build alpha better".to_string()),
                charter: Some("# Alpha Charter\n\nRenamed company.".to_string()),
            }),
        )
        .await;
        assert_eq!(update_status, StatusCode::OK);
        assert_eq!(updated_company["name"], "Alpha Labs Renamed");

        let (updated_detail_status, Json(updated_detail)) =
            get_company(State(state.clone()), Path(opc_id.clone())).await;
        assert_eq!(updated_detail_status, StatusCode::OK);
        assert!(updated_detail["charter_md"]
            .as_str()
            .unwrap_or_default()
            .contains("Alpha Charter"));

        let (delete_status, Json(deleted)) =
            delete_company(State(state.clone()), Path(opc_id.clone())).await;
        assert_eq!(delete_status, StatusCode::OK);
        assert_eq!(deleted["ok"], true);
        assert!(!root.join(&opc_id).exists());

        let (post_delete_list_status, Json(post_delete_companies)) =
            list_companies(State(state)).await;
        assert_eq!(post_delete_list_status, StatusCode::OK);
        assert!(post_delete_companies.as_array().unwrap().is_empty());

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn approval_endpoint_approves_and_reenters_with_receipt() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
        let work_order_id = "wo-yellow-approval-endpoint";
        let contract_hash = "e".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        create_yellow_work_order(state.clone(), work_order_id, &contract_hash).await;
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
        let approval_id = wait_body["approval_id"].as_str().unwrap().to_string();

        let (status, Json(body)) = decide_work_order_approval(
            State(state),
            Path(work_order_id.to_string()),
            Json(ApprovalDecisionRequest {
                approval_id: approval_id.clone(),
                decision: "approve".to_string(),
                comment: Some("approved in timeline".to_string()),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["approval_receipt"], approval_id);
        assert_eq!(body["approval_comment"], "approved in timeline");
        assert_eq!(body["status"], "Completed");
        assert!(count_rows(&pool, "worker_runs", work_order_id).await >= 1);
    }

    #[tokio::test]
    async fn yellow_execute_rejects_arbitrary_identity_string_as_approval_receipt() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
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
            governance_proposal: None,
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
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
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
        assert_eq!(
            count_rows(&pool, "worker_sessions", action_mismatch_wo).await,
            0
        );
    }
}
