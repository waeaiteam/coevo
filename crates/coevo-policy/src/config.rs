use async_trait::async_trait;
use coevo_core::contract::MCLSpec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::traits::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub policy_id: String,
    pub max_risk_score: f64,
    pub required_data_boundary_prefixes: Vec<String>,
    pub allowed_action_urn_prefixes: Vec<String>,
    pub forbidden_action_urns: Vec<String>,
    pub forbidden_action_urn_prefixes: Vec<String>,
}

impl PolicyConfig {
    pub fn baseline() -> Self {
        Self {
            policy_id: "coevo-config-baseline-v1".to_string(),
            max_risk_score: 0.8,
            required_data_boundary_prefixes: vec!["urn:coevo:data:".to_string()],
            allowed_action_urn_prefixes: vec!["urn:coevo:action:".to_string()],
            forbidden_action_urns: vec![
                "urn:coevo:action:production:drop_database".to_string(),
                "urn:coevo:action:production:delete_customer_data".to_string(),
            ],
            forbidden_action_urn_prefixes: vec![
                "urn:coevo:action:production:destructive:".to_string(),
                "urn:coevo:action:secrets:exfiltrate".to_string(),
            ],
        }
    }

    pub fn from_json(value: &str) -> Result<Self, PolicyEngineError> {
        serde_json::from_str(value).map_err(|err| PolicyEngineError::Internal(err.to_string()))
    }
}

pub struct ConfigDrivenPolicyEngine {
    config: PolicyConfig,
    policy_version: String,
}

impl ConfigDrivenPolicyEngine {
    pub fn baseline() -> Self {
        Self::new(PolicyConfig::baseline())
    }

    pub fn from_env_or_baseline() -> Self {
        match Self::from_env() {
            Ok(Some(engine)) => engine,
            Ok(None) => Self::baseline(),
            Err(err) => {
                tracing::error!(error = %err, "failed to load COEVO policy config; using fail-closed config");
                Self::new(PolicyConfig {
                    policy_id: "coevo-policy-config-load-failed".to_string(),
                    max_risk_score: -1.0,
                    required_data_boundary_prefixes: vec!["urn:coevo:data:".to_string()],
                    allowed_action_urn_prefixes: vec![],
                    forbidden_action_urns: vec![],
                    forbidden_action_urn_prefixes: vec!["urn:".to_string()],
                })
            }
        }
    }

    pub fn from_env() -> Result<Option<Self>, PolicyEngineError> {
        if let Ok(raw) = std::env::var("COEVO_POLICY_CONFIG_JSON") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Ok(Some(Self::new(PolicyConfig::from_json(trimmed)?)));
            }
        }
        if let Ok(path) = std::env::var("COEVO_POLICY_CONFIG_PATH") {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                let raw = std::fs::read_to_string(trimmed)
                    .map_err(|err| PolicyEngineError::Internal(err.to_string()))?;
                return Ok(Some(Self::new(PolicyConfig::from_json(&raw)?)));
            }
        }
        Ok(None)
    }

    pub fn new(config: PolicyConfig) -> Self {
        let policy_version = policy_version(&config);
        Self {
            config,
            policy_version,
        }
    }

    fn violation(&self, policy_urn: &str, description: impl Into<String>) -> PolicyViolation {
        PolicyViolation {
            policy_urn: policy_urn.to_string(),
            description: description.into(),
            remediation: Some(
                "update the company policy config or reduce the requested authority".to_string(),
            ),
        }
    }

    fn action_forbidden(&self, action_urn: &str) -> bool {
        self.config
            .forbidden_action_urns
            .iter()
            .any(|forbidden| forbidden == action_urn)
            || self
                .config
                .forbidden_action_urn_prefixes
                .iter()
                .any(|prefix| action_urn.starts_with(prefix))
    }

    fn action_allowed_by_prefix(&self, action_urn: &str) -> bool {
        !self.config.allowed_action_urn_prefixes.is_empty()
            && self
                .config
                .allowed_action_urn_prefixes
                .iter()
                .any(|prefix| action_urn.starts_with(prefix))
    }

    fn checked_policies() -> Vec<String> {
        vec![
            "urn:coevo:policy:data-boundary".to_string(),
            "urn:coevo:policy:risk-tolerance".to_string(),
            "urn:coevo:policy:allowed-action".to_string(),
            "urn:coevo:policy:forbidden-action".to_string(),
        ]
    }
}

fn policy_version(config: &PolicyConfig) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(config).unwrap_or_default());
    hex::encode(hasher.finalize())
}

