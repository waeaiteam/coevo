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
        // Fail closed: a policy-engine error is NOT an implicit allow. If the
        // engine cannot be reached or errors, we deny rather than silently
        // skipping the policy layer (which previously happened on `Err`).
        let policy_result = self
            .policy_engine
            .evaluate_action(&action.action_urn, contract)
            .await;
        match policy_result {
            Ok(result) => {
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
            Err(e) => {
                return GateDecisionSpec {
                    decision: GateDecision::Deny,
                    required_confidence: required_conf,
                    available_confidence: 0.0,
                    action_risk,
                    inaction_risk,
                    reason: format!("Policy engine unavailable; failing closed (deny): {e}"),
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
            // Emergency lease path. NOTE: the gate itself cannot mint a lease
            // (it has no LeaseManager / DB handle), so `lease` is left `None`
            // here. An `AllowWithLease` with `lease == None` is NOT yet
            // actionable — the caller MUST provision a real dual-signed lease
            // via `LeaseManager::grant` before performing the action. Use
            // `GateDecisionSpec::is_actionable` to enforce this fail-closed.
            return GateDecisionSpec {
                decision: GateDecision::AllowWithLease,
                required_confidence: required_conf,
                available_confidence: available_conf,
                action_risk,
                inaction_risk,
                reason: format!(
                    "InactionRisk {:.2} > ActionRisk {:.2}; emergency lease REQUIRED before action (caller must call LeaseManager::grant)",
                    inaction_risk, action_risk
                ),
                mfa_auth_url: None,
                task_status_url: None,
                lease: None, // unprovisioned — caller must grant a lease
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
            task_status_url: approval_status_url(),
            lease: None,
        }
    }
}

/// Build the human-approval status polling URL from the deployment's public
/// base URL. Returns `Some("<base>/approval/status")` when
/// `COEVO_PUBLIC_BASE_URL` is set (trailing slashes trimmed), or `None`
/// otherwise — never a hardcoded `coevo.local` placeholder that points nowhere.
fn approval_status_url() -> Option<String> {
    let base = std::env::var("COEVO_PUBLIC_BASE_URL").ok()?;
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    Some(format!("{base}/approval/status"))
}

/// Whether a gate decision authorizes the action *as it stands*.
///
/// Fail-closed companion to [`RiskGate::evaluate`]: an `AllowWithLease` verdict
/// is only actionable once a real lease has been attached (`lease.is_some()`).
/// A bare `Allow` is actionable; everything else (deny / approval / defer /
/// escalate, or an `AllowWithLease` with no lease) is not. Callers that map
/// gate decisions to an "allow" outcome should gate on this rather than
/// treating `AllowWithLease` as an unconditional allow.
pub fn is_actionable(spec: &GateDecisionSpec) -> bool {
    match spec.decision {
        GateDecision::Allow => true,
        GateDecision::AllowWithLease => spec.lease.is_some(),
        _ => false,
    }
}
