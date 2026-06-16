//! Fail-closed policy engine.
//!
//! [`DenyAllPolicyEngine`] denies every contract and action. It is the
//! production default wherever a real OPA policy engine is not wired in: a
//! missing/unconfigured policy engine must never silently authorize work.
//! Callers select it instead of [`crate::mock::MockPolicyEngine`] unless running
//! under tests or with `COEVO_ENABLE_MOCK_POLICY_ENGINE=1`.
//!
//! This mirrors the `DenyAllPolicyEngine` used by `coevo-worker`'s GovernGate so
//! that the track runners enforce the same fail-closed default without taking a
//! dependency on the worker crate.

use async_trait::async_trait;
use coevo_core::contract::MCLSpec;

use crate::traits::*;

/// A policy engine that denies everything. Fail-closed default.
pub struct DenyAllPolicyEngine;

impl DenyAllPolicyEngine {
    const VERSION: &'static str = "unavailable";

    fn denied(policy_urn: &str) -> PolicyResult {
        PolicyResult {
            passed: false,
            violations: vec![PolicyViolation {
                policy_urn: policy_urn.to_string(),
                description: "policy engine unavailable (fail-closed deny)".to_string(),
                remediation: Some(
                    "configure a real policy engine, or set \
                     COEVO_ENABLE_MOCK_POLICY_ENGINE=1 for dev/test"
                        .to_string(),
                ),
            }],
            policy_version: Self::VERSION.to_string(),
            policies_checked: vec![policy_urn.to_string()],
        }
    }
}

#[async_trait]
impl PolicyEngine for DenyAllPolicyEngine {
    async fn validate_contract(
        &self,
        _contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError> {
        Ok(Self::denied("urn:coevo:policy:unavailable"))
    }

    async fn dry_run(&self, contract: &MCLSpec) -> Result<PolicyResult, PolicyEngineError> {
        self.validate_contract(contract).await
    }

    fn policy_version(&self) -> String {
        Self::VERSION.to_string()
    }

    async fn evaluate_action(
        &self,
        action_urn: &str,
        _contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError> {
        Ok(Self::denied(action_urn))
    }

    async fn diff_policies(
        &self,
        _old_version: &str,
        _new_version: &str,
    ) -> Result<PolicyDiff, PolicyEngineError> {
        Err(PolicyEngineError::Internal(
            "policy engine unavailable".to_string(),
        ))
    }

    async fn health_check(&self) -> Result<bool, PolicyEngineError> {
        Ok(false)
    }

    async fn rollback(&mut self, _target_version: &str) -> Result<(), PolicyEngineError> {
        Err(PolicyEngineError::Internal(
            "policy engine unavailable".to_string(),
        ))
    }
}
