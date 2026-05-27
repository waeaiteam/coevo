//! Placeholder for ResolutionEngine handler (filled in step 10).
use axum::{extract::State, Json};
use coevo_core::problem::ProblemDetails;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub issue: String,
    pub stances: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ResolveResponse {
    pub verdict: String,
    pub adr_id: Option<String>,
}

/// POST /resolution/process — stub (filled in step 10)
#[utoipa::path(
    post,
    path = "/resolution/process",
    tag = "Resolution",
    request_body = ResolveRequest,
    responses(
        (status = 200, description = "Conflict resolved")
    )
)]
pub async fn resolve_conflict(
    State(_state): State<AppState>,
    Json(_req): Json<ResolveRequest>,
) -> Result<Json<ResolveResponse>, ProblemDetails> {
    Ok(Json(ResolveResponse {
        verdict: "resolved".to_string(),
        adr_id: None,
    }))
}
