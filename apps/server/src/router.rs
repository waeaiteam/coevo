//! Axum router — all API routes, OpenAPI, and middleware.

use axum::{
    middleware,
    routing::{get, post},
    Router,
};

use crate::docs;
use crate::handlers;
use crate::middleware::validate_metadata;
use crate::state::AppState;

/// Build the full axum Router with all API routes.
pub fn build_router(state: AppState) -> Router {
    // Public routes (no metadata validation)
    let public = Router::new()
        .route("/health", get(handlers::health::health_check))
        .route("/openapi.json", get(docs::openapi_json))
        .route("/docs", get(docs::swagger_ui))
        .route("/redoc", get(docs::redoc));

    // Authenticated API routes (with metadata validation)
    let api = Router::new()
        .route("/mcl/compile", post(handlers::compile::compile_contract))
        .route("/router/route", post(handlers::route::route_plan))
        .route("/customs/propose", post(handlers::propose::propose_fact))
        .route("/risk/evaluate", post(handlers::evaluate::evaluate_risk))
        .route("/resolution/process", post(handlers::resolve::resolve_conflict))
        .route("/demo/green", post(handlers::demo::run_green_demo))
        .route("/demo/yellow", post(handlers::demo::run_yellow_demo))
        .route("/demo/red", post(handlers::demo::run_red_demo))
        .layer(middleware::from_fn(validate_metadata));

    Router::new()
        .merge(public)
        .merge(api)
        .with_state(state)
}
