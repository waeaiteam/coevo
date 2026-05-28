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
        .route("/redoc", get(docs::redoc))
        // OPC routes
        .route("/opc/profile/user", get(handlers::opc::get_user_profile).put(handlers::opc::put_user_profile))
        .route("/opc/memory", get(handlers::opc::list_memory).post(handlers::opc::create_memory))
        .route("/opc/memory/{id}/stale", post(handlers::opc::stale_memory))
        .route("/opc/memory/{id}/revoke", post(handlers::opc::revoke_memory))
        .route("/opc/agents/employees", get(handlers::opc::list_employees))
        .route("/opc/agents/employees/seed", post(handlers::opc::seed_employees_handler))
        .route("/opc/work-orders", get(handlers::opc::list_work_orders).post(handlers::opc::create_work_order));

    // Authenticated API routes (with metadata validation)
    let api = Router::new()
        .route("/mcl/compile", post(handlers::compile::compile_contract))
        .route("/router/route", post(handlers::route::route_plan))
        .route("/customs/propose", post(handlers::propose::propose_fact))
        .route("/risk/evaluate", post(handlers::evaluate::evaluate_risk))
        .route(
            "/resolution/process",
            post(handlers::resolve::resolve_conflict),
        )
        .route("/demo/green", post(handlers::demo::run_green_demo))
        .route("/demo/yellow", post(handlers::demo::run_yellow_demo))
        .route("/demo/red", post(handlers::demo::run_red_demo))
        .layer(middleware::from_fn(validate_metadata));

    Router::new().merge(public).merge(api).with_state(state)
}
