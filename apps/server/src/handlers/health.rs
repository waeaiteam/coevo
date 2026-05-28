use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// GET /health
#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Server is healthy", body = HealthResponse))
)]
pub async fn health_check(State(_state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
