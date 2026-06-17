//! Axum router — all API routes, OpenAPI, and middleware.

use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};

use crate::docs;
use crate::handlers;
#[cfg(not(test))]
use crate::middleware::configured_sidecar_token;
use crate::middleware::{require_sidecar_token, validate_metadata};
use crate::state::AppState;

/// Build the full axum Router with all API routes.
#[cfg(not(test))]
pub fn build_router(state: AppState) -> Router {
    build_router_with_sidecar_token(state, configured_sidecar_token())
}

/// Test-only default router: keep behavioral tests focused on the route under
/// test instead of forcing every fixture to provision sidecar auth headers.
/// Targeted auth tests call `build_router_with_sidecar_token` explicitly.
#[cfg(test)]
pub fn build_router(state: AppState) -> Router {
    build_router_with_sidecar_token(state, Some(String::new()))
}

#[doc(hidden)]
pub fn build_router_with_sidecar_token(state: AppState, sidecar_token: Option<String>) -> Router {
    let docs = Router::new()
        .route("/health", get(handlers::health::health_check))
        .route("/openapi.json", get(docs::openapi_json))
        .route("/docs", get(docs::swagger_ui))
        .route("/redoc", get(docs::redoc));

    // Operational routes are protected by the sidecar token when configured.
    let public = Router::new()
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
            "/companies/{opc_id}/profile/company",
            get(handlers::opc::get_company_profile_canonical)
                .put(handlers::opc::put_company_profile_canonical),
        )
        .route(
            "/companies/{opc_id}/memory",
            get(handlers::opc::list_company_memory).post(handlers::opc::create_company_memory),
        )
        .route(
            "/companies/{opc_id}/memory/{id}/stale",
            post(handlers::opc::stale_company_memory),
        )
        .route(
            "/companies/{opc_id}/memory/{id}/revoke",
            post(handlers::opc::revoke_company_memory),
        )
        .route(
            "/companies/{opc_id}/shared",
            get(handlers::opc::list_company_shared_files)
                .post(handlers::opc::put_company_shared_file),
        )
        .route(
            "/companies/{opc_id}/playground/run",
            post(handlers::models::company_playground_run),
        )
        .route(
            "/companies/{opc_id}/employees",
            get(handlers::opc::list_company_employees).post(handlers::opc::create_company_employee),
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
            "/companies/{opc_id}/skills",
            get(handlers::opc::list_company_skills),
        )
        .route(
            "/companies/{opc_id}/skills/install",
            post(handlers::opc::install_company_skill),
        )
        .route(
            "/companies/{opc_id}/skills/seed",
            post(handlers::opc::seed_company_skills_handler),
        )
        .route(
            "/companies/{opc_id}/skills/evolution/proposals",
            get(handlers::opc::list_company_skill_evolution_proposals),
        )
        .route(
            "/companies/{opc_id}/skills/evolution/run",
            post(handlers::opc::run_company_skill_evolution),
        )
        .route(
            "/companies/{opc_id}/skills/evolution/proposals/{id}/verify",
            post(handlers::opc::verify_company_skill_evolution_proposal),
        )
        .route(
            "/companies/{opc_id}/skills/evolution/proposals/{id}/approve",
            post(handlers::opc::approve_company_skill_evolution_proposal),
        )
        .route(
            "/companies/{opc_id}/skills/evolution/proposals/{id}/reject",
            post(handlers::opc::reject_company_skill_evolution_proposal),
        )
        .route(
            "/companies/{opc_id}/skills/{skill_name}",
            delete(handlers::opc::delete_company_skill),
        )
        .route(
            "/companies/{opc_id}/employees/{agent_id}/skills",
            get(handlers::opc::list_company_employee_skills),
        )
        .route(
            "/companies/{opc_id}/employees/{agent_id}/memory",
            get(handlers::opc::get_company_agent_memory),
        )
        .route(
            "/companies/{opc_id}/employees/{agent_id}/growth",
            get(handlers::opc::get_company_agent_growth),
        )
        .route(
            "/companies/{opc_id}/employees/{agent_id}/improvements",
            get(handlers::opc::list_company_agent_improvements),
        )
        .route(
            "/companies/{opc_id}/employees/{agent_id}/improvements/{pid}/approve",
            post(handlers::opc::approve_company_agent_improvement),
        )
        .route(
            "/companies/{opc_id}/employees/{agent_id}/improvements/{pid}/reject",
            post(handlers::opc::reject_company_agent_improvement),
        )
        .route(
            "/companies/{opc_id}/work-orders",
            get(handlers::opc::list_company_work_orders)
                .post(handlers::opc::create_company_work_order),
        )
        .route(
            "/companies/{opc_id}/work-orders/{id}/execute",
            post(handlers::opc::execute_company_work_order),
        )
        .route(
            "/companies/{opc_id}/work-orders/{id}/approval",
            post(handlers::opc::decide_company_work_order_approval),
        )
        .route(
            "/companies/{opc_id}/work-orders/{id}/cancel",
            post(handlers::opc::cancel_company_work_order),
        )
        .route(
            "/companies/{opc_id}/work-orders/{id}/feedback",
            post(handlers::opc::company_work_order_feedback),
        )
        .route(
            "/companies/{opc_id}/work-orders/{id}/timeline",
            get(handlers::opc::company_work_order_timeline),
        )
        .route(
            "/companies/{opc_id}/work-orders/{id}/audit-export",
            get(handlers::opc::company_work_order_audit_export),
        )
        .route(
            "/companies/{opc_id}/audit",
            get(handlers::timeline::list_company_audit_events),
        )
        .route(
            "/companies/{opc_id}/conversations",
            get(handlers::conversations::list_company_conversations)
                .post(handlers::conversations::create_company_conversation),
        )
        .route(
            "/companies/{opc_id}/conversations/{id}",
            get(handlers::conversations::get_company_conversation),
        )
        .route(
            "/companies/{opc_id}/conversations/{id}/messages",
            get(handlers::conversations::list_company_conversation_messages)
                .post(handlers::conversations::append_company_conversation_message),
        )
        .route(
            "/companies/{opc_id}/meetings",
            get(handlers::organization::list_meetings).post(handlers::organization::create_meeting),
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
            get(handlers::evaluations::list_datasets).post(handlers::evaluations::create_dataset),
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
    let public = public
        .route(
            "/opc/mcp/servers",
            get(handlers::mcp::list_mcp_servers).post(handlers::mcp::create_mcp_server),
        )
        .route(
            "/opc/mcp/servers/{id}",
            get(handlers::mcp::get_mcp_server)
                .put(handlers::mcp::update_mcp_server)
                .delete(handlers::mcp::delete_mcp_server),
        )
        .route(
            "/opc/mcp/servers/{id}/connect",
            post(handlers::mcp::connect_mcp_server),
        )
        .route(
            "/opc/mcp/servers/{id}/disconnect",
            post(handlers::mcp::disconnect_mcp_server),
        )
        .route(
            "/opc/mcp/servers/test",
            post(handlers::mcp::test_mcp_server),
        )
        .route(
            "/opc/mcp/servers/{id}/tools",
            get(handlers::mcp::list_mcp_server_tools),
        );
    let public = public.route(
        "/opc/audit/{opc_id}",
        get(handlers::timeline::list_audit_events),
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

    let secured = Router::new()
        .merge(public)
        .merge(api)
        .layer(middleware::from_fn_with_state(
            sidecar_token,
            require_sidecar_token,
        ));

    Router::new().merge(docs).merge(secured).with_state(state)
}

#[cfg(test)]
fn playground_result_looks_real(item: &serde_json::Value) -> bool {
    item["input_tokens"].as_u64().unwrap_or_default() > 0
        && item["output_tokens"].as_u64().unwrap_or_default() > 0
        && item["cost_usd"].as_f64().unwrap_or_default() > 0.0
        && item["latency_ms"].as_u64().unwrap_or_default() > 0
        && item["error"].is_null()
        && !item["output"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .is_empty()
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
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
    use coevo_store::pool::create_pool;
    use coevo_store::repos::audit_repo::AuditRepo;
    use coevo_store::repos::model_config_repo::ModelConfigRepo;
    use coevo_store::repos::worker_run_repo::WorkerRunRepo;
    use coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo;
    use coevo_store::repos_opc::work_order_repo::WorkOrderRepo;
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};
    use http_body_util::BodyExt;
    use std::sync::Mutex;
    use tower::ServiceExt;

    static REAL_PROVIDER_LOCK: Mutex<()> = Mutex::new(());

    fn real_provider_env(name: &str) -> Option<String> {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    async fn configure_real_deepseek_provider_if_env(
        pool: &sqlx::SqlitePool,
    ) -> Option<(String, String, String)> {
        let api_key = real_provider_env("COEVO_REAL_DEEPSEEK_API_KEY")?;
        let model = real_provider_env("COEVO_REAL_DEEPSEEK_MODEL")
            .unwrap_or_else(|| "deepseek-v4-flash".to_string());
        let base_url = real_provider_env("COEVO_REAL_DEEPSEEK_BASE_URL")
            .unwrap_or_else(|| "https://api.deepseek.com/v1".to_string());
        ModelConfigRepo::upsert_config(
            pool,
            "desktop-real-deepseek",
            "DeepSeek",
            &base_url,
            &api_key,
            &ModelConfigRepo::mask_key(&api_key),
            &model,
            &model,
            &model,
            &model,
            4096,
            0.2,
            30000,
            5.0,
        )
        .await
        .unwrap();
        Some((api_key, base_url, model))
    }

    async fn insert_contract(pool: &sqlx::SqlitePool, hash: &str) {
        use coevo_core::contract::*;
        use coevo_store::repos::contract_repo::ContractRepo;

        let contract = MCLSpec {
            mcl_version: "1.0".to_string(),
            mcl_state: ContractState::DraftContract,
            parent_contract_hash: "0".repeat(64),
            goal_tree: GoalTree {
                root: GoalNode {
                    id: "root".to_string(),
                    description: "test contract".to_string(),
                    status: GoalStatus::Pending,
                    children: vec![],
                    depends_on: vec![],
                },
            },
            institution_policy_hash: "0".repeat(64),
            data_boundary: vec![],
            allowed_action_modes: vec![ActionMode::DraftOnly],
            human_approval_policy: HumanApprovalPolicy {
                approval_mode: ApprovalMode::NegativeConsent,
                authorized_roles: vec!["Admin".to_string()],
                negative_consent_timeout_secs: 300,
                mfa_auth_url: None,
            },
            evidence_requirement: EvidenceRequirement {
                minimum_level: "none".to_string(),
                require_json_report: false,
            },
            risk_tolerance_profile: RiskToleranceProfile {
                max_risk_score: 0.6,
                allow_emergency_lease: false,
            },
            termination_policy: TerminationPolicy {
                max_token_budget: 10000,
                max_hops: 3,
                max_latency_ms: 60000,
                max_stance_rounds: 3,
            },
            responsibility_anchor_policy: ResponsibilityAnchorPolicy {
                required_human_roles: vec!["Admin".to_string()],
                agent_forbidden_actions: vec![],
            },
        };
        ContractRepo::insert(pool, &contract, hash).await.unwrap();
    }

    async fn response_status_and_text(response: axum::response::Response) -> (StatusCode, String) {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        (status, text)
    }

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
    async fn operational_routes_require_sidecar_token_when_configured() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-router-auth-{}", uuid::Uuid::new_v4()));
        let app = build_router_with_sidecar_token(
            AppState::new(pool, root.clone()),
            Some("test-sidecar-token".to_string()),
        );

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Unauthorized Co",
                            "mission": "should fail without token"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let health = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);

        let authorized = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .header("x-coevo-token", "test-sidecar-token")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Authorized Co",
                            "mission": "should pass with token"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn router_denies_operational_routes_when_sidecar_token_is_unconfigured() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-auth-capture-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router_with_sidecar_token(AppState::new(pool, root.clone()), None);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Late Token Co",
                            "mission": "should stay public after router build"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn http_create_then_execute_work_order_does_not_return_not_found() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        ModelConfigRepo::upsert_config(
            &pool,
            "desktop-test",
            "OpenAICompatible",
            "https://api.deepseek.com/v1",
            "sk-test",
            "sk-test",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            4096,
            0.2,
            30000,
            5.0,
        )
        .await
        .unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-legacy-execute-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));
        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "HTTP Legacy Execute Co",
                            "mission": "router execute check"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

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
        let work_order_id = "wo-http-create-execute";

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/work-orders")
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": work_order_id,
                            "contract_hash": "a".repeat(64),
                            "plan_hash": "b".repeat(64),
                            "user_id": "default-founder",
                            "opc_id": opc_id,
                            "mission_intent": "Analyze README.md",
                            "selected_agents": ["agent-founder-01"],
                            "selected_executors": [],
                            "required_skills": ["skill-mission-draft"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let execute_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/opc/work-orders/{work_order_id}/execute"))
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(execute_response.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn http_company_scoped_create_then_execute_work_order_does_not_return_not_found() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        ModelConfigRepo::upsert_config(
            &pool,
            "desktop-test",
            "OpenAICompatible",
            "https://api.deepseek.com/v1",
            "sk-test",
            "sk-test",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            4096,
            0.2,
            30000,
            5.0,
        )
        .await
        .unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-execute-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "HTTP Scoped Execute Co",
                            "mission": "router execute check"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

        let seed_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies/{opc_id}/employees/seed".replace("{opc_id}", &opc_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed_response.status(), StatusCode::OK);

        let work_order_id = "wo-http-company-create-execute";
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/work-orders")
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": work_order_id,
                            "contract_hash": "a".repeat(64),
                            "plan_hash": "b".repeat(64),
                            "user_id": "default-founder",
                            "opc_id": opc_id,
                            "mission_intent": "Analyze README.md",
                            "selected_agents": ["agent-founder-01"],
                            "selected_executors": [],
                            "required_skills": ["skill-mission-draft"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let execute_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/opc/work-orders/{work_order_id}/execute"))
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(execute_response.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn public_router_exposes_company_crud_routes() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-router-company-{}", uuid::Uuid::new_v4()));
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
    async fn company_work_order_routes_create_list_and_cancel_with_canonical_path() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-work-orders-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Work Orders Co",
                            "mission": "path-scoped work orders"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

        let work_order_id = "wo-company-canonical";
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/work-orders"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": work_order_id,
                            "contract_hash": "a".repeat(64),
                            "plan_hash": "b".repeat(64),
                            "user_id": "default-founder",
                            "opc_id": opc_id,
                            "mission_intent": "Analyze README.md",
                            "selected_agents": [],
                            "selected_executors": [],
                            "required_skills": []
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/work-orders"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(list_body
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["work_order_id"] == work_order_id));

        let cancel_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/work-orders/{work_order_id}/cancel"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cancel_response.status(), StatusCode::OK);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_work_order_audit_routes_use_canonical_path() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-work-order-audit-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Work Order Audit Co",
                            "mission": "path-scoped work order audit"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

        let work_order_id = "wo-company-canonical-audit";
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/work-orders"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": work_order_id,
                            "contract_hash": "a".repeat(64),
                            "plan_hash": "b".repeat(64),
                            "user_id": "default-founder",
                            "opc_id": opc_id,
                            "mission_intent": "Audit canonical timeline export",
                            "selected_agents": [],
                            "selected_executors": [],
                            "required_skills": []
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let timeline_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{opc_id}/work-orders/{work_order_id}/timeline"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(timeline_response.status(), StatusCode::OK);

        let audit_export_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{opc_id}/work-orders/{work_order_id}/audit-export"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(audit_export_response.status(), StatusCode::OK);
        let audit_export_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(audit_export_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            audit_export_body["work_order"]["work_order_id"],
            work_order_id
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_audit_route_lists_scoped_events() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-audit-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = build_router(state.clone());

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Audit Co",
                            "mission": "path-scoped audit"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

        AuditRepo::insert(
            &pool,
            "worker.governance",
            Some("contract-path-audit"),
            Some("agent-founder-01"),
            None,
            &opc_id,
            &serde_json::json!({
                "work_order_id": "wo-path-audit",
                "run_id": "run-path-audit",
                "round": 1
            })
            .to_string(),
        )
        .await
        .unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/audit?limit=5"))
                    .header("x-coevo-opc-id", &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let rows = body.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["event_type"], "worker.governance");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_growth_routes_use_canonical_path() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-growth-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool.clone(), root.clone());
        let app = build_router(state.clone());

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Growth Co",
                            "mission": "path-scoped growth"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

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

        let growth_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{opc_id}/employees/agent-founder-01/growth"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(growth_response.status(), StatusCode::OK);

        let company_pool = coevo_store::pool::create_pool(
            &state
                .company_workspace
                .company_db_path(&opc_id)
                .to_string_lossy(),
        )
        .await
        .unwrap();
        let proposal = coevo_core::skills::SkillEvolutionProposal {
            proposal_id: "proposal-company-growth-route-1".to_string(),
            source_type: coevo_core::skills::EvolutionSourceType::Failure,
            source_refs: vec!["run-growth-company-1".to_string()],
            target_skill_id: "skill-mission-draft".to_string(),
            proposal_type: coevo_core::skills::EvolutionProposalType::PatchSkill,
            diagnosis: "Need clearer mission guidance".to_string(),
            proposed_changes: "Add a concise mission decomposition checklist.".to_string(),
            expected_benefit: "Higher first-pass task success.".to_string(),
            risk_assessment: "LOW".to_string(),
            generated_tests: vec![],
            status: coevo_core::skills::EvolutionProposalStatus::NeedsHumanReview,
            created_by_agent: "agent-founder-01".to_string(),
            created_at_ms: 1,
        };
        coevo_store::repos_opc::skill_evolution_repo::SkillEvolutionRepo::create_proposal(
            &company_pool,
            &proposal,
        )
        .await
        .unwrap();
        company_pool.close().await;

        let improvements_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{opc_id}/employees/agent-founder-01/improvements"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(improvements_response.status(), StatusCode::OK);
        let improvements_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(improvements_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(improvements_body
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["proposal_id"] == "proposal-company-growth-route-1"));

        let approve_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/employees/agent-founder-01/improvements/proposal-company-growth-route-1/approve"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(approve_response.status(), StatusCode::OK);

        let reject_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/employees/agent-founder-01/improvements/proposal-company-growth-route-1/reject"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reject_response.status(), StatusCode::OK);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_skill_install_and_delete_routes_use_canonical_path() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-skills-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Skills Co",
                            "mission": "path-scoped skills"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

        let install_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/skills/install"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "skill_id": "skill-mission-draft"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(install_response.status(), StatusCode::OK);
        let install_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(install_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(install_body["skill_name"], "skill-mission-draft");

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/skills"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(list_body
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["skill_name"] == "skill-mission-draft"));

        let delete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/companies/{opc_id}/skills/skill-mission-draft"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);

        let list_after_delete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/skills"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_after_delete_response.status(), StatusCode::OK);
        let list_after_delete_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_after_delete_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(list_after_delete_body
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["skill_name"] != "skill-mission-draft"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_skill_routes_use_company_scope_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-legacy-skill-scope-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Legacy Skill Scope Co",
                            "mission": "legacy header isolation"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

        let seed_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/skills/seed")
                    .header("x-coevo-opc-id", &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed_response.status(), StatusCode::OK);

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/skills")
                    .header("x-coevo-opc-id", &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(list_body
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["skill_id"] == "skill-mission-draft"
                || item["name"] == "skill-mission-draft"));

        let company_list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/skills"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_list_response.status(), StatusCode::OK);
        let company_list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(company_list_body
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["skill_name"] == "skill-mission-draft"));
        assert!(root
            .join(&opc_id)
            .join("skills")
            .join("skill-mission-draft")
            .join("SKILL.md")
            .exists());

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_conversation_routes_use_company_scope_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-legacy-conversation-scope-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let alpha_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Legacy Alpha",
                            "mission": "alpha conversations"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(alpha_response.status(), StatusCode::OK);
        let alpha_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(alpha_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let alpha = alpha_body["opc_id"].as_str().unwrap().to_string();

        let beta_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Legacy Beta",
                            "mission": "beta conversations"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(beta_response.status(), StatusCode::OK);
        let beta_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(beta_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let beta = beta_body["opc_id"].as_str().unwrap().to_string();

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/conversations")
                    .header("x-coevo-opc-id", &alpha)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "conversation_id": "conv-legacy-alpha",
                            "title": "Alpha legacy thread"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let legacy_list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/conversations")
                    .header("x-coevo-opc-id", &alpha)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(legacy_list_response.status(), StatusCode::OK);
        let legacy_list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(legacy_list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(legacy_list_body
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["conversation_id"] == "conv-legacy-alpha"));

        let alpha_company_list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{alpha}/conversations"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(alpha_company_list_response.status(), StatusCode::OK);
        let alpha_company_list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(alpha_company_list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(alpha_company_list_body
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["conversation_id"] == "conv-legacy-alpha"));

        let beta_company_list_response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{beta}/conversations"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(beta_company_list_response.status(), StatusCode::OK);
        let beta_company_list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(beta_company_list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(beta_company_list_body.as_array().unwrap().is_empty());

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_playground_run_route_executes_multiple_models_and_reads_company_prompt() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("desktop-test")
            .bind("OpenAICompatible")
            .bind("https://api.openai.com/v1")
            .bind("sk-test")
            .bind("sk-t****test")
            .bind("gpt-4o")
            .bind("gpt-4o-mini")
            .bind("o3-mini")
            .bind("gpt-4o")
            .bind(16384)
            .bind(0.2)
            .bind(30000)
            .bind(5.0)
            .bind(1)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-playground-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Playground Co",
                            "mission": "path-scoped playground"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

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

        let run_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/playground/run"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "agent_id": "agent-founder-01",
                            "user_input": "Explain product discovery in one sentence.",
                            "models": ["gpt-4o", "gpt-4o-mini"],
                            "temperature": 0.2,
                            "max_tokens": 128
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(run_response.status(), StatusCode::OK);
        let run_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(run_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!run_body["run_id"].as_str().unwrap_or_default().is_empty());
        let results = run_body["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|item| item["model"].as_str().is_some()));
        assert!(results.iter().all(|item| item.get("latency_ms").is_some()));
        assert!(results
            .iter()
            .all(|item| item["input_tokens"].as_u64().unwrap_or_default() > 0));
        assert!(results
            .iter()
            .all(|item| item["output_tokens"].as_u64().unwrap_or_default() > 0));
        assert!(results.iter().all(|item| item.get("cost_usd").is_some()));
        assert!(results
            .iter()
            .all(|item| item["cost_usd"].as_f64().unwrap_or_default() > 0.0));
        assert!(results.iter().all(|item| item["error"].is_null()));
        assert!(results.iter().all(|item| !item["output"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .is_empty()));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_playground_run_rejects_empty_model_list() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-playground-empty-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Canonical Playground Empty Co",
                            "mission": "path-scoped playground validation"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

        let run_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/playground/run"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "system_prompt": "You are a concise assistant.",
                            "user_input": "hello",
                            "models": []
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(run_response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_conversation_routes_are_isolated_per_company() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-conversations-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        async fn create_company(app: &axum::Router, name: &str) -> String {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/companies")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({ "name": name, "mission": "conversation isolation" })
                                .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap();
            body["opc_id"].as_str().unwrap().to_string()
        }

        let alpha = create_company(&app, "Alpha Conversations").await;
        let beta = create_company(&app, "Beta Conversations").await;

        let create_alpha = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{alpha}/conversations"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "conversation_id": "conv-alpha",
                            "title": "Alpha thread"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_alpha.status(), StatusCode::OK);

        let list_alpha = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{alpha}/conversations"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_alpha.status(), StatusCode::OK);
        let alpha_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_alpha.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(alpha_body.as_array().unwrap().len(), 1);
        assert_eq!(alpha_body[0]["conversation_id"], "conv-alpha");

        let list_beta = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{beta}/conversations"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_beta.status(), StatusCode::OK);
        let beta_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_beta.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(beta_body.as_array().unwrap().is_empty());

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
    async fn legacy_opc_employee_routes_can_isolate_by_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-legacy-header-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        async fn create_company(app: &axum::Router, name: &str) -> String {
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
                                "mission": "legacy header isolation"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap();
            body["opc_id"].as_str().unwrap().to_string()
        }

        let company_a = create_company(&app, "Legacy A").await;
        let company_b = create_company(&app, "Legacy B").await;

        for opc_id in [&company_a, &company_b] {
            let seed = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/opc/agents/employees/seed")
                        .header("x-coevo-opc-id", opc_id)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(seed.status(), StatusCode::OK);
        }

        let state_for_db = AppState::new(create_test_pool().await.unwrap(), root.clone());
        let pool_b = create_pool(
            &state_for_db
                .company_workspace
                .company_db_path(&company_b)
                .to_string_lossy(),
        )
        .await
        .unwrap();
        let mut base_employee = AgentEmployeeRepo::get(&pool_b, "agent-founder-01")
            .await
            .unwrap()
            .unwrap();
        pool_b.close().await;
        base_employee.agent_id = "agent-legacy-b-only".to_string();
        base_employee.display_name = "Legacy B Only".to_string();
        base_employee.passport.passport_id = "passport-agent-legacy-b-only".to_string();
        base_employee.system_prompt = "Legacy B prompt".to_string();

        let create_in_b = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/agents/employees")
                    .header("x-coevo-opc-id", &company_b)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&base_employee).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_in_b.status(), StatusCode::OK);

        let list_a = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/agents/employees")
                    .header("x-coevo-opc-id", &company_a)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list_a_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_a.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!list_a_body
            .as_array()
            .unwrap()
            .iter()
            .any(|row| { row["agent_id"] == "agent-legacy-b-only" }));

        let list_b = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/opc/agents/employees")
                    .header("x-coevo-opc-id", &company_b)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list_b_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_b.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(list_b_body
            .as_array()
            .unwrap()
            .iter()
            .any(|row| { row["agent_id"] == "agent-legacy-b-only" }));

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

        async fn create_company(app: &Router, name: &str, mission: &str) -> String {
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

        async fn list_employees(app: &Router, opc_id: &str) -> serde_json::Value {
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
        assert!(employee_dir.join("prompt_versions").exists());

        let employee_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/employees/{agent_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(employee_response.status(), StatusCode::OK);
        let employee: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(employee_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let passport_file: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(employee_dir.join("passport.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(employee["passport"], passport_file);

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
                    .uri(format!(
                        "/companies/{opc_id}/employees/{agent_id}/prompt/versions"
                    ))
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
        for entry in versions.as_array().unwrap() {
            let path = entry["path"].as_str().unwrap();
            assert!(path.starts_with(&format!("employees/{agent_id}/prompt_versions/")));
            assert!(!std::path::Path::new(path).is_absolute());
        }

        let rollback_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/employees/{agent_id}/prompt/rollback"
                    ))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({ "version": 1 }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rollback_response.status(), StatusCode::OK);
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            first_prompt
        );
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt_versions").join("current.txt"))
                .unwrap()
                .trim(),
            "1"
        );

        let get_prompt_response = app
            .clone()
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
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            prompt_body["content_md"].as_str().unwrap()
        );
        let versions_after_rollback = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{opc_id}/employees/{agent_id}/prompt/versions"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(versions_after_rollback.status(), StatusCode::OK);
        let versions_after_rollback_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(versions_after_rollback.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let versions = versions_after_rollback_body.as_array().unwrap();
        let version_1 = versions.iter().find(|entry| entry["version"] == 1).unwrap();
        let version_2 = versions.iter().find(|entry| entry["version"] == 2).unwrap();
        assert_eq!(version_1["current"], true);
        assert_eq!(version_2["current"], false);
        let company_pool =
            coevo_store::pool::create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
                .await
                .unwrap();
        let total_versions: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM prompt_versions WHERE prompt_id = ?")
                .bind(agent_id)
                .fetch_one(&company_pool)
                .await
                .unwrap();
        let published_version: i64 = sqlx::query_scalar(
            "SELECT version_number FROM prompt_versions WHERE prompt_id = ? AND status = 'PUBLISHED'",
        )
        .bind(agent_id)
        .fetch_one(&company_pool)
        .await
        .unwrap();
        company_pool.close().await;
        assert_eq!(total_versions, 2);
        assert_eq!(published_version, 1);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_prompt_version_routes_use_company_scope_header_and_sync_prompt_files() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-legacy-prompt-scope-{}",
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
                            "name": "Legacy Prompt Co",
                            "mission": "legacy prompt scope"
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
        let original_prompt = std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap();

        let create_version_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/prompts/versions")
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "prompt_id": agent_id,
                            "content": "Legacy prompt v2 from /opc/prompts",
                            "variables": [],
                            "change_summary": "legacy route"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_version_response.status(), StatusCode::OK);
        let created_version: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_version_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let version_id = created_version["version_id"].as_str().unwrap().to_string();
        assert_eq!(created_version["prompt_id"], agent_id);

        let list_versions_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/opc/prompts/{agent_id}/versions"))
                    .header("x-coevo-opc-id", &opc_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_versions_response.status(), StatusCode::OK);
        let list_versions_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_versions_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(list_versions_body
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["version_id"] == version_id));

        let publish_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/opc/prompts/versions/{version_id}/publish"))
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(publish_response.status(), StatusCode::OK);

        assert_ne!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            original_prompt
        );
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            "Legacy prompt v2 from /opc/prompts"
        );
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt_versions").join("current.txt"))
                .unwrap()
                .trim(),
            "1"
        );
        let company_pool =
            coevo_store::pool::create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
                .await
                .unwrap();
        let published_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM prompt_versions WHERE prompt_id = ? AND status = 'PUBLISHED'",
        )
        .bind(agent_id)
        .fetch_one(&company_pool)
        .await
        .unwrap();
        company_pool.close().await;
        assert_eq!(published_count, 1);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_employee_prompt_bodies_are_file_backed_not_persisted_in_db_plaintext() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-prompt-storage-invariant-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool.clone(), root.clone()));

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Prompt Storage Co",
                            "mission": "Prompt bodies must stay file-backed"
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
        let prompt_v1 = "You are prompt storage v1.";
        let prompt_v2 = "You are prompt storage v2.";
        for (prompt, summary) in [(prompt_v1, "initial"), (prompt_v2, "refine")] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!("/companies/{opc_id}/employees/{agent_id}/prompt"))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "system_prompt": prompt,
                                "change_summary": summary
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt.md")).unwrap(),
            prompt_v2
        );
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt_versions").join("v1.md")).unwrap(),
            prompt_v1
        );
        assert_eq!(
            std::fs::read_to_string(employee_dir.join("prompt_versions").join("v2.md")).unwrap(),
            prompt_v2
        );

        let company_pool =
            coevo_store::pool::create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
                .await
                .unwrap();
        let stored_system_prompt: String =
            sqlx::query_scalar("SELECT system_prompt FROM agent_employees WHERE agent_id = ?")
                .bind(agent_id)
                .fetch_one(&company_pool)
                .await
                .unwrap();
        let stored_prompt_contents: Vec<String> = sqlx::query_scalar(
            "SELECT content FROM prompt_versions WHERE prompt_id = ? ORDER BY version_number",
        )
        .bind(agent_id)
        .fetch_all(&company_pool)
        .await
        .unwrap();
        company_pool.close().await;

        assert_eq!(
            stored_system_prompt, prompt_v2,
            "expected agent_employees.system_prompt to retain the latest published prompt"
        );
        assert!(
            stored_prompt_contents
                .iter()
                .all(|content| content.trim().is_empty()),
            "expected prompt_versions.content to avoid plaintext prompt bodies, got {:?}",
            stored_prompt_contents
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_skill_routes_materialize_company_scoped_skill_files() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-company-skills-{}",
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
                            "name": "Skill Route Co",
                            "mission": "Verify company skill file layering"
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

        let seed_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/skills/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed_response.status(), StatusCode::OK);

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/skills"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let skills = list_body.as_array().unwrap();
        assert!(!skills.is_empty());
        assert!(skills.iter().all(|skill| skill["scope"] == "company"));
        assert!(root
            .join(&opc_id)
            .join("skills")
            .join("skill-mission-draft")
            .join("SKILL.md")
            .exists());

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn real_deepseek_integration_acceptance_paths_pass_when_env_is_configured() {
        let _lock = REAL_PROVIDER_LOCK.lock().unwrap();
        let Some(api_key) = real_provider_env("COEVO_REAL_DEEPSEEK_API_KEY") else {
            return;
        };
        keyring_core::set_default_store(keyring_core::sample::Store::new().unwrap());

        let result = async {
            let pool = create_test_pool().await.unwrap();
            run_migrations(&pool).await.unwrap();
            let root = std::env::temp_dir().join(format!(
                "coevo-real-deepseek-acceptance-{}",
                uuid::Uuid::new_v4()
            ));
            let workspace = root.join("workspace");
            std::fs::create_dir_all(&workspace).unwrap();
            std::fs::write(
                workspace.join("mission-notes.md"),
                "Mission evidence token: COEVO-B2B-7QX9. Strongest B2B signal: three enterprise pilots requested security review.",
            )
            .unwrap();
            let previous_workspace = std::env::var("COEVO_WORKSPACE_DIR").ok();
            std::env::set_var("COEVO_WORKSPACE_DIR", &workspace);
            let app = build_router(AppState::new(pool.clone(), root.clone()));

            let result = async {
                let candidate_model = real_provider_env("COEVO_REAL_DEEPSEEK_MODEL")
                    .unwrap_or_else(|| "deepseek-v4-flash".to_string());
                let candidate_base_url = real_provider_env("COEVO_REAL_DEEPSEEK_BASE_URL")
                    .unwrap_or_else(|| "https://api.deepseek.com/v1".to_string());

                let test_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/opc/models/test")
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "config": {
                                        "provider_id": "candidate-deepseek",
                                        "kind": "DeepSeek",
                                        "base_url": candidate_base_url,
                                        "api_key": api_key,
                                        "default_model": candidate_model,
                                        "fast_model": candidate_model,
                                        "reasoning_model": candidate_model,
                                        "structured_output_model": candidate_model,
                                        "max_tokens": 4096,
                                        "temperature": 0.2,
                                        "timeout_ms": 30000,
                                        "max_cost_per_task_usd": 5.0
                                    }
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(test_response.status(), StatusCode::OK);
                let test_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(test_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(test_body["provider_kind"], "deep_seek");
                assert!(test_body["latency_ms"].as_u64().unwrap_or_default() > 0);

                let persisted_count_before: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM model_provider_configs")
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                assert_eq!(persisted_count_before, 0);

                let (_real_key, base_url, model) =
                    configure_real_deepseek_provider_if_env(&pool).await.unwrap();

                let discover_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/opc/models/discover")
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "config": {
                                        "provider_id": "candidate-deepseek",
                                        "kind": "DeepSeek",
                                        "base_url": base_url,
                                        "api_key": api_key,
                                        "default_model": model,
                                        "fast_model": model,
                                        "reasoning_model": model,
                                        "structured_output_model": model,
                                        "max_tokens": 4096,
                                        "temperature": 0.2,
                                        "timeout_ms": 30000,
                                        "max_cost_per_task_usd": 5.0
                                    }
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(discover_response.status(), StatusCode::OK);
                let discover_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(discover_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                let discovered_models = discover_body["models"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|entry| entry["id"].as_str().map(|id| id.to_string()))
                    .collect::<Vec<_>>();
                assert!(discovered_models.iter().any(|id| id == &model));
                let playground_models = if discovered_models.len() >= 2 {
                    vec![discovered_models[0].clone(), discovered_models[1].clone()]
                } else {
                    vec![model.clone(), model.clone()]
                };

                let chat_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/opc/models/chat")
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "role": "Synthesizer",
                                    "messages": [
                                        {"role": "user", "content": "Explain product discovery in one concise sentence."}
                                    ],
                                    "temperature": 0.2,
                                    "max_tokens": 128
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let (chat_status, chat_text) = response_status_and_text(chat_response).await;
                assert_eq!(
                    chat_status,
                    StatusCode::OK,
                    "real deepseek /opc/models/chat failed: {}",
                    chat_text
                );
                let chat_body: serde_json::Value = serde_json::from_str(&chat_text).unwrap();
                assert!(!chat_body["content"].as_str().unwrap_or_default().trim().is_empty());
                assert!(chat_body["usage"]["total_tokens"].as_u64().unwrap_or_default() > 0);
                assert!(chat_body["latency_ms"].as_u64().unwrap_or_default() > 0);

                let company_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/companies")
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "name": "Real DeepSeek Co",
                                    "mission": "Acceptance verification"
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(company_response.status(), StatusCode::OK);
                let company_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                let opc_id = company_body["opc_id"].as_str().unwrap().to_string();

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

                let playground_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/companies/{opc_id}/playground/run"))
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "agent_id": "agent-founder-01",
                                    "user_input": "Explain product discovery in one sentence.",
                                    "models": playground_models,
                                    "temperature": 0.2,
                                    "max_tokens": 128
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let (playground_status, playground_text) =
                    response_status_and_text(playground_response).await;
                assert_eq!(
                    playground_status,
                    StatusCode::OK,
                    "real deepseek playground run failed: {}",
                    playground_text
                );
                let playground_body: serde_json::Value =
                    serde_json::from_str(&playground_text).unwrap();
                let playground_results = playground_body["results"].as_array().unwrap();
                assert_eq!(playground_results.len(), 2);
                if discovered_models.len() >= 2 {
                    assert_ne!(playground_results[0]["model"], playground_results[1]["model"]);
                    assert_ne!(
                        playground_results[0]["output"].as_str().unwrap_or_default().trim(),
                        playground_results[1]["output"].as_str().unwrap_or_default().trim()
                    );
                }
                assert!(playground_results.iter().all(playground_result_looks_real));

                let contract_hash = "c".repeat(64);
                insert_contract(&pool, &contract_hash).await;

                let work_order_id = format!("wo-real-{}", uuid::Uuid::new_v4().simple());
                let create_work_order_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/opc/work-orders")
                            .header("x-coevo-opc-id", &opc_id)
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "work_order_id": work_order_id,
                                    "contract_hash": contract_hash,
                                    "plan_hash": "d".repeat(64),
                                    "user_id": "default-founder",
                                    "opc_id": opc_id,
                                    "mission_intent": "Read mission-notes.md with the native file-readonly tool, then return the exact mission evidence token and the strongest B2B signal in one sentence. Do not answer unless you have inspected the file.",
                                    "selected_agents": ["agent-founder-01"],
                                    "selected_executors": [],
                                    "required_skills": []
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(create_work_order_response.status(), StatusCode::OK);
                let create_work_order_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(create_work_order_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(create_work_order_body["track"], "green");

                let now = chrono::Utc::now().timestamp_millis() as u64;
                WorkOrderRepo::create(
                    &pool,
                    &WorkOrder {
                        work_order_id: work_order_id.clone(),
                        conversation_id: None,
                        contract_hash: contract_hash.clone(),
                        plan_hash: "d".repeat(64),
                        user_id: "default-founder".to_string(),
                        opc_id: opc_id.clone(),
                        mission_intent: "Read mission-notes.md with the native file-readonly tool, then return the exact mission evidence token and the strongest B2B signal in one sentence. Do not answer unless you have inspected the file."
                            .to_string(),
                        selected_agents: vec!["agent-founder-01".to_string()],
                        selected_executors: vec![],
                        required_skills: vec![],
                        track: "green".to_string(),
                        status: WorkOrderStatus::Planned,
                        allowed_actions: vec!["read".to_string(), "analyze".to_string()],
                        restricted_actions: vec![
                            "delete".to_string(),
                            "payment".to_string(),
                            "production".to_string(),
                        ],
                        risk_summary:
                            "Server RiskGate: low-risk read/analyze intent. Green Track auto-execution is allowed."
                                .to_string(),
                        governance_proposal: None,
                        governance_verdict: None,
                        created_at_ms: now,
                        updated_at_ms: now,
                    },
                )
                .await
                .unwrap();

                let execute_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/opc/work-orders/{work_order_id}/execute"))
                            .header("x-coevo-opc-id", &opc_id)
                            .header("content-type", "application/json")
                            .body(Body::from("{}"))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let execute_status = execute_response.status();
                let execute_bytes = axum::body::to_bytes(execute_response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                assert_eq!(
                    execute_status,
                    StatusCode::OK,
                    "real deepseek execute failed: {}",
                    String::from_utf8_lossy(&execute_bytes)
                );
                let execute_body: serde_json::Value = serde_json::from_slice(&execute_bytes).unwrap();
                assert_eq!(execute_body["status"], "Completed");
                let worker_runs = execute_body["worker_runs"].as_array().unwrap();
                assert!(!worker_runs.is_empty());
                let run_id = worker_runs[0]["run_id"].as_str().unwrap().to_string();
                assert!(worker_runs[0]["total_tokens"].as_i64().unwrap_or_default() > 0);
                assert!(worker_runs[0]["total_cost_usd"].as_f64().unwrap_or_default() > 0.0);
                assert!(worker_runs[0]["latency_ms"].as_i64().unwrap_or_default() > 500);
                assert!(execute_body["summary"].as_str().unwrap_or_default().len() > 0);
                assert!(execute_body["reflection_id"].as_str().unwrap_or_default().len() > 0);

                let missing_header_events_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("GET")
                            .uri(format!("/opc/workers/runs/{run_id}/events"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(missing_header_events_response.status(), StatusCode::BAD_REQUEST);

                let missing_header_stream_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("GET")
                            .uri(format!("/opc/workers/runs/{run_id}/events/stream"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(missing_header_stream_response.status(), StatusCode::BAD_REQUEST);

                let run_events_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("GET")
                            .uri(format!("/opc/workers/runs/{run_id}/events"))
                            .header("x-coevo-opc-id", &opc_id)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(run_events_response.status(), StatusCode::OK);
                let run_events_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(run_events_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                let event_rows = run_events_body.as_array().unwrap();
                let event_types = event_rows
                    .iter()
                    .filter_map(|row| row["event_type"].as_str())
                    .collect::<Vec<_>>();
                let expected_stream_core_events = [
                    "ReasoningDelta",
                    "ContentDelta",
                    "ToolCallDelta",
                    "Usage",
                    "Done",
                ];
                for event in expected_stream_core_events {
                    assert!(
                        event_types.iter().any(|seen| *seen == event),
                        "real deepseek run did not persist {event}; seen={event_types:?}, rows={event_rows:?}"
                    );
                }
                let tool_call_rows = event_rows
                    .iter()
                    .filter(|row| row["event_type"] == "ToolCallDelta")
                    .collect::<Vec<_>>();
                assert!(
                    !tool_call_rows.is_empty(),
                    "expected at least one ToolCallDelta row in real deepseek run"
                );
                assert!(tool_call_rows.iter().any(|row| {
                    row["payload_json"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("file-readonly")
                }));
                let persisted_file_tool_calls: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id = ? AND tool_id = 'file-readonly'",
                )
                .bind(&run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert!(
                    persisted_file_tool_calls > 0,
                    "expected at least one persisted file-readonly tool call for real deepseek run"
                );

                let run_events_stream_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("GET")
                            .uri(format!("/opc/workers/runs/{run_id}/events/stream"))
                            .header("x-coevo-opc-id", &opc_id)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(run_events_stream_response.status(), StatusCode::OK);
                let mut stream_body = run_events_stream_response.into_body();
                let mut frames = Vec::new();
                let mut seen_stream_events = std::collections::BTreeSet::new();
                let stream_deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(45);
                while std::time::Instant::now() < stream_deadline {
                    let remaining = stream_deadline
                        .saturating_duration_since(std::time::Instant::now());
                    let frame_wait = remaining.min(std::time::Duration::from_secs(2));
                    let frame = tokio::time::timeout(frame_wait, stream_body.frame())
                        .await
                        .expect("timed out waiting for real SSE frame")
                        .expect("real SSE stream ended unexpectedly")
                        .expect("failed to read real SSE frame");
                    let bytes = frame.into_data().expect("expected real SSE data frame");
                    let text = std::str::from_utf8(&bytes).unwrap().to_string();
                    for event_type in text
                        .lines()
                        .filter_map(|line| line.strip_prefix("event: ").map(str::trim))
                    {
                        seen_stream_events.insert(event_type.to_string());
                    }
                    frames.push(text);
                    if seen_stream_events.contains("Done") {
                        break;
                    }
                }
                assert!(
                    seen_stream_events.contains("Done"),
                    "real SSE stream did not reach Done; seen={seen_stream_events:?}, frames={frames:?}"
                );
                for event in [
                    "ReasoningDelta",
                    "ContentDelta",
                    "ToolCallDelta",
                    "Usage",
                    "Done",
                ] {
                    assert!(
                        seen_stream_events.contains(event),
                        "real SSE stream missed {event}; seen={seen_stream_events:?}, frames={frames:?}"
                    );
                }
                assert!(frames.iter().any(|frame| frame.contains("file-readonly")));

                let create_kpi_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/companies/{opc_id}/employees/agent-founder-01/kpi"))
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "work_order_id": work_order_id,
                                    "scores": {
                                        "completion": 92,
                                        "speed": 84,
                                        "clarity": 90
                                    },
                                    "reviewer": "agent-pm-01",
                                    "comment": "Strong real-model execution evidence."
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(create_kpi_response.status(), StatusCode::OK);

                let create_meeting_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/companies/{opc_id}/meetings"))
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "topic": "Should we shift our primary product motion from B2C to B2B?",
                                    "participants": ["agent-founder-01", "agent-pm-01", "agent-critic-01"],
                                    "close_mode": "vote"
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let (create_meeting_status, create_meeting_text) =
                    response_status_and_text(create_meeting_response).await;
                assert_eq!(
                    create_meeting_status,
                    StatusCode::OK,
                    "real deepseek meeting creation failed: {}",
                    create_meeting_text
                );
                let create_meeting_body: serde_json::Value =
                    serde_json::from_str(&create_meeting_text).unwrap();
                let meeting_id = create_meeting_body["meeting_id"].as_str().unwrap();

                let meeting_detail_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("GET")
                            .uri(format!("/companies/{opc_id}/meetings/{meeting_id}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(meeting_detail_response.status(), StatusCode::OK);
                let meeting_detail_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(meeting_detail_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                let transcript = meeting_detail_body["transcript"].as_array().unwrap();
                assert!(transcript.len() >= 3);
                let distinct_texts = transcript
                    .iter()
                    .filter_map(|turn| turn["text"].as_str())
                    .collect::<std::collections::BTreeSet<_>>();
                assert!(distinct_texts.len() >= 3);
                let distinct_speakers = transcript
                    .iter()
                    .filter_map(|turn| turn["agent_id"].as_str())
                    .collect::<std::collections::BTreeSet<_>>();
                assert!(distinct_speakers.contains("agent-founder-01"));
                assert!(distinct_speakers.contains("agent-pm-01"));
                assert!(distinct_speakers.contains("agent-critic-01"));
                assert!(transcript.iter().any(|turn| {
                    turn["agent_id"] == "agent-critic-01" && turn["stance"] == "oppose"
                }));
                let resolution = meeting_detail_body["resolution_md"]
                    .as_str()
                    .unwrap_or_default();
                assert!(!resolution.trim().is_empty());
                assert!(!resolution.contains("Proceed with the discussion outcome"));
                assert!(root
                    .join(&opc_id)
                    .join(".meetings")
                    .join(meeting_id)
                    .join("resolution.md")
                    .exists());

                let create_dataset_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/companies/{opc_id}/eval/datasets"))
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "name": "Real Eval",
                                    "description": "DeepSeek acceptance"
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(create_dataset_response.status(), StatusCode::OK);
                let create_dataset_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(create_dataset_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                let dataset_id = create_dataset_body["dataset_id"].as_str().unwrap();

                let create_case_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/companies/{opc_id}/eval/datasets/{dataset_id}/cases"))
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "input": "Say alpha",
                                    "expected": "alpha",
                                    "tags": ["real"]
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(create_case_response.status(), StatusCode::OK);

                let run_eval_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/companies/{opc_id}/eval/run"))
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "target": {"kind": "prompt", "system_prompt": "Answer with the single word alpha."},
                                    "dataset_id": dataset_id,
                                    "judge_model": model,
                                    "exec_model": model,
                                    "metrics": ["accuracy", "relevance"]
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let (run_eval_status, run_eval_text) =
                    response_status_and_text(run_eval_response).await;
                assert_eq!(
                    run_eval_status,
                    StatusCode::OK,
                    "real deepseek eval run failed: {}",
                    run_eval_text
                );
                let run_eval_body: serde_json::Value = serde_json::from_str(&run_eval_text).unwrap();
                let experiment_id = run_eval_body["experiment_id"].as_str().unwrap();

                let eval_detail_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("GET")
                            .uri(format!("/companies/{opc_id}/eval/experiments/{experiment_id}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(eval_detail_response.status(), StatusCode::OK);
                let eval_detail_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(eval_detail_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(eval_detail_body["status"], "completed");
                assert!(
                    eval_detail_body["case_results"][0]["judge_reasoning"]
                        .as_str()
                        .unwrap_or_default()
                        .len()
                        > 0
                );
                assert!(eval_detail_body["overall_score"].as_f64().unwrap_or_default() > 0.0);

                let failed_source_pool = create_pool(
                    &root
                        .join(&opc_id)
                        .join("data.db")
                        .to_string_lossy(),
                )
                .await
                .unwrap();
                run_migrations(&failed_source_pool).await.unwrap();
                let failed_source_now = chrono::Utc::now().timestamp_millis() as u64;
                WorkOrderRepo::create(
                    &failed_source_pool,
                    &WorkOrder {
                        work_order_id: format!(
                            "wo-real-failed-source-{}",
                            uuid::Uuid::new_v4().simple()
                        ),
                        conversation_id: None,
                        contract_hash: "f".repeat(64),
                        plan_hash: "e".repeat(64),
                        user_id: "default-founder".to_string(),
                        opc_id: opc_id.clone(),
                        mission_intent:
                            "Attempted governed task failed because the file-reading plan broke."
                                .to_string(),
                        selected_agents: vec!["agent-founder-01".to_string()],
                        selected_executors: vec![],
                        required_skills: vec!["skill-mission-draft".to_string()],
                        track: "yellow".to_string(),
                        status: WorkOrderStatus::Failed,
                        allowed_actions: vec!["read".to_string()],
                        restricted_actions: vec!["delete".to_string()],
                        risk_summary:
                            "file-readonly planning drift caused the governed task to fail"
                                .to_string(),
                        governance_proposal: None,
                        governance_verdict: None,
                        created_at_ms: failed_source_now,
                        updated_at_ms: failed_source_now,
                    },
                )
                .await
                .unwrap();
                failed_source_pool.close().await;

                let evolution_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/companies/{opc_id}/skills/evolution/run"))
                            .header("content-type", "application/json")
                            .body(Body::from("{}"))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let (evolution_status, evolution_text) =
                    response_status_and_text(evolution_response).await;
                assert_eq!(
                    evolution_status,
                    StatusCode::OK,
                    "real deepseek evolution run failed: {}",
                    evolution_text
                );
                let evolution_body: serde_json::Value =
                    serde_json::from_str(&evolution_text).unwrap();
                assert!(!evolution_body["proposed_changes"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .is_empty());
                assert_ne!(
                    evolution_body["proposed_changes"].as_str().unwrap_or_default(),
                    "auto-patch"
                );

                let report_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("/companies/{opc_id}/reports/generate"))
                            .header("content-type", "application/json")
                            .body(Body::from(
                                serde_json::json!({"period": "daily"}).to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(report_response.status(), StatusCode::OK);
                let report_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(report_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                let report_id = report_body["report_id"].as_str().unwrap();

                let report_detail_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("GET")
                            .uri(format!("/companies/{opc_id}/reports/{report_id}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(report_detail_response.status(), StatusCode::OK);
                let report_detail_body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(report_detail_response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert!(report_detail_body["report_md"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Token Usage"));
                assert!(!report_detail_body["kpi_summary"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .is_empty());
                assert!(!report_detail_body["token_usage"]["by_agent"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .is_empty());

                let run_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM worker_runs WHERE work_order_id = ?",
                )
                .bind(&work_order_id)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert!(run_count > 0);

                let _ = WorkerRunRepo::get(&pool, &run_id).await.unwrap();
            }
            .await;

            if let Some(previous_workspace) = previous_workspace {
                std::env::set_var("COEVO_WORKSPACE_DIR", previous_workspace);
            } else {
                std::env::remove_var("COEVO_WORKSPACE_DIR");
            }
            std::fs::remove_dir_all(root).ok();
            result
        }
        .await;

        keyring_core::unset_default_store();
        result
    }

    #[tokio::test]
    async fn execute_work_order_returns_worker_run_summary_columns() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        ModelConfigRepo::upsert_config(
            &pool,
            "desktop-test",
            "OpenAICompatible",
            "https://api.deepseek.com/v1",
            "sk-test",
            "sk-test",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
            4096,
            0.2,
            30000,
            5.0,
        )
        .await
        .unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-router-summary-columns-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));
        let company_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Summary Columns Co",
                            "mission": "worker summary check"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(company_response.status(), StatusCode::OK);
        let company_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company_body["opc_id"].as_str().unwrap().to_string();
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
        let work_order_id = "wo-summary-columns";

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opc/work-orders")
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": work_order_id,
                            "contract_hash": "a".repeat(64),
                            "plan_hash": "b".repeat(64),
                            "user_id": "default-founder",
                            "opc_id": opc_id,
                            "mission_intent": "Analyze README.md",
                            "selected_agents": ["agent-founder-01"],
                            "selected_executors": [],
                            "required_skills": ["skill-mission-draft"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let execute_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/opc/work-orders/{work_order_id}/execute"))
                    .header("x-coevo-opc-id", &opc_id)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let execute_status = execute_response.status();
        let execute_bytes = axum::body::to_bytes(execute_response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            execute_status,
            StatusCode::OK,
            "summary-columns execute failed: {}",
            String::from_utf8_lossy(&execute_bytes)
        );
        let execute_body: serde_json::Value = serde_json::from_slice(&execute_bytes).unwrap();
        std::fs::remove_dir_all(root).ok();
        let worker_runs = execute_body["worker_runs"].as_array().unwrap();
        assert!(!worker_runs.is_empty());
        assert!(worker_runs[0].get("total_tokens").is_some());
        assert!(worker_runs[0].get("latency_ms").is_some());
        assert!(worker_runs[0].get("total_cost_usd").is_some());
    }

    #[test]
    fn real_playground_result_accepts_low_latency_when_usage_is_real() {
        let result = serde_json::json!({
            "model": "deepseek-v4-flash",
            "output": "Product discovery reduces uncertainty before teams commit roadmap effort.",
            "input_tokens": 21,
            "output_tokens": 13,
            "cost_usd": 0.0001,
            "latency_ms": 12,
            "error": serde_json::Value::Null,
        });

        assert!(playground_result_looks_real(&result));
    }
}
