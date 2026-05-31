//! Risk quantification — ActionRisk and InactionRisk computation.
//! Per coevo whitepaper Section 9.1.

use coevo_core::decision::ActionProposalSpec;

/// Weights for ActionRisk formula: w1·BR + w2·IR + w3·ES + w4·RV
const W1: f64 = 0.30; // blast radius weight
const W2: f64 = 0.35; // irreversibility weight
const W3: f64 = 0.20; // environment sensitivity weight
const W4: f64 = 0.15; // reversibility weight

/// Compute ActionRisk from an action proposal.
pub fn compute_action_risk(action: &ActionProposalSpec) -> f64 {
    let br = action.blast_radius as f64 / 3.0;
    let ir = action.irreversibility as f64 / 3.0;
    let es = action.environment_sensitivity as f64 / 3.0;
    let rv = action.reversibility as f64 / 3.0;

    let raw = W1 * br + W2 * ir + W3 * es + W4 * rv;
    (raw * 100.0).round() / 100.0
}

/// Weights for InactionRisk: w5·SI + w6·TC + w7·FP
const W5: f64 = 0.40; // service impact
const W6: f64 = 0.35; // time criticality
const W7: f64 = 0.25; // failure propagation

/// Compute InactionRisk based on environment metrics.
pub fn compute_inaction_risk(
    service_impact: f64,
    time_criticality: f64,
    failure_propagation: f64,
) -> f64 {
    let raw = W5 * service_impact + W6 * time_criticality + W7 * failure_propagation;
    (raw * 100.0).round() / 100.0
}

/// Compute RequiredConfidence from ActionRisk.
/// f(ActionRisk) = ActionRisk (linear mapping for simplicity).
pub fn required_confidence(action_risk: f64) -> f64 {
    action_risk
}

/// Compute AvailableConfidence from support and opposition stances.
pub fn available_confidence(support_confidence: f64, opposition_confidence: f64) -> f64 {
    let raw = support_confidence - opposition_confidence;
    raw.clamp(0.0, 1.0)
}

/// Compute SupportConfidence: Σ(Reputation_i · EvidenceWeight_i)
pub fn support_confidence(reputations: &[f64], evidence_weights: &[f64]) -> f64 {
    reputations
        .iter()
        .zip(evidence_weights.iter())
        .map(|(r, e)| r * e)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_core::decision::ActionProposalSpec;

    #[test]
    fn test_green_track_risk() {
        let action = ActionProposalSpec {
            action_urn: "read".to_string(),
            target_environment: "development".to_string(),
            parameters: serde_json::json!({}),
            emergency_mode: false,
            blast_radius: 0,
            irreversibility: 0,
            environment_sensitivity: 0,
            reversibility: 0,
        };
        let risk = compute_action_risk(&action);
        assert_eq!(risk, 0.0);
    }

    #[test]
    fn test_red_track_risk() {
        let action = ActionProposalSpec {
            action_urn: "deploy-production".to_string(),
            target_environment: "production".to_string(),
            parameters: serde_json::json!({}),
            emergency_mode: false,
            blast_radius: 3,
            irreversibility: 3,
            environment_sensitivity: 3,
            reversibility: 3,
        };
        let risk = compute_action_risk(&action);
        assert_eq!(risk, 1.0);
    }
}
