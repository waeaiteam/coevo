//! Pluggable policy engine trait — Institution Policy enforcement.
//! Per coevo whitepaper Section 5.

use async_trait::async_trait;
use coevo_core::contract::MCLSpec;
use serde::{Deserialize, Serialize};

/// Result of a policy evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyResult {
    /// Whether the contract passes policy validation.
    pub passed: bool,
    /// List of policy violations (if any).
    pub violations: Vec<PolicyViolation>,
    /// SHA256 of the policy bundle that was evaluated.
    pub policy_version: String,
    /// List of policy URNs that were checked.
    pub policies_checked: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyViolation {
    /// URN of the violated policy rule.
    pub policy_urn: String,
    /// Human-readable description of the violation.
    pub description: String,
    /// Suggested remediation.
    pub remediation: Option<String>,
}

/// The pluggable policy engine trait.
/// Any implementation must satisfy the 8 runtime protocol features
/// defined in coevo whitepaper Section 5.
#[async_trait]
pub trait PolicyEngine: Send + Sync {
    /// Evaluate a contract against the institution policy.
    async fn validate_contract(
        &self,
        contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError>;

    /// Dry-run evaluation without side effects.
    async fn dry_run(&self, contract: &MCLSpec) -> Result<PolicyResult, PolicyEngineError>;

    /// Get the current policy bundle hash.
    fn policy_version(&self) -> String;

    /// Evaluate whether an action URN is allowed by policy.
    async fn evaluate_action(
        &self,
        action_urn: &str,
        contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError>;

    /// Diff two policy versions.
    async fn diff_policies(
        &self,
        old_version: &str,
        new_version: &str,
    ) -> Result<PolicyDiff, PolicyEngineError>;

    /// Health check.
    async fn health_check(&self) -> Result<bool, PolicyEngineError>;

    /// Rollback to a previous signed policy version.
    async fn rollback(&mut self, target_version: &str) -> Result<(), PolicyEngineError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDiff {
    pub added_rules: Vec<String>,
    pub removed_rules: Vec<String>,
    pub modified_rules: Vec<String>,
    pub affected_agents: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyEngineError {
    #[error("policy engine internal error: {0}")]
    Internal(String),
    #[error("policy version not found: {0}")]
    VersionNotFound(String),
    #[error("policy bundle verification failed")]
    BundleVerificationFailed,
    #[error("rollback timeout: {0}")]
    RollbackTimeout(String),
}
