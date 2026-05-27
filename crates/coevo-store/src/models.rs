//! Database row models — thin wrappers matching SQL table schemas.

use serde::{Deserialize, Serialize};

// ---- Contracts ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ContractRow {
    pub id: String,
    pub contract_hash: String,
    pub mcl_version: String,
    pub mcl_state: String,
    pub parent_contract_hash: String,
    pub goal_tree_json: String,
    pub institution_policy_hash: String,
    pub data_boundary_json: String,
    pub allowed_action_modes_json: String,
    pub human_approval_policy_json: String,
    pub evidence_requirement_json: String,
    pub risk_tolerance_profile_json: String,
    pub termination_policy_json: String,
    pub responsibility_anchor_policy_json: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

// ---- Execution Plans ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExecutionPlanRow {
    pub id: String,
    pub plan_hash: String,
    pub contract_hash: String,
    pub execution_plan_version: String,
    pub parent_plan_hash: String,
    pub primary_path_dag_json: String,
    pub agent_configs_json: String,
    pub failback_rules_json: String,
    pub hard_resource_ceilings_json: String,
    pub exploration_budget_quota: f64,
    pub created_at_ms: i64,
}

// ---- Agent Registry ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: String,
    pub agent_id: String,
    pub passport_json: String,
    pub capabilities_json: String,
    pub status: String,
    pub registered_at_ms: i64,
}

// ---- Blackboard ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BlackboardEntryRow {
    pub id: String,
    pub entry_key: String,
    pub version: i64,
    pub value_json: String,
    pub cognitive_layer: String,
    pub source_agent_id: String,
    pub contract_hash: String,
    pub is_valid: i64,
    pub created_at_ms: i64,
    pub expires_at_ms: Option<i64>,
}

// ---- Provenance ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProvenanceEnvelopeRow {
    pub id: String,
    pub entry_id: String,
    pub source_agent_id: String,
    pub verification_tool_urn: String,
    pub environmental_scope_json: String,
    pub ttl_seconds: i64,
    pub cryptographic_signature: String,
    pub verification_report_json: Option<String>,
    pub created_at: String,
}

// ---- Cognitive Edges ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CognitiveEdgeRow {
    pub id: String,
    pub source_entry_id: String,
    pub target_entry_id: String,
    pub edge_type: String,
    pub created_at_ms: i64,
}

// ---- Risk Decisions ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RiskDecisionRow {
    pub id: String,
    pub decision_id: String,
    pub contract_hash: String,
    pub agent_id: String,
    pub action_urn: String,
    pub decision: String,
    pub required_confidence: f64,
    pub available_confidence: f64,
    pub action_risk: f64,
    pub inaction_risk: f64,
    pub reason: String,
    pub decided_at_ms: i64,
}

// ---- ADR Records ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdrRow {
    pub id: String,
    pub decision_id: String,
    pub mcl_reference: String,
    pub proposer_agent: String,
    pub critic_objections_json: String,
    pub blocker_conflict_status: String,
    pub selected_option: String,
    pub rejected_alternatives_json: String,
    pub risk_accepted_json: String,
    pub human_override_reason: Option<String>,
    pub responsibility_anchor_json: String,
    pub follow_up_monitoring_plan: Option<String>,
    pub post_execution_feedback_json: Option<String>,
    pub created_at_ms: i64,
}

// ---- Reputation ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReputationVectorRow {
    pub id: String,
    pub agent_id: String,
    pub task_domain_competence: f64,
    pub uncertainty_honesty: f64,
    pub policy_compliance: f64,
    pub resource_efficiency: f64,
    pub task_count: i64,
    pub high_difficulty_avoidance_count: i32,
    pub last_updated_ms: i64,
}

// ---- Audit ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEventRow {
    pub id: String,
    pub event_type: String,
    pub contract_hash: Option<String>,
    pub agent_id: Option<String>,
    pub traceparent: Option<String>,
    pub tenant_id: String,
    pub event_data_json: String,
    pub recorded_at_ms: i64,
}

// ---- Leases ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LeaseRow {
    pub id: String,
    pub lease_id: String,
    pub contract_hash: String,
    pub agent_id: String,
    pub lease_scope_json: String,
    pub lease_budget: i32,
    pub operations_used: i32,
    pub granted_at_ms: i64,
    pub expires_at_ms: i64,
    pub ttl_ms: i64,
    pub monitoring_signature: String,
    pub diagnostic_signature: String,
    pub is_active: i64,
    pub was_revoked: i64,
}

// ---- Approval Requests ----

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApprovalRequestRow {
    pub id: String,
    pub contract_hash: String,
    pub action_urn: String,
    pub approval_mode: String,
    pub status: String,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub requested_at_ms: i64,
    pub expires_at_ms: i64,
    pub decided_at_ms: Option<i64>,
}
