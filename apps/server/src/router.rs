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
        .route("/opc/models/discover", post(handlers::models::discover_models))
        .route("/opc/models/chat", post(handlers::models::chat))
        .route("/opc/models/structured", post(handlers::models::structured))
        .route("/opc/models/profiles", get(handlers::models::list_model_profiles))
        .route("/opc/models/route", post(handlers::models::route_model))
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
        .route("/opc/conversations", get(handlers::conversations::list_conversations).post(handlers::conversations::create_conversation))
        .route("/opc/conversations/{id}", get(handlers::conversations::get_conversation))
        .route("/opc/conversations/{id}/messages", get(handlers::conversations::list_conversation_messages).post(handlers::conversations::append_conversation_message))
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
        .route("/opc/workers/runs/{run_id}/reflection", get(handlers::workers::get_run_reflection))
        .route("/opc/workers/assign", post(handlers::tools::assign_worker))
        .route("/opc/workers/{id}/run", post(handlers::tools::run_worker))
        .route("/opc/workers/{id}/cancel", post(handlers::tools::cancel_worker))
        .route("/opc/tools", get(handlers::tools::list_tools))
        .route("/opc/tools/{id}", get(handlers::tools::get_tool))
        .route("/opc/tools/{id}/health", post(handlers::tools::tool_health))
        .route("/opc/tools/{id}/dry-run", post(handlers::tools::tool_dry_run))
        .route("/opc/tools/{id}/execute", post(handlers::tools::tool_execute))
        // Worker sessions + timeline
        .route("/opc/workers/sessions", get(handlers::timeline::list_worker_sessions))
        .route("/opc/workers/sessions/{id}", get(handlers::timeline::get_worker_session))
        .route("/opc/workers/sessions/{id}/steps", get(handlers::timeline::get_session_steps))
        .route("/opc/workers/sessions/{id}/events", get(handlers::timeline::get_session_events))
        .route("/opc/work-orders/{id}/timeline", get(handlers::timeline::timeline))
        .route("/opc/work-orders/{id}/audit-export", get(handlers::timeline::work_order_audit_export));

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
        .layer(middleware::from_fn(validate_metadata));

    Router::new().merge(public).merge(api).with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use coevo_core::metadata::CommonMetadataHeader;
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};
    use tower::ServiceExt;

    #[tokio::test]
    async fn public_router_does_not_mount_demo_routes() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let app = build_router(AppState::new(pool));
        let meta = CommonMetadataHeader::new(
            "0".repeat(64),
            "0".repeat(64),
            uuid::Uuid::new_v4().to_string(),
            "0".repeat(64),
            "Test".to_string(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/demo/green")
                    .header("content-type", "application/json")
                    .header("x-coevo-tenant-id", meta.tenant_id)
                    .header("x-coevo-actor-role", meta.actor_role)
                    .header("x-coevo-contract-hash", meta.contract_hash)
                    .header("x-coevo-policy-version", meta.policy_version)
                    .header("x-coevo-execution-plan-hash", meta.execution_plan_hash)
                    .header("x-coevo-causality-parent-id", meta.causality_parent_id)
                    .header("x-coevo-idempotency-key", meta.idempotency_key)
                    .header("x-coevo-request-ttl-ms", meta.request_ttl_ms.to_string())
                    .header("x-coevo-replay-mode", meta.replay_mode.to_string())
                    .header("x-coevo-timestamp", meta.timestamp.to_string())
                    .header("traceparent", meta.traceparent)
                    .body(Body::from(r#"{"tenant_id":"test","agent_ids":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
