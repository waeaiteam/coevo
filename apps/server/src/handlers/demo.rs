use axum::{extract::State, Json};
use coevo_core::problem::ProblemDetails;
use coevo_tracks::dispatch;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct DemoRequest {
    pub tenant_id: Option<String>,
    pub agent_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RedDemoRequest {
    pub tenant_id: Option<String>,
    pub agent_ids: Option<Vec<String>>,
    pub caller_identity_proof: Option<String>,
    pub monitoring_signature: Option<String>,
    pub diagnostic_signature: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DemoResponse {
    pub track: String,
    pub contract_hash: String,
    pub plan_hash: String,
    pub traceparent: String,
    pub ambiguity_score: Option<f64>,
    pub warnings: Vec<String>,
    pub entries_created: Vec<String>,
    pub elapsed_ms: u64,
}

/// POST /demo/green — Green Track end-to-end
#[utoipa::path(
    post,
    path = "/demo/green",
    tag = "Demo",
    request_body = DemoRequest,
    responses(
        (status = 200, description = "Green Track demo completed", body = DemoResponse),
        (status = 500, description = "Internal error", body = ProblemDetails)
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
        ambiguity_score: Some(gr.ambiguity_score),
        warnings: gr.warnings,
        entries_created: gr.entries_created,
        elapsed_ms: gr.total_elapsed_ms,
    }))
}

/// POST /demo/yellow — Yellow Track end-to-end
#[utoipa::path(
    post,
    path = "/demo/yellow",
    tag = "Demo",
    request_body = DemoRequest,
    responses(
        (status = 200, description = "Yellow Track demo completed", body = DemoResponse),
        (status = 500, description = "Internal error", body = ProblemDetails)
    )
)]
pub async fn run_yellow_demo(
    State(state): State<AppState>,
    Json(req): Json<DemoRequest>,
) -> Result<Json<DemoResponse>, ProblemDetails> {
    let tenant_id = req.tenant_id.unwrap_or_else(|| "demo-tenant".to_string());
    let agents = req
        .agent_ids
        .unwrap_or_else(|| vec!["agent-synthesizer-01".to_string()]);

    let intent = "Send deployment notification to the team and write changelog to the staging wiki";

    let result = dispatch::dispatch_yellow(&state.pool, intent, agents, &tenant_id, "staging")
        .await
        .map_err(|e| ProblemDetails::internal_error("/demo/yellow", &e.to_string()))?;

    let yr = result.yellow_result.unwrap();
    Ok(Json(DemoResponse {
        track: "yellow".to_string(),
        contract_hash: yr.contract_hash,
        plan_hash: yr.plan_hash,
        traceparent: yr.traceparent,
        ambiguity_score: None,
        warnings: vec![],
        entries_created: yr.entries_created,
        elapsed_ms: 0,
    }))
}

/// POST /demo/red — Red Track end-to-end (requires identity_proof + dual-sign for lease)
#[utoipa::path(
    post,
    path = "/demo/red",
    tag = "Demo",
    request_body = RedDemoRequest,
    responses(
        (status = 200, description = "Red Track demo completed", body = DemoResponse),
        (status = 403, description = "Missing caller_identity_proof", body = ProblemDetails),
        (status = 500, description = "Internal error", body = ProblemDetails)
    )
)]
pub async fn run_red_demo(
    State(state): State<AppState>,
    Json(req): Json<RedDemoRequest>,
) -> Result<Json<DemoResponse>, ProblemDetails> {
    let tenant_id = req.tenant_id.unwrap_or_else(|| "demo-tenant".to_string());
    let agents = req
        .agent_ids
        .unwrap_or_else(|| vec!["agent-synthesizer-01".to_string()]);

    let caller_proof = req.caller_identity_proof.as_deref();

    // Red Track requires caller_identity_proof
    if caller_proof.is_none() || caller_proof.unwrap().is_empty() {
        return Err(ProblemDetails::forbidden(
            "/demo/red",
            "caller_identity_proof is required for Red Track",
        ));
    }

    let intent =
        "Emergency fix for production database connection pool exhaustion causing P1 outage";

    // Register agent for this demo
    coevo_store::repos::agent_repo::AgentRepo::register(
        &state.pool,
        agents.first().unwrap(),
        r#"{"roles":["Proposer"]}"#,
        r#"{"tools":["deploy-production"]}"#,
    )
    .await
    .ok();

    let result = dispatch::dispatch_red(
        &state.pool,
        intent,
        agents,
        &tenant_id,
        req.caller_identity_proof.as_deref(),
        req.monitoring_signature.as_deref(),
        req.diagnostic_signature.as_deref(),
    )
    .await
    .map_err(|e| {
        let err_str = e.to_string();
        if err_str.contains("caller_identity_proof") || err_str.contains("dual-sign")
            || err_str.contains("monitoring_signature") || err_str.contains("diagnostic_signature")
        {
            ProblemDetails::forbidden("/demo/red", &err_str)
        } else {
            ProblemDetails::internal_error("/demo/red", &err_str)
        }
    })?;

    let rr = result.red_result.unwrap();
    Ok(Json(DemoResponse {
        track: "red".to_string(),
        contract_hash: rr.contract_hash,
        plan_hash: rr.plan_hash,
        traceparent: rr.traceparent,
        ambiguity_score: None,
        warnings: vec![],
        entries_created: rr.entries_created,
        elapsed_ms: 0,
    }))
}
