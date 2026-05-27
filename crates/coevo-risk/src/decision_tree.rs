//! Risk Gate decision tree — Rule-First, Number-Second.
//! Per coevo whitepaper Section 9.2.

use coevo_core::contract::MCLSpec;
use coevo_core::decision::{ActionProposalSpec, GateDecision, GateDecisionSpec};
use coevo_policy::traits::PolicyEngine;

use crate::quantify::*;

/// The Risk Gate — physical circuit breaker for agent actions.
pub struct RiskGate {
    policy_engine: Box<dyn PolicyEngine>,
}

impl RiskGate {
    pub fn new(policy_engine: Box<dyn PolicyEngine>) -> Self {
        Self { policy_engine }
    }

    /// Evaluate an action proposal and return a gating decision.
    /// Follows the Rule-First, Number-Second decision tree.
    pub async fn evaluate(
        &self,
        action: &ActionProposalSpec,
        contract: &MCLSpec,
        support_reputations: &[f64],
        support_evidence: &[f64],
        opposition_reputations: &[f64],
        opposition_evidence: &[f64],
        service_impact: f64,
        time_criticality: f64,
        failure_propagation: f64,
        has_veto: bool,
    ) -> GateDecisionSpec {
        let action_risk = compute_action_risk(action);
        let inaction_risk =
            compute_inaction_risk(service_impact, time_criticality, failure_propagation);
        let required_conf = required_confidence(action_risk);

        // ---- Layer 1: OPA Policy Filter ----
        let policy_result = self
            .policy_engine
            .evaluate_action(&action.action_urn, contract)
            .await;
        if let Ok(result) = policy_result {
            if !result.passed {
                return GateDecisionSpec {
                    decision: GateDecision::Deny,
                    required_confidence: required_conf,
                    available_confidence: 0.0,
                    action_risk,
                    inaction_risk,
                    reason: format!("OPA policy denied: {:?}", result.violations),
                    mfa_auth_url: None,
                    task_status_url: None,
                    lease: None,
                };
            }
        }

        // ---- Layer 2: One-Vote Veto Detection ----
        if has_veto {
            return GateDecisionSpec {
                decision: GateDecision::Deny,
                required_confidence: required_conf,
                available_confidence: 0.0,
                action_risk,
                inaction_risk,
                reason: "Blocked by one-vote veto from privileged agent".to_string(),
                mfa_auth_url: None,
                task_status_url: None,
                lease: None,
            };
        }

        // ---- Layer 3: Confidence Comparison ----
        let sup = support_confidence(support_reputations, support_evidence);
        let opp = support_confidence(opposition_reputations, opposition_evidence);
        let available_conf = available_confidence(sup, opp);

        if available_conf >= required_conf {
            return GateDecisionSpec {
                decision: GateDecision::Allow,
                required_confidence: required_conf,
                available_confidence: available_conf,
                action_risk,
                inaction_risk,
                reason: format!(
                    "Confidence {:.2} meets required {:.2}",
                    available_conf, required_conf
                ),
                mfa_auth_url: None,
                task_status_url: None,
                lease: None,
            };
        }

        // Insufficient confidence: compare with InactionRisk
        if inaction_risk > action_risk && contract.risk_tolerance_profile.allow_emergency_lease {
            // Emergency lease path
            return GateDecisionSpec {
                decision: GateDecision::AllowWithLease,
                required_confidence: required_conf,
                available_confidence: available_conf,
                action_risk,
                inaction_risk,
                reason: format!(
                    "InactionRisk {:.2} > ActionRisk {:.2}; emergency lease granted",
                    inaction_risk, action_risk
                ),
                mfa_auth_url: None,
                task_status_url: None,
                lease: None, // filled by lease manager
            };
        }

        // Default: require human approval
        GateDecisionSpec {
            decision: GateDecision::RequireHumanApproval,
            required_confidence: required_conf,
            available_confidence: available_conf,
            action_risk,
            inaction_risk,
            reason: format!(
                "Available {:.2} < Required {:.2}; human approval needed",
                available_conf, required_conf
            ),
            mfa_auth_url: contract.human_approval_policy.mfa_auth_url.clone(),
            task_status_url: Some("https://coevo.local/approval/status".to_string()),
            lease: None,
        }
    }
}
