//! Gate decision types and ADR-A specification.
//! Per coevo whitepaper Sections 9 & 10.

use serde::{Deserialize, Serialize};

/// Risk gate decision output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateDecisionSpec {
    /// The gate verdict.
    pub decision: GateDecision,
    /// Required confidence level for this action.
    pub required_confidence: f64,
    /// Available confidence from the support/opposition calculation.
    pub available_confidence: f64,
    /// Computed action risk score.
    pub action_risk: f64,
    /// Computed inaction risk score.
    pub inaction_risk: f64,
    /// Human-readable reason.
    pub reason: String,
    /// If APPROVAL_REQUIRED, URL for MFA challenge.
    pub mfa_auth_url: Option<String>,
    /// If APPROVAL_REQUIRED, polling URL for status.
    pub task_status_url: Option<String>,
    /// If ALLOW_WITH_LEASE, the lease details.
    pub lease: Option<EmergencyLeaseSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateDecision {
    Allow,
    Deny,
    RequireHumanApproval,
    DeferForMoreEvidence,
    AllowWithLease,
    EscalateToResolution,
}

/// Action proposal submitted for risk evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionProposalSpec {
    /// URN of the action.
    pub action_urn: String,
    /// Target environment (dev, staging, prod).
    pub target_environment: String,
    /// Action parameters.
    pub parameters: serde_json::Value,
    /// Whether this is declared as an emergency self-healing action.
    pub emergency_mode: bool,
    /// Impact radius (0-3).
    pub blast_radius: u8,
    /// Irreversibility score (0-3).
    pub irreversibility: u8,
    /// Environment sensitivity (0-3).
    pub environment_sensitivity: u8,
    /// Reversibility score (0-3, higher = harder to reverse).
    pub reversibility: u8,
}

impl ActionProposalSpec {
    /// Manually set risk factors (bypasses auto-classification).
    pub fn with_risk_factors(mut self, br: u8, ir: u8, es: u8, rv: u8) -> Self {
        self.blast_radius = br;
        self.irreversibility = ir;
        self.environment_sensitivity = es;
        self.reversibility = rv;
        self
    }
}

/// Emergency lease specification (from coevo-core/lease).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyLeaseSpec {
    pub lease_id: String,
    pub ttl_seconds: u64,
    pub lease_scope: Vec<String>,
    pub lease_budget: u32,
    pub monitoring_signature: String,
    pub diagnostic_signature: String,
}

// ---- ADR-A (Agent Decision Record) ----

/// ADR-A: full architectural decision record for audit and accountability.
/// Per coevo whitepaper Section 10.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdrA {
    /// Globally unique decision identifier.
    pub decision_id: String,
    /// SHA256 of the bound MCL contract.
    pub mcl_reference: String,
    /// Agent that proposed the action.
    pub proposer_agent: String,
    /// All critic objections with evidence chain URNs.
    pub critic_objections: Vec<CriticObjection>,
    /// Current conflict status.
    pub blocker_conflict_status: ConflictStatus,
    /// The selected decision option.
    pub selected_option: String,
    /// All rejected alternatives with structured reasons and evidence.
    pub rejected_alternatives: Vec<RejectedAlternative>,
    /// Residual risk explicitly accepted.
    pub risk_accepted: AcceptedRisk,
    /// Human override reason (if applicable), signed with private key.
    pub human_override_reason: Option<String>,
    /// Responsibility anchoring: human role + MFA signature.
    pub responsibility_anchor: ResponsibilityAnchor,
    /// Post-execution monitoring plan.
    pub follow_up_monitoring_plan: Option<String>,
    /// 24h post-execution feedback for reputation back-propagation.
    pub post_execution_feedback: Option<PostExecutionFeedback>,
    /// Creation timestamp (Unix ms).
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticObjection {
    pub critic_agent_id: String,
    pub objection_reason: String,
    pub evidence_chain_urns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictStatus {
    Consensus,
    TradeOff,
    Divergence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedAlternative {
    pub option_id: String,
    pub description: String,
    pub rejection_reason: String,
    pub evidence_chain: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedRisk {
    pub risk_description: String,
    pub risk_score: f64,
    pub mitigation_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsibilityAnchor {
    /// Required human role (e.g., "CISO", "Lead_SRE").
    pub human_role: String,
    /// MFA-verified digital signature fingerprint.
    pub mfa_signature_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostExecutionFeedback {
    pub actual_outcome: String,
    pub was_successful: bool,
    pub observed_risk_delta: f64,
    pub notes: String,
    pub recorded_at_ms: u64,
}
