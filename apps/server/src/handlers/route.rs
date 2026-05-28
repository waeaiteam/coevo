use axum::{extract::State, Json};
use coevo_core::problem::ProblemDetails;
use coevo_router::pcdt::PcdtRouter;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct RouteRequest {
    /// MCL contract specification as JSON (serde_json::Value for OpenAPI compatibility).
    pub contract: serde_json::Value,
    pub agent_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RouteResponse {
    pub plan: serde_json::Value,
    pub plan_hash: String,
}

/// POST /router/route
#[utoipa::path(
    post,
    path = "/router/route",
    tag = "Router",
    request_body = RouteRequest,
    responses(
        (status = 200, description = "Plan computed", body = RouteResponse),
        (status = 422, description = "No compliant path or budget exceeded", body = ProblemDetails)
    )
)]
pub async fn route_plan(
    State(_state): State<AppState>,
    Json(req): Json<RouteRequest>,
) -> Result<Json<RouteResponse>, ProblemDetails> {
    let contract: coevo_core::contract::MCLSpec =
        serde_json::from_value(req.contract).map_err(|e| {
            ProblemDetails::mcl_compilation_error(
                "/router/route",
                &format!("invalid contract: {}", e),
            )
        })?;

    let result = PcdtRouter::compute(&contract, req.agent_ids, None).map_err(|e| match e {
        coevo_router::pcdt::RoutingError::NoCompliantPath { blockers } => {
            ProblemDetails::routing_no_path("/router/route", &format!("blockers: {:?}", blockers))
        }
        coevo_router::pcdt::RoutingError::BudgetExceeded { budget, estimated } => {
            ProblemDetails::budget_exceeded(
                "/router/route",
                &format!("needed {}, budget {}", estimated, budget),
            )
        }
        _ => ProblemDetails::routing_no_path("/router/route", &e.to_string()),
    })?;

    Ok(Json(RouteResponse {
        plan: serde_json::to_value(&result.plan).unwrap(),
        plan_hash: result.plan_hash,
    }))
}
