use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use coevo_audit::logger::AuditLogger;
use coevo_core::opc::*;
use coevo_core::skills::*;
use coevo_evolution::{
    analyzer::FailureAnalyzer, generator::SkillGenerator, verifier::SkillVerifier,
};
use coevo_store::company_workspace::CompanyEmployeeFiles;
use coevo_store::pool::create_pool;
use coevo_store::repos::audit_repo::AuditRepo;
use coevo_store::repos::{approval_repo::ApprovalRepo, contract_repo::ContractRepo};
use coevo_store::repos_opc::*;
use coevo_worker::harness::{WorkerHarness, WorkerHarnessOptions};
use serde::Deserialize;
use serde::Serialize;
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
pub struct CompanySkillInstallRequest {
    pub skill_id: Option<String>,
    pub template: Option<String>,
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

#[derive(Deserialize)]
pub struct SharedFileUpsertRequest {
    pub path: String,
    pub content_md: String,
}

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

fn validate_plain_identifier(
    value: &str,
    field_name: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if value.trim().is_empty() {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{field_name} is required")
        ));
    }
    if !is_plain_identifier(value) {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{field_name} must be a plain identifier")
        ));
    }
    Ok(())
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

fn legacy_work_order_opc_mismatch(
    header_opc_id: &str,
    body_opc_id: &str,
) -> Option<(StatusCode, Json<serde_json::Value>)> {
    let body_opc_id = body_opc_id.trim();
    if !body_opc_id.is_empty() && body_opc_id != header_opc_id {
        Some(err!(
            StatusCode::CONFLICT,
            format!(
                "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match body opc_id={}",
                header_opc_id, body_opc_id
            )
        ))
    } else {
        None
    }
}

async fn scoped_legacy_work_order_pool(
    state: &AppState,
    headers: &HeaderMap,
    body_opc_id: Option<&str>,
) -> Result<Option<sqlx::SqlitePool>, (StatusCode, Json<serde_json::Value>)> {
    let header_opc_id = require_legacy_opc_id(headers, "/opc/work-orders routes")?;
    if let Some(body_opc_id) = body_opc_id {
        if let Some(err) = legacy_work_order_opc_mismatch(&header_opc_id, body_opc_id) {
            return Err(err);
        }
    }
    company_pool(state, &header_opc_id).await.map(Some)
}

async fn load_scoped_work_order(
    state: &AppState,
    headers: &HeaderMap,
    work_order_id: &str,
) -> Result<(WorkOrder, Option<sqlx::SqlitePool>), (StatusCode, Json<serde_json::Value>)> {
    let scoped_pool = scoped_legacy_work_order_pool(state, headers, None).await?;
    let pool_ref = scoped_pool.as_ref().unwrap_or(&state.pool);
    let work_order = match work_order_repo::WorkOrderRepo::get(pool_ref, work_order_id).await {
        Ok(Some(work_order)) => work_order,
        Ok(None) => return Err(err!(StatusCode::NOT_FOUND, "Work order not found")),
        Err(e) => return Err(err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    };
    if let Some(header_opc_id) = legacy_opc_id(headers) {
        if work_order.opc_id != header_opc_id {
            return Err(err!(
                StatusCode::CONFLICT,
                format!(
                    "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match stored opc_id={}",
                    header_opc_id, work_order.opc_id
                )
            ));
        }
    }
    Ok((work_order, scoped_pool))
}

async fn lookup_legacy_work_order_location(
    state: &AppState,
    work_order_id: &str,
) -> Result<Option<String>, (StatusCode, Json<serde_json::Value>)> {
    match work_order_repo::WorkOrderRepo::get(&state.pool, work_order_id).await {
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
        let found = match work_order_repo::WorkOrderRepo::get(&pool, work_order_id).await {
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

async fn resolve_legacy_executor_work_order(
    state: &AppState,
    headers: &HeaderMap,
    work_order_id: &str,
) -> Result<(WorkOrder, Option<sqlx::SqlitePool>), (StatusCode, Json<serde_json::Value>)> {
    let Some(location) = lookup_legacy_work_order_location(state, work_order_id).await? else {
        return Err(err!(StatusCode::NOT_FOUND, "Work order not found"));
    };

    if location == "default-opc" {
        let work_order = match work_order_repo::WorkOrderRepo::get(&state.pool, work_order_id).await
        {
            Ok(Some(work_order)) => work_order,
            Ok(None) => return Err(err!(StatusCode::NOT_FOUND, "Work order not found")),
            Err(e) => return Err(err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        };
        return Ok((work_order, None));
    }

    let header_opc_id = require_legacy_opc_id(headers, "/opc/executors dry-run routes")?;
    if location != header_opc_id {
        return Err(err!(
            StatusCode::CONFLICT,
            format!(
                "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match stored opc_id={}",
                header_opc_id, location
            )
        ));
    }

    load_scoped_work_order(state, headers, work_order_id).await
}

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

pub(crate) async fn company_pool(
    state: &AppState,
    opc_id: &str,
) -> Result<sqlx::SqlitePool, (StatusCode, Json<serde_json::Value>)> {
    if !is_plain_identifier(opc_id) {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "opc_id must be a plain identifier"
        ));
    }
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

fn company_employee_prompt_path(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
) -> std::path::PathBuf {
    state
        .company_workspace
        .company_employee_prompt_path(opc_id, agent_id)
}

fn company_employee_prompt_versions_dir(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
) -> std::path::PathBuf {
    state
        .company_workspace
        .company_employee_prompt_versions_dir(opc_id, agent_id)
}

fn company_employee_prompt_version_path(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
    version: i32,
) -> std::path::PathBuf {
    state
        .company_workspace
        .company_employee_prompt_version_path(opc_id, agent_id, version)
}

fn company_memory_markdown_path(
    state: &AppState,
    opc_id: &str,
    memory_id: &str,
) -> std::path::PathBuf {
    state
        .company_workspace
        .company_dir(opc_id)
        .join("memory")
        .join(format!("{memory_id}.md"))
}

fn company_shared_root(state: &AppState, opc_id: &str) -> std::path::PathBuf {
    state.company_workspace.company_dir(opc_id).join("shared")
}

fn resolve_company_shared_path(
    state: &AppState,
    opc_id: &str,
    relative_path: &str,
) -> Result<std::path::PathBuf, String> {
    let trimmed = relative_path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("shared path is required".to_string());
    }
    let relative = std::path::Path::new(&trimmed);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("shared path must stay within company shared/".to_string());
    }
    Ok(company_shared_root(state, opc_id).join(relative))
}

fn count_files_recursively(root: &std::path::Path) -> usize {
    let mut total = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                total += 1;
            }
        }
    }
    total
}

fn company_skill_markdown_path(
    state: &AppState,
    opc_id: &str,
    skill_id: &str,
) -> std::path::PathBuf {
    state
        .company_workspace
        .company_skill_markdown_path(opc_id, skill_id)
}

fn company_employee_skill_markdown_path(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
    skill_id: &str,
) -> std::path::PathBuf {
    state
        .company_workspace
        .company_employee_skill_markdown_path(opc_id, agent_id, skill_id)
}

fn company_skill_scope(skill: &AgentSkillPackage, agent_id: Option<&str>) -> &'static str {
    if is_employee_skill(skill) && agent_id.is_some_and(|agent_id| skill.owner_agent_id == agent_id)
    {
        "employee"
    } else {
        "company"
    }
}

fn is_employee_skill(skill: &AgentSkillPackage) -> bool {
    skill.provenance.starts_with("skill-evolution-")
}

fn ensure_company_skill_file(
    state: &AppState,
    opc_id: &str,
    skill: &AgentSkillPackage,
    agent_id: Option<&str>,
) -> Result<(), String> {
    state
        .company_workspace
        .write_company_skill_markdown(opc_id, skill, agent_id)
        .map_err(|e| e.to_string())
}

fn company_skill_response(
    state: &AppState,
    opc_id: &str,
    skill: &AgentSkillPackage,
    agent_id: Option<&str>,
) -> serde_json::Value {
    let scope = company_skill_scope(skill, agent_id);
    let path = if let Some(agent_id) = agent_id {
        format!("employees/{agent_id}/skills/{}/SKILL.md", skill.skill_id)
    } else {
        format!("skills/{}/SKILL.md", skill.skill_id)
    };
    let markdown_path = if let Some(agent_id) = agent_id {
        company_employee_skill_markdown_path(state, opc_id, agent_id, &skill.skill_id)
    } else {
        company_skill_markdown_path(state, opc_id, &skill.skill_id)
    };
    let content_md = std::fs::read_to_string(markdown_path).unwrap_or_default();
    serde_json::json!({
        "skill_name": skill.skill_id,
        "scope": scope,
        "version": skill.version,
        "description": skill.description,
        "allowed_tools": skill.required_tools,
        "source": skill.provenance,
        "path": path,
        "content_md": content_md,
    })
}

#[derive(Clone, Serialize, Deserialize)]
struct CompanyEmployeePassportFile {
    agent_id: String,
    display_name: String,
    department: Department,
    role: String,
    supervisor_agent_id: Option<String>,
    lifecycle_status: LifecycleStatus,
    created_at_ms: u64,
    issued_passport: AgentPassport,
}

#[derive(Serialize)]
struct CompanyEmployeeDetailResponse {
    agent_id: String,
    display_name: String,
    department: Department,
    role: String,
    lifecycle_status: LifecycleStatus,
    risk_ceiling: f64,
    reputation: f64,
    level: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    supervisor_agent_id: Option<String>,
    passport: CompanyEmployeePassportFile,
    identity_md: String,
    soul_md: String,
    prompt_md: String,
    agents_md: String,
    owner_md: String,
    tools_md: String,
    tool_policy: serde_json::Value,
    model_profile: ModelProviderProfile,
}

#[derive(Serialize)]
struct CompanyEmployeeSummaryResponse {
    agent_id: String,
    display_name: String,
    department: Department,
    role: String,
    supervisor_agent_id: Option<String>,
    lifecycle_status: LifecycleStatus,
    risk_ceiling: f64,
    reputation: f64,
    level: String,
    created_at_ms: u64,
}

fn department_label(department: Department) -> &'static str {
    match department {
        Department::FounderOffice => "founder office",
        Department::Product => "product",
        Department::Engineering => "engineering",
        Department::Research => "research",
        Department::Growth => "growth",
        Department::Finance => "finance",
        Department::Legal => "legal",
        Department::SRE => "sre",
        Department::Design => "design",
        Department::Content => "content",
        Department::Governance => "governance",
        Department::Custom => "custom",
    }
}

fn lifecycle_label(status: LifecycleStatus) -> &'static str {
    match status {
        LifecycleStatus::Draft => "draft",
        LifecycleStatus::Active => "active",
        LifecycleStatus::Suspended => "suspended",
        LifecycleStatus::Retired => "retired",
    }
}

fn employee_passport_file(employee: &AgentEmployee) -> CompanyEmployeePassportFile {
    CompanyEmployeePassportFile {
        agent_id: employee.agent_id.clone(),
        display_name: employee.display_name.clone(),
        department: employee.department,
        role: employee.role.clone(),
        supervisor_agent_id: employee.supervisor_agent_id.clone(),
        lifecycle_status: employee.lifecycle_status,
        created_at_ms: employee.created_at_ms,
        issued_passport: employee.passport.clone(),
    }
}

fn employee_reputation_score(employee: &AgentEmployee) -> f64 {
    let v = &employee.reputation_vector;
    (v.task_domain_competence + v.uncertainty_honesty + v.policy_compliance + v.resource_efficiency)
        / 4.0
}

fn employee_reputation_level(score: f64) -> String {
    if score >= 0.8 {
        "senior".to_string()
    } else if score >= 0.6 {
        "mid".to_string()
    } else if score > 0.0 {
        "junior".to_string()
    } else {
        "new".to_string()
    }
}

fn company_employee_summary_response(employee: &AgentEmployee) -> CompanyEmployeeSummaryResponse {
    let reputation = employee_reputation_score(employee);
    CompanyEmployeeSummaryResponse {
        agent_id: employee.agent_id.clone(),
        display_name: employee.display_name.clone(),
        department: employee.department,
        role: employee.role.clone(),
        supervisor_agent_id: employee.supervisor_agent_id.clone(),
        lifecycle_status: employee.lifecycle_status,
        risk_ceiling: employee.risk_ceiling,
        reputation,
        level: employee_reputation_level(reputation),
        created_at_ms: employee.created_at_ms,
    }
}

fn default_employee_identity_md(employee: &AgentEmployee) -> String {
    format!(
        "# {}\n\n- Agent ID: {}\n- Department: {}\n- Role: {}\n- Status: {}\n",
        employee.display_name,
        employee.agent_id,
        department_label(employee.department),
        employee.role,
        lifecycle_label(employee.lifecycle_status)
    )
}

fn default_employee_soul_md(employee: &AgentEmployee) -> String {
    format!(
        "# Soul\n\n{} works as a {} employee for the {} team. Preserve stable working style, boundaries, and long-term preferences here.\n",
        employee.display_name,
        employee.role,
        department_label(employee.department)
    )
}

fn default_employee_agents_md(employee: &AgentEmployee) -> String {
    format!(
        "# Rules\n\n- Stay within the {} role.\n- Respect company governance and approval boundaries.\n- Keep work aligned with supervisor expectations for {}.\n",
        employee.role,
        employee.display_name
    )
}

fn default_employee_owner_md(employee: &AgentEmployee) -> String {
    format!(
        "# Owner\n\nPrimary employer: default-founder.\nPreferred working relationship with {} should be captured here.\n",
        employee.display_name
    )
}

fn default_employee_tools_md(employee: &AgentEmployee) -> String {
    format!(
        "# Tools\n\nPreferred tool usage notes for {} belong here. Treat this as human-authored guidance layered above the tool policy.\n",
        employee.display_name
    )
}

fn default_employee_tool_policy(employee: &AgentEmployee) -> serde_json::Value {
    serde_json::json!({
        "allowed_tools": employee.tool_scopes,
        "risk_ceiling": employee.risk_ceiling,
        "permission_boundary": employee.permission_boundary,
    })
}

fn render_memory_markdown(memory: &MemoryRecord) -> String {
    let mut lines = vec![
        "---".to_string(),
        format!("memory_id: {}", memory.memory_id),
        format!("scope: {:?}", memory.scope).to_lowercase(),
        format!("owner_id: {}", memory.owner_id),
        format!("source: {}", memory.source),
        format!("provenance: {}", memory.provenance),
        format!("status: {:?}", memory.status).to_lowercase(),
        "---".to_string(),
        String::new(),
        format!("# {}", memory.title),
        String::new(),
        memory.content.trim().to_string(),
    ];
    if !memory.tags.is_empty() {
        lines.push(String::new());
        lines.push(format!("tags: {}", memory.tags.join(", ")));
    }
    lines.join("\n").trim_end().to_string() + "\n"
}

fn write_company_memory_markdown(
    state: &AppState,
    opc_id: &str,
    memory: &MemoryRecord,
) -> Result<(), String> {
    if !is_plain_identifier(&memory.memory_id) {
        return Err("memory_id must be a plain identifier".to_string());
    }
    let path = company_memory_markdown_path(state, opc_id, &memory.memory_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, render_memory_markdown(memory)).map_err(|e| e.to_string())
}

fn company_employee_detail_response(
    state: &AppState,
    opc_id: &str,
    employee: &AgentEmployee,
) -> Result<CompanyEmployeeDetailResponse, String> {
    let files = state
        .company_workspace
        .read_company_employee_files(opc_id, &employee.agent_id)
        .map_err(|e| e.to_string())?;
    let passport = if files.passport_json.is_null() {
        serde_json::to_value(employee_passport_file(employee)).map_err(|e| e.to_string())?
    } else {
        files.passport_json.clone()
    };
    let identity_md = if files.identity_md.trim().is_empty() {
        default_employee_identity_md(employee)
    } else {
        files.identity_md
    };
    let soul_md = if files.soul_md.trim().is_empty() {
        default_employee_soul_md(employee)
    } else {
        files.soul_md
    };
    let prompt_md = files.prompt_md;
    let agents_md = if files.agents_md.trim().is_empty() {
        default_employee_agents_md(employee)
    } else {
        files.agents_md
    };
    let owner_md = if files.owner_md.trim().is_empty() {
        default_employee_owner_md(employee)
    } else {
        files.owner_md
    };
    let tools_md = if files.tools_md.trim().is_empty() {
        default_employee_tools_md(employee)
    } else {
        files.tools_md
    };
    let tool_policy = if files.tool_policy_json.is_null() {
        default_employee_tool_policy(employee)
    } else {
        files.tool_policy_json
    };

    let reputation = employee_reputation_score(employee);

    Ok(CompanyEmployeeDetailResponse {
        agent_id: employee.agent_id.clone(),
        display_name: employee.display_name.clone(),
        department: employee.department,
        role: employee.role.clone(),
        lifecycle_status: employee.lifecycle_status,
        risk_ceiling: employee.risk_ceiling,
        reputation,
        level: employee_reputation_level(reputation),
        created_at_ms: employee.created_at_ms,
        updated_at_ms: employee.updated_at_ms,
        supervisor_agent_id: employee.supervisor_agent_id.clone(),
        passport: serde_json::from_value(passport).map_err(|e| e.to_string())?,
        identity_md,
        soul_md,
        prompt_md,
        agents_md,
        owner_md,
        tools_md,
        tool_policy,
        model_profile: employee.model_profile.clone(),
    })
}

