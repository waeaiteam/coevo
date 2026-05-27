//! Placeholder for RiskGate evaluate handler (filled in step 8).
use axum::{extract::State, Json};
use coevo_core::problem::ProblemDetails;
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
    pub action_risk: f64,
    pub inaction_risk: f64,
}

/// POST /risk/evaluate — stub (filled in step 8)
#[utoipa::path(
    post,
    path = "/risk/evaluate",
    tag = "Risk",
    request_body = EvaluateRequest,
    responses(
        (status = 200, description = "Risk evaluated")
    )
)]
pub async fn evaluate_risk(
    State(_state): State<AppState>,
    Json(_req): Json<EvaluateRequest>,
) -> Result<Json<EvaluateResponse>, ProblemDetails> {
    Ok(Json(EvaluateResponse {
        decision: "ALLOW".to_string(),
        action_risk: 0.0,
        inaction_risk: 0.0,
    }))
}
