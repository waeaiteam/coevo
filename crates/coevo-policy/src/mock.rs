//! Mock policy engine implementation for development and testing.
//! Simulates an OPA WebAssembly reference profile.

use async_trait::async_trait;
use coevo_core::contract::MCLSpec;
use sha2::{Digest, Sha256};

use crate::traits::*;

/// A mock policy engine that applies simple rule checks.
/// In production this would be replaced with OPA Wasm.
pub struct MockPolicyEngine {
    policy_version: String,
    /// Forbidden action URNs for testing.
    forbidden_actions: Vec<String>,
    /// Required data boundary prefixes.
    required_boundaries: Vec<String>,
}

impl MockPolicyEngine {
    pub fn new() -> Self {
        // Generate a deterministic mock policy version
        let mut hasher = Sha256::new();
        hasher.update(b"coevo-mock-policy-v1");
        let hash = hex::encode(hasher.finalize());
        Self {
            policy_version: hash,
            forbidden_actions: vec![
                "urn:coevo:action:production:drop_database".to_string(),
                "urn:coevo:action:production:delete_customer_data".to_string(),
            ],
            required_boundaries: vec!["urn:coevo:data:".to_string()],
        }
    }

    /// Set forbidden actions for testing.
    pub fn with_forbidden_actions(mut self, actions: Vec<String>) -> Self {
        self.forbidden_actions = actions;
        self
    }
}

impl Default for MockPolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PolicyEngine for MockPolicyEngine {
    async fn validate_contract(
        &self,
        contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError> {
        let mut violations: Vec<PolicyViolation> = vec![];

        // Check data boundaries
        for boundary in &contract.data_boundary {
            let covered = self
                .required_boundaries
                .iter()
                .any(|rb| boundary.starts_with(rb.as_str()));
            if !covered {
                violations.push(PolicyViolation {
                    policy_urn: "urn:coevo:policy:data-boundary".to_string(),
                    description: format!("Data boundary '{}' not in required scope", boundary),
                    remediation: Some("Add urn:coevo:data:* boundary".to_string()),
                });
            }
        }

        // Check forbidden actions
        for action in &contract.allowed_action_modes {
            let action_str = serde_json::to_string(action).unwrap();
            for forbidden in &self.forbidden_actions {
                if action_str.contains(forbidden) {
                    violations.push(PolicyViolation {
                        policy_urn: "urn:coevo:policy:forbidden-action".to_string(),
                        description: format!(
                            "Action '{}' matches forbidden action '{}'",
                            action_str, forbidden
                        ),
                        remediation: Some("Remove forbidden action from contract".to_string()),
                    });
                }
            }
        }

        // Check risk tolerance
        if contract.risk_tolerance_profile.max_risk_score > 0.8 {
            violations.push(PolicyViolation {
                policy_urn: "urn:coevo:policy:risk-tolerance".to_string(),
                description: format!(
                    "Risk tolerance {} exceeds maximum 0.8",
                    contract.risk_tolerance_profile.max_risk_score
                ),
                remediation: Some("Lower max_risk_score to <= 0.8".to_string()),
            });
        }

        Ok(PolicyResult {
            passed: violations.is_empty(),
            violations,
            policy_version: self.policy_version.clone(),
            policies_checked: vec![
                "urn:coevo:policy:data-boundary".to_string(),
                "urn:coevo:policy:forbidden-action".to_string(),
                "urn:coevo:policy:risk-tolerance".to_string(),
            ],
        })
    }

    async fn dry_run(&self, contract: &MCLSpec) -> Result<PolicyResult, PolicyEngineError> {
        // Dry-run is identical to validate but marked as simulation
        let result = self.validate_contract(contract).await?;
        // In real OPA, dry-run would bypass enforcement
        Ok(result)
    }

    fn policy_version(&self) -> String {
        self.policy_version.clone()
    }

    async fn evaluate_action(
        &self,
        action_urn: &str,
        _contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError> {
        let violated = self.forbidden_actions.contains(&action_urn.to_string());
        Ok(PolicyResult {
            passed: !violated,
            violations: if violated {
                vec![PolicyViolation {
                    policy_urn: "urn:coevo:policy:forbidden-action".to_string(),
                    description: format!("Action '{}' is forbidden", action_urn),
                    remediation: None,
                }]
            } else {
                vec![]
            },
            policy_version: self.policy_version.clone(),
            policies_checked: vec!["urn:coevo:policy:forbidden-action".to_string()],
        })
    }

    async fn diff_policies(
        &self,
        _old_version: &str,
        _new_version: &str,
    ) -> Result<PolicyDiff, PolicyEngineError> {
        Ok(PolicyDiff {
            added_rules: vec![],
            removed_rules: vec![],
            modified_rules: vec![],
            affected_agents: vec![],
        })
    }

    async fn health_check(&self) -> Result<bool, PolicyEngineError> {
        Ok(true)
    }

    async fn rollback(&mut self, target_version: &str) -> Result<(), PolicyEngineError> {
        // Mock rollback — just update version
        self.policy_version = target_version.to_string();
        Ok(())
    }
}
