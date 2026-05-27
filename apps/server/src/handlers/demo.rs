use axum::{extract::State, Json};
use coevo_core::problem::ProblemDetails;
use coevo_tracks::dispatch;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DemoRequest {
    pub tenant_id: Option<String>,
    pub agent_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct DemoResponse {
    pub track: String,
    pub contract_hash: String,
    pub plan_hash: String,
    pub traceparent: String,
    pub ambiguity_score: f64,
    pub warnings: Vec<String>,
    pub entries_created: Vec<String>,
    pub elapsed_ms: u64,
}

/// POST /demo/green — run a complete Green Track scenario
#[utoipa::path(
    post,
    path = "/demo/green",
    tag = "Demo",
    responses(
        (status = 200, description = "Green Track demo completed", body = DemoResponse)
    )
)]
pub async fn run_green_demo(
    State(state): State<AppState>,
    Json(req): Json<DemoRequest>,
) -> Result<Json<DemoResponse>, ProblemDetails> {
    let tenant_id = req.tenant_id.unwrap_or_else(|| "demo-tenant".to_string());
    let agents = req
        .agent_ids
        .unwrap_or_else(|| vec!["agent-synthesizer-01".to_string()]);

    let intent = "Read and analyze the latest system health metrics from the database in the development environment";

    let result = dispatch::dispatch_green(&state.pool, intent, agents, &tenant_id)
        .await
        .map_err(|e| ProblemDetails::internal_error("/demo/green", &e.to_string()))?;

    let gr = result.green_result.unwrap();
    Ok(Json(DemoResponse {
        track: "green".to_string(),
        contract_hash: gr.contract_hash,
        plan_hash: gr.plan_hash,
        traceparent: gr.traceparent,
        ambiguity_score: gr.ambiguity_score,
        warnings: gr.warnings,
        entries_created: gr.entries_created,
        elapsed_ms: gr.total_elapsed_ms,
    }))
}

/// POST /demo/yellow — stub (filled in step 8)
#[utoipa::path(
    post,
    path = "/demo/yellow",
    tag = "Demo",
    responses((status = 200, description = "Yellow Track demo"))
)]
pub async fn run_yellow_demo(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ProblemDetails> {
    Ok(Json(serde_json::json!({
        "track": "yellow",
        "status": "not_yet_implemented"
    })))
}

/// POST /demo/red — stub (filled in step 9)
#[utoipa::path(
    post,
    path = "/demo/red",
    tag = "Demo",
    responses((status = 200, description = "Red Track demo"))
)]
pub async fn run_red_demo(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ProblemDetails> {
    Ok(Json(serde_json::json!({
        "track": "red",
        "status": "not_yet_implemented"
    })))
}
