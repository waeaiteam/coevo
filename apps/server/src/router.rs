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
        .route(
            "/founder",
            get(handlers::opc::get_founder).put(handlers::opc::put_founder),
        )
        .route(
            "/companies",
            get(handlers::opc::list_companies).post(handlers::opc::create_company),
        )
        .route(
            "/companies/{opc_id}",
            get(handlers::opc::get_company)
                .put(handlers::opc::put_company)
                .delete(handlers::opc::delete_company),
        )
        .route(
            "/companies/{opc_id}/employees",
            get(handlers::opc::list_company_employees)
                .post(handlers::opc::create_company_employee),
        )
        .route(
            "/companies/{opc_id}/employees/seed",
            post(handlers::opc::seed_company_employees_handler),
        )
        .route(
            "/companies/{opc_id}/employees/{id}",
            get(handlers::opc::get_company_employee)
                .put(handlers::opc::update_company_employee)
                .delete(handlers::opc::delete_company_employee),
        )
        .route(
            "/companies/{opc_id}/employees/{id}/prompt",
            get(handlers::opc::get_company_employee_prompt)
                .put(handlers::opc::update_company_employee_prompt),
        )
        .route(
            "/companies/{opc_id}/employees/{id}/prompt/versions",
            get(handlers::opc::list_company_employee_prompt_versions),
        )
        .route(
            "/companies/{opc_id}/employees/{id}/prompt/versions/{version}",
            get(handlers::opc::get_company_employee_prompt_version),
        )
        .route(
            "/companies/{opc_id}/employees/{id}/prompt/rollback",
            post(handlers::opc::rollback_company_employee_prompt),
        )
        .route(
            "/companies/{opc_id}/meetings",
            get(handlers::organization::list_meetings)
                .post(handlers::organization::create_meeting),
        )
        .route(
            "/companies/{opc_id}/meetings/{id}",
            get(handlers::organization::get_meeting),
        )
        .route(
            "/companies/{opc_id}/employees/{agent_id}/kpi",
            get(handlers::organization::list_employee_kpi)
                .post(handlers::organization::create_employee_kpi),
        )
        .route(
            "/companies/{opc_id}/reports",
            get(handlers::organization::list_reports),
        )
        .route(
            "/companies/{opc_id}/reports/{id}",
            get(handlers::organization::get_report),
        )
        .route(
            "/companies/{opc_id}/reports/generate",
            post(handlers::organization::generate_report),
        )
        .route(
            "/companies/{opc_id}/cost",
            get(handlers::organization::get_cost_summary),
        )
        .route(
            "/companies/{opc_id}/cost/quota",
            axum::routing::put(handlers::organization::put_cost_quota),
        )
        .route(
            "/companies/{opc_id}/eval/datasets",
            get(handlers::evaluations::list_datasets)
                .post(handlers::evaluations::create_dataset),
        )
        .route(
            "/companies/{opc_id}/eval/datasets/{id}/cases",
            get(handlers::evaluations::list_dataset_cases)
                .post(handlers::evaluations::create_dataset_case),
        )
        .route(
            "/companies/{opc_id}/eval/datasets/{id}/cases/{case_id}",
            axum::routing::delete(handlers::evaluations::delete_dataset_case),
        )
        .route(
            "/companies/{opc_id}/eval/run",
            post(handlers::evaluations::run_eval),
        )
        .route(
            "/companies/{opc_id}/eval/experiments",
            get(handlers::evaluations::list_experiments),
        )
        .route(
            "/companies/{opc_id}/eval/experiments/{id}",
            get(handlers::evaluations::get_experiment),
        )
        .route(
            "/companies/{opc_id}/eval/compare",
            post(handlers::evaluations::compare_eval),
        )
        .route(
            "/companies/{opc_id}/traces",
            get(handlers::traces::list_company_traces),
        )
        .route(
            "/companies/{opc_id}/traces/{trace_id}/spans",
            get(handlers::traces::get_company_trace_spans),
        )
        // Model routes
        .route(
            "/opc/models/config",
            get(handlers::models::get_config_handler).put(handlers::models::put_config_handler),
        )
        .route("/opc/models/test", post(handlers::models::test_connection))
        .route(
            "/opc/models/discover",
            post(handlers::models::discover_models),
        )
        .route("/opc/models/chat", post(handlers::models::chat))
        .route("/opc/models/structured", post(handlers::models::structured))
        .route(
            "/opc/models/profiles",
            get(handlers::models::list_model_profiles),
        )
        .route("/opc/models/route", post(handlers::models::route_model))
        // OPC routes — Profiles
        .route(
            "/opc/profile/user",
            get(handlers::opc::get_user_profile).put(handlers::opc::put_user_profile),
        )
        .route(
            "/opc/profile/company",
            get(handlers::opc::get_company_profile).put(handlers::opc::put_company_profile),
        )
        // OPC — Memory
        .route(
            "/opc/memory",
            get(handlers::opc::list_memory).post(handlers::opc::create_memory),
        )
        .route("/opc/memory/{id}/stale", post(handlers::opc::stale_memory))
        .route(
            "/opc/memory/{id}/revoke",
            post(handlers::opc::revoke_memory),
        )
        // OPC — Employees
        .route(
            "/opc/agents/employees",
            get(handlers::opc::list_employees).post(handlers::opc::create_employee),
        )
        .route(
            "/opc/agents/employees/seed",
            post(handlers::opc::seed_employees_handler),
        )
        .route(
            "/opc/agents/employees/{id}",
            get(handlers::opc::get_employee)
                .put(handlers::opc::update_employee)
                .delete(handlers::opc::delete_employee),
        )
        .route(
            "/opc/agents/employees/{id}/prompt",
            axum::routing::put(handlers::opc::update_employee_prompt),
        )
        .route(
            "/opc/agents/employees/{id}/memory",
            get(handlers::opc::get_agent_memory),
        )
        .route(
            "/opc/agents/employees/{id}/growth",
            get(handlers::opc::get_agent_growth),
        )
        // OPC — Executors
        .route("/opc/executors", get(handlers::opc::list_executors))
        .route(
            "/opc/executors/register",
            post(handlers::opc::register_executor),
        )
        .route(
            "/opc/executors/{id}/disable",
            post(handlers::opc::disable_executor),
        )
        .route(
            "/opc/executors/{id}/health",
            post(handlers::opc::executor_health),
        )
        .route(
            "/opc/executors/{id}/dry-run",
            post(handlers::opc::executor_dry_run),
        )
        // OPC — Work Orders
        .route(
            "/opc/work-orders",
            get(handlers::opc::list_work_orders).post(handlers::opc::create_work_order),
        )
        .route(
            "/opc/work-orders/{id}/execute",
            post(handlers::opc::execute_work_order),
        )
        .route(
            "/opc/work-orders/{id}/approval",
            post(handlers::opc::decide_work_order_approval),
        )
        .route(
            "/opc/work-orders/{id}/cancel",
            post(handlers::opc::cancel_work_order),
        )
        .route(
            "/opc/work-orders/{id}/feedback",
            post(handlers::opc::work_order_feedback),
        )
        .route(
            "/opc/conversations",
            get(handlers::conversations::list_conversations)
                .post(handlers::conversations::create_conversation),
        )
        .route(
            "/opc/conversations/{id}",
            get(handlers::conversations::get_conversation),
        )
        .route(
            "/opc/conversations/{id}/messages",
            get(handlers::conversations::list_conversation_messages)
                .post(handlers::conversations::append_conversation_message),
        )
        // OPC — Skills
        .route("/opc/skills", get(handlers::opc::list_skills))
        .route("/opc/skills/seed", post(handlers::opc::seed_skills))
        .route(
            "/opc/skills/{id}/{ver}/activate",
            post(handlers::opc::activate_skill),
        )
        .route(
            "/opc/skills/{id}/{ver}/rollback",
            post(handlers::opc::rollback_skill),
        )
        // OPC — Skill Evolution
        .route(
            "/opc/skills/evolution/proposals",
            get(handlers::opc::list_proposals),
        )
        .route(
            "/opc/skills/evolution/run",
            post(handlers::opc::run_evolution),
        )
        .route(
            "/opc/skills/evolution/proposals/{id}/verify",
            post(handlers::opc::verify_proposal),
        )
        .route(
            "/opc/skills/evolution/proposals/{id}/approve",
            post(handlers::opc::approve_proposal),
        )
        .route(
            "/opc/skills/evolution/proposals/{id}/reject",
            post(handlers::opc::reject_proposal),
        )
        // Prompt version control
        .route(
            "/opc/prompts/versions",
            post(handlers::prompts::create_prompt_version),
        )
        .route(
            "/opc/prompts/versions/{version_id}/publish",
            post(handlers::prompts::publish_prompt_version),
        )
        .route(
            "/opc/prompts/{prompt_id}/versions",
            get(handlers::prompts::list_prompt_versions),
        )
        .route(
            "/opc/prompts/versions/{version_id}",
            get(handlers::prompts::get_prompt_version),
        );

    // Worker routes
    let public = public
        .route("/opc/workers", get(handlers::workers::list_workers))
        .route("/opc/workers/{id}", get(handlers::workers::get_worker))
        .route(
            "/opc/workers/{id}/runs",
            get(handlers::workers::get_worker_runs),
        )
        .route(
            "/opc/workers/runs/{run_id}",
            get(handlers::workers::get_run),
        )
        .route(
            "/opc/workers/runs/{run_id}/steps",
            get(handlers::workers::get_run_steps),
        )
        .route(
            "/opc/workers/runs/{run_id}/events",
            get(handlers::workers::get_run_events),
        )
        .route(
            "/opc/workers/runs/{run_id}/events/stream",
            get(handlers::workers::stream_run_events),
        )
        .route(
            "/opc/workers/runs/{run_id}/reflection",
            get(handlers::workers::get_run_reflection),
        )
        .route("/opc/timeline", get(handlers::timeline::global_timeline))
        .route("/opc/workers/assign", post(handlers::tools::assign_worker))
        .route("/opc/workers/{id}/run", post(handlers::tools::run_worker))
        .route(
            "/opc/workers/{id}/cancel",
            post(handlers::tools::cancel_worker),
        )
        .route("/opc/tools", get(handlers::tools::list_tools))
        .route("/opc/tools/{id}", get(handlers::tools::get_tool))
        .route("/opc/tools/{id}/health", post(handlers::tools::tool_health))
        .route(
            "/opc/tools/{id}/dry-run",
            post(handlers::tools::tool_dry_run),
        )
        .route(
            "/opc/tools/{id}/execute",
            post(handlers::tools::tool_execute),
        )
        // Worker sessions + timeline
        .route(
            "/opc/workers/sessions",
            get(handlers::timeline::list_worker_sessions),
        )
        .route(
            "/opc/workers/sessions/{id}",
            get(handlers::timeline::get_worker_session),
        )
        .route(
            "/opc/workers/sessions/{id}/steps",
            get(handlers::timeline::get_session_steps),
        )
        .route(
            "/opc/workers/sessions/{id}/events",
            get(handlers::timeline::get_session_events),
        )
        .route(
            "/opc/work-orders/{id}/timeline",
            get(handlers::timeline::timeline),
        )
        .route(
            "/opc/work-orders/{id}/audit-export",
            get(handlers::timeline::work_order_audit_export),
        );

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
        let app = build_router(AppState::new(pool, std::env::temp_dir()));
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

    #[tokio::test]
    async fn public_router_exposes_company_crud_routes() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-router-company-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool, root.clone()));

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Router Labs",
                            "mission": "Route check"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap().to_string();

        let detail_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail_response.status(), StatusCode::OK);

        let delete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/companies/{opc_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);

        let detail_after_delete = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail_after_delete.status(), StatusCode::NOT_FOUND);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn public_router_exposes_company_scoped_employee_routes() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-router-employees-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool, root.clone()));

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Employee Isolation Labs",
                            "mission": "Route employee checks"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap();

        let list_before_seed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/employees"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_before_seed.status(), StatusCode::OK);
        let before_seed: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_before_seed.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(before_seed.as_array().unwrap().len(), 0);

        let seed_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed_response.status(), StatusCode::OK);

        let list_after_seed = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/employees"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_after_seed.status(), StatusCode::OK);
        let after_seed: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_after_seed.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!after_seed.as_array().unwrap().is_empty());

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_scoped_employee_routes_isolate_each_company() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-isolation-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        async fn create_company(
            app: &Router,
            name: &str,
            mission: &str,
        ) -> String {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/companies")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "name": name,
                                "mission": mission
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let created: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap();
            created["opc_id"].as_str().unwrap().to_string()
        }

        async fn list_employees(
            app: &Router,
            opc_id: &str,
        ) -> serde_json::Value {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/companies/{opc_id}/employees"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap()
        }

        let alpha = create_company(&app, "Alpha Co", "Alpha mission").await;
        let beta = create_company(&app, "Beta Co", "Beta mission").await;

        let alpha_before = list_employees(&app, &alpha).await;
        let beta_before = list_employees(&app, &beta).await;
        assert_eq!(alpha_before.as_array().unwrap().len(), 0);
        assert_eq!(beta_before.as_array().unwrap().len(), 0);

        let seed_alpha = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{alpha}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed_alpha.status(), StatusCode::OK);

        let alpha_after_seed = list_employees(&app, &alpha).await;
        let beta_after_alpha_seed = list_employees(&app, &beta).await;
        assert!(!alpha_after_seed.as_array().unwrap().is_empty());
        assert_eq!(beta_after_alpha_seed.as_array().unwrap().len(), 0);

        let delete_beta = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/companies/{beta}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_beta.status(), StatusCode::OK);

        let alpha_after_beta_delete = list_employees(&app, &alpha).await;
        assert_eq!(
            alpha_after_beta_delete.as_array().unwrap().len(),
            alpha_after_seed.as_array().unwrap().len()
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_employee_prompt_routes_use_files_and_support_rollback() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-prompt-files-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Prompt Files Co",
                            "mission": "Prompt versioning"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = created["opc_id"].as_str().unwrap().to_string();
        let agent_id = "agent-pm-01";

        let seed_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed_response.status(), StatusCode::OK);

        let employee_dir = root.join(&opc_id).join("employees").join(agent_id);
        assert!(employee_dir.join("passport.json").exists());
        assert!(employee_dir.join("prompt.md").exists());

        let first_prompt = "You are the phase-2 prompt v1.";
        let first_update = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/companies/{opc_id}/employees/{agent_id}/prompt"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "system_prompt": first_prompt,
                            "change_summary": "initial"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first_update.status(), StatusCode::OK);
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            first_prompt
        );
        assert!(employee_dir.join("prompt_versions").join("v1.md").exists());

        let second_prompt = "You are the phase-2 prompt v2.";
        let second_update = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/companies/{opc_id}/employees/{agent_id}/prompt"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "system_prompt": second_prompt,
                            "change_summary": "refine"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second_update.status(), StatusCode::OK);
        assert!(employee_dir.join("prompt_versions").join("v2.md").exists());

        let versions_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/employees/{agent_id}/prompt/versions"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(versions_response.status(), StatusCode::OK);
        let versions: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(versions_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(versions.as_array().unwrap().len(), 2);

        let rollback_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/{agent_id}/prompt/rollback"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "version": 1 }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rollback_response.status(), StatusCode::OK);
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            first_prompt
        );

        let get_prompt_response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/employees/{agent_id}/prompt"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_prompt_response.status(), StatusCode::OK);
        let prompt_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(get_prompt_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(prompt_body["content_md"], first_prompt);
        assert_eq!(prompt_body["version"], 1);

        std::fs::remove_dir_all(root).ok();
    }
}
