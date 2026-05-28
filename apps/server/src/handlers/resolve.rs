//! ResolutionEngine handler — full implementation.
use axum::{extract::State, Json};
use coevo_core::problem::ProblemDetails;
use coevo_core::stance::*;
use coevo_resolution::engine::ResolutionEngine;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveRequest {
    pub issue: String,
    pub context_ref: Option<String>,
    pub stances: Vec<StanceRequestEntry>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StanceRequestEntry {
    pub agent_id: String,
    pub position: String,
    pub weight: f64,
    pub evidence_urns: Vec<String>,
    pub has_veto: bool,
    pub compromise_proposal: Option<String>,
    pub round: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResolveResponse {
    pub verdict: String,
    pub adr_id: Option<String>,
    pub blocking_nodes: Vec<String>,
    pub escalation: Option<String>,
}

/// POST /resolution/process
#[utoipa::path(
    post,
    path = "/resolution/process",
    tag = "Resolution",
    request_body = ResolveRequest,
    responses(
        (status = 200, description = "Conflict resolved", body = ResolveResponse),
        (status = 422, description = "Deadlock detected", body = ProblemDetails)
    )
)]
pub async fn resolve_conflict(
    State(state): State<AppState>,
    Json(req): Json<ResolveRequest>,
) -> Result<Json<ResolveResponse>, ProblemDetails> {
    let stances: Vec<StanceEntry> = req
        .stances
        .into_iter()
        .map(|s| StanceEntry {
            agent_id: s.agent_id,
            position: match s.position.as_str() {
                "SUPPORT" | "Support" | "support" => StancePosition::Support,
                _ => StancePosition::Oppose,
            },
            weight: s.weight,
            evidence_urns: s.evidence_urns,
            has_veto: s.has_veto,
            compromise_proposal: s.compromise_proposal,
            round: s.round,
        })
        .collect();

    let max_round = stances.iter().map(|s| s.round).max().unwrap_or(0) + 3;

    let matrix = StanceMatrixSpec {
        stances,
        issue: req.issue,
        context_ref: req.context_ref.unwrap_or_else(|| "0".repeat(64)),
        max_rounds: max_round,
    };

    let engine = ResolutionEngine::new();
    let result = engine
        .process(&state.pool, &matrix)
        .await
        .map_err(|e| ProblemDetails::internal_error("/resolution/process", &e.to_string()))?;

    if result.decision == ResolutionVerdict::Deadlocked {
        return Err(ProblemDetails::deadlock_detected(
            "/resolution/process",
            &format!("blocking: {:?}", result.blocking_nodes),
        ));
    }

    Ok(Json(ResolveResponse {
        verdict: format!("{:?}", result.decision),
        adr_id: result.adr.as_ref().map(|a| a.decision_id.clone()),
        blocking_nodes: result.blocking_nodes,
        escalation: result.escalation.map(|e| e.reason),
    }))
}