#[async_trait]
impl PolicyEngine for ConfigDrivenPolicyEngine {
    async fn validate_contract(
        &self,
        contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError> {
        let mut violations = Vec::new();

        if contract.risk_tolerance_profile.max_risk_score > self.config.max_risk_score {
            violations.push(self.violation(
                "urn:coevo:policy:risk-tolerance",
                format!(
                    "risk tolerance {} exceeds configured maximum {}",
                    contract.risk_tolerance_profile.max_risk_score, self.config.max_risk_score
                ),
            ));
        }

        for boundary in &contract.data_boundary {
            let covered = self
                .config
                .required_data_boundary_prefixes
                .iter()
                .any(|prefix| boundary.starts_with(prefix));
            if !covered {
                violations.push(self.violation(
                    "urn:coevo:policy:data-boundary",
                    format!("data boundary '{boundary}' is outside configured policy scope"),
                ));
            }
        }

        for forbidden in &contract
            .responsibility_anchor_policy
            .agent_forbidden_actions
        {
            if self.action_forbidden(forbidden) {
                violations.push(self.violation(
                    "urn:coevo:policy:forbidden-action",
                    format!("contract includes forbidden agent action '{forbidden}'"),
                ));
            }
        }

        Ok(PolicyResult {
            passed: violations.is_empty(),
            violations,
            policy_version: self.policy_version.clone(),
            policies_checked: Self::checked_policies(),
        })
    }

    async fn dry_run(&self, contract: &MCLSpec) -> Result<PolicyResult, PolicyEngineError> {
        self.validate_contract(contract).await
    }

    fn policy_version(&self) -> String {
        self.policy_version.clone()
    }

    async fn evaluate_action(
        &self,
        action_urn: &str,
        _contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError> {
        let mut violations = Vec::new();
        if self.action_forbidden(action_urn) {
            violations.push(self.violation(
                "urn:coevo:policy:forbidden-action",
                format!("action '{action_urn}' is forbidden by policy"),
            ));
        } else if !self.action_allowed_by_prefix(action_urn) {
            violations.push(self.violation(
                "urn:coevo:policy:allowed-action",
                format!("action '{action_urn}' is outside configured allowed prefixes"),
            ));
        }

        Ok(PolicyResult {
            passed: violations.is_empty(),
            violations,
            policy_version: self.policy_version.clone(),
            policies_checked: vec![
                "urn:coevo:policy:allowed-action".to_string(),
                "urn:coevo:policy:forbidden-action".to_string(),
            ],
        })
    }

    async fn diff_policies(
        &self,
        old_version: &str,
        new_version: &str,
    ) -> Result<PolicyDiff, PolicyEngineError> {
        Ok(PolicyDiff {
            added_rules: if old_version != new_version {
                vec![new_version.to_string()]
            } else {
                vec![]
            },
            removed_rules: if old_version != new_version {
                vec![old_version.to_string()]
            } else {
                vec![]
            },
            modified_rules: vec![],
            affected_agents: vec![],
        })
    }

    async fn health_check(&self) -> Result<bool, PolicyEngineError> {
        Ok(!self.policy_version.is_empty())
    }

    async fn rollback(&mut self, target_version: &str) -> Result<(), PolicyEngineError> {
        if target_version == self.policy_version {
            Ok(())
        } else {
            Err(PolicyEngineError::VersionNotFound(
                target_version.to_string(),
            ))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use coevo_core::contract::{
        ActionMode, ApprovalMode, ContractState, EvidenceRequirement, GoalNode, GoalStatus,
        GoalTree, HumanApprovalPolicy, ResponsibilityAnchorPolicy, RiskToleranceProfile,
        TerminationPolicy,
    };

    fn contract(risk: f64, data_boundary: Vec<&str>) -> MCLSpec {
        MCLSpec {
            mcl_version: "1.0".to_string(),
            mcl_state: ContractState::ActiveContract,
            parent_contract_hash: "0".repeat(64),
            goal_tree: GoalTree {
                root: GoalNode {
                    id: "goal".to_string(),
                    description: "test".to_string(),
                    status: GoalStatus::InProgress,
                    children: vec![],
                    depends_on: vec![],
                },
            },
            institution_policy_hash: "p".repeat(64),
            data_boundary: data_boundary.into_iter().map(str::to_string).collect(),
            allowed_action_modes: vec![ActionMode::DraftOnly],
            human_approval_policy: HumanApprovalPolicy {
                approval_mode: ApprovalMode::ExplicitApproval,
                authorized_roles: vec!["founder".to_string()],
                negative_consent_timeout_secs: 0,
                mfa_auth_url: None,
            },
            evidence_requirement: EvidenceRequirement {
                minimum_level: "self_report".to_string(),
                require_json_report: false,
            },
            risk_tolerance_profile: RiskToleranceProfile {
                max_risk_score: risk,
                allow_emergency_lease: false,
            },
            termination_policy: TerminationPolicy {
                max_token_budget: 1024,
                max_hops: 4,
                max_latency_ms: 1000,
                max_stance_rounds: 4,
            },
            responsibility_anchor_policy: ResponsibilityAnchorPolicy {
                required_human_roles: vec!["founder".to_string()],
                agent_forbidden_actions: vec![],
            },
        }
    }

    #[tokio::test]
    async fn baseline_allows_low_risk_workspace_contract() {
        let engine = ConfigDrivenPolicyEngine::baseline();
        let result = engine
            .validate_contract(&contract(0.3, vec!["urn:coevo:data:workspace"]))
            .await
            .unwrap();
        assert!(result.passed, "{result:?}");
    }

    #[tokio::test]
    async fn baseline_denies_excessive_risk() {
        let engine = ConfigDrivenPolicyEngine::baseline();
        let result = engine
            .validate_contract(&contract(0.95, vec!["urn:coevo:data:workspace"]))
            .await
            .unwrap();
        assert!(!result.passed);
        assert!(result
            .violations
            .iter()
            .any(|v| v.policy_urn == "urn:coevo:policy:risk-tolerance"));
    }

    #[tokio::test]
    async fn baseline_denies_forbidden_action() {
        let engine = ConfigDrivenPolicyEngine::baseline();
        let result = engine
            .evaluate_action(
                "urn:coevo:action:production:drop_database",
                &contract(0.3, vec!["urn:coevo:data:workspace"]),
            )
            .await
            .unwrap();
        assert!(!result.passed);
        assert!(result
            .violations
            .iter()
            .any(|v| v.policy_urn == "urn:coevo:policy:forbidden-action"));
    }
}