fn ensure_company_employee_files(
    state: &AppState,
    opc_id: &str,
    employee: &AgentEmployee,
) -> Result<(), String> {
    state
        .company_workspace
        .ensure_company_employee_skeleton(opc_id, &employee.agent_id)
        .map_err(|e| e.to_string())?;
    let current = state
        .company_workspace
        .read_company_employee_files(opc_id, &employee.agent_id)
        .map_err(|e| e.to_string())?;
    let prompt_md = if current.prompt_md.trim().is_empty() {
        let migrated = employee.system_prompt.trim();
        if !migrated.is_empty() {
            migrated.to_string()
        } else {
            String::new()
        }
    } else {
        current.prompt_md
    };
    state
        .company_workspace
        .write_company_employee_files(
            opc_id,
            &employee.agent_id,
            &CompanyEmployeeFiles {
                passport_json: if current.passport_json.is_null() {
                    serde_json::to_value(employee_passport_file(employee))
                        .map_err(|e| e.to_string())?
                } else {
                    current.passport_json
                },
                prompt_md,
                identity_md: if current.identity_md.trim().is_empty() {
                    default_employee_identity_md(employee)
                } else {
                    current.identity_md
                },
                soul_md: if current.soul_md.trim().is_empty() {
                    default_employee_soul_md(employee)
                } else {
                    current.soul_md
                },
                agents_md: if current.agents_md.trim().is_empty() {
                    default_employee_agents_md(employee)
                } else {
                    current.agents_md
                },
                owner_md: if current.owner_md.trim().is_empty() {
                    default_employee_owner_md(employee)
                } else {
                    current.owner_md
                },
                tools_md: if current.tools_md.trim().is_empty() {
                    default_employee_tools_md(employee)
                } else {
                    current.tools_md
                },
                tool_policy_json: if current.tool_policy_json.is_null() {
                    default_employee_tool_policy(employee)
                } else {
                    current.tool_policy_json
                },
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn read_company_employee_current_prompt_version(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
) -> Result<Option<i32>, String> {
    state
        .company_workspace
        .read_company_employee_current_prompt_version(opc_id, agent_id)
        .map_err(|e| e.to_string())
}

fn write_company_employee_prompt_version(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
    version: i32,
    content: &str,
) -> Result<(), String> {
    state
        .company_workspace
        .write_company_employee_prompt_version(opc_id, agent_id, version, content, true)
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
    let blocked = false;

    GovernanceVerdict {
        effective_track: track_decision.track.to_string(),
        effective_tier,
        requested_ceiling: proposal.autonomy_ceiling,
        downgraded,
        downgrade_reason: downgraded.then(|| {
            "Requested autonomy exceeds the server RiskGate ceiling for this task.".to_string()
        }),
        blocked,
        block_reason: None,
        resolved_agent_id,
    }
}

// Server-authoritative Red/Yellow trigger keyword lists. These are bilingual
// (English + 中文) and aligned with the MCL `CONCEPTS` table in
// crates/coevo-mcl/src/compiler.rs so that the work-order track classifier on
// the server agrees with the contract compiler. Chinese deploy-to-production,
// delete-database, payment, and PII/customer-data missions MUST classify to the
// same elevated track as their English equivalents.
//
// Red = irreversible / production / financial / sensitive-data operations.
// (MCL concepts: Production 生产/线上/正式, Delete 删除/移除, Payment 支付/付款,
//  CustomerData 客户数据/用户数据/个人信息/隐私.)
const RED_TRIGGERS: &[&str] = &[
    // --- English ---
    "production",
    "prod",
    "live env",
    "critical",
    "database mutation",
    "drop table",
    "truncate",
    "rollback",
    "roll back",
    "payment",
    "pay",
    "transfer",
    "refund",
    "payout",
    "delete",
    "remove",
    "purge",
    "wipe",
    "p1",
    "emergency",
    "financial",
    "customer data",
    "user data",
    "personal information",
    "personal info",
    "privacy",
    "pii",
    // --- 中文 (Chinese) ---
    // Production / 线上正式环境 (MCL Production concept)
    "生产",
    "线上",
    "正式环境",
    "正式发布",
    // Delete / 删除移除 (MCL Delete concept)
    "删除",
    "移除",
    "清空",
    "清除",
    "删库",
    // Payment / 支付付款 (MCL Payment concept)
    "支付",
    "付款",
    "转账",
    "退款",
    // Customer data / PII (MCL CustomerData concept)
    "客户数据",
    "用户数据",
    "个人信息",
    "隐私",
    // Generic high-risk markers
    "紧急",
    "回滚",
];
// Yellow = reversible internal writes / deploys-to-non-prod / notifications.
// (MCL concepts: Write 写入/修改, Deploy 部署/发布/上线, Staging 预发/测试环境,
//  Database 数据库, EmailNotify 通知/邮件, Shell 命令/脚本.)
const YELLOW_TRIGGERS: &[&str] = &[
    // --- English ---
    "deploy",
    "release",
    "publish",
    "rollout",
    "roll out",
    "canary",
    "notification",
    "notify",
    "broadcast",
    "alert",
    "staging",
    "preprod",
    "pre-prod",
    "send",
    "create ticket",
    "write",
    "update",
    "changelog",
    "internal",
    "modify",
    "database",
    "schema",
    "shell",
    "script",
    "run command",
    // --- 中文 (Chinese) ---
    // Deploy / 部署发布上线 (MCL Deploy concept)
    "部署",
    "发布",
    "上线",
    "灰度",
    // Write / 写入修改 (MCL Write concept)
    "写入",
    "修改",
    "更新",
    "创建",
    "新建",
    "编辑",
    // Staging / 预发测试环境 (MCL Staging concept)
    "预发",
    "测试环境",
    "沙箱",
    // Database / 数据库 (MCL Database concept)
    "数据库",
    "库表",
    // Notify / 通知邮件 (MCL EmailNotify concept)
    "通知",
    "邮件",
    "群发",
    // Shell / 命令脚本 (MCL Shell concept)
    "命令",
    "脚本",
    "终端",
    "执行命令",
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
    for &trigger in RED_TRIGGERS {
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
    for &trigger in YELLOW_TRIGGERS {
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

fn approval_receipt_for_track<'a>(req: &'a ExecuteRequest, track: &str) -> Option<&'a str> {
    // Alpha compatibility: Yellow approval receipts may arrive through the legacy
    // caller_identity_proof field or lease_id. Red Track must stay stricter and
    // only accept the explicit approval-receipt field so lease IDs cannot be
    // misrouted into the human-approval path.
    let caller_receipt = req
        .caller_identity_proof
        .as_deref()
        .filter(|s| !s.trim().is_empty());
    if caller_receipt.is_some() {
        return caller_receipt;
    }

    if track == "yellow" {
        req.lease_id.as_deref().filter(|s| !s.trim().is_empty())
    } else {
        None
    }
}

fn parse_approval_receipt_proof(proof: &str) -> (&str, Option<&str>) {
    match proof.split_once(':') {
        Some((approval_id, digest))
            if !approval_id.trim().is_empty() && !digest.trim().is_empty() =>
        {
            (approval_id.trim(), Some(digest.trim()))
        }
        _ => (proof.trim(), None),
    }
}

fn compose_approval_receipt(approval_id: &str, pending_action_digest: Option<&str>) -> String {
    match pending_action_digest {
        Some(digest) if !digest.trim().is_empty() => format!("{approval_id}:{}", digest.trim()),
        _ => approval_id.to_string(),
    }
}

fn approval_actor(headers: &HeaderMap, fallback_actor: &str) -> String {
    let _ = headers;
    fallback_actor.to_string()
}

async fn pending_action_digest_for_work_order(
    pool: &sqlx::SqlitePool,
    work_order_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT messages_json FROM worker_sessions WHERE session_id=?")
            .bind(format!("session-{work_order_id}"))
            .fetch_optional(pool)
            .await?;
    let Some(messages_json) = row else {
        return Ok(None);
    };
    let Ok(cursor) = serde_json::from_str::<serde_json::Value>(&messages_json) else {
        return Ok(None);
    };
    if cursor.get("kind").and_then(|value| value.as_str()) != Some("controlled_react_cursor") {
        return Ok(None);
    }
    Ok(cursor
        .get("pending_action_digest")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned))
}

/// Map an `ExecutorError` to an HTTP status + message for the executor routes.
///
/// `NotRegistered`/`Disabled` are 403 (the executor is structurally unusable);
/// `RiskCeilingExceeded`/`PermissionDenied` are 403 (governance rejection);
/// `Timeout` is 504; everything else (`Internal`) is 502 (the upstream runtime
/// — HTTP runtime / docker / local process — failed).
fn executor_error_status(err: &coevo_executors::ExecutorError) -> StatusCode {
    use coevo_executors::ExecutorError::*;
    match err {
        NotRegistered | Disabled | RiskCeilingExceeded { .. } | PermissionDenied(_) => {
            StatusCode::FORBIDDEN
        }
        Timeout => StatusCode::GATEWAY_TIMEOUT,
        Internal(_) => StatusCode::BAD_GATEWAY,
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
    let display_name = req.display_name.unwrap_or_else(|| {
        existing
            .as_ref()
            .map(|p| p.display_name.clone())
            .unwrap_or_else(|| "Founder".to_string())
    });
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
        approval_preferences: existing
            .as_ref()
            .map(|p| p.approval_preferences.clone())
            .unwrap_or(ApprovalPreferences {
                auto_approve_below_risk: 0.3,
                require_explicit_for_yellow: true,
                require_mfa_for_red: true,
                negative_consent_timeout_secs: 300,
            }),
        data_boundaries: existing
            .as_ref()
            .map(|p| p.data_boundaries.clone())
            .unwrap_or_default(),
        budget_limits: existing
            .as_ref()
            .map(|p| p.budget_limits.clone())
            .unwrap_or(BudgetLimits {
                max_cost_per_task_usd: 50.0,
                max_cost_per_day_usd: 500.0,
                max_agents_per_task: 5,
            }),
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
            let mut enriched = Vec::with_capacity(items.len());
            for company in items {
                let company_dir = s.company_workspace.company_dir(&company.opc_id);
                let company_identity = std::fs::read_to_string(company_dir.join("company.json"))
                    .ok()
                    .and_then(|raw| {
                        serde_json::from_str::<coevo_store::company_workspace::CompanyIdentity>(
                            &raw,
                        )
                        .ok()
                    });
                let db_path = s.company_workspace.company_db_path(&company.opc_id);
                let employee_count = if db_path.exists() {
                    let database_url = format!("sqlite://{}", db_path.to_string_lossy());
                    match create_pool(&database_url).await {
                        Ok(pool) => {
                            let count = agent_employee_repo::AgentEmployeeRepo::list(&pool)
                                .await
                                .map(|items| items.len())
                                .unwrap_or(0);
                            pool.close().await;
                            count
                        }
                        Err(_) => 0,
                    }
                } else {
                    0
                };
                enriched.push(serde_json::json!({
                    "opc_id": company.opc_id,
                    "name": company.name,
                    "mission": company_identity
                        .as_ref()
                        .map(|identity| identity.mission.clone())
                        .unwrap_or_default(),
                    "employee_count": employee_count,
                    "created_at_ms": company.created_at_ms,
                    "dir": company.dir,
                }));
            }
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
        Ok(company) => {
            let pool = match company_pool(&s, &company.opc_id).await {
                Ok(pool) => pool,
                Err(err) => return err,
            };
            if let Err(e) = skill_repo::SkillRepo::seed_default(&pool).await {
                pool.close().await;
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
            let skills = match skill_repo::SkillRepo::list(&pool, None).await {
                Ok(skills) => skills,
                Err(e) => {
                    pool.close().await;
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                }
            };
            for skill in skills
                .into_iter()
                .filter(|skill| skill.status == SkillStatus::Active && !is_employee_skill(skill))
            {
                if let Err(e) = ensure_company_skill_file(&s, &company.opc_id, &skill, None) {
                    pool.close().await;
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
                }
            }
            // Seed the default org for the new company: the secretary (intelligent
            // dispatcher) plus one head per department. Every company starts with a
            // working org chart so the secretary can route the founder's first task.
            let mut employee_count = 0usize;
            if let Err(e) = agent_employee_repo::AgentEmployeeRepo::seed(&pool).await {
                pool.close().await;
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
            if let Ok(employees) = agent_employee_repo::AgentEmployeeRepo::list(&pool).await {
                employee_count = employees.len();
                for employee in &employees {
                    if let Err(error) = ensure_company_employee_files(&s, &company.opc_id, employee)
                    {
                        pool.close().await;
                        return err!(StatusCode::INTERNAL_SERVER_ERROR, error);
                    }
                }
            }
            pool.close().await;
            ok!(serde_json::json!({
                "opc_id": company.opc_id,
                "name": company.name,
                "mission": req.mission.unwrap_or_default(),
                "employee_count": employee_count,
                "created_at_ms": company.created_at_ms,
                "dir": company.dir,
            }))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_company_shared_files(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match company_pool(&s, &opc_id).await {
        Ok(pool) => pool.close().await,
        Err(err) => return err,
    }
    let shared_root = company_shared_root(&s, &opc_id);
    let mut items = Vec::new();
    let mut stack = vec![shared_root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(&shared_root)
                .ok()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let content_md = std::fs::read_to_string(&path).unwrap_or_default();
            items.push(serde_json::json!({
                "path": relative,
                "content_md": content_md,
            }));
        }
    }
    ok!(serde_json::Value::Array(items))
}

pub async fn put_company_shared_file(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<SharedFileUpsertRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match company_pool(&s, &opc_id).await {
        Ok(pool) => pool.close().await,
        Err(err) => return err,
    }
    let target = match resolve_company_shared_path(&s, &opc_id, &req.path) {
        Ok(path) => path,
        Err(message) => return err!(StatusCode::UNPROCESSABLE_ENTITY, message),
    };
    if let Some(parent) = target.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    }
    if let Err(e) = std::fs::write(&target, &req.content_md) {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    ok!(serde_json::json!({"ok": true, "path": req.path}))
}

pub async fn get_company(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let listed = match s.company_workspace.list_companies().await {
        Ok(items) => items,
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    let Some(company) = listed.into_iter().find(|company| company.opc_id == opc_id) else {
        pool.close().await;
        return err!(StatusCode::NOT_FOUND, "company not found");
    };
    let company_dir = s.company_workspace.company_dir(&company.opc_id);
    let company_identity = std::fs::read_to_string(company_dir.join("company.json"))
        .ok()
        .and_then(|raw| {
            serde_json::from_str::<coevo_store::company_workspace::CompanyIdentity>(&raw).ok()
        });
    let charter_path = company_dir.join("charter.md");
    let charter_md = std::fs::read_to_string(charter_path).unwrap_or_default();
    let employee_count = agent_employee_repo::AgentEmployeeRepo::list(&pool)
        .await
        .map(|items| items.len())
        .unwrap_or(0);
    let memory_count = memory_repo::MemoryRepo::list(
        &pool,
        memory_scope_query_to_db(Some("company")),
        None,
        false,
    )
    .await
    .map(|items| items.len())
    .unwrap_or(0);
    let company_profile = opc_profile_repo::OPCProfileRepo::get(&pool, &company.opc_id)
        .await
        .ok()
        .flatten();
    pool.close().await;
    let shared_files_count = count_files_recursively(&company_dir.join("shared"));
    let report_count = count_files_recursively(&company_dir.join("reports"));
    ok!(serde_json::json!({
        "opc_id": company.opc_id,
        "name": company.name,
        "mission": company_identity
            .as_ref()
            .map(|identity| identity.mission.clone())
            .unwrap_or_default(),
        "employee_count": employee_count,
        "created_at_ms": company.created_at_ms,
        "dir": company.dir,
        "charter_md": charter_md,
        "goals": company_profile
            .as_ref()
            .map(|profile| profile.active_projects.clone())
            .unwrap_or_default(),
        "departments": company_profile
            .as_ref()
            .map(|profile| profile.default_departments.clone())
            .unwrap_or_default(),
        "shared_files_count": shared_files_count,
        "memory_count": memory_count,
        "report_count": report_count,
    }))
}

pub async fn put_company(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<UpdateCompanyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match company_pool(&s, &opc_id).await {
        Ok(pool) => pool.close().await,
        Err(err) => return err,
    }
    let company_dir = s.company_workspace.company_dir(&opc_id);

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
    if let Err(e) = std::fs::write(
        &company_json,
        serde_json::to_string_pretty(&identity).unwrap(),
    ) {
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
    if let Err(e) = std::fs::write(
        index_path,
        serde_json::to_string_pretty(&updated_index).unwrap(),
    ) {
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
    if !is_plain_identifier(&opc_id) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "opc_id must be a plain identifier"
        );
    }
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
    headers: HeaderMap,
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/profile/company") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = opc_profile_repo::OPCProfileRepo::get(&pool, &opc_id).await;
    pool.close().await;
    match result {
        Ok(Some(p)) => ok!(serde_json::to_value(p).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "OPC profile not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_company_profile_canonical(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = opc_profile_repo::OPCProfileRepo::get(&pool, &opc_id).await;
    pool.close().await;
    match result {
        Ok(Some(p)) => ok!(serde_json::to_value(p).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "OPC profile not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn put_company_profile(
    headers: HeaderMap,
    State(s): State<AppState>,
    Json(mut p): Json<OPCProfile>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/profile/company") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    p.opc_id = opc_id.clone();
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = opc_profile_repo::OPCProfileRepo::upsert(&pool, &p).await;
    pool.close().await;
    result.map_or_else(
        |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        |_| ok!(serde_json::json!({"ok":true,"opc_id":opc_id})),
    )
}

pub async fn put_company_profile_canonical(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(mut p): Json<OPCProfile>,
) -> (StatusCode, Json<serde_json::Value>) {
    p.opc_id = opc_id.clone();
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = opc_profile_repo::OPCProfileRepo::upsert(&pool, &p).await;
    pool.close().await;
    result.map_or_else(
        |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        |_| ok!(serde_json::json!({"ok":true,"opc_id":opc_id})),
    )
}

// === Memory ===
pub async fn list_memory(
    headers: HeaderMap,
    State(s): State<AppState>,
    Query(mut q): Query<MemoryQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/memory routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    q.scope = Some("company".to_string());
    q.owner_id = Some(opc_id.clone());
    let scope = memory_scope_query_to_db(q.scope.as_deref());
    let res = if let Some(ref query) = q.q {
        memory_repo::MemoryRepo::search(&pool, query, scope, q.owner_id.as_deref()).await
    } else {
        memory_repo::MemoryRepo::list(
            &pool,
            scope,
            q.owner_id.as_deref(),
            q.include_revoked.unwrap_or(false),
        )
        .await
    };
    pool.close().await;
    res.map_or_else(
        |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        |items| ok!(serde_json::to_value(items).unwrap()),
    )
}
pub async fn create_memory(
    headers: HeaderMap,
    State(s): State<AppState>,
    Json(mut m): Json<MemoryRecord>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/memory routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    if let Err(err) = validate_plain_identifier(&m.memory_id, "memory_id") {
        return err;
    }
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    m.scope = MemoryScope::Company;
    m.owner_id = opc_id.clone();
    let result = memory_repo::MemoryRepo::create(&pool, &m).await;
    if result.is_ok() {
        if let Err(e) = write_company_memory_markdown(&s, &opc_id, &m) {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    }
    pool.close().await;
    match result {
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

pub async fn list_company_memory(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Query(mut q): Query<MemoryQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    q.scope = Some("company".to_string());
    q.owner_id = Some(opc_id.clone());
    let scope = memory_scope_query_to_db(q.scope.as_deref());
    let result = if let Some(ref query) = q.q {
        memory_repo::MemoryRepo::search(&pool, query, scope, q.owner_id.as_deref()).await
    } else {
        memory_repo::MemoryRepo::list(
            &pool,
            scope,
            q.owner_id.as_deref(),
            q.include_revoked.unwrap_or(false),
        )
        .await
    };
    pool.close().await;
    result.map_or_else(
        |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        |items| ok!(serde_json::to_value(items).unwrap()),
    )
}

pub async fn create_company_memory(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(mut m): Json<MemoryRecord>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(err) = validate_plain_identifier(&m.memory_id, "memory_id") {
        return err;
    }
    m.scope = MemoryScope::Company;
    m.owner_id = opc_id.clone();
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = memory_repo::MemoryRepo::create(&pool, &m).await;
    if result.is_ok() {
        if let Err(e) = write_company_memory_markdown(&s, &opc_id, &m) {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    }
    pool.close().await;
    match result {
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
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/memory routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    memory_repo::MemoryRepo::mark_stale(&pool, &id).await.ok();
    pool.close().await;
    ok!(serde_json::json!({"ok":true}))
}
pub async fn revoke_memory(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/memory routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    memory_repo::MemoryRepo::revoke(&pool, &id).await.ok();
    pool.close().await;
    ok!(serde_json::json!({"ok":true}))
}

pub async fn stale_company_memory(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    stale_memory(legacy_company_headers(&opc_id), State(s), Path(id)).await
}

pub async fn revoke_company_memory(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    revoke_memory(legacy_company_headers(&opc_id), State(s), Path(id)).await
}

// === Employees ===
pub async fn list_employees(
    headers: HeaderMap,
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    list_company_employees(State(s), Path(opc_id)).await
}

pub async fn list_company_employees(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result: Result<Vec<CompanyEmployeeSummaryResponse>, sqlx::Error> =
        match agent_employee_repo::AgentEmployeeRepo::list(&pool).await {
            Ok(employees) => {
                for employee in &employees {
                    if let Err(error) = ensure_company_employee_files(&s, &opc_id, employee) {
                        return err!(StatusCode::INTERNAL_SERVER_ERROR, error);
                    }
                }
                Ok(employees
                    .into_iter()
                    .map(|employee| company_employee_summary_response(&employee))
                    .collect::<Vec<_>>())
            }
            Err(e) => Err(e),
        };
    pool.close().await;
    result.map_or_else(
        |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        |items| ok!(serde_json::to_value(items).unwrap()),
    )
}
pub async fn seed_employees_handler(
    headers: HeaderMap,
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    seed_company_employees_handler(State(s), Path(opc_id)).await
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
            let employees = match agent_employee_repo::AgentEmployeeRepo::list(&pool).await {
                Ok(employees) => employees,
                Err(e) => {
                    pool.close().await;
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                }
            };
            for employee in &employees {
                if let Err(error) = ensure_company_employee_files(&s, &opc_id, employee) {
                    pool.close().await;
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, error);
                }
            }
            let count = employees.len();
            ok!(serde_json::json!({"ok":true,"inserted":count,"total":count}))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    pool.close().await;
    response
}
pub async fn get_agent_memory(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = agent_memory_repo::AgentMemoryRepo::get(&pool, &id).await;
    pool.close().await;
    match result {
        Ok(Some(m)) => ok!(serde_json::to_value(m).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Agent memory not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// Employee growth: a plain-language view of how an AI employee is performing
/// over time — aggregated run stats, a reputation trend, and any pending
/// improvement suggestions awaiting the founder's approval.
pub async fn get_company_agent_memory(
    State(s): State<AppState>,
    Path((opc_id, agent_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = agent_memory_repo::AgentMemoryRepo::get(&pool, &agent_id).await;
    pool.close().await;
    match result {
        Ok(Some(m)) => ok!(serde_json::to_value(m).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "Agent memory not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_agent_growth(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    use coevo_store::repos::reputation_repo::ReputationHistoryRepo;
    use coevo_store::repos::worker_run_repo::WorkerRunRepo;

    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let db = &pool;

    let (total, completed, failed, avg_latency, tokens, cost) =
        WorkerRunRepo::agent_run_stats(db, &id)
            .await
            .unwrap_or((0, 0, 0, 0.0, 0, 0.0));

    let history = ReputationHistoryRepo::list_by_agent(db, &id, 100)
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
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(db, None)
        .await
        .unwrap_or_default();
    let pending: Vec<serde_json::Value> = proposals
        .iter()
        .filter_map(|p| {
            if matches!(
                p.status,
                EvolutionProposalStatus::Draft
                    | EvolutionProposalStatus::UnderVerification
                    | EvolutionProposalStatus::NeedsHumanReview
            ) {
                Some(serde_json::json!({
                    "proposal_id": p.proposal_id,
                    "diagnosis": p.diagnosis,
                    "suggested_prompt": p.proposed_changes,
                    "evidence": { "source_refs": p.source_refs },
                    "status": p.status,
                    "risk": p.risk_assessment,
                }))
            } else {
                None
            }
        })
        .collect();

    pool.close().await;

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

pub async fn get_company_agent_growth(
    State(s): State<AppState>,
    Path((opc_id, agent_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    get_agent_growth(legacy_company_headers(&opc_id), State(s), Path(agent_id)).await
}

pub async fn list_company_agent_improvements(
    State(s): State<AppState>,
    Path((opc_id, agent_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (status, Json(body)) =
        get_agent_growth(legacy_company_headers(&opc_id), State(s), Path(agent_id)).await;
    if status != StatusCode::OK {
        return (status, Json(body));
    }
    let pending = body
        .get("pending_improvements")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    ok!(pending)
}

pub async fn approve_company_agent_improvement(
    State(s): State<AppState>,
    Path((opc_id, _agent_id, proposal_id)): Path<(String, String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    approve_proposal(State(s), legacy_company_headers(&opc_id), Path(proposal_id)).await
}

pub async fn reject_company_agent_improvement(
    State(s): State<AppState>,
    Path((opc_id, _agent_id, proposal_id)): Path<(String, String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    reject_proposal(State(s), legacy_company_headers(&opc_id), Path(proposal_id)).await
}

pub async fn list_company_skill_evolution_proposals(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    list_proposals(State(s), legacy_company_headers(&opc_id)).await
}

pub async fn run_company_skill_evolution(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    run_evolution(State(s), legacy_company_headers(&opc_id)).await
}

pub async fn verify_company_skill_evolution_proposal(
    State(s): State<AppState>,
    Path((opc_id, proposal_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    verify_proposal(State(s), legacy_company_headers(&opc_id), Path(proposal_id)).await
}

pub async fn approve_company_skill_evolution_proposal(
    State(s): State<AppState>,
    Path((opc_id, proposal_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    approve_proposal(State(s), legacy_company_headers(&opc_id), Path(proposal_id)).await
}

pub async fn reject_company_skill_evolution_proposal(
    State(s): State<AppState>,
    Path((opc_id, proposal_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    reject_proposal(State(s), legacy_company_headers(&opc_id), Path(proposal_id)).await
}

// === Agent Workbench: employee CRUD + prompt management ===
pub async fn get_employee(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    get_company_employee(State(s), Path((opc_id, id))).await
}

pub async fn get_company_employee(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = agent_employee_repo::AgentEmployeeRepo::get(&pool, &id)
        .await
        .map(|employee| {
            if let Some(ref employee) = employee {
                let _ = ensure_company_employee_files(&s, &opc_id, employee);
            }
            employee
        });
    pool.close().await;
    match result {
        Ok(Some(e)) => match company_employee_detail_response(&s, &opc_id, &e) {
            Ok(detail) => ok!(serde_json::to_value(detail).unwrap()),
            Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e),
        },
        Ok(None) => err!(StatusCode::NOT_FOUND, "Employee not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_employee(
    headers: HeaderMap,
    State(s): State<AppState>,
    Json(employee): Json<AgentEmployee>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    create_company_employee(State(s), Path(opc_id), Json(employee)).await
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
    if let Err(err) = validate_plain_identifier(&employee.agent_id, "agent_id") {
        pool.close().await;
        return err;
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
    // One-department-one-head rule: every non-Custom department has a single head. If this
    // department already has a head and the new hire didn't declare a supervisor, attach
    // them under the existing head as a team member (subagent) rather than a rival head.
    if !matches!(employee.department, coevo_core::opc::Department::Custom)
        && employee
            .supervisor_agent_id
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        if let Ok(existing) = agent_employee_repo::AgentEmployeeRepo::list(&pool).await {
            if let Some(head) = existing.iter().find(|e| {
                e.department == employee.department && e.agent_id != employee.agent_id
            }) {
                employee.supervisor_agent_id = Some(head.agent_id.clone());
            }
        }
    }
    let result = agent_employee_repo::AgentEmployeeRepo::upsert(&pool, &employee)
        .await
        .map_err(|e| e.to_string())
        .and_then(|_| ensure_company_employee_files(&s, &opc_id, &employee).map(|_| ()));
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::to_value(company_employee_summary_response(&employee)).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn update_employee(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(employee): Json<AgentEmployee>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    update_company_employee(State(s), Path((opc_id, id)), Json(employee)).await
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
        Ok(()) => ok!(serde_json::to_value(company_employee_summary_response(&employee)).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn delete_employee(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    delete_company_employee(State(s), Path((opc_id, id))).await
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
        Ok(()) => {
            let employee_dir = s.company_workspace.company_employee_dir(&opc_id, &id);
            if employee_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&employee_dir) {
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                }
            }
            ok!(serde_json::json!({"ok": true, "deleted": id}))
        }
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
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePromptRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/agents/employees routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    update_company_employee_prompt(State(s), Path((opc_id, id)), Json(req)).await
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
    if let Err(e) = ensure_company_employee_files(&s, &opc_id, &employee) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    let current_version = match read_company_employee_current_prompt_version(&s, &opc_id, &id) {
        Ok(Some(version)) => version,
        Ok(None) => 0,
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    };
    let next_version = current_version + 1;
    if let Err(e) =
        write_company_employee_prompt_version(&s, &opc_id, &id, next_version, &req.system_prompt)
    {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    if let Err(e) = crate::handlers::prompts::publish_prompt_version_number_in_company(
        &s,
        &pool,
        &opc_id,
        &id,
        next_version,
        &req.system_prompt,
        "company-workspace",
        req.change_summary.as_deref(),
    )
    .await
    {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    if let Err(e) =
        agent_employee_repo::AgentEmployeeRepo::update_system_prompt(&pool, &id, &req.system_prompt)
            .await
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
            "path": format!("employees/{id}/prompt_versions/v{version}.md"),
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
    if let Err(e) = crate::handlers::prompts::publish_prompt_version_number_in_company(
        &s,
        &pool,
        &opc_id,
        &id,
        req.version,
        &content,
        "company-workspace-rollback",
        Some("rollback"),
    )
    .await
    {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    if let Err(e) =
        agent_employee_repo::AgentEmployeeRepo::update_system_prompt(&pool, &id, &content).await
    {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    pool.close().await;
    ok!(serde_json::json!({"version": req.version, "ok": true}))
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
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let ex = match executor_repo::ExecutorRepo::get(&s.pool, &id).await {
        Ok(Some(e)) => e,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "Executor not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let adapter = coevo_executors::build_executor_ref(&ex);
    match adapter.health_check().await {
        Ok(h) => ok!(serde_json::json!({
            "executor_id": id,
            "online": h.online,
            "latency_ms": h.latency_ms,
            "version": h.version,
        })),
        Err(e) => {
            // A health probe that cannot reach the runtime is reported as a
            // structured offline status (200) rather than an HTTP error, so the
            // desktop can render "offline" instead of a failed request.
            ok!(serde_json::json!({
                "executor_id": id,
                "online": false,
                "latency_ms": 0,
                "version": "unreachable",
                "error": e.to_string(),
            }))
        }
    }
}
pub async fn executor_dry_run(
    headers: HeaderMap,
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
    let wo = match resolve_legacy_executor_work_order(&s, &headers, &req.work_order_id).await {
        Ok((work_order, scoped_pool)) => {
            if let Some(pool) = scoped_pool {
                pool.close().await;
            }
            work_order
        }
        Err(err) => return err,
    };
    let risk = track_risk(&wo.track);
    if ex.risk_ceiling < risk {
        return err!(
            StatusCode::FORBIDDEN,
            format!("risk_ceiling {} < track risk {}", ex.risk_ceiling, risk)
        );
    }
    let adapter = coevo_executors::build_executor_ref(&ex);
    match adapter.dry_run(&wo).await {
        Ok(r) => ok!(serde_json::to_value(r).unwrap()),
        Err(e) => err!(executor_error_status(&e), e.to_string()),
    }
}

// === Work Orders ===
pub async fn create_work_order(
    headers: HeaderMap,
    State(s): State<AppState>,
    Json(req): Json<CreateWORequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.contract_hash.is_empty() || req.plan_hash.is_empty() {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "contract_hash and plan_hash required"
        );
    }
    let header_opc_id = match require_legacy_opc_id(&headers, "/opc/work-orders routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    if !req.opc_id.trim().is_empty() && req.opc_id != header_opc_id {
        return err!(
            StatusCode::CONFLICT,
            format!(
                "LEGACY_OPC_HEADER_BODY_MISMATCH: {LEGACY_OPC_ID_HEADER}={} does not match body opc_id={}",
                header_opc_id, req.opc_id
            )
        );
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let track_decision = classify_mission_track(&req.mission_intent);
    let scoped_pool = match scoped_legacy_work_order_pool(&s, &headers, Some(&header_opc_id)).await
    {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let scoped_pool_ref = scoped_pool.as_ref().unwrap_or(&s.pool);
    let employees = agent_employee_repo::AgentEmployeeRepo::list(scoped_pool_ref)
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
        opc_id: header_opc_id,
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
    match work_order_repo::WorkOrderRepo::create(scoped_pool_ref, &wo).await {
        Ok(()) => {
            let _ = AuditLogger::log_json(
                &s.pool,
                "work_order.governance.planned",
                Some(&wo.contract_hash),
                wo.selected_agents.first().map(String::as_str),
                None,
                &wo.opc_id,
                &serde_json::json!({
                    "work_order_id": wo.work_order_id,
                    "track": wo.track,
                    "status": "Planned",
                    "mission_intent": wo.mission_intent,
                    "risk_summary": wo.risk_summary,
                    "allowed_actions": wo.allowed_actions,
                    "restricted_actions": wo.restricted_actions,
                    "governance_proposal": proposal,
                    "governance_verdict": verdict,
                    "created_at_ms": now,
                }),
            )
            .await;
            ok!(
                serde_json::json!({"ok":true,"work_order_id":wo.work_order_id,"status":"Planned","track":wo.track,"risk_summary":wo.risk_summary,"allowed_actions":wo.allowed_actions,"restricted_actions":wo.restricted_actions,"governance_proposal":proposal,"governance_verdict":verdict,"created_at_ms":now})
            )
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
pub async fn list_work_orders(
    headers: HeaderMap,
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/work-orders routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = work_order_repo::WorkOrderRepo::list_by_opc(&pool, &opc_id).await;
    pool.close().await;
    result.map_or_else(
        |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        |items| ok!(serde_json::to_value(items).unwrap()),
    )
}

fn legacy_company_headers(opc_id: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(value) = axum::http::HeaderValue::from_str(opc_id) {
        headers.insert(LEGACY_OPC_ID_HEADER, value);
    }
    headers
}

fn work_order_status_from_worker_status(worker_status: &str) -> &str {
    match worker_status {
        // Worker runs distinguish timeout as a runtime outcome, but the
        // persisted work-order lifecycle intentionally stays on the narrower
        // product status set used across list/detail UIs.
        "TimedOut" => "Failed",
        other => other,
    }
}

pub async fn list_company_work_orders(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    list_work_orders(legacy_company_headers(&opc_id), State(s)).await
}

pub async fn create_company_work_order(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(mut req): Json<CreateWORequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    req.opc_id = opc_id.clone();
    create_work_order(legacy_company_headers(&opc_id), State(s), Json(req)).await
}

pub async fn execute_work_order(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // 1. Load work order
    let (wo, scoped_work_order_pool) = match load_scoped_work_order(&s, &headers, &id).await {
        Ok(result) => result,
        Err(err) => return err,
    };
    let scoped_work_order_pool_ref = scoped_work_order_pool.as_ref().unwrap_or(&s.pool);
    // 2. Validate hashes
    if wo.contract_hash.is_empty() || wo.plan_hash.is_empty() {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Missing contract_hash/plan_hash"
        );
    }
    let risk = track_risk(&wo.track);
    let mut approval_receipt = req.caller_identity_proof.clone().or(req.lease_id.clone());

    // 3. Validate agents
    let employee_pool = if !wo.opc_id.trim().is_empty() && wo.opc_id != "default-opc" {
        match company_pool(&s, &wo.opc_id).await {
            Ok(pool) => Some(pool),
            Err((status, body)) => return (status, body),
        }
    } else {
        None
    };
    let employee_pool_ref = employee_pool.as_ref().unwrap_or(&s.pool);
    let employees = agent_employee_repo::AgentEmployeeRepo::list(employee_pool_ref)
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
    if let Some(pool) = employee_pool {
        pool.close().await;
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
    // 5. Yellow & Red Tracks: require a real approval receipt anchored to a
    // structurally valid WorkOrder. Red is the higher-assurance gate (explicit
    // human approval); Yellow uses negative consent. Neither is hard-blocked.
    if wo.track == "yellow" || wo.track == "red" {
        let approval_mode = if wo.track == "red" {
            "EXPLICIT_APPROVAL"
        } else {
            "NEGATIVE_CONSENT"
        };
        let contract = match ContractRepo::find_by_hash(&s.pool, &wo.contract_hash).await {
            Ok(Some(c)) => c,
            Ok(None) => {
                return err!(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "CONTRACT_ANCHOR_REQUIRED_FOR_APPROVAL: compile and persist the contract before approval-gated execution"
                )
            }
            Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        let receipt = approval_receipt_for_track(&req, wo.track.as_str());
        if receipt.is_none() {
            let approval_id = match ApprovalRepo::create(
                &s.pool,
                &wo.opc_id,
                &wo.contract_hash,
                &format!("urn:coevo:work-order:{}:execute", id),
                approval_mode,
                &wo.user_id,
                300_000,
            )
            .await
            {
                Ok(id) => id,
                Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            };
            work_order_repo::WorkOrderRepo::update_status(
                scoped_work_order_pool_ref,
                &id,
                "WaitingApproval",
            )
            .await
            .ok();
            let _ = AuditRepo::insert(
                &s.pool,
                "work_order.approval.required",
                Some(&wo.contract_hash),
                wo.selected_agents.first().map(String::as_str),
                None,
                &wo.opc_id,
                &serde_json::json!({
                    "work_order_id": id,
                    "approval_id": approval_id,
                    "approval_mode": approval_mode,
                    "track": wo.track,
                    "message": if wo.track == "red" {
                        "Red Track requires explicit human approval before execution."
                    } else {
                        "Yellow Track requires an approved approval receipt before execution."
                    }
                })
                .to_string(),
            )
            .await;
            let message = if wo.track == "red" {
                "Red Track requires explicit human approval before execution."
            } else {
                "Yellow Track requires an approved approval receipt before execution."
            };
            return ok!(serde_json::json!({
                "ok":true,
                "status":"WaitingApproval",
                "approval_id":approval_id,
                "approval_mode":approval_mode,
                "contract_hash":contract.contract_hash,
                "action_urn":format!("urn:coevo:work-order:{}:execute", id),
                "message":message
            }));
        }
        let (receipt_id, provided_digest) = parse_approval_receipt_proof(receipt.unwrap());
        let approval = match ApprovalRepo::find_by_id(&s.pool, &wo.opc_id, receipt_id).await {
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
        let pending_action_digest =
            match pending_action_digest_for_work_order(scoped_work_order_pool_ref, &id).await {
                Ok(value) => value,
                Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            };
        if let Some(expected_digest) = pending_action_digest.as_deref() {
            let Some(candidate_digest) = provided_digest else {
                return err!(StatusCode::FORBIDDEN, "APPROVAL_RECEIPT_DIGEST_REQUIRED");
            };
            if candidate_digest != expected_digest {
                return err!(StatusCode::FORBIDDEN, "APPROVAL_RECEIPT_DIGEST_MISMATCH");
            }
        }
        approval_receipt = Some(compose_approval_receipt(
            receipt_id,
            pending_action_digest.as_deref(),
        ));
        let _ = AuditLogger::log_json(
            &s.pool,
            "work_order.approval.receipt.accepted",
            Some(&wo.contract_hash),
            wo.selected_agents.first().map(String::as_str),
            None,
            &wo.opc_id,
            &serde_json::json!({
                "work_order_id": id,
                "approval_id": receipt_id,
                "approval_mode": approval.approval_mode,
                "track": wo.track,
                "action_urn": action_urn,
            }),
        )
        .await;
    }
    // 6. Green/Yellow with approval: use WorkerHarness
    let opc_pool = if !wo.opc_id.trim().is_empty() && wo.opc_id != "default-opc" {
        match company_pool(&s, &wo.opc_id).await {
            Ok(pool) => Some(pool),
            Err((status, body)) => return (status, body),
        }
    } else {
        None
    };
    let opc_pool_ref = opc_pool.as_ref().unwrap_or(&s.pool);
    let harness_result = match WorkerHarness::run_work_order_with_pools(
        &s.pool,
        opc_pool_ref,
        &id,
        WorkerHarnessOptions {
            approval_receipt,
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
            let _ = work_order_repo::WorkOrderRepo::update_status(
                scoped_work_order_pool_ref,
                &id,
                "Failed",
            )
            .await;
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
    let work_order_status = work_order_status_from_worker_status(&harness_result.status);
    if let Err(e) = work_order_repo::WorkOrderRepo::update_status(
        scoped_work_order_pool_ref,
        &id,
        work_order_status,
    )
    .await
    {
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update status: {}", e)
        );
    }
    ok!(serde_json::json!({
        "ok":true,"status":work_order_status,"worker_status":harness_result.status,"termination_reason":harness_result.termination_reason,"summary":harness_result.summary,
        "worker_session_ids":worker_session_ids,"synthesized_summary":synthesized,
        "worker_runs":harness_result.worker_runs,"worker_steps":harness_result.worker_steps,
        "worker_events":harness_result.worker_events,"skill_usage":harness_result.skill_usage,
        "tool_calls":harness_result.tool_calls,"memory_ids":harness_result.memory_ids,
        "reflection_id":harness_result.reflection_id,"proposal_id":harness_result.proposal_id
    }))
}

pub async fn execute_company_work_order(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
    Json(req): Json<ExecuteRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    execute_work_order(
        legacy_company_headers(&opc_id),
        State(s),
        Path(id),
        Json(req),
    )
    .await
}

pub async fn decide_work_order_approval(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ApprovalDecisionRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (work_order, scoped_work_order_pool) = match load_scoped_work_order(&s, &headers, &id).await
    {
        Ok(result) => result,
        Err(err) => return err,
    };
    let scoped_work_order_pool_ref = scoped_work_order_pool.as_ref().unwrap_or(&s.pool);
    let approval =
        match ApprovalRepo::find_by_id(&s.pool, &work_order.opc_id, &req.approval_id).await {
            Ok(Some(approval)) => approval,
            Ok(None) => return err!(StatusCode::NOT_FOUND, "Approval request not found"),
            Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
    let expected_action_urn = format!("urn:coevo:work-order:{}:execute", id);
    if approval.action_urn != expected_action_urn {
        return err!(StatusCode::FORBIDDEN, "APPROVAL_ACTION_MISMATCH");
    }

    let actor = approval_actor(&headers, &work_order.user_id);
    match req.decision.as_str() {
        "approve" | "approved" => {
            if let Err(e) =
                ApprovalRepo::approve(&s.pool, &work_order.opc_id, &req.approval_id, &actor).await
            {
                if matches!(e, sqlx::Error::RowNotFound) {
                    return err!(
                        StatusCode::CONFLICT,
                        "Approval request is no longer pending or has expired"
                    );
                }
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
            let _ = AuditRepo::insert(
                &s.pool,
                "work_order.approval.approved",
                Some(&work_order.contract_hash),
                Some(actor.as_str()),
                None,
                &work_order.opc_id,
                &serde_json::json!({
                    "work_order_id": id,
                    "approval_id": req.approval_id,
                    "comment": req.comment,
                })
                .to_string(),
            )
            .await;
            let approval_receipt =
                match pending_action_digest_for_work_order(scoped_work_order_pool_ref, &id).await {
                    Ok(digest) => compose_approval_receipt(&req.approval_id, digest.as_deref()),
                    Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
                };
            let (status, Json(mut body)) = execute_work_order(
                headers.clone(),
                State(s.clone()),
                Path(id.clone()),
                Json(ExecuteRequest {
                    caller_identity_proof: Some(approval_receipt.clone()),
                    monitoring_signature: None,
                    diagnostic_signature: None,
                    lease_id: None,
                }),
            )
            .await;
            if let Some(obj) = body.as_object_mut() {
                obj.insert(
                    "approval_receipt".to_string(),
                    serde_json::json!(approval_receipt),
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
            if let Err(e) =
                ApprovalRepo::deny(&s.pool, &work_order.opc_id, &req.approval_id, &actor).await
            {
                if matches!(e, sqlx::Error::RowNotFound) {
                    return err!(
                        StatusCode::CONFLICT,
                        "Approval request is no longer pending or has expired"
                    );
                }
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
            let _ = AuditRepo::insert(
                &s.pool,
                "work_order.approval.rejected",
                Some(&work_order.contract_hash),
                Some(actor.as_str()),
                None,
                &work_order.opc_id,
                &serde_json::json!({
                    "work_order_id": id,
                    "approval_id": req.approval_id,
                    "comment": req.comment,
                })
                .to_string(),
            )
            .await;
            let _ = work_order_repo::WorkOrderRepo::update_status(
                scoped_work_order_pool_ref,
                &id,
                "Failed",
            )
            .await;
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

pub async fn decide_company_work_order_approval(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
    Json(req): Json<ApprovalDecisionRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    decide_work_order_approval(
        legacy_company_headers(&opc_id),
        State(s),
        Path(id),
        Json(req),
    )
    .await
}

pub async fn cancel_work_order(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (_, scoped_work_order_pool) = match load_scoped_work_order(&s, &headers, &id).await {
        Ok(result) => result,
        Err(err) => return err,
    };
    let scoped_work_order_pool_ref = scoped_work_order_pool.as_ref().unwrap_or(&s.pool);
    work_order_repo::WorkOrderRepo::update_status(scoped_work_order_pool_ref, &id, "Cancelled")
        .await
        .ok();
    ok!(serde_json::json!({"ok":true}))
}

pub async fn cancel_company_work_order(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    cancel_work_order(legacy_company_headers(&opc_id), State(s), Path(id)).await
}

pub async fn work_order_feedback(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<WorkOrderFeedback>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (wo, scoped_work_order_pool) = match load_scoped_work_order(&s, &headers, &id).await {
        Ok(result) => result,
        Err(err) => return err,
    };
    let company_proposal_pool = if scoped_work_order_pool.is_some() {
        None
    } else if !wo.opc_id.trim().is_empty() && wo.opc_id != "default-opc" {
        match company_pool(&s, &wo.opc_id).await {
            Ok(pool) => Some(pool),
            Err((status, body)) => return (status, body),
        }
    } else {
        None
    };
    let proposal_pool_ref = scoped_work_order_pool
        .as_ref()
        .or(company_proposal_pool.as_ref())
        .unwrap_or(&s.pool);
    let analysis = FailureAnalyzer::analyze(&req.feedback, false, false, None);
    let proposal = match SkillGenerator::generate_from_failure(
        &s.pool,
        &analysis,
        "skill-mission-draft",
        req.agent_id.as_deref().unwrap_or("system"),
    )
    .await
    {
        Ok(proposal) => proposal,
        Err(e) => return err!(StatusCode::BAD_GATEWAY, e),
    };
    if let Err(e) =
        skill_evolution_repo::SkillEvolutionRepo::create_proposal(proposal_pool_ref, &proposal)
            .await
    {
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create proposal: {}", e)
        );
    }
    if let Err(e) =
        work_order_repo::WorkOrderRepo::update_status(proposal_pool_ref, &id, "Failed").await
    {
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update status: {}", e)
        );
    }
    ok!(serde_json::json!({"ok":true,"proposal_id":proposal.proposal_id}))
}

pub async fn company_work_order_feedback(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
    Json(req): Json<WorkOrderFeedback>,
) -> (StatusCode, Json<serde_json::Value>) {
    work_order_feedback(
        legacy_company_headers(&opc_id),
        State(s),
        Path(id),
        Json(req),
    )
    .await
}

pub async fn company_work_order_timeline(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    crate::handlers::timeline::timeline(legacy_company_headers(&opc_id), State(s), Path(id)).await
}

pub async fn company_work_order_audit_export(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    crate::handlers::timeline::work_order_audit_export(
        legacy_company_headers(&opc_id),
        State(s),
        Path(id),
    )
    .await
}

// === Skills ===
pub async fn list_skills(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SkillsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    let response = match skill_repo::SkillRepo::list(&pool, q.agent_id.as_deref()).await {
        Ok(items) => {
            for skill in items
                .iter()
                .filter(|skill| skill.status == SkillStatus::Active)
            {
                let is_employee_scoped = is_employee_skill(&skill)
                    && q.agent_id
                        .as_deref()
                        .is_some_and(|agent_id| skill.owner_agent_id == agent_id);
                let agent_id = if is_employee_scoped {
                    q.agent_id.as_deref()
                } else {
                    None
                };
                if let Err(e) = ensure_company_skill_file(&s, &opc_id, &skill, agent_id) {
                    pool.close().await;
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
                }
            }
            ok!(serde_json::to_value(items).unwrap())
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    pool.close().await;
    response
}

pub async fn list_company_skills(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = skill_repo::SkillRepo::list(&pool, None).await;
    pool.close().await;
    match result {
        Ok(skills) => {
            let mut items = Vec::new();
            for skill in skills
                .into_iter()
                .filter(|skill| skill.status == SkillStatus::Active && !is_employee_skill(skill))
            {
                if let Err(e) = ensure_company_skill_file(&s, &opc_id, &skill, None) {
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
                }
                items.push(company_skill_response(&s, &opc_id, &skill, None));
            }
            ok!(serde_json::Value::Array(items))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_company_employee_skills(
    State(s): State<AppState>,
    Path((opc_id, agent_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = skill_repo::SkillRepo::list(&pool, Some(&agent_id)).await;
    pool.close().await;
    match result {
        Ok(skills) => {
            let mut items = Vec::new();
            for skill in skills
                .into_iter()
                .filter(|skill| skill.status == SkillStatus::Active && is_employee_skill(skill))
            {
                if let Err(e) = ensure_company_skill_file(&s, &opc_id, &skill, Some(&agent_id)) {
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
                }
                items.push(company_skill_response(&s, &opc_id, &skill, Some(&agent_id)));
            }
            ok!(serde_json::Value::Array(items))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn install_company_skill(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<CompanySkillInstallRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let requested_skill_id = req
        .skill_id
        .as_deref()
        .or(req.template.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let skill_id = match requested_skill_id {
        Some(skill_id) => skill_id.to_string(),
        None => return err!(StatusCode::BAD_REQUEST, "skill_id or template is required"),
    };

    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };

    if let Err(e) = skill_repo::SkillRepo::seed_default(&pool).await {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    let skill = match skill_repo::SkillRepo::get(&pool, &skill_id, None).await {
        Ok(Some(skill)) if skill.status == SkillStatus::Active && !is_employee_skill(&skill) => {
            skill
        }
        Ok(Some(_)) => {
            pool.close().await;
            return err!(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Only active company-shared skills can be installed via this route"
            );
        }
        Ok(None) => {
            pool.close().await;
            return err!(
                StatusCode::NOT_FOUND,
                format!("Unknown skill template: {skill_id}")
            );
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };

    if let Err(e) = ensure_company_skill_file(&s, &opc_id, &skill, None) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    let response = company_skill_response(&s, &opc_id, &skill, None);
    pool.close().await;
    ok!(response)
}

pub async fn delete_company_skill(
    State(s): State<AppState>,
    Path((opc_id, skill_name)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let skill = match skill_repo::SkillRepo::get(&pool, &skill_name, None).await {
        Ok(Some(skill)) => skill,
        Ok(None) => {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Skill not found");
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    if is_employee_skill(&skill) {
        pool.close().await;
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Employee-evolved skills must be managed under the employee skill scope"
        );
    }

    let mut revoked = skill.clone();
    revoked.status = SkillStatus::Revoked;
    revoked.updated_at_ms = chrono::Utc::now().timestamp_millis() as u64;
    if let Err(e) = skill_repo::SkillRepo::upsert(&pool, &revoked).await {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let markdown_path = company_skill_markdown_path(&s, &opc_id, &skill_name);
    if markdown_path.exists() {
        std::fs::remove_file(&markdown_path).ok();
    }
    pool.close().await;
    ok!(serde_json::json!({"ok": true}))
}

pub async fn seed_skills(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    if let Err(e) = skill_repo::SkillRepo::seed_default(&pool).await {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let skills = match skill_repo::SkillRepo::list(&pool, None).await {
        Ok(skills) => skills,
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    for skill in skills
        .into_iter()
        .filter(|skill| skill.status == SkillStatus::Active && !is_employee_skill(skill))
    {
        if let Err(e) = ensure_company_skill_file(&s, &opc_id, &skill, None) {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    }
    pool.close().await;
    ok!(serde_json::json!({"ok":true}))
}

pub async fn seed_company_skills_handler(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = match skill_repo::SkillRepo::seed_default(&pool).await {
        Ok(()) => skill_repo::SkillRepo::list(&pool, None).await,
        Err(e) => Err(e),
    };
    let response = match result {
        Ok(skills) => {
            let active = skills
                .into_iter()
                .filter(|skill| skill.status == SkillStatus::Active)
                .collect::<Vec<_>>();
            for skill in &active {
                if let Err(e) = ensure_company_skill_file(&s, &opc_id, skill, None) {
                    pool.close().await;
                    return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
                }
            }
            ok!(serde_json::json!({"ok": true, "total": active.len()}))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    pool.close().await;
    response
}
pub async fn activate_skill(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((id, ver)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    let response = match skill_repo::SkillRepo::activate(&pool, &id, &ver).await {
        Ok(()) => ok!(serde_json::json!({"ok":true})),
        Err(e) => err!(StatusCode::FORBIDDEN, e.to_string()),
    };
    pool.close().await;
    response
}
pub async fn rollback_skill(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((id, ver)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    if let Err(e) = skill_repo::SkillRepo::rollback(&pool, &id, &ver).await {
        pool.close().await;
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
    if let Err(e) = skill_evolution_repo::SkillEvolutionRepo::record_version(&pool, &vr).await {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    pool.close().await;
    ok!(serde_json::json!({"ok":true}))
}

// === Skill Evolution ===
pub async fn list_proposals(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills/evolution routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let proposal_pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    let result = skill_evolution_repo::SkillEvolutionRepo::list(&proposal_pool, None)
        .await
        .map_or_else(
            |e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            |items| ok!(serde_json::to_value(items).unwrap()),
        );
    proposal_pool.close().await;
    result
}
pub async fn run_evolution(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills/evolution routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let proposal_pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    let failed_work_orders =
        match work_order_repo::WorkOrderRepo::list_by_opc(&proposal_pool, &opc_id).await {
            Ok(items) => items
                .into_iter()
                .filter(|item| item.status == WorkOrderStatus::Failed)
                .collect::<Vec<_>>(),
            Err(e) => {
                proposal_pool.close().await;
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
        };
    let latest_failed_work_order = match failed_work_orders
        .into_iter()
        .max_by_key(|item| item.updated_at_ms.max(item.created_at_ms))
    {
        Some(item) => item,
        None => {
            proposal_pool.close().await;
            return err!(
                StatusCode::UNPROCESSABLE_ENTITY,
                "run evolution requires at least one failed work order in this company as a real diagnosis source"
            );
        }
    };
    let failure_signal = if !latest_failed_work_order.risk_summary.trim().is_empty() {
        latest_failed_work_order.risk_summary.trim().to_string()
    } else {
        latest_failed_work_order.mission_intent.trim().to_string()
    };
    if failure_signal.is_empty() {
        proposal_pool.close().await;
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "run evolution requires a non-empty failure signal for failed work order {}",
                latest_failed_work_order.work_order_id
            )
        );
    }
    let analysis = FailureAnalyzer::analyze(&failure_signal, false, false, None);
    let p = match SkillGenerator::generate_from_failure(
        &s.pool,
        &analysis,
        "skill-mission-draft",
        "scheduler",
    )
    .await
    {
        Ok(proposal) => proposal,
        Err(e) => {
            proposal_pool.close().await;
            return err!(StatusCode::BAD_GATEWAY, e);
        }
    };
    if let Err(e) =
        skill_evolution_repo::SkillEvolutionRepo::create_proposal(&proposal_pool, &p).await
    {
        proposal_pool.close().await;
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create proposal: {}", e)
        );
    }
    proposal_pool.close().await;
    ok!(serde_json::to_value(&p).unwrap())
}
pub async fn verify_proposal(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills/evolution routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let proposal_pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&proposal_pool, None)
        .await
        .unwrap_or_default();
    let proposal = match proposals.into_iter().find(|p| p.proposal_id == id) {
        Some(p) => p,
        None => {
            proposal_pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Proposal not found");
        }
    };
    let skill = skill_repo::SkillRepo::get(&proposal_pool, &proposal.target_skill_id, None)
        .await
        .ok()
        .flatten();
    let eval = SkillVerifier::verify(&proposal, skill.as_ref());
    skill_evolution_repo::SkillEvolutionRepo::append_eval(&proposal_pool, &eval)
        .await
        .ok();
    let new_status = if eval.passed && !proposal.risk_assessment.contains("HIGH") {
        "Approved"
    } else {
        "NeedsHumanReview"
    };
    skill_evolution_repo::SkillEvolutionRepo::update_status(&proposal_pool, &id, new_status)
        .await
        .ok();
    proposal_pool.close().await;
    ok!(serde_json::to_value(&eval).unwrap())
}
pub async fn approve_proposal(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills/evolution routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let proposal_pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&proposal_pool, None)
        .await
        .unwrap_or_default();
    let proposal = match proposals.into_iter().find(|p| p.proposal_id == id) {
        Some(p) => p,
        None => {
            proposal_pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Proposal not found");
        }
    };
    // HIGH risk requires human
    if proposal.risk_assessment.to_uppercase().contains("HIGH")
        || proposal.risk_assessment.to_uppercase().contains("RED")
    {
        proposal_pool.close().await;
        return err!(
            StatusCode::FORBIDDEN,
            "HIGH/RED risk skill proposal requires explicit human approval marker"
        );
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let mut applied_skill: Option<AgentSkillPackage> = None;
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
            skill_repo::SkillRepo::upsert(&proposal_pool, &sk)
                .await
                .ok();
            applied_skill = Some(sk);
        }
        EvolutionProposalType::PatchSkill => {
            let existing =
                skill_repo::SkillRepo::get(&proposal_pool, &proposal.target_skill_id, None)
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
            skill_repo::SkillRepo::upsert(&proposal_pool, &patched)
                .await
                .ok();
            applied_skill = Some(patched);
        }
        EvolutionProposalType::DeprecateSkill => {
            if let Ok(Some(mut sk)) =
                skill_repo::SkillRepo::get(&proposal_pool, &proposal.target_skill_id, None).await
            {
                sk.status = SkillStatus::Deprecated;
                sk.updated_at_ms = now;
                skill_repo::SkillRepo::upsert(&proposal_pool, &sk)
                    .await
                    .ok();
                applied_skill = Some(sk);
            }
        }
        EvolutionProposalType::SplitSkill | EvolutionProposalType::MergeSkills => {
            proposal_pool.close().await;
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
    if let Err(e) =
        skill_evolution_repo::SkillEvolutionRepo::record_version(&proposal_pool, &vr).await
    {
        proposal_pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    // Converge the evolution upgrade into the prompt-version history so the
    // approved prompt becomes the published version the runtime + UI both read.
    if let Err(e) = crate::handlers::prompts::record_and_publish_version(
        &s,
        &proposal_pool,
        &opc_id,
        &proposal.target_skill_id,
        &proposal.proposed_changes,
        "skill-evolution",
        Some(&proposal.diagnosis),
    )
    .await
    {
        proposal_pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    if let Some(skill) = applied_skill.as_ref() {
        if let Err(e) =
            ensure_company_skill_file(&s, &opc_id, skill, Some(&proposal.created_by_agent))
        {
            proposal_pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    }
    if let Err(e) =
        skill_evolution_repo::SkillEvolutionRepo::update_status(&proposal_pool, &id, "Applied")
            .await
    {
        proposal_pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    proposal_pool.close().await;
    ok!(serde_json::json!({"ok":true,"proposal_id":id}))
}
pub async fn reject_proposal(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers, "/opc/skills/evolution routes") {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let proposal_pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err((status, body)) => return (status, body),
    };
    skill_evolution_repo::SkillEvolutionRepo::update_status(&proposal_pool, &id, "Rejected")
        .await
        .ok();
    proposal_pool.close().await;
    ok!(serde_json::json!({"ok":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::timeline::work_order_audit_export;
    use crate::state::AppState;
    use axum::{
        body::Body,
        http::{HeaderValue, Request, StatusCode},
        routing::get,
        Router,
    };
    use coevo_core::contract::*;
    use coevo_core::reputation::ReputationVector;
    use coevo_core::skills::{
        EvolutionProposalStatus, EvolutionProposalType, EvolutionSourceType, SkillEvolutionProposal,
    };
    use coevo_store::repos::{approval_repo::ApprovalRepo, contract_repo::ContractRepo};
    use coevo_store::{
        migrate::run_migrations, pool::create_test_pool,
        repos::reputation_repo::ReputationHistoryRepo, repos::worker_run_repo::WorkerRunRepo,
        repos_opc::work_order_repo::WorkOrderRepo,
    };
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

    async fn remove_file_with_retry(path: &std::path::Path) {
        for _ in 0..10 {
            match std::fs::remove_file(path) {
                Ok(()) => return,
                Err(err) if cfg!(windows) => {
                    if err.kind() == std::io::ErrorKind::NotFound {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
                Err(err) => panic!("failed to remove {}: {}", path.display(), err),
            }
        }
        std::fs::remove_file(path).unwrap_or_else(|err| {
            panic!("failed to remove {} after retry: {}", path.display(), err)
        });
    }

    async fn create_yellow_work_order(
        state: AppState,
        opc_id: &str,
        work_order_id: &str,
        contract_hash: &str,
    ) {
        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: contract_hash.to_string(),
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.to_string(),
            mission_intent: "Draft an internal update".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, Json(created)) =
            create_work_order(legacy_company_headers(opc_id), State(state), Json(create)).await;
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
        let root =
            std::env::temp_dir().join(format!("coevo-company-handler-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool, root.clone());
        (state, root)
    }

    async fn seeded_legacy_company_state() -> (AppState, std::path::PathBuf, String) {
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Legacy Scoped Co",
                Some("legacy scoped tests"),
                "default-founder",
            )
            .await
            .unwrap();
        let pool = company_pool(&state, &company.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        pool.close().await;
        (state, root, company.opc_id)
    }

    #[tokio::test]
    async fn company_routes_create_fetch_and_delete_real_companies() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let app = Router::new()
            .route("/companies", get(list_companies).post(create_company))
            .route(
                "/companies/{opc_id}",
                get(get_company).delete(delete_company),
            )
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
        // create_company now auto-seeds the default org (secretary + department heads).
        let seeded_count = coevo_store::seed::seed_employees().len() as u64;
        assert!(opc_id.starts_with("opc-"));
        assert_eq!(created["name"], "Alpha Labs");
        assert_eq!(created["mission"], "Build alpha");
        assert_eq!(created["employee_count"], seeded_count);
        let company_skill_path = state
            .company_workspace
            .company_skill_markdown_path(&opc_id, "skill-mission-draft");
        assert!(company_skill_path.exists());
        let skill_body = std::fs::read_to_string(&company_skill_path).unwrap();
        assert!(skill_body.contains("skill-mission-draft"));

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
        assert_eq!(listed[0]["employee_count"], seeded_count);

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
        assert_eq!(detail["employee_count"], seeded_count);

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
    async fn company_detail_returns_real_mission_and_employee_count() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Gamma Ops",
                Some("Run reliable operations"),
                "default-founder",
            )
            .await
            .unwrap();
        let pool = company_pool(&state, &company.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        let seeded_count = agent_employee_repo::AgentEmployeeRepo::list(&pool)
            .await
            .unwrap()
            .len();
        pool.close().await;

        let (status, Json(detail)) = get_company(State(state), Path(company.opc_id)).await;
        assert_eq!(status, StatusCode::OK, "{detail:?}");
        assert_eq!(detail["mission"], "Run reliable operations");
        assert_eq!(detail["employee_count"], seeded_count);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_detail_returns_profile_backed_goals_departments_and_recursive_report_count() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Detail Shape Co",
                Some("Inspect detail shape"),
                "default-founder",
            )
            .await
            .unwrap();
        let opc_id = company.opc_id.clone();
        let pool = company_pool(&state, &opc_id).await.unwrap();
        opc_profile_repo::OPCProfileRepo::upsert(
            &pool,
            &OPCProfile {
                opc_id: opc_id.clone(),
                founder_user_id: "default-founder".to_string(),
                name: "Detail Shape Co".to_string(),
                mission: "Inspect detail shape".to_string(),
                current_strategy: "Keep details real".to_string(),
                operating_principles: vec!["No placeholders".to_string()],
                active_projects: vec!["launch-okr".to_string(), "ops-hardening".to_string()],
                asset_indexes: vec![],
                policy_profile: "policy/default".to_string(),
                memory_policy: MemoryPolicy {
                    fact_ttl_default_seconds: 3600,
                    require_provenance_for_fact: true,
                    auto_stale_days: 30,
                },
                default_departments: vec!["FounderOffice".to_string(), "Operations".to_string()],
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .await
        .unwrap();
        pool.close().await;

        let reports_dir = state.company_workspace.company_dir(&opc_id).join("reports");
        std::fs::create_dir_all(reports_dir.join("2026").join("06")).unwrap();
        std::fs::write(reports_dir.join("daily.md"), "# Daily").unwrap();
        std::fs::write(
            reports_dir.join("2026").join("06").join("monthly.md"),
            "# Monthly",
        )
        .unwrap();

        let (status, Json(detail)) = get_company(State(state), Path(opc_id)).await;
        assert_eq!(status, StatusCode::OK, "{detail:?}");
        assert_eq!(
            detail["goals"],
            serde_json::json!(["launch-okr", "ops-hardening"])
        );
        assert_eq!(
            detail["departments"],
            serde_json::json!(["FounderOffice", "Operations"])
        );
        assert_eq!(detail["report_count"], 2);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_list_returns_real_mission_and_employee_count() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Delta Ops",
                Some("Ship reliable releases"),
                "default-founder",
            )
            .await
            .unwrap();
        let pool = company_pool(&state, &company.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&pool)
            .await
            .unwrap();
        let seeded_count = agent_employee_repo::AgentEmployeeRepo::list(&pool)
            .await
            .unwrap()
            .len();
        pool.close().await;

        let (status, Json(list)) = list_companies(State(state)).await;
        assert_eq!(status, StatusCode::OK, "{list:?}");
        let companies = list.as_array().unwrap();
        let listed = companies
            .iter()
            .find(|item| item["opc_id"] == company.opc_id)
            .expect("company should be listed");
        assert_eq!(listed["mission"], "Ship reliable releases");
        assert_eq!(listed["employee_count"], seeded_count);

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
            seed_company_employees_handler(State(state.clone()), Path(company.opc_id.clone()))
                .await;
        assert_eq!(seed_status, StatusCode::OK);

        let (get_status, Json(before)) = get_company_employee(
            State(state.clone()),
            Path((company.opc_id.clone(), "agent-pm-01".to_string())),
        )
        .await;
        assert_eq!(get_status, StatusCode::OK);
        let original_passport_id = before["passport"]["issued_passport"]["passport_id"]
            .as_str()
            .unwrap()
            .to_string();
        let pool = company_pool(&state, &company.opc_id).await.unwrap();
        let mut employee = agent_employee_repo::AgentEmployeeRepo::get(&pool, "agent-pm-01")
            .await
            .unwrap()
            .unwrap();
        pool.close().await;
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
        assert_eq!(
            after["passport"]["issued_passport"]["passport_id"],
            original_passport_id
        );
        let employee_dir = root
            .join(&company.opc_id)
            .join("employees")
            .join("agent-pm-01");
        let passport_file: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(employee_dir.join("passport.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            passport_file["issued_passport"]["passport_id"],
            original_passport_id
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_employee_files_materialize_persona_and_file_backed_detail() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Persona Co",
                Some("File backed employees"),
                "default-founder",
            )
            .await
            .unwrap();

        let (seed_status, _) =
            seed_company_employees_handler(State(state.clone()), Path(company.opc_id.clone()))
                .await;
        assert_eq!(seed_status, StatusCode::OK);

        let employee_dir = root
            .join(&company.opc_id)
            .join("employees")
            .join("agent-pm-01");
        assert!(employee_dir.join("passport.json").exists());
        assert!(employee_dir.join("prompt.md").exists());
        assert!(employee_dir.join("prompt_versions").exists());
        assert!(employee_dir.join("identity.md").exists());
        assert!(employee_dir.join("soul.md").exists());
        assert!(employee_dir.join("agents.md").exists());

        std::fs::write(
            employee_dir.join("identity.md"),
            "# Identity\n\nCustom identity",
        )
        .unwrap();
        std::fs::write(employee_dir.join("soul.md"), "# Soul\n\nCustom soul").unwrap();
        std::fs::write(employee_dir.join("agents.md"), "# Rules\n\nCustom rules").unwrap();
        std::fs::write(employee_dir.join("prompt.md"), "Custom prompt body").unwrap();

        let (status, Json(detail)) = get_company_employee(
            State(state),
            Path((company.opc_id.clone(), "agent-pm-01".to_string())),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{detail:?}");
        assert_eq!(detail["identity_md"], "# Identity\n\nCustom identity");
        assert_eq!(detail["soul_md"], "# Soul\n\nCustom soul");
        assert_eq!(detail["agents_md"], "# Rules\n\nCustom rules");
        assert_eq!(detail["prompt_md"], "Custom prompt body");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_skill_routes_are_file_backed_and_scope_split() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company("Skill Co", Some("Skill layering"), "default-founder")
            .await
            .unwrap();

        let (seed_employee_status, _) =
            seed_company_employees_handler(State(state.clone()), Path(company.opc_id.clone()))
                .await;
        assert_eq!(seed_employee_status, StatusCode::OK);

        let seed_status =
            seed_company_skills_handler(State(state.clone()), Path(company.opc_id.clone()))
                .await
                .0;
        assert_eq!(seed_status, StatusCode::OK);

        let (company_status, Json(company_skills)) =
            list_company_skills(State(state.clone()), Path(company.opc_id.clone())).await;
        assert_eq!(company_status, StatusCode::OK);
        assert!(!company_skills.as_array().unwrap_or(&Vec::new()).is_empty());

        let (employee_status, Json(employee_skills)) = list_company_employee_skills(
            State(state.clone()),
            Path((company.opc_id.clone(), "agent-founder-01".to_string())),
        )
        .await;
        assert_eq!(employee_status, StatusCode::OK);
        assert!(employee_skills.as_array().unwrap_or(&Vec::new()).is_empty());

        let company_skill_path = state
            .company_workspace
            .company_skill_markdown_path(&company.opc_id, "skill-mission-draft");
        assert!(company_skill_path.exists());
        let body = std::fs::read_to_string(&company_skill_path).unwrap();
        assert!(body.contains("skill-mission-draft"));

        let employee_skill_dir = state
            .company_workspace
            .company_employee_skills_dir(&company.opc_id, "agent-founder-01");
        assert!(employee_skill_dir.exists());

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_skill_activate_and_rollback_use_company_scope_header() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Legacy Skill Scope Co",
                Some("Skill scope"),
                "default-founder",
            )
            .await
            .unwrap();

        let pool = company_pool(&state, &company.opc_id).await.unwrap();
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let mut version = skill_repo::SkillRepo::get(&pool, "skill-mission-draft", None)
            .await
            .unwrap()
            .unwrap();
        version.version = "1.1.0".to_string();
        version.status = SkillStatus::Draft;
        version.updated_at_ms = version.updated_at_ms.saturating_add(1);
        skill_repo::SkillRepo::upsert(&pool, &version)
            .await
            .unwrap();
        pool.close().await;

        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_OPC_ID_HEADER,
            company.opc_id.parse().expect("valid opc header"),
        );

        let (activate_status, Json(activate_body)) = activate_skill(
            State(state.clone()),
            headers.clone(),
            Path(("skill-mission-draft".to_string(), "1.1.0".to_string())),
        )
        .await;
        assert_eq!(activate_status, StatusCode::OK, "{activate_body:?}");

        let scoped_pool = company_pool(&state, &company.opc_id).await.unwrap();
        let activated =
            skill_repo::SkillRepo::get(&scoped_pool, "skill-mission-draft", Some("1.1.0"))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(activated.status, SkillStatus::Active);
        scoped_pool.close().await;

        let (rollback_status, Json(rollback_body)) = rollback_skill(
            State(state.clone()),
            headers,
            Path(("skill-mission-draft".to_string(), "1.0.0".to_string())),
        )
        .await;
        assert_eq!(rollback_status, StatusCode::OK, "{rollback_body:?}");

        let scoped_pool = company_pool(&state, &company.opc_id).await.unwrap();
        let rolled_back =
            skill_repo::SkillRepo::get(&scoped_pool, "skill-mission-draft", Some("1.0.0"))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(rolled_back.status, SkillStatus::Active);
        scoped_pool.close().await;

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_employee_create_and_update_keep_summary_shape() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Summary Co",
                Some("Employee summary shape"),
                "default-founder",
            )
            .await
            .unwrap();
        let (seed_status, _) =
            seed_company_employees_handler(State(state.clone()), Path(company.opc_id.clone()))
                .await;
        assert_eq!(seed_status, StatusCode::OK);

        let pool = company_pool(&state, &company.opc_id).await.unwrap();
        let mut employee = agent_employee_repo::AgentEmployeeRepo::get(&pool, "agent-pm-01")
            .await
            .unwrap()
            .unwrap();
        pool.close().await;
        employee.agent_id = "agent-summary-01".to_string();
        employee.display_name = "Summary Agent".to_string();

        let (create_status, Json(created)) = create_company_employee(
            State(state.clone()),
            Path(company.opc_id.clone()),
            Json(employee.clone()),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);
        assert_eq!(created["agent_id"], "agent-summary-01");
        assert!(created.get("identity_md").is_none());
        let employee_dir = root
            .join(&company.opc_id)
            .join("employees")
            .join("agent-summary-01");
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            employee.system_prompt
        );

        employee.display_name = "Summary Agent Updated".to_string();
        employee.system_prompt = "Updated summary prompt".to_string();
        let (update_status, Json(updated)) = update_company_employee(
            State(state),
            Path((company.opc_id.clone(), "agent-summary-01".to_string())),
            Json(employee),
        )
        .await;
        assert_eq!(update_status, StatusCode::OK);
        assert_eq!(updated["display_name"], "Summary Agent Updated");
        assert!(updated.get("identity_md").is_none());
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            "Updated summary prompt"
        );

        let pool = create_pool(&root.join(&company.opc_id).join("data.db").to_string_lossy())
            .await
            .unwrap();
        let db_prompt: String =
            sqlx::query_scalar("SELECT system_prompt FROM agent_employees WHERE agent_id = ?")
                .bind("agent-summary-01")
                .fetch_one(&pool)
                .await
                .unwrap();
        pool.close().await;
        assert_eq!(db_prompt, "Updated summary prompt");

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
    fn classify_mission_track_handles_chinese_triggers() {
        // Chinese missions MUST classify to the same elevated track as their
        // English equivalents (aligned with the MCL CONCEPTS table).
        let red_cases = [
            "把后端部署到生产环境",   // deploy to production -> Red (production)
            "删除生产数据库",         // delete production database -> Red
            "给客户发起退款",         // issue customer refund -> Red (payment)
            "导出用户数据和个人信息", // export user data + PII -> Red (customer data)
            "清空线上订单表",         // wipe live orders table -> Red (line/production + delete)
        ];
        for intent in red_cases {
            let decision = classify_mission_track(intent);
            assert_eq!(decision.track, "red", "expected red for: {intent}");
        }

        let yellow_cases = [
            "部署到测试环境",           // deploy to staging -> Yellow
            "修改内部更新日志",         // modify internal changelog -> Yellow (write)
            "发布预发布说明并通知团队", // publish preprod notes + notify -> Yellow
            "执行一个脚本检查数据库",   // run a script against the database -> Yellow
        ];
        for intent in yellow_cases {
            let decision = classify_mission_track(intent);
            assert_eq!(decision.track, "yellow", "expected yellow for: {intent}");
        }

        // A purely read-only Chinese mission stays Green.
        let green = classify_mission_track("分析并总结上周的指标");
        assert_eq!(green.track, "green");
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
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company("Memory Scope Co", Some("query scope"), "default-founder")
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, company.opc_id.parse().unwrap());
        let memory = MemoryRecord {
            memory_id: "mem-snake-query".to_string(),
            scope: MemoryScope::Company,
            owner_id: company.opc_id.clone(),
            title: "Operating Principles".to_string(),
            content: "Company rules".to_string(),
            tags: vec!["company-foundation".to_string()],
            source: "first-run".to_string(),
            provenance: format!("first-run:{}:company-foundation", company.opc_id),
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
            create_memory(headers.clone(), State(state.clone()), Json(memory)).await;
        assert_eq!(create_status, StatusCode::OK, "{create_body:?}");

        let (list_status, Json(list_body)) = list_memory(
            headers,
            State(state),
            Query(MemoryQuery {
                scope: Some("company".to_string()),
                owner_id: Some(company.opc_id),
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
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_memory_endpoints_normalize_company_scope_and_owner() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Legacy Memory Normalize Co",
                Some("normalize legacy memory"),
                "default-founder",
            )
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, company.opc_id.parse().unwrap());

        let memory = MemoryRecord {
            memory_id: "mem-legacy-normalize".to_string(),
            scope: MemoryScope::Agent,
            owner_id: "agent-founder-01".to_string(),
            title: "Legacy Memory".to_string(),
            content: "should normalize".to_string(),
            source: "test".to_string(),
            provenance: "test:legacy-normalize".to_string(),
            tags: vec!["legacy".to_string()],
            confidence: 1.0,
            ttl_seconds: 60,
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
            create_memory(headers.clone(), State(state.clone()), Json(memory)).await;
        assert_eq!(create_status, StatusCode::OK, "{create_body:?}");

        let (list_status, Json(list_body)) = list_memory(
            headers,
            State(state.clone()),
            Query(MemoryQuery {
                scope: Some("agent".to_string()),
                owner_id: Some("agent-founder-01".to_string()),
                include_revoked: None,
                q: None,
            }),
        )
        .await;

        assert_eq!(list_status, StatusCode::OK, "{list_body:?}");
        assert_eq!(list_body.as_array().unwrap().len(), 1);
        assert_eq!(list_body[0]["memory_id"], "mem-legacy-normalize");
        assert_eq!(list_body[0]["scope"], "company");
        assert_eq!(list_body[0]["owner_id"], company.opc_id);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_memory_endpoints_reject_memory_id_path_traversal() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Legacy Memory Traversal Co",
                Some("reject memory path traversal"),
                "default-founder",
            )
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, company.opc_id.parse().unwrap());

        let memory = MemoryRecord {
            memory_id: "..\\escape".to_string(),
            scope: MemoryScope::Company,
            owner_id: company.opc_id.clone(),
            title: "Traversal".to_string(),
            content: "must be rejected".to_string(),
            tags: vec!["security".to_string()],
            source: "test".to_string(),
            provenance: "test:memory-traversal".to_string(),
            confidence: 1.0,
            ttl_seconds: 60,
            created_at_ms: 1,
            updated_at_ms: 1,
            access_policy: "opc-local".to_string(),
            status: MemoryStatus::Active,
            cognitive_layer: coevo_core::cognitive::CognitiveLayer::Suggestion,
            linked_contract_hash: None,
            linked_plan_hash: None,
            linked_adr_id: None,
        };

        let (status, Json(body)) = create_memory(headers, State(state), Json(memory)).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body:?}");
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("memory_id must be a plain identifier"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn create_company_employee_rejects_agent_id_path_traversal() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let company = state
            .company_workspace
            .create_company(
                "Employee Traversal Co",
                Some("reject employee traversal"),
                "default-founder",
            )
            .await
            .unwrap();

        let employee = AgentEmployee {
            agent_id: "../escape".to_string(),
            display_name: "Traversal".to_string(),
            department: Department::Product,
            role: "Product".to_string(),
            supervisor_agent_id: None,
            passport: AgentPassport {
                passport_id: "passport-escape".to_string(),
                issued_by: "test".to_string(),
                roles: vec!["Product".to_string()],
                capabilities: vec!["read".to_string()],
                restrictions: vec![],
                expires_at_ms: None,
            },
            system_prompt: "prompt".to_string(),
            model_profile: ModelProviderProfile {
                provider: "mock".to_string(),
                base_url: String::new(),
                api_key_ref: String::new(),
                default_model: "mock-model".to_string(),
                fast_model: "mock-model".to_string(),
                reasoning_model: "mock-model".to_string(),
                structured_output_model: "mock-model".to_string(),
                timeout_ms: 1000,
                max_tokens: 256,
                max_cost_per_task_usd: 0.0,
            },
            tool_scopes: vec!["read".to_string()],
            memory_scope: MemoryScope::Company,
            risk_ceiling: 0.3,
            permission_boundary: PermissionBoundary {
                max_risk_score: 0.3,
                can_write_fact: false,
                can_write_decision: false,
                can_access_network: false,
                can_access_filesystem: true,
                can_call_external_executor: false,
                can_propose_skill: false,
            },
            allowed_cognitive_layers: vec!["Suggestion".to_string()],
            allowed_action_modes: vec!["read".to_string()],
            reputation_vector: ReputationVector::new("../escape".to_string()),
            lifecycle_status: LifecycleStatus::Draft,
            created_at_ms: 0,
            updated_at_ms: 0,
        };

        let (status, Json(body)) =
            create_company_employee(State(state), Path(company.opc_id), Json(employee)).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body:?}");
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("agent_id must be a plain identifier"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn canonical_company_routes_reject_malformed_opc_id() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;

        let (status, Json(body)) = list_company_memory(
            State(state),
            Path("../escape".to_string()),
            Query(MemoryQuery {
                owner_id: None,
                scope: None,
                include_revoked: None,
                q: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("opc_id must be a plain identifier"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn canonical_company_detail_rejects_malformed_opc_id() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;

        let (status, Json(body)) = get_company(State(state), Path("../escape".to_string())).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("opc_id must be a plain identifier"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn canonical_company_shared_routes_reject_malformed_opc_id() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;

        let (list_status, Json(list_body)) =
            list_company_shared_files(State(state.clone()), Path("../escape".to_string())).await;
        assert_eq!(list_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(list_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("opc_id must be a plain identifier"));

        let (put_status, Json(put_body)) = put_company_shared_file(
            State(state),
            Path("../escape".to_string()),
            Json(SharedFileUpsertRequest {
                path: "playbooks/launch.md".to_string(),
                content_md: "# launch".to_string(),
            }),
        )
        .await;
        assert_eq!(put_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(put_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("opc_id must be a plain identifier"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_company_scoped_routes_require_opc_header() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;

        let profile = OPCProfile {
            opc_id: "ignored".to_string(),
            founder_user_id: "default-founder".to_string(),
            name: "Header Required Co".to_string(),
            mission: "mission".to_string(),
            current_strategy: "strategy".to_string(),
            operating_principles: vec![],
            active_projects: vec![],
            asset_indexes: vec![],
            policy_profile: "policy/default".to_string(),
            memory_policy: MemoryPolicy {
                fact_ttl_default_seconds: 3600,
                require_provenance_for_fact: true,
                auto_stale_days: 30,
            },
            default_departments: vec!["FounderOffice".to_string()],
            created_at_ms: 0,
            updated_at_ms: 0,
        };

        let (profile_get_status, Json(profile_get_body)) =
            get_company_profile(HeaderMap::new(), State(state.clone())).await;
        assert_eq!(profile_get_status, StatusCode::BAD_REQUEST);
        assert!(profile_get_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (profile_put_status, Json(profile_put_body)) =
            put_company_profile(HeaderMap::new(), State(state.clone()), Json(profile)).await;
        assert_eq!(profile_put_status, StatusCode::BAD_REQUEST);
        assert!(profile_put_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let memory = MemoryRecord {
            memory_id: "mem-header-required".to_string(),
            scope: MemoryScope::Company,
            owner_id: "ignored".to_string(),
            title: "Header Required".to_string(),
            content: "must be scoped".to_string(),
            source: "test".to_string(),
            provenance: "test:header-required".to_string(),
            tags: vec![],
            confidence: 1.0,
            ttl_seconds: 60,
            created_at_ms: 1,
            updated_at_ms: 1,
            access_policy: "opc-local".to_string(),
            status: MemoryStatus::Active,
            cognitive_layer: coevo_core::cognitive::CognitiveLayer::Suggestion,
            linked_contract_hash: None,
            linked_plan_hash: None,
            linked_adr_id: None,
        };
        let (memory_create_status, Json(memory_create_body)) =
            create_memory(HeaderMap::new(), State(state.clone()), Json(memory)).await;
        assert_eq!(memory_create_status, StatusCode::BAD_REQUEST);
        assert!(memory_create_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (memory_list_status, Json(memory_list_body)) = list_memory(
            HeaderMap::new(),
            State(state.clone()),
            Query(MemoryQuery {
                scope: Some("company".to_string()),
                owner_id: None,
                include_revoked: None,
                q: None,
            }),
        )
        .await;
        assert_eq!(memory_list_status, StatusCode::BAD_REQUEST);
        assert!(memory_list_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (employee_list_status, Json(employee_list_body)) =
            list_employees(HeaderMap::new(), State(state.clone())).await;
        assert_eq!(employee_list_status, StatusCode::BAD_REQUEST);
        assert!(employee_list_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (seed_status, Json(seed_body)) =
            seed_employees_handler(HeaderMap::new(), State(state.clone())).await;
        assert_eq!(seed_status, StatusCode::BAD_REQUEST);
        assert!(seed_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (skills_status, Json(skills_body)) = list_skills(
            State(state.clone()),
            HeaderMap::new(),
            Query(SkillsQuery { agent_id: None }),
        )
        .await;
        assert_eq!(skills_status, StatusCode::BAD_REQUEST);
        assert!(skills_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (seed_skills_status, Json(seed_skills_body)) =
            seed_skills(State(state.clone()), HeaderMap::new()).await;
        assert_eq!(seed_skills_status, StatusCode::BAD_REQUEST);
        assert!(seed_skills_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (evolution_status, Json(evolution_body)) =
            run_evolution(State(state.clone()), HeaderMap::new()).await;
        assert_eq!(evolution_status, StatusCode::BAD_REQUEST);
        assert!(evolution_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let create = CreateWORequest {
            work_order_id: Some("wo-header-required".to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "header required".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            governance_proposal: None,
        };
        let (work_order_status, Json(work_order_body)) =
            create_work_order(HeaderMap::new(), State(state.clone()), Json(create)).await;
        assert_eq!(work_order_status, StatusCode::BAD_REQUEST);
        assert!(work_order_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_memory_routes_materialize_markdown_and_update_company_detail_counts() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let app = crate::router::build_router(state.clone());

        let create_company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Memory Materialization Co",
                            "mission": "Validate company memory file backing"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_company_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let create_memory_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/memory"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "memory_id": "mem-company-file-1",
                            "scope": "company",
                            "owner_id": opc_id,
                            "title": "Market signal",
                            "content": "A durable company memory should also land in markdown.",
                            "tags": ["market", "signal"],
                            "source": "integration-test",
                            "provenance": "integration-test:mem-company-file-1",
                            "confidence": 0.91,
                            "ttl_seconds": 86400,
                            "created_at_ms": chrono::Utc::now().timestamp_millis() as u64,
                            "updated_at_ms": chrono::Utc::now().timestamp_millis() as u64,
                            "access_policy": "company",
                            "status": "active",
                            "cognitive_layer": "Suggestion",
                            "linked_contract_hash": serde_json::Value::Null,
                            "linked_plan_hash": serde_json::Value::Null,
                            "linked_adr_id": serde_json::Value::Null
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_memory_response.status(), StatusCode::OK);

        let list_memory_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/memory?scope=company"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_memory_response.status(), StatusCode::OK);
        let listed: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_memory_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["memory_id"] == "mem-company-file-1"));

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
        assert_eq!(detail["memory_count"], 1);

        let memory_md_path = state
            .company_workspace
            .company_dir(&opc_id)
            .join("memory")
            .join("mem-company-file-1.md");
        assert!(
            memory_md_path.exists(),
            "expected company memory markdown file"
        );
        let memory_md = std::fs::read_to_string(memory_md_path).unwrap();
        assert!(memory_md.contains("Market signal"));
        assert!(memory_md.contains("durable company memory"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn canonical_company_memory_routes_force_company_scope_and_owner() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let app = crate::router::build_router(state.clone());

        let create_company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Memory Scope Co",
                            "mission": "Force canonical company memory scope"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_company_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let create_memory_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/memory"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "memory_id": "mem-company-force-scope-1",
                            "scope": "agent",
                            "owner_id": "agent-founder-01",
                            "title": "Canonical scope override",
                            "content": "Server should coerce this into company-scoped memory.",
                            "tags": ["canonical", "scope"],
                            "source": "integration-test",
                            "provenance": "integration-test:mem-company-force-scope-1",
                            "confidence": 0.88,
                            "ttl_seconds": 7200,
                            "created_at_ms": chrono::Utc::now().timestamp_millis() as u64,
                            "updated_at_ms": chrono::Utc::now().timestamp_millis() as u64,
                            "access_policy": "company",
                            "status": "active",
                            "cognitive_layer": "Suggestion",
                            "linked_contract_hash": serde_json::Value::Null,
                            "linked_plan_hash": serde_json::Value::Null,
                            "linked_adr_id": serde_json::Value::Null
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_memory_response.status(), StatusCode::OK);

        let company_db = company_pool(&state, &opc_id).await.unwrap();
        let stored = memory_repo::MemoryRepo::list(&company_db, None, None, false)
            .await
            .unwrap();
        company_db.close().await;
        let created_memory = stored
            .iter()
            .find(|row| row.memory_id == "mem-company-force-scope-1")
            .expect("memory should be stored");
        assert_eq!(created_memory.scope, MemoryScope::Company);
        assert_eq!(created_memory.owner_id, opc_id);

        let list_memory_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/memory"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_memory_response.status(), StatusCode::OK);
        let listed: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_memory_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let listed_memory = listed
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["memory_id"] == "mem-company-force-scope-1")
            .expect("memory should appear in canonical list");
        assert_eq!(listed_memory["scope"], "company");
        assert_eq!(listed_memory["owner_id"], opc_id);

        let memory_md_path = state
            .company_workspace
            .company_dir(&opc_id)
            .join("memory")
            .join("mem-company-force-scope-1.md");
        assert!(
            memory_md_path.exists(),
            "expected company memory markdown file"
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_shared_routes_materialize_files_and_update_company_detail_counts() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let app = crate::router::build_router(state.clone());

        let create_company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Shared Files Co",
                            "mission": "Validate company shared file backing"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_company_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let create_shared_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/shared"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "path": "playbooks/launch.md",
                            "content_md": "# Launch Playbook\n\nShared company guidance lives here."
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_shared_response.status(), StatusCode::OK);

        let list_shared_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/shared"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_shared_response.status(), StatusCode::OK);
        let listed: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_shared_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["path"] == "playbooks/launch.md"));

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
        assert_eq!(detail["shared_files_count"], 1);

        let shared_path = state
            .company_workspace
            .company_dir(&opc_id)
            .join("shared")
            .join("playbooks")
            .join("launch.md");
        assert!(
            shared_path.exists(),
            "expected shared file to exist on disk"
        );
        let content = std::fs::read_to_string(shared_path).unwrap();
        assert!(content.contains("Launch Playbook"));
        assert!(content.contains("Shared company guidance"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_detail_counts_nested_shared_files_by_file_not_directory() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let app = crate::router::build_router(state.clone());

        let create_company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Nested Shared Count Co",
                            "mission": "Validate shared file counting semantics"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_company_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        for (path, content_md) in [
            (
                "playbooks/launch.md",
                "# Launch Playbook\n\nShared company guidance lives here.",
            ),
            (
                "playbooks/handoff.md",
                "# Handoff Playbook\n\nSecond shared file in the same nested directory.",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/companies/{opc_id}/shared"))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "path": path,
                                "content_md": content_md
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

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
        assert_eq!(detail["shared_files_count"], 2);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn canonical_company_memory_routes_stale_and_revoke_company_scoped_records() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let app = crate::router::build_router(state.clone());

        let create_company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Memory Control Co",
                            "mission": "Validate canonical stale/revoke routes"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_company_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let create_memory = |memory_id: &str| {
            Request::builder()
                .method("POST")
                .uri(format!("/companies/{opc_id}/memory"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "memory_id": memory_id,
                        "scope": "company",
                        "owner_id": opc_id,
                        "title": format!("Memory {memory_id}"),
                        "content": "Canonical memory control should be company-scoped.",
                        "tags": ["canonical", "memory"],
                        "source": "integration-test",
                        "provenance": format!("integration-test:{memory_id}"),
                        "confidence": 0.88,
                        "ttl_seconds": 7200,
                        "created_at_ms": chrono::Utc::now().timestamp_millis() as u64,
                        "updated_at_ms": chrono::Utc::now().timestamp_millis() as u64,
                        "access_policy": "company",
                        "status": "active",
                        "cognitive_layer": "Suggestion",
                        "linked_contract_hash": serde_json::Value::Null,
                        "linked_plan_hash": serde_json::Value::Null,
                        "linked_adr_id": serde_json::Value::Null
                    })
                    .to_string(),
                ))
                .unwrap()
        };

        let stale_create_response = app
            .clone()
            .oneshot(create_memory("mem-stale-1"))
            .await
            .unwrap();
        assert_eq!(stale_create_response.status(), StatusCode::OK);

        let revoke_create_response = app
            .clone()
            .oneshot(create_memory("mem-revoke-1"))
            .await
            .unwrap();
        assert_eq!(revoke_create_response.status(), StatusCode::OK);

        let stale_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/memory/mem-stale-1/stale"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale_response.status(), StatusCode::OK);

        let revoke_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/memory/mem-revoke-1/revoke"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revoke_response.status(), StatusCode::OK);

        let company_db = company_pool(&state, &opc_id).await.unwrap();
        let all_records = memory_repo::MemoryRepo::list(&company_db, None, None, true)
            .await
            .unwrap();
        company_db.close().await;

        let stale_record = all_records
            .iter()
            .find(|record| record.memory_id == "mem-stale-1")
            .unwrap();
        assert_eq!(
            serde_json::to_value(stale_record.status).unwrap(),
            serde_json::json!("stale")
        );

        let revoked_record = all_records
            .iter()
            .find(|record| record.memory_id == "mem-revoke-1")
            .unwrap();
        assert_eq!(
            serde_json::to_value(revoked_record.status).unwrap(),
            serde_json::json!("revoked")
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn red_execute_waits_for_explicit_approval_without_starting_worker_rows() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        let contract_hash = "c".repeat(64);
        let work_order_id = "wo-red-alpha-block";
        insert_contract(&pool, &contract_hash).await;
        let company_db = company_pool(&state, &opc_id).await.unwrap();
        let mut risk_employee =
            agent_employee_repo::AgentEmployeeRepo::get(&company_db, "agent-risk-01")
                .await
                .unwrap()
                .unwrap();
        risk_employee.risk_ceiling = 1.0;
        risk_employee.permission_boundary.max_risk_score = 1.0;
        agent_employee_repo::AgentEmployeeRepo::upsert(&company_db, &risk_employee)
            .await
            .unwrap();
        company_db.close().await;

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: contract_hash.clone(),
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Delete production customer data".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, _) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "WaitingApproval");
        assert_eq!(body["approval_mode"], "EXPLICIT_APPROVAL");
        assert!(body["approval_id"].as_str().unwrap_or_default().len() > 0);
        assert!(body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Red Track requires explicit human approval"));
        assert_eq!(count_rows(&pool, "worker_sessions", work_order_id).await, 0);
        assert_eq!(count_rows(&pool, "worker_runs", work_order_id).await, 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn red_execute_ignores_lease_id_as_approval_receipt() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        let contract_hash = "d".repeat(64);
        let work_order_id = "wo-red-lease-id-is-not-receipt";
        insert_contract(&pool, &contract_hash).await;
        let company_db = company_pool(&state, &opc_id).await.unwrap();
        let mut risk_employee =
            agent_employee_repo::AgentEmployeeRepo::get(&company_db, "agent-risk-01")
                .await
                .unwrap()
                .unwrap();
        risk_employee.risk_ceiling = 1.0;
        risk_employee.permission_boundary.max_risk_score = 1.0;
        agent_employee_repo::AgentEmployeeRepo::upsert(&company_db, &risk_employee)
            .await
            .unwrap();
        company_db.close().await;

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: contract_hash.clone(),
            plan_hash: "e".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Delete production customer data".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, _) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);

        let approval_id = ApprovalRepo::create(
            &pool,
            &opc_id,
            &contract_hash,
            &format!("urn:coevo:work-order:{}:execute", work_order_id),
            "EXPLICIT_APPROVAL",
            "default-founder",
            300_000,
        )
        .await
        .unwrap();
        ApprovalRepo::approve(&pool, &opc_id, &approval_id, "default-founder")
            .await
            .unwrap();

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
            State(state),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: Some(approval_id.clone()),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["status"], "WaitingApproval");
        assert_ne!(
            body["approval_id"].as_str().unwrap_or_default(),
            approval_id,
            "red approval gating must not accept lease_id as the approval receipt"
        );
        assert_eq!(count_rows(&pool, "worker_sessions", work_order_id).await, 0);
        assert_eq!(count_rows(&pool, "worker_runs", work_order_id).await, 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn create_work_order_overrides_client_track_with_server_classification() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let work_order_id = "wo-server-classifies-red";

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Delete production customer data".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };

        let (status, Json(body)) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["track"], "red");
        assert_eq!(body["governance_verdict"]["effective_track"], "red");
        assert_eq!(body["governance_verdict"]["blocked"], false);
        assert!(body["governance_verdict"]["block_reason"].is_null());
        assert!(body["risk_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("high-risk trigger"));
        let scoped_pool = company_pool(&state, &opc_id).await.unwrap();
        let stored = work_order_repo::WorkOrderRepo::get(&scoped_pool, work_order_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.track, "red");
        assert!(stored.restricted_actions.contains(&"delete".to_string()));
        assert!(stored
            .restricted_actions
            .contains(&"production".to_string()));
        scoped_pool.close().await;
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn governance_verdict_downgrades_requested_tier_on_server() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let work_order_id = "wo-verdict-downgrade";

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
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

        let (status, Json(body)) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;

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
        let scoped_pool = company_pool(&state, &opc_id).await.unwrap();
        let stored = work_order_repo::WorkOrderRepo::get(&scoped_pool, work_order_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.governance_verdict.unwrap().effective_tier,
            AutonomyCeiling::ReadOnly
        );
        scoped_pool.close().await;
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn green_execute_uses_scoped_file_readonly_tool() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, company_root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        configure_active_openai_compatible(&pool).await;
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
            opc_id: opc_id.clone(),
            mission_intent: "Analyze mission-notes.md for launch readiness".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, _) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        std::fs::remove_dir_all(company_root).ok();

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
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        let work_order_id = "wo-green-provider-required";

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Analyze README.md".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, Json(created)) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn green_execute_routes_model_calls_to_active_provider_config_not_mock() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, company_root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        configure_active_openai_compatible(&pool).await;
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
            opc_id: opc_id.clone(),
            mission_intent: "Analyze mission-notes.md for model routing".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, Json(created)) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        std::fs::remove_dir_all(company_root).ok();

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
    async fn legacy_work_order_execute_uses_company_scoped_employees() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-work-order-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let (_, Json(company)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Scoped Execute Co".to_string(),
                mission: Some("Validate company-scoped work order execution".to_string()),
            }),
        )
        .await;
        let opc_id = company["opc_id"].as_str().unwrap().to_string();
        let company_db = company_pool(&state, &opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&company_db)
            .await
            .unwrap();
        company_db.close().await;

        let workspace =
            std::env::temp_dir().join(format!("coevo-company-workspace-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace.join("mission-notes.md"),
            "company scoped execution evidence",
        )
        .unwrap();
        std::env::set_var("COEVO_WORKSPACE_DIR", &workspace);

        let work_order_id = "wo-company-scoped-execute";
        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Analyze mission-notes.md for company scoped execution".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, Json(created)) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        std::fs::remove_dir_all(&workspace).ok();
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["status"], "Completed");
    }

    #[tokio::test]
    async fn execute_work_order_surfaces_repo_errors_instead_of_masking_as_not_found() {
        let (state, root) = company_test_state().await;
        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Broken Company DB".to_string(),
                mission: Some("Force a scoped repo error".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();
        let db_path = state.company_workspace.company_db_path(&opc_id);
        remove_file_with_retry(&db_path).await;
        std::fs::write(&db_path, b"").unwrap();

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
            State(state),
            Path("wo-closed-pool".to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body:?}");
        assert_ne!(body["error"], "Work order not found");
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_skill_proposal_list_uses_company_scope_header() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-proposals-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Proposal Scope Co".to_string(),
                mission: Some("Validate company-scoped proposal listing".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        let proposal = SkillEvolutionProposal {
            proposal_id: "proposal-company-scope-1".to_string(),
            source_type: EvolutionSourceType::Failure,
            source_refs: vec!["run-company-scope-1".to_string()],
            target_skill_id: "skill-mission-draft".to_string(),
            proposal_type: EvolutionProposalType::PatchSkill,
            diagnosis: "Company-scoped failure".to_string(),
            proposed_changes: "Tighten tool-use recovery instructions.".to_string(),
            expected_benefit: "Proposal should be visible through legacy scoped endpoint."
                .to_string(),
            risk_assessment: "LOW".to_string(),
            generated_tests: vec![],
            status: EvolutionProposalStatus::Draft,
            created_by_agent: "agent-founder-01".to_string(),
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        };
        skill_evolution_repo::SkillEvolutionRepo::create_proposal(&company_pool, &proposal)
            .await
            .unwrap();
        company_pool.close().await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/skills/evolution/proposals")
                    .header("x-coevo-opc-id", &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let listed: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["proposal_id"] == proposal.proposal_id));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn approve_proposal_materializes_employee_skill_markdown_immediately() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-approve-skill-file-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Approve Skill File Co".to_string(),
                mission: Some("Validate employee skill markdown write on approval".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_db = company_pool(&state, &opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&company_db)
            .await
            .unwrap();
        let proposal = SkillEvolutionProposal {
            proposal_id: "proposal-approve-skill-file-1".to_string(),
            source_type: EvolutionSourceType::Failure,
            source_refs: vec!["run-approve-skill-file-1".to_string()],
            target_skill_id: "skill-evolved-founder-playbook".to_string(),
            proposal_type: EvolutionProposalType::CreateNewSkill,
            diagnosis: "Create a founder-specific evolved skill".to_string(),
            proposed_changes:
                "Use a tighter founder escalation checklist before approving risky actions."
                    .to_string(),
            expected_benefit:
                "The evolved skill should be visible from employee markdown immediately."
                    .to_string(),
            risk_assessment: "LOW".to_string(),
            generated_tests: vec![],
            status: EvolutionProposalStatus::Draft,
            created_by_agent: "agent-founder-01".to_string(),
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        };
        skill_evolution_repo::SkillEvolutionRepo::create_proposal(&company_db, &proposal)
            .await
            .unwrap();
        company_db.close().await;

        let (status, Json(body)) = approve_proposal(
            State(state.clone()),
            {
                let mut headers = HeaderMap::new();
                headers.insert("x-coevo-opc-id", opc_id.parse().unwrap());
                headers
            },
            Path(proposal.proposal_id.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");

        let scoped_pool = company_pool(&state, &opc_id).await.unwrap();
        let stored = skill_repo::SkillRepo::get(&scoped_pool, &proposal.target_skill_id, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.provenance,
            format!("skill-evolution-{}", proposal.proposal_id)
        );
        let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&scoped_pool, None)
            .await
            .unwrap();
        assert!(proposals
            .iter()
            .any(|row| row.proposal_id == proposal.proposal_id
                && row.status == EvolutionProposalStatus::Applied));
        scoped_pool.close().await;

        let skill_path = state
            .company_workspace
            .company_employee_skill_markdown_path(
                &opc_id,
                "agent-founder-01",
                &proposal.target_skill_id,
            );
        assert!(
            skill_path.exists(),
            "expected employee skill markdown to exist"
        );
        let content = std::fs::read_to_string(&skill_path).unwrap();
        assert!(content.contains("skill-evolved-founder-playbook"));
        assert!(content.contains("founder escalation checklist"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn approve_proposal_publishes_employee_prompt_files_through_record_and_publish_path() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-prompt-sync-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Prompt Sync Co".to_string(),
                mission: Some("Validate proposal approval prompt sync".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_db = company_pool(&state, &opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&company_db)
            .await
            .unwrap();
        let proposal = SkillEvolutionProposal {
            proposal_id: "proposal-prompt-sync-1".to_string(),
            source_type: EvolutionSourceType::Failure,
            source_refs: vec!["run-prompt-sync-1".to_string()],
            target_skill_id: "agent-founder-01".to_string(),
            proposal_type: EvolutionProposalType::PatchSkill,
            diagnosis: "Prompt sync should land on markdown source of truth.".to_string(),
            proposed_changes: "Use the approved prompt body from proposal sync test.".to_string(),
            expected_benefit:
                "Published employee prompt should update prompt.md and version history.".to_string(),
            risk_assessment: "LOW".to_string(),
            generated_tests: vec![],
            status: EvolutionProposalStatus::Draft,
            created_by_agent: "agent-founder-01".to_string(),
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        };
        skill_evolution_repo::SkillEvolutionRepo::create_proposal(&company_db, &proposal)
            .await
            .unwrap();
        let employee = agent_employee_repo::AgentEmployeeRepo::get(&company_db, "agent-founder-01")
            .await
            .unwrap()
            .unwrap();
        ensure_company_employee_files(&state, &opc_id, &employee).unwrap();
        company_db.close().await;

        let employee_dir = state
            .company_workspace
            .company_employee_dir(&opc_id, "agent-founder-01");
        let prompt_path = employee_dir.join("prompt.md");
        let current_before = std::fs::read_to_string(&prompt_path).unwrap();

        let (status, Json(body)) = approve_proposal(
            State(state.clone()),
            {
                let mut headers = HeaderMap::new();
                headers.insert("x-coevo-opc-id", opc_id.parse().unwrap());
                headers
            },
            Path(proposal.proposal_id.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");

        assert!(
            prompt_path.exists(),
            "expected prompt.md to exist after approval"
        );
        let current_after = std::fs::read_to_string(&prompt_path).unwrap();
        assert_eq!(current_after, proposal.proposed_changes);
        assert_ne!(current_after, current_before);

        let current_version_path = employee_dir.join("prompt_versions").join("current.txt");
        assert!(
            current_version_path.exists(),
            "expected current.txt to exist after approval"
        );
        let current_version = std::fs::read_to_string(&current_version_path)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        let version_body = std::fs::read_to_string(
            employee_dir
                .join("prompt_versions")
                .join(format!("v{current_version}.md")),
        )
        .unwrap();
        assert_eq!(version_body, proposal.proposed_changes);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn canonical_company_skill_evolution_routes_operate_on_company_scoped_proposals() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-skill-evolution-canonical-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Canonical Skill Evolution Co".to_string(),
                mission: Some("Validate canonical proposal routes".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_db = company_pool(&state, &opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&company_db)
            .await
            .unwrap();
        let proposal = SkillEvolutionProposal {
            proposal_id: "proposal-company-canonical-1".to_string(),
            source_type: EvolutionSourceType::Failure,
            source_refs: vec!["run-company-canonical-1".to_string()],
            target_skill_id: "skill-company-canonical".to_string(),
            proposal_type: EvolutionProposalType::CreateNewSkill,
            diagnosis: "Create a canonical company-scoped evolved skill".to_string(),
            proposed_changes:
                "Use canonical company-scoped proposal routes when approving prompt improvements."
                    .to_string(),
            expected_benefit: "Company-scoped proposal workflow should not depend on legacy /opc."
                .to_string(),
            risk_assessment: "LOW".to_string(),
            generated_tests: vec![],
            status: EvolutionProposalStatus::Draft,
            created_by_agent: "agent-founder-01".to_string(),
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        };
        skill_evolution_repo::SkillEvolutionRepo::create_proposal(&company_db, &proposal)
            .await
            .unwrap();
        company_db.close().await;

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/skills/evolution/proposals"))
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
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["proposal_id"] == proposal.proposal_id));

        let verify_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/skills/evolution/proposals/{}/verify",
                        proposal.proposal_id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(verify_response.status(), StatusCode::OK);

        let approve_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/skills/evolution/proposals/{}/approve",
                        proposal.proposal_id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(approve_response.status(), StatusCode::OK);

        let scoped_pool = company_pool(&state, &opc_id).await.unwrap();
        let stored = skill_repo::SkillRepo::get(&scoped_pool, &proposal.target_skill_id, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.provenance,
            format!("skill-evolution-{}", proposal.proposal_id)
        );
        let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&scoped_pool, None)
            .await
            .unwrap();
        assert!(proposals
            .iter()
            .any(|row| row.proposal_id == proposal.proposal_id
                && row.status == EvolutionProposalStatus::Applied));
        scoped_pool.close().await;

        let reject_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/skills/evolution/proposals/{}/reject",
                        proposal.proposal_id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reject_response.status(), StatusCode::OK);

        let scoped_pool = company_pool(&state, &opc_id).await.unwrap();
        let proposals = skill_evolution_repo::SkillEvolutionRepo::list(&scoped_pool, None)
            .await
            .unwrap();
        assert!(proposals
            .iter()
            .any(|row| row.proposal_id == proposal.proposal_id
                && row.status == EvolutionProposalStatus::Rejected));
        scoped_pool.close().await;

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_company_profile_uses_company_scope_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-profile-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Profile Scope Co".to_string(),
                mission: Some("Validate company-scoped profile listing".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        opc_profile_repo::OPCProfileRepo::upsert(
            &company_pool,
            &OPCProfile {
                opc_id: opc_id.clone(),
                founder_user_id: "default-founder".to_string(),
                name: "Scoped Profile".to_string(),
                mission: "Company-only mission".to_string(),
                current_strategy: "Stay company scoped".to_string(),
                operating_principles: vec!["isolation-first".to_string()],
                active_projects: vec!["legacy-scope".to_string()],
                asset_indexes: vec!["index-a".to_string()],
                policy_profile: "policy/company".to_string(),
                memory_policy: MemoryPolicy {
                    fact_ttl_default_seconds: 3600,
                    require_provenance_for_fact: true,
                    auto_stale_days: 45,
                },
                default_departments: vec!["FounderOffice".to_string()],
                created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
                updated_at_ms: chrono::Utc::now().timestamp_millis() as u64,
            },
        )
        .await
        .unwrap();
        company_pool.close().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/profile/company")
                    .header("x-coevo-opc-id", &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["opc_id"], opc_id);
        assert_eq!(body["name"], "Scoped Profile");
        assert_eq!(body["mission"], "Company-only mission");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn canonical_company_profile_route_reads_and_writes_company_scoped_profile() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-profile-canonical-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Canonical Profile Co".to_string(),
                mission: Some("Validate canonical company profile route".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let payload = OPCProfile {
            opc_id: "mismatch-will-be-overwritten".to_string(),
            founder_user_id: "default-founder".to_string(),
            name: "Canonical Scoped Profile".to_string(),
            mission: "Company profile through canonical path".to_string(),
            current_strategy: "Prefer explicit company path".to_string(),
            operating_principles: vec!["explicit-scope".to_string()],
            active_projects: vec!["profile-migration".to_string()],
            asset_indexes: vec!["asset-a".to_string()],
            policy_profile: "policy/canonical".to_string(),
            memory_policy: MemoryPolicy {
                fact_ttl_default_seconds: 7200,
                require_provenance_for_fact: true,
                auto_stale_days: 60,
            },
            default_departments: vec!["FounderOffice".to_string(), "Operations".to_string()],
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
            updated_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        };

        let put_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/companies/{opc_id}/profile/company"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put_response.status(), StatusCode::OK);
        let put_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(put_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(put_body["ok"], true);
        assert_eq!(put_body["opc_id"], opc_id);

        let get_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/profile/company"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let get_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(get_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(get_body["opc_id"], opc_id);
        assert_eq!(get_body["name"], "Canonical Scoped Profile");
        assert_eq!(
            get_body["mission"],
            "Company profile through canonical path"
        );
        assert_eq!(get_body["policy_profile"], "policy/canonical");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn canonical_company_employee_memory_route_reads_company_scoped_memory() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-agent-memory-canonical-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Canonical Agent Memory Co".to_string(),
                mission: Some("Validate canonical employee memory route".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_db = company_pool(&state, &opc_id).await.unwrap();
        agent_memory_repo::AgentMemoryRepo::upsert(
            &company_db,
            &AgentMemory {
                agent_id: "agent-founder-01".to_string(),
                memory_records: vec!["remember launch prep".to_string()],
                working_preferences: "Prefer concise planning.".to_string(),
                learned_constraints: vec!["stay within budget".to_string()],
                recurring_failures: vec!["over-explains status".to_string()],
                successful_patterns: vec!["ship narrow backend diffs".to_string()],
                recent_tasks: vec!["profile canonicalization".to_string()],
                performance_notes: "Improving steadily.".to_string(),
                skill_usage_stats: "{\"skill-mission-draft\":3}".to_string(),
            },
        )
        .await
        .unwrap();
        company_db.close().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{opc_id}/employees/agent-founder-01/memory"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["agent_id"], "agent-founder-01");
        assert_eq!(body["working_preferences"], "Prefer concise planning.");
        assert_eq!(body["successful_patterns"][0], "ship narrow backend diffs");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_memory_endpoints_use_company_scope_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-memory-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Memory Scope Co".to_string(),
                mission: Some("Validate company-scoped memory endpoints".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/memory")
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "memory_id": "mem-company-scope-1",
                            "scope": "company",
                            "owner_id": "opc-scope",
                            "title": "Scoped memory",
                            "content": "Company-scoped memory should stay isolated.",
                            "tags": ["scope", "company"],
                            "source": "integration-test",
                            "status": "active",
                            "provenance": "integration-test",
                            "confidence": 0.9,
                            "ttl_seconds": 3600,
                            "access_policy": "company",
                            "cognitive_layer": "Suggestion",
                            "created_at_ms": chrono::Utc::now().timestamp_millis() as u64,
                            "updated_at_ms": chrono::Utc::now().timestamp_millis() as u64,
                            "linked_contract_hash": serde_json::Value::Null,
                            "linked_plan_hash": serde_json::Value::Null,
                            "linked_adr_id": serde_json::Value::Null
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let create_status = create_response.status();
        let create_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(create_status, StatusCode::OK, "{create_body:?}");

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/memory")
                    .header("x-coevo-opc-id", &opc_id)
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
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["memory_id"] == "mem-company-scope-1"));

        let global_rows = memory_repo::MemoryRepo::list(&pool, None, None, false)
            .await
            .unwrap();
        assert!(global_rows
            .iter()
            .all(|row| row.memory_id != "mem-company-scope-1"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_agent_growth_uses_company_scope_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-growth-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Growth Scope Co".to_string(),
                mission: Some("Validate company-scoped growth endpoint".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&company_pool)
            .await
            .unwrap();
        WorkerRunRepo::create(
            &company_pool,
            &opc_id,
            "run-growth-company-1",
            "wo-growth-company-1",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-growth-company-1",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            10,
            Some(20),
        )
        .await
        .unwrap();
        WorkerRunRepo::record_summary(&company_pool, "run-growth-company-1", 5, 7, 12, 0.34, 10)
            .await
            .unwrap();
        ReputationHistoryRepo::snapshot(
            &company_pool,
            "agent-founder-01",
            Some("run-growth-company-1"),
            0.9,
            0.8,
            0.95,
            0.85,
            1,
        )
        .await
        .unwrap();
        company_pool.close().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/agents/employees/agent-founder-01/growth")
                    .header("x-coevo-opc-id", &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["agent_id"], "agent-founder-01");
        assert_eq!(body["total_tasks"], 1);
        assert_eq!(body["completed_tasks"], 1);
        assert_eq!(body["total_usage"], 12);
        assert_eq!(body["trend"].as_array().unwrap().len(), 1);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_work_order_list_uses_company_scope_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-work-order-list-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "WorkOrder List Scope Co".to_string(),
                mission: Some("Validate company-scoped legacy work order listing".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        WorkOrderRepo::create(
            &pool,
            &WorkOrder {
                work_order_id: "wo-global-only".to_string(),
                conversation_id: None,
                contract_hash: "a".repeat(64),
                plan_hash: "b".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: "default-opc".to_string(),
                mission_intent: "global work order".to_string(),
                selected_agents: vec!["agent-founder-01".to_string()],
                selected_executors: vec![],
                required_skills: vec![],
                track: "green".to_string(),
                status: WorkOrderStatus::Planned,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec![],
                risk_summary: "global".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
                updated_at_ms: chrono::Utc::now().timestamp_millis() as u64,
            },
        )
        .await
        .unwrap();

        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        WorkOrderRepo::create(
            &company_pool,
            &WorkOrder {
                work_order_id: "wo-company-only".to_string(),
                conversation_id: None,
                contract_hash: "c".repeat(64),
                plan_hash: "d".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: opc_id.clone(),
                mission_intent: "company work order".to_string(),
                selected_agents: vec!["agent-founder-01".to_string()],
                selected_executors: vec![],
                required_skills: vec![],
                track: "green".to_string(),
                status: WorkOrderStatus::Planned,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec![],
                risk_summary: "company".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
                updated_at_ms: chrono::Utc::now().timestamp_millis() as u64,
            },
        )
        .await
        .unwrap();
        company_pool.close().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/work-orders")
                    .header("x-coevo-opc-id", &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let rows = body.as_array().unwrap();
        assert!(rows
            .iter()
            .any(|row| row["work_order_id"] == "wo-company-only"));
        assert!(!rows
            .iter()
            .any(|row| row["work_order_id"] == "wo-global-only"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_work_order_create_and_list_are_consistent_under_company_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-work-order-create-list-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "WorkOrder Create/List Scope Co".to_string(),
                mission: Some("Validate legacy create/list company consistency".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&company_pool)
            .await
            .unwrap();
        company_pool.close().await;

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/work-orders")
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": "wo-legacy-header-create",
                            "conversation_id": serde_json::Value::Null,
                            "contract_hash": "a".repeat(64),
                            "plan_hash": "b".repeat(64),
                            "user_id": "default-founder",
                            "opc_id": opc_id,
                            "mission_intent": "legacy header create/list consistency",
                            "selected_agents": ["agent-founder-01"],
                            "selected_executors": [],
                            "required_skills": [],
                            "governance_proposal": serde_json::Value::Null
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/work-orders")
                    .header("x-coevo-opc-id", &opc_id)
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
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["work_order_id"] == "wo-legacy-header-create"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_work_order_create_and_execute_are_consistent_under_company_header() {
        let _lock = ENV_LOCK.lock().unwrap();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        skill_repo::SkillRepo::seed_default(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-work-order-create-execute-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (create_status, Json(created)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "WorkOrder Create/Execute Scope Co".to_string(),
                mission: Some("Validate legacy create/execute company consistency".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&company_pool)
            .await
            .unwrap();
        company_pool.close().await;

        let workspace = std::env::temp_dir().join(format!(
            "coevo-header-company-workspace-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace.join("mission-notes.md"),
            "legacy header company execution evidence",
        )
        .unwrap();
        std::env::set_var("COEVO_WORKSPACE_DIR", &workspace);

        let work_order_id = "wo-legacy-header-create-execute";
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/work-orders")
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": work_order_id,
                            "conversation_id": serde_json::Value::Null,
                            "contract_hash": "a".repeat(64),
                            "plan_hash": "b".repeat(64),
                            "user_id": "default-founder",
                            "opc_id": opc_id,
                            "mission_intent": "Analyze mission-notes.md through legacy header company execution",
                            "selected_agents": ["agent-founder-01"],
                            "selected_executors": [],
                            "required_skills": ["skill-mission-draft"],
                            "governance_proposal": serde_json::Value::Null
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let execute_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/opc/work-orders/{work_order_id}/execute"))
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let execute_status = execute_response.status();
        let execute_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(execute_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(&workspace).ok();
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(execute_status, StatusCode::OK, "{execute_body:?}");
        assert_eq!(execute_body["status"], "Completed");
    }

    #[tokio::test]
    async fn legacy_work_order_header_and_body_opc_id_must_match() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-work-order-mismatch-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool, root.clone());
        let app = crate::router::build_router(state.clone());

        let (_, Json(company_a)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Header Scope A".to_string(),
                mission: Some("header/body mismatch A".to_string()),
            }),
        )
        .await;
        let (_, Json(company_b)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Header Scope B".to_string(),
                mission: Some("header/body mismatch B".to_string()),
            }),
        )
        .await;
        let opc_a = company_a["opc_id"].as_str().unwrap().to_string();
        let opc_b = company_b["opc_id"].as_str().unwrap().to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/work-orders")
                    .header("x-coevo-opc-id", &opc_a)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": "wo-legacy-header-body-mismatch",
                            "conversation_id": serde_json::Value::Null,
                            "contract_hash": "a".repeat(64),
                            "plan_hash": "b".repeat(64),
                            "user_id": "default-founder",
                            "opc_id": opc_b,
                            "mission_intent": "header/body mismatch should be rejected",
                            "selected_agents": [],
                            "selected_executors": [],
                            "required_skills": [],
                            "governance_proposal": serde_json::Value::Null
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        std::fs::remove_dir_all(root).ok();

        assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_HEADER_BODY_MISMATCH"));
    }

    #[tokio::test]
    async fn legacy_executor_dry_run_uses_company_scoped_work_order() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-executor-dry-run-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = crate::router::build_router(state.clone());

        let (_, Json(company)) = create_company(
            State(state.clone()),
            Json(CreateCompanyRequest {
                name: "Executor Dry Run Scope Co".to_string(),
                mission: Some("Validate legacy executor dry-run company consistency".to_string()),
            }),
        )
        .await;
        let opc_id = company["opc_id"].as_str().unwrap().to_string();

        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-legacy-executor-dry-run".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "executor dry run scoped lookup".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec!["delete".to_string()],
            risk_summary: "scope".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        work_order_repo::WorkOrderRepo::create(&company_pool, &work_order)
            .await
            .unwrap();
        company_pool.close().await;

        let executor = ExternalExecutorPassport {
            executor_id: "executor-hermes-legacy-scope".to_string(),
            display_name: "Legacy Scope Hermes".to_string(),
            // MCP source: its dry_run is deterministic (no network/filesystem
            // dependency) so this test exercises the company-scoped work-order
            // resolution path, not a live runtime probe.
            source_type: ExecutorSourceType::MCP,
            runtime_endpoint: "http://127.0.0.1:9999".to_string(),
            capabilities: vec!["analysis".to_string()],
            required_credentials: vec![],
            permission_boundary: PermissionBoundary {
                max_risk_score: 1.0,
                can_write_fact: false,
                can_write_decision: false,
                can_access_network: false,
                can_access_filesystem: false,
                can_call_external_executor: false,
                can_propose_skill: false,
            },
            file_scope: vec![],
            network_scope: vec![],
            memory_scope: MemoryScope::Executor,
            risk_ceiling: 1.0,
            supported_actions: vec!["read".to_string()],
            sandbox_level: SandboxLevel::Process,
            health_check_url: "http://127.0.0.1:9999/health".to_string(),
            audit_callback_url: "http://127.0.0.1:9999/audit".to_string(),
            status: ExecutorStatus::Registered,
            created_at_ms: now,
            updated_at_ms: now,
        };
        executor_repo::ExecutorRepo::register(&pool, &executor)
            .await
            .unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/executors/executor-hermes-legacy-scope/dry-run")
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": work_order.work_order_id
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        std::fs::remove_dir_all(root).ok();

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["passed"], true);
    }

    #[tokio::test]
    async fn legacy_executor_dry_run_allows_default_opc_work_order_without_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(
            pool.clone(),
            std::env::temp_dir().join(format!(
                "coevo-executor-dry-run-default-{}",
                uuid::Uuid::new_v4()
            )),
        );

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-default-executor-dry-run".to_string(),
            conversation_id: None,
            contract_hash: "c".repeat(64),
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "default opc executor dry run".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec![],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec!["delete".to_string()],
            risk_summary: "default-opc".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        work_order_repo::WorkOrderRepo::create(&pool, &work_order)
            .await
            .unwrap();

        let executor = ExternalExecutorPassport {
            executor_id: "executor-hermes-default-scope".to_string(),
            display_name: "Default Scope Hermes".to_string(),
            // MCP source: deterministic dry_run (see legacy-scope test above).
            source_type: ExecutorSourceType::MCP,
            runtime_endpoint: "http://127.0.0.1:9999".to_string(),
            capabilities: vec!["analysis".to_string()],
            required_credentials: vec![],
            permission_boundary: PermissionBoundary {
                max_risk_score: 1.0,
                can_write_fact: false,
                can_write_decision: false,
                can_access_network: false,
                can_access_filesystem: false,
                can_call_external_executor: false,
                can_propose_skill: false,
            },
            file_scope: vec![],
            network_scope: vec![],
            memory_scope: MemoryScope::Executor,
            risk_ceiling: 1.0,
            supported_actions: vec!["read".to_string()],
            sandbox_level: SandboxLevel::Process,
            health_check_url: "http://127.0.0.1:9999/health".to_string(),
            audit_callback_url: "http://127.0.0.1:9999/audit".to_string(),
            status: ExecutorStatus::Registered,
            created_at_ms: now,
            updated_at_ms: now,
        };
        executor_repo::ExecutorRepo::register(&pool, &executor)
            .await
            .unwrap();

        let (status, Json(body)) = executor_dry_run(
            HeaderMap::new(),
            State(state),
            Path("executor-hermes-default-scope".to_string()),
            Json(ExecutorDryRunReq {
                work_order_id: work_order.work_order_id.clone(),
            }),
        )
        .await;

        eprintln!("red approval test status={status} body={body:?}");
        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["passed"], true);
    }

    #[tokio::test]
    async fn audit_export_includes_work_order_execution_and_memory_evidence() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, company_root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        configure_active_openai_compatible(&pool).await;
        let work_order_id = "wo-audit-export";
        let root =
            std::env::temp_dir().join(format!("coevo-audit-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mission-notes.md"), "launch readiness evidence").unwrap();
        std::env::set_var("COEVO_WORKSPACE_DIR", &root);

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Analyze mission-notes.md for launch readiness".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, _) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);
        let (execute_status, _) = execute_work_order(
            legacy_company_headers(&opc_id),
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

        let (export_status, Json(export)) = work_order_audit_export(
            legacy_company_headers(&opc_id),
            State(state),
            Path(work_order_id.to_string()),
        )
        .await;

        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(company_root).ok();

        assert_eq!(export_status, StatusCode::OK, "{export:?}");
        assert_eq!(export["schema_version"], "coevo.audit_export.v1");
        assert_eq!(export["work_order"]["work_order_id"], work_order_id);
        assert_eq!(export["governance"]["track"], "green");
        assert!(export["worker_runs"].as_array().unwrap().len() >= 1);
        assert!(export["worker_steps"].as_array().unwrap().len() >= 1);
        assert!(export["worker_events"].as_array().unwrap().len() >= 1);
        assert!(export["tool_calls"].is_array());
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
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        configure_active_openai_compatible(&pool).await;
        let work_order_id = "wo-yellow-approval";
        let contract_hash = "c".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash: contract_hash.clone(),
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Draft a changelog update for internal release".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, Json(created)) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);
        assert_eq!(created["track"], "yellow");

        let (wait_status, Json(wait_body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        let approval = ApprovalRepo::find_by_id(&pool, &opc_id, approval_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(approval.status, "pending");
        assert_eq!(approval.contract_hash, contract_hash);

        let (blocked_status, Json(blocked_body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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

        ApprovalRepo::approve(&pool, &opc_id, approval_id, "default-founder")
            .await
            .unwrap();
        let (execute_status, Json(execute_body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        std::fs::remove_dir_all(root).ok();
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
        assert!(detail["charter_md"]
            .as_str()
            .unwrap_or_default()
            .contains("Alpha Labs"));

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
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        configure_active_openai_compatible(&pool).await;
        let work_order_id = "wo-yellow-approval-endpoint";
        let contract_hash = "e".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        create_yellow_work_order(state.clone(), &opc_id, work_order_id, &contract_hash).await;
        let (wait_status, Json(wait_body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
            legacy_company_headers(&opc_id),
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
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn approval_endpoint_ignores_spoofed_header_actor_id() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        configure_active_openai_compatible(&pool).await;
        let work_order_id = "wo-yellow-approval-actor";
        let contract_hash = "f".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        create_yellow_work_order(state.clone(), &opc_id, work_order_id, &contract_hash).await;
        let (wait_status, Json(wait_body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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

        let mut headers = legacy_company_headers(&opc_id);
        headers.insert("x-coevo-actor-id", HeaderValue::from_static("approver-42"));
        let (status, _) = decide_work_order_approval(
            headers,
            State(state),
            Path(work_order_id.to_string()),
            Json(ApprovalDecisionRequest {
                approval_id: approval_id.clone(),
                decision: "approve".to_string(),
                comment: Some("approved by actor header".to_string()),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let approval = ApprovalRepo::find_by_id(&pool, &opc_id, &approval_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(approval.approved_by.as_deref(), Some("default-founder"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn execute_work_order_rejects_bare_receipt_when_resume_cursor_requires_digest() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        let work_order_id = "wo-yellow-resume-digest";
        let contract_hash = "a".repeat(64);
        insert_contract(&pool, &contract_hash).await;
        let company_db = company_pool(&state, &opc_id).await.unwrap();

        create_yellow_work_order(state.clone(), &opc_id, work_order_id, &contract_hash).await;
        let approval_id = ApprovalRepo::create(
            &pool,
            &opc_id,
            &contract_hash,
            &format!("urn:coevo:work-order:{work_order_id}:execute"),
            "NEGATIVE_CONSENT",
            "default-founder",
            300_000,
        )
        .await
        .unwrap();
        ApprovalRepo::approve(&pool, &opc_id, &approval_id, "default-founder")
            .await
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let cursor = serde_json::json!({
            "kind": "controlled_react_cursor",
            "version": 1,
            "run_id": "run-resume-digest-1",
            "round": 2,
            "pending_action_digest": "digest-123",
            "reason": "approval required",
            "authorization_serialized": false,
        });
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
        .bind(format!("session-{work_order_id}"))
        .bind(&opc_id)
        .bind("worker-agent-risk-01")
        .bind(work_order_id)
        .bind("agent-risk-01")
        .bind("MissionChat")
        .bind(cursor.to_string())
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("WaitingApproval")
        .bind(now)
        .bind(now)
        .execute(&company_db)
        .await
        .unwrap();
        company_db.close().await;

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
            State(state),
            Path(work_order_id.to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: Some(approval_id),
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
            .contains("APPROVAL_RECEIPT_DIGEST_REQUIRED"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn approval_endpoint_rejects_cross_company_approval_receipt() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let pool = state.pool.clone();
        configure_active_openai_compatible(&pool).await;

        let alpha = state
            .company_workspace
            .create_company("Alpha Approval Co", Some("alpha"), "default-founder")
            .await
            .unwrap();
        let beta = state
            .company_workspace
            .create_company("Beta Approval Co", Some("beta"), "default-founder")
            .await
            .unwrap();

        let alpha_pool = company_pool(&state, &alpha.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&alpha_pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&alpha_pool)
            .await
            .unwrap();
        alpha_pool.close().await;

        let beta_pool = company_pool(&state, &beta.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&beta_pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&beta_pool)
            .await
            .unwrap();
        beta_pool.close().await;

        let alpha_contract_hash = "a".repeat(64);
        insert_contract(&pool, &alpha_contract_hash).await;
        create_yellow_work_order(
            state.clone(),
            &alpha.opc_id,
            "wo-alpha-cross-approval",
            &alpha_contract_hash,
        )
        .await;
        let (alpha_wait_status, Json(alpha_wait_body)) = execute_work_order(
            legacy_company_headers(&alpha.opc_id),
            State(state.clone()),
            Path("wo-alpha-cross-approval".to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(alpha_wait_status, StatusCode::OK, "{alpha_wait_body:?}");
        let alpha_approval_id = alpha_wait_body["approval_id"].as_str().unwrap().to_string();

        let beta_contract_hash = "b".repeat(64);
        insert_contract(&pool, &beta_contract_hash).await;
        create_yellow_work_order(
            state.clone(),
            &beta.opc_id,
            "wo-beta-cross-approval",
            &beta_contract_hash,
        )
        .await;
        let (beta_wait_status, Json(beta_wait_body)) = execute_work_order(
            legacy_company_headers(&beta.opc_id),
            State(state.clone()),
            Path("wo-beta-cross-approval".to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;
        assert_eq!(beta_wait_status, StatusCode::OK, "{beta_wait_body:?}");

        let (status, Json(body)) = decide_work_order_approval(
            legacy_company_headers(&beta.opc_id),
            State(state.clone()),
            Path("wo-beta-cross-approval".to_string()),
            Json(ApprovalDecisionRequest {
                approval_id: alpha_approval_id,
                decision: "approve".to_string(),
                comment: Some("wrong company".to_string()),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");
        assert_eq!(
            count_rows(&pool, "worker_runs", "wo-beta-cross-approval").await,
            0
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn execute_work_order_rejects_legacy_header_stored_opc_mismatch() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        let pool = state.pool.clone();
        configure_active_openai_compatible(&pool).await;

        let alpha = state
            .company_workspace
            .create_company("Alpha Execute Co", Some("alpha"), "default-founder")
            .await
            .unwrap();
        let beta = state
            .company_workspace
            .create_company("Beta Execute Co", Some("beta"), "default-founder")
            .await
            .unwrap();

        let alpha_pool = company_pool(&state, &alpha.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&alpha_pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&alpha_pool)
            .await
            .unwrap();
        alpha_pool.close().await;

        let beta_pool = company_pool(&state, &beta.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&beta_pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&beta_pool)
            .await
            .unwrap();
        beta_pool.close().await;

        let contract_hash = "c".repeat(64);
        insert_contract(&pool, &contract_hash).await;
        let create = CreateWORequest {
            work_order_id: Some("wo-alpha-header-mismatch".to_string()),
            conversation_id: None,
            contract_hash,
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: alpha.opc_id.clone(),
            mission_intent: "Validate mismatched legacy header".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, Json(created)) = create_work_order(
            legacy_company_headers(&alpha.opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&beta.opc_id),
            State(state.clone()),
            Path("wo-alpha-header-mismatch".to_string()),
            Json(ExecuteRequest {
                caller_identity_proof: None,
                monitoring_signature: None,
                diagnostic_signature: None,
                lease_id: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");

        let alpha_pool = company_pool(&state, &alpha.opc_id).await.unwrap();
        let stored = WorkOrderRepo::get(&alpha_pool, "wo-alpha-header-mismatch")
            .await
            .unwrap()
            .unwrap();
        alpha_pool.close().await;
        assert_eq!(stored.status, WorkOrderStatus::Planned);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn work_order_feedback_creates_proposal_in_scoped_pool_only() {
        let _lock = ENV_LOCK.lock().unwrap();
        let (state, root) = company_test_state().await;
        configure_active_openai_compatible(&state.pool).await;

        let alpha = state
            .company_workspace
            .create_company("Alpha Feedback Co", Some("alpha"), "default-founder")
            .await
            .unwrap();
        let beta = state
            .company_workspace
            .create_company("Beta Feedback Co", Some("beta"), "default-founder")
            .await
            .unwrap();

        let alpha_pool = company_pool(&state, &alpha.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&alpha_pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&alpha_pool)
            .await
            .unwrap();
        alpha_pool.close().await;

        let beta_pool = company_pool(&state, &beta.opc_id).await.unwrap();
        agent_employee_repo::AgentEmployeeRepo::seed(&beta_pool)
            .await
            .unwrap();
        skill_repo::SkillRepo::seed_default(&beta_pool)
            .await
            .unwrap();
        beta_pool.close().await;

        let create = CreateWORequest {
            work_order_id: Some("wo-alpha-feedback-scope".to_string()),
            conversation_id: None,
            contract_hash: "1".repeat(64),
            plan_hash: "2".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: alpha.opc_id.clone(),
            mission_intent: "Feedback isolation check".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, Json(created)) = create_work_order(
            legacy_company_headers(&alpha.opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");

        let (status, Json(body)) = work_order_feedback(
            legacy_company_headers(&alpha.opc_id),
            State(state.clone()),
            Path("wo-alpha-feedback-scope".to_string()),
            Json(WorkOrderFeedback {
                feedback: "The worker failed to follow instructions and should tighten its planning prompt.".to_string(),
                agent_id: Some("agent-founder-01".to_string()),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        let proposal_id = body["proposal_id"].as_str().unwrap().to_string();

        let alpha_pool = company_pool(&state, &alpha.opc_id).await.unwrap();
        let alpha_proposals = skill_evolution_repo::SkillEvolutionRepo::list(&alpha_pool, None)
            .await
            .unwrap();
        let alpha_work_order = WorkOrderRepo::get(&alpha_pool, "wo-alpha-feedback-scope")
            .await
            .unwrap()
            .unwrap();
        alpha_pool.close().await;

        let beta_pool = company_pool(&state, &beta.opc_id).await.unwrap();
        let beta_proposals = skill_evolution_repo::SkillEvolutionRepo::list(&beta_pool, None)
            .await
            .unwrap();
        beta_pool.close().await;

        let global_proposals = skill_evolution_repo::SkillEvolutionRepo::list(&state.pool, None)
            .await
            .unwrap();

        assert!(alpha_proposals.iter().any(|p| p.proposal_id == proposal_id));
        assert_eq!(alpha_work_order.status, WorkOrderStatus::Failed);
        assert!(!beta_proposals.iter().any(|p| p.proposal_id == proposal_id));
        assert!(!global_proposals
            .iter()
            .any(|p| p.proposal_id == proposal_id));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn run_evolution_should_not_return_placeholder_patch_text() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        configure_active_openai_compatible(&state.pool).await;
        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        WorkOrderRepo::create(
            &company_pool,
            &WorkOrder {
                work_order_id: "wo-evolution-failed-source".to_string(),
                conversation_id: None,
                contract_hash: "e".repeat(64),
                plan_hash: "f".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: opc_id.clone(),
                mission_intent: "Recover from a failed governed task".to_string(),
                selected_agents: vec!["agent-risk-01".to_string()],
                selected_executors: vec![],
                required_skills: vec!["skill-mission-draft".to_string()],
                track: "yellow".to_string(),
                status: WorkOrderStatus::Failed,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec!["delete".to_string()],
                risk_summary: "tool misuse caused the failure".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .await
        .unwrap();
        company_pool.close().await;

        let (status, Json(body)) =
            run_evolution(State(state), legacy_company_headers(&opc_id)).await;

        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_ne!(body["proposed_changes"], "auto-patch");
        assert!(!body["proposed_changes"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .is_empty());
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn run_evolution_returns_error_without_failed_company_work_order_source() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        configure_active_openai_compatible(&state.pool).await;

        let (status, Json(body)) =
            run_evolution(State(state), legacy_company_headers(&opc_id)).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body:?}");
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("failed work order"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn run_evolution_returns_error_when_only_mock_provider_is_available() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        WorkOrderRepo::create(
            &company_pool,
            &WorkOrder {
                work_order_id: "wo-evolution-mock-provider-source".to_string(),
                conversation_id: None,
                contract_hash: "1".repeat(64),
                plan_hash: "2".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: opc_id.clone(),
                mission_intent: "Recover from prompt procedure drift".to_string(),
                selected_agents: vec!["agent-risk-01".to_string()],
                selected_executors: vec![],
                required_skills: vec!["skill-mission-draft".to_string()],
                track: "yellow".to_string(),
                status: WorkOrderStatus::Failed,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec!["delete".to_string()],
                risk_summary: "prompt procedure drift caused the governed task to fail".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .await
        .unwrap();
        company_pool.close().await;

        let (status, Json(body)) =
            run_evolution(State(state), legacy_company_headers(&opc_id)).await;

        assert_eq!(status, StatusCode::BAD_GATEWAY, "{body:?}");
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("provider returned no valid structured proposal"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn yellow_execute_rejects_arbitrary_identity_string_as_approval_receipt() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        let work_order_id = "wo-yellow-no-freeform-proof";
        let contract_hash = "f".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let create = CreateWORequest {
            work_order_id: Some(work_order_id.to_string()),
            conversation_id: None,
            contract_hash,
            plan_hash: "d".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: opc_id.clone(),
            mission_intent: "Draft an internal update".to_string(),
            selected_agents: vec!["agent-risk-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            governance_proposal: None,
        };
        let (create_status, Json(created)) = create_work_order(
            legacy_company_headers(&opc_id),
            State(state.clone()),
            Json(create),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);
        assert_eq!(created["track"], "yellow");

        let (status, Json(body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn yellow_execute_rejects_expired_denied_or_wrong_action_receipts() {
        let (state, root, opc_id) = seeded_legacy_company_state().await;
        let pool = state.pool.clone();
        let contract_hash = "e".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let expired_wo = "wo-yellow-expired-receipt";
        create_yellow_work_order(state.clone(), &opc_id, expired_wo, &contract_hash).await;
        let expired_id = ApprovalRepo::create(
            &pool,
            &opc_id,
            &contract_hash,
            &format!("urn:coevo:work-order:{}:execute", expired_wo),
            "NEGATIVE_CONSENT",
            "default-founder",
            -1,
        )
        .await
        .unwrap();
        let (expired_status, Json(expired_body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        create_yellow_work_order(state.clone(), &opc_id, denied_wo, &contract_hash).await;
        let denied_id = ApprovalRepo::create(
            &pool,
            &opc_id,
            &contract_hash,
            &format!("urn:coevo:work-order:{}:execute", denied_wo),
            "NEGATIVE_CONSENT",
            "default-founder",
            300_000,
        )
        .await
        .unwrap();
        ApprovalRepo::deny(&pool, &opc_id, &denied_id, "default-founder")
            .await
            .unwrap();
        let (denied_status, Json(denied_body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        create_yellow_work_order(state.clone(), &opc_id, action_mismatch_wo, &contract_hash).await;
        let wrong_action_id = ApprovalRepo::create(
            &pool,
            &opc_id,
            &contract_hash,
            "urn:coevo:work-order:other-work-order:execute",
            "NEGATIVE_CONSENT",
            "default-founder",
            300_000,
        )
        .await
        .unwrap();
        ApprovalRepo::approve(&pool, &opc_id, &wrong_action_id, "default-founder")
            .await
            .unwrap();
        let (mismatch_status, Json(mismatch_body)) = execute_work_order(
            legacy_company_headers(&opc_id),
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
        std::fs::remove_dir_all(root).ok();
    }
}
