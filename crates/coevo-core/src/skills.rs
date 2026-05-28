//! coevo Agent Skill Package + Skill Evolution model.
//! Skills are versioned, testable, verifiable, rollback-capable capability bundles.
//! Skill Evolution must not bypass MCL, RiskGate, Cognitive Customs, or ADR-A.

use serde::{Deserialize, Serialize};

// ---- Agent Skill Package ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkillPackage {
    pub skill_id: String,
    pub name: String,
    pub version: String,
    pub owner_agent_id: String,
    pub department: String,
    pub description: String,
    pub trigger_patterns: Vec<String>,
    pub applicable_domains: Vec<String>,
    pub required_tools: Vec<String>,
    pub required_model_profile: Option<super::opc::ModelProviderProfile>,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub prompt_template: String,
    pub procedure_steps: Vec<String>,
    pub guardrails: Vec<String>,
    pub examples: Vec<serde_json::Value>,
    pub tests: Vec<SkillTestCase>,
    pub evals: Vec<SkillEvalResult>,
    pub permissions_required: Vec<String>,
    pub allowed_cognitive_layers: Vec<String>,
    pub allowed_action_modes: Vec<String>,
    pub risk_ceiling: f64,
    pub provenance: String,
    pub status: SkillStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Draft, Proposed, Verified, Approved, Active, Deprecated, Revoked,
}

// ---- Skill Test Case ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTestCase {
    pub test_id: String,
    pub description: String,
    pub input: serde_json::Value,
    pub expected_output_schema: serde_json::Value,
    pub forbidden_behaviors: Vec<String>,
    pub required_evidence: Vec<String>,
    pub pass_criteria: Vec<String>,
}

// ---- Skill Eval Result ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvalResult {
    pub eval_id: String,
    pub skill_id: String,
    pub version: String,
    pub run_id: String,
    pub passed: bool,
    pub score: f64,
    pub failures: Vec<String>,
    pub regression_detected: bool,
    pub verifier_notes: String,
    pub created_at_ms: u64,
}

// ---- Skill Evolution Proposal ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvolutionProposal {
    pub proposal_id: String,
    pub source_type: EvolutionSourceType,
    pub source_refs: Vec<String>,
    pub target_skill_id: String,
    pub proposal_type: EvolutionProposalType,
    pub diagnosis: String,
    pub proposed_changes: String,
    pub expected_benefit: String,
    pub risk_assessment: String,
    pub generated_tests: Vec<SkillTestCase>,
    pub status: EvolutionProposalStatus,
    pub created_by_agent: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionSourceType {
    Failure, RepeatedSuccess, UserFeedback, AgentReflection,
    CriticObjection, ADRFeedback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionProposalType {
    CreateNewSkill, PatchSkill, DeprecateSkill, SplitSkill, MergeSkills,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionProposalStatus {
    Draft, UnderVerification, NeedsHumanReview, Approved, Rejected, Applied,
}

// ---- Skill Version Record ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionRecord {
    pub skill_id: String,
    pub version: String,
    pub parent_version: String,
    pub diff_summary: String,
    pub change_reason: String,
    pub verifier_result: Option<SkillEvalResult>,
    pub approved_by: Option<String>,
    pub rollback_available: bool,
    pub created_at_ms: u64,
}

// ---- Skill Evolution Policy ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvolutionPolicy {
    pub auto_propose_enabled: bool,
    pub auto_apply_green_skills: bool,
    pub require_human_review_above_risk: f64,
    pub min_evals_before_activation: u32,
    pub regression_threshold: f64,
    pub allowed_skill_writers: Vec<String>,
    pub forbidden_domains: Vec<String>,
    pub max_daily_skill_updates: u32,
}

// ---- Failure Analysis ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAnalysis {
    pub category: FailureCategory,
    pub root_cause: String,
    pub suspected_missing_skill: Option<String>,
    pub suspected_skill_bug: Option<String>,
    pub required_memory_update: bool,
    pub required_policy_update: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    MissingCapability, WrongToolUse, BadPromptProcedure,
    InsufficientEvidence, PolicyViolation, MemoryStale,
    ExternalExecutorFailure, HallucinatedFact,
    OverConfidentDecision, UserPreferenceMismatch,
}
