//! MCLSpec — Mission Contract Language specification types.
//! Per coevo whitepaper Sections 3 & 4.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The mission contract specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCLSpec {
    pub mcl_version: String,
    pub mcl_state: ContractState,
    /// SHA256 of parent contract; zero-hash for v1.
    pub parent_contract_hash: String,
    /// Hierarchical goal decomposition with leaf dependencies.
    pub goal_tree: GoalTree,
    /// SHA256 of the institution policy this contract inherits.
    pub institution_policy_hash: String,
    /// Allowed data domain URN whitelist.
    pub data_boundary: Vec<String>,
    /// Permitted action modes for this contract.
    pub allowed_action_modes: Vec<ActionMode>,
    /// Human approval workflow configuration.
    pub human_approval_policy: HumanApprovalPolicy,
    /// Required evidence level for fact promotion.
    pub evidence_requirement: EvidenceRequirement,
    /// Risk tolerance configuration.
    pub risk_tolerance_profile: RiskToleranceProfile,
    /// Termination bounds (token, hops, time, rounds).
    pub termination_policy: TerminationPolicy,
    /// Human responsibility anchoring.
    pub responsibility_anchor_policy: ResponsibilityAnchorPolicy,
}

pub fn hash_contract(contract: &MCLSpec) -> Result<String, serde_json::Error> {
    let contract_json = serde_json::to_string(contract)?;
    let mut hasher = Sha256::new();
    hasher.update(contract_json.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

/// MCL contract lifecycle states. Transition is strictly one-way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ContractState {
    DraftContract,
    ValidatedContract,
    ActiveContract,
    SuspendedContract,
    ClosedContract,
}

impl ContractState {
    pub fn can_transition_to(&self, target: ContractState) -> bool {
        matches!(
            (self, target),
            (
                ContractState::DraftContract,
                ContractState::ValidatedContract
            ) | (
                ContractState::ValidatedContract,
                ContractState::ActiveContract
            ) | (
                ContractState::ActiveContract,
                ContractState::SuspendedContract
            ) | (ContractState::ActiveContract, ContractState::ClosedContract)
                | (
                    ContractState::SuspendedContract,
                    ContractState::ActiveContract
                )
                | (
                    ContractState::SuspendedContract,
                    ContractState::ClosedContract
                )
        )
    }
}

/// Goal decomposition tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalTree {
    pub root: GoalNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalNode {
    pub id: String,
    pub description: String,
    pub status: GoalStatus,
    pub children: Vec<GoalNode>,
    /// Dependencies on sibling goals for this node to be achievable.
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Pending,
    InProgress,
    Achieved,
    Blocked,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionMode {
    DraftOnly,
    MutableWrite,
    CommitReady,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanApprovalPolicy {
    /// NEGATIVE_CONSENT: auto-approve after timeout window.
    /// EXPLICIT_APPROVAL: require explicit human sign-off.
    pub approval_mode: ApprovalMode,
    /// Human roles authorized to approve.
    pub authorized_roles: Vec<String>,
    /// Timeout in seconds before negative-consent auto-approves.
    pub negative_consent_timeout_secs: u64,
    /// URL for MFA challenge.
    pub mfa_auth_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    NegativeConsent,
    ExplicitApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRequirement {
    /// Minimum MCP verification level required (e.g., "unit_tests_passing", "integration_verified").
    pub minimum_level: String,
    /// Whether a JSON verification report must be attached.
    pub require_json_report: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskToleranceProfile {
    /// Maximum acceptable risk score (0.0 to 1.0).
    pub max_risk_score: f64,
    /// Whether emergency self-healing lease can be activated.
    pub allow_emergency_lease: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminationPolicy {
    /// Maximum token consumption.
    pub max_token_budget: u64,
    /// Maximum agent hops in the execution DAG.
    pub max_hops: u32,
    /// Maximum execution clock time in milliseconds.
    pub max_latency_ms: u64,
    /// Maximum rounds of stance divergence before forced resolution.
    pub max_stance_rounds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsibilityAnchorPolicy {
    /// Roles that must anchor high-risk actions (e.g., "CISO", "SRE_Lead").
    pub required_human_roles: Vec<String>,
    /// Actions that are forbidden for agents regardless of role.
    pub agent_forbidden_actions: Vec<String>,
}
