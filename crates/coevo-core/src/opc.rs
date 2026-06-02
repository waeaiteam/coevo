//! coevo-opc types: User Profile, OPC Profile, Memory, Agent Employee,
//! External Executor, Work Order — the OPC OS data model.
//! Per coevo whitepaper governance: all execution must pass MCL, RiskGate,
//! Cognitive Customs, and ADR-A.

use serde::{Deserialize, Serialize};

// ---- User Profile ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub display_name: String,
    pub preferred_language: String,
    pub timezone: String,
    pub risk_preference: RiskPreference,
    pub default_mission_mode: MissionMode,
    pub long_term_goals: Vec<String>,
    pub business_domains: Vec<String>,
    pub communication_style: String,
    pub approval_preferences: ApprovalPreferences,
    pub data_boundaries: Vec<String>,
    pub budget_limits: BudgetLimits,
    pub favorite_tools: Vec<String>,
    pub active_projects: Vec<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskPreference {
    Conservative,
    Balanced,
    Aggressive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionMode {
    Auto,
    ReadOnly,
    Collaborative,
    HighRiskRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPreferences {
    pub auto_approve_below_risk: f64,
    pub require_explicit_for_yellow: bool,
    pub require_mfa_for_red: bool,
    pub negative_consent_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetLimits {
    pub max_cost_per_task_usd: f64,
    pub max_cost_per_day_usd: f64,
    pub max_agents_per_task: u32,
}

// ---- OPC Profile ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OPCProfile {
    pub opc_id: String,
    pub founder_user_id: String,
    pub name: String,
    pub mission: String,
    pub current_strategy: String,
    pub operating_principles: Vec<String>,
    pub active_projects: Vec<String>,
    pub asset_indexes: Vec<String>,
    pub policy_profile: String,
    pub memory_policy: MemoryPolicy,
    pub default_departments: Vec<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPolicy {
    pub fact_ttl_default_seconds: i64,
    pub require_provenance_for_fact: bool,
    pub auto_stale_days: u32,
}

// ---- Asset ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRecord {
    pub asset_id: String,
    pub opc_id: String,
    pub asset_type: AssetType,
    pub name: String,
    pub description: String,
    pub uri: String,
    pub tags: Vec<String>,
    pub owner: String,
    pub access_policy: String,
    pub provenance: String,
    pub status: AssetStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    Repository,
    Document,
    Website,
    Dataset,
    CredentialRef,
    BrandAsset,
    CustomerLead,
    Report,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Active,
    Archived,
    Revoked,
}

// ---- Memory Record ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub memory_id: String,
    pub scope: MemoryScope,
    pub owner_id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub source: String,
    pub provenance: String,
    pub confidence: f64,
    pub ttl_seconds: i64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub access_policy: String,
    pub status: MemoryStatus,
    pub cognitive_layer: super::cognitive::CognitiveLayer,
    pub linked_contract_hash: Option<String>,
    pub linked_plan_hash: Option<String>,
    pub linked_adr_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    User,
    Company,
    Agent,
    Task,
    Skill,
    Executor,
    Audit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Stale,
    Revoked,
}

// ---- Agent Employee ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEmployee {
    pub agent_id: String,
    pub display_name: String,
    pub department: Department,
    pub role: String,
    pub passport: AgentPassport,
    pub model_profile: ModelProviderProfile,
    pub tool_scopes: Vec<String>,
    pub memory_scope: MemoryScope,
    pub permission_boundary: PermissionBoundary,
    pub allowed_cognitive_layers: Vec<String>,
    pub allowed_action_modes: Vec<String>,
    pub risk_ceiling: f64,
    pub reputation_vector: super::reputation::ReputationVector,
    pub supervisor_agent_id: Option<String>,
    pub lifecycle_status: LifecycleStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Department {
    FounderOffice,
    Product,
    Engineering,
    Research,
    Growth,
    Finance,
    Legal,
    SRE,
    Design,
    Content,
    Governance,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPassport {
    pub passport_id: String,
    pub issued_by: String,
    pub roles: Vec<String>,
    pub capabilities: Vec<String>,
    pub restrictions: Vec<String>,
    pub expires_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionBoundary {
    pub max_risk_score: f64,
    pub can_write_fact: bool,
    pub can_write_decision: bool,
    pub can_access_network: bool,
    pub can_access_filesystem: bool,
    pub can_call_external_executor: bool,
    pub can_propose_skill: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Draft,
    Active,
    Suspended,
    Retired,
}

// ---- Model Provider Profile ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderProfile {
    pub provider: String,
    pub base_url: String,
    pub api_key_ref: String,
    pub default_model: String,
    pub fast_model: String,
    pub reasoning_model: String,
    pub structured_output_model: String,
    pub timeout_ms: u64,
    pub max_tokens: u32,
    pub max_cost_per_task_usd: f64,
}

// ---- Work Order Governance Proposal/Verdict ----
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyCeiling {
    ReadOnly,
    WorkspaceWrite,
    FullAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPreference {
    Fast,
    Standard,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceProposal {
    pub autonomy_ceiling: AutonomyCeiling,
    pub model_preference: ModelPreference,
    pub assigned_agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceVerdict {
    pub effective_track: String,
    pub effective_tier: AutonomyCeiling,
    pub requested_ceiling: AutonomyCeiling,
    pub downgraded: bool,
    pub downgrade_reason: Option<String>,
    pub blocked: bool,
    pub block_reason: Option<String>,
    pub resolved_agent_id: Option<String>,
}

// ---- Agent Memory ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemory {
    pub agent_id: String,
    pub memory_records: Vec<String>,
    pub working_preferences: String,
    pub learned_constraints: Vec<String>,
    pub recurring_failures: Vec<String>,
    pub successful_patterns: Vec<String>,
    pub recent_tasks: Vec<String>,
    pub performance_notes: String,
    pub skill_usage_stats: String,
}

// ---- External Executor ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalExecutorPassport {
    pub executor_id: String,
    pub display_name: String,
    pub source_type: ExecutorSourceType,
    pub runtime_endpoint: String,
    pub capabilities: Vec<String>,
    pub required_credentials: Vec<String>,
    pub permission_boundary: PermissionBoundary,
    pub file_scope: Vec<String>,
    pub network_scope: Vec<String>,
    pub memory_scope: MemoryScope,
    pub risk_ceiling: f64,
    pub supported_actions: Vec<String>,
    pub sandbox_level: SandboxLevel,
    pub health_check_url: String,
    pub audit_callback_url: String,
    pub status: ExecutorStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorSourceType {
    Hermes,
    OpenClaw,
    MCP,
    Local302AI,
    LocalProcess,
    Browser,
    Docker,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxLevel {
    None,
    Process,
    Container,
    VM,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorStatus {
    Draft,
    Registered,
    Disabled,
}

// ---- Work Order ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkOrder {
    pub work_order_id: String,
    pub conversation_id: Option<String>,
    pub contract_hash: String,
    pub plan_hash: String,
    pub user_id: String,
    pub opc_id: String,
    pub mission_intent: String,
    pub selected_agents: Vec<String>,
    pub selected_executors: Vec<String>,
    pub required_skills: Vec<String>,
    pub track: String,
    pub status: WorkOrderStatus,
    pub allowed_actions: Vec<String>,
    pub restricted_actions: Vec<String>,
    pub risk_summary: String,
    pub governance_proposal: Option<GovernanceProposal>,
    pub governance_verdict: Option<GovernanceVerdict>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkOrderStatus {
    Draft,
    Planned,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
    Blocked,
}
