//! RiskGate evaluate handler — full implementation.
use axum::{extract::State, Json};
use coevo_core::decision::ActionProposalSpec;
use coevo_core::problem::ProblemDetails;
use coevo_policy::mock::MockPolicyEngine;
use coevo_risk::decision_tree::RiskGate;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    pub action_urn: String,
    pub target_environment: String,
    pub parameters: serde_json::Value,
    pub emergency_mode: bool,
    pub blast_radius: u8,
    pub irreversibility: u8,
    pub environment_sensitivity: u8,
    pub reversibility: u8,
}

#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub decision: String,
    pub required_confidence: f64,
    pub available_confidence: f64,
    pub action_risk: f64,
    pub inaction_risk: f64,
    pub reason: String,
}

#[utoipa::path(
    post,
    path = "/risk/evaluate",
    tag = "Risk",
    request_body = EvaluateRequest,
    responses(
        (status = 200, description = "Risk evaluated"),
        (status = 403, description = "Risk denied")
    )
)]
pub async fn evaluate_risk(
    State(state): State<AppState>,
    Json(req): Json<EvaluateRequest>,
) -> Result<Json<EvaluateResponse>, ProblemDetails> {
    let policy = Box::new(MockPolicyEngine::new());
    let gate = RiskGate::new(policy);

    let action = ActionProposalSpec {
        action_urn: req.action_urn,
        target_environment: req.target_environment,
        parameters: req.parameters,
        emergency_mode: req.emergency_mode,
        blast_radius: req.blast_radius,
        irreversibility: req.irreversibility,
        environment_sensitivity: req.environment_sensitivity,
        reversibility: req.reversibility,
    };

    // Default contract for standalone evaluation
    let contract = coevo_core::contract::MCLSpec {
        mcl_version: "1.0".to_string(),
        mcl_state: coevo_core::contract::ContractState::ActiveContract,
        parent_contract_hash: "0".repeat(64),
        goal_tree: coevo_core::contract::GoalTree {
            root: coevo_core::contract::GoalNode {
                id: "root".to_string(),
                description: "risk-eval".to_string(),
                status: coevo_core::contract::GoalStatus::Pending,
                children: vec![],
                depends_on: vec![],
            },
        },
        institution_policy_hash: "0".repeat(64),
        data_boundary: vec![],
        allowed_action_modes: vec![coevo_core::contract::ActionMode::CommitReady],
        human_approval_policy: coevo_core::contract::HumanApprovalPolicy {
            approval_mode: coevo_core::contract::ApprovalMode::ExplicitApproval,
            authorized_roles: vec!["Admin".to_string()],
            negative_consent_timeout_secs: 0,
            mfa_auth_url: Some("https://coevo.local/mfa".to_string()),
        },
        evidence_requirement: coevo_core::contract::EvidenceRequirement {
            minimum_level: "unit_tests_passing".to_string(),
            require_json_report: true,
        },
        risk_tolerance_profile: coevo_core::contract::RiskToleranceProfile {
            max_risk_score: 0.8,
            allow_emergency_lease: true,
        },
        termination_policy: coevo_core::contract::TerminationPolicy {
            max_token_budget: 100000,
            max_hops: 6,
            max_latency_ms: 300000,
            max_stance_rounds: 3,
        },
        responsibility_anchor_policy: coevo_core::contract::ResponsibilityAnchorPolicy {
            required_human_roles: vec!["CISO".to_string()],
            agent_forbidden_actions: vec![],
        },
    };

    let gating = gate
        .evaluate(
            &action,
            &contract,
            &[0.7],
            &[0.8],
            &[],
            &[],
            0.3,
            0.2,
            0.1,
            false,
        )
        .await;

    if matches!(gating.decision, coevo_core::decision::GateDecision::Deny) {
        return Err(ProblemDetails::risk_denied(
            "/risk/evaluate",
            &gating.reason,
        ));
    }

    Ok(Json(EvaluateResponse {
        decision: format!("{:?}", gating.decision),
        required_confidence: gating.required_confidence,
        available_confidence: gating.available_confidence,
        action_risk: gating.action_risk,
        inaction_risk: gating.inaction_risk,
        reason: gating.reason,
    }))
}
