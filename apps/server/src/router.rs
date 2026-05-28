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
        // Model routes
        .route("/opc/models/config", get(handlers::models::get_config_handler).put(handlers::models::put_config_handler))
        .route("/opc/models/test", post(handlers::models::test_connection))
        .route("/opc/models/chat", post(handlers::models::chat))
        .route("/opc/models/structured", post(handlers::models::structured))
        // OPC routes — Profiles
        .route("/opc/profile/user", get(handlers::opc::get_user_profile).put(handlers::opc::put_user_profile))
        .route("/opc/profile/company", get(handlers::opc::get_company_profile).put(handlers::opc::put_company_profile))
        // OPC — Memory
        .route("/opc/memory", get(handlers::opc::list_memory).post(handlers::opc::create_memory))
        .route("/opc/memory/{id}/stale", post(handlers::opc::stale_memory))
        .route("/opc/memory/{id}/revoke", post(handlers::opc::revoke_memory))
        // OPC — Employees
        .route("/opc/agents/employees", get(handlers::opc::list_employees))
        .route("/opc/agents/employees/seed", post(handlers::opc::seed_employees_handler))
        .route("/opc/agents/employees/{id}/memory", get(handlers::opc::get_agent_memory))
        // OPC — Executors
        .route("/opc/executors", get(handlers::opc::list_executors))
        .route("/opc/executors/register", post(handlers::opc::register_executor))
        .route("/opc/executors/{id}/disable", post(handlers::opc::disable_executor))
        .route("/opc/executors/{id}/health", post(handlers::opc::executor_health))
        .route("/opc/executors/{id}/dry-run", post(handlers::opc::executor_dry_run))
        // OPC — Work Orders
        .route("/opc/work-orders", get(handlers::opc::list_work_orders).post(handlers::opc::create_work_order))
        .route("/opc/work-orders/{id}/execute", post(handlers::opc::execute_work_order))
        .route("/opc/work-orders/{id}/cancel", post(handlers::opc::cancel_work_order))
        .route("/opc/work-orders/{id}/feedback", post(handlers::opc::work_order_feedback))
        // OPC — Skills
        .route("/opc/skills", get(handlers::opc::list_skills))
        .route("/opc/skills/seed", post(handlers::opc::seed_skills))
        .route("/opc/skills/{id}/{ver}/activate", post(handlers::opc::activate_skill))
        .route("/opc/skills/{id}/{ver}/rollback", post(handlers::opc::rollback_skill))
        // OPC — Skill Evolution
        .route("/opc/skills/evolution/proposals", get(handlers::opc::list_proposals))
        .route("/opc/skills/evolution/run", post(handlers::opc::run_evolution))
        .route("/opc/skills/evolution/proposals/{id}/verify", post(handlers::opc::verify_proposal))
        .route("/opc/skills/evolution/proposals/{id}/approve", post(handlers::opc::approve_proposal))
        .route("/opc/skills/evolution/proposals/{id}/reject", post(handlers::opc::reject_proposal));

    // Worker routes
    let public = public
        .route("/opc/workers", get(handlers::workers::list_workers))
        .route("/opc/workers/{id}", get(handlers::workers::get_worker))
        .route("/opc/workers/{id}/runs", get(handlers::workers::get_worker_runs))
        .route("/opc/workers/runs/{run_id}", get(handlers::workers::get_run))
        .route("/opc/workers/runs/{run_id}/steps", get(handlers::workers::get_run_steps))
        .route("/opc/workers/runs/{run_id}/events", get(handlers::workers::get_run_events))
        .route("/opc/workers/runs/{run_id}/reflection", get(handlers::workers::get_run_reflection));

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
