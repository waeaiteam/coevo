use axum::{extract::State, Json};
use coevo_core::contract::hash_contract;
use coevo_core::problem::ProblemDetails;
use coevo_router::pcdt::PcdtRouter;
use coevo_store::repos::{contract_repo::ContractRepo, plan_repo::PlanRepo};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct RouteRequest {
    /// Hash returned by `/mcl/compile`; routing must anchor plans to a persisted contract.
    pub contract_hash: String,
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
    State(state): State<AppState>,
    Json(req): Json<RouteRequest>,
) -> Result<Json<RouteResponse>, ProblemDetails> {
    if req.contract_hash.is_empty() {
        return Err(ProblemDetails::mcl_compilation_error(
            "/router/route",
            "contract_hash is required to anchor the execution plan",
        ));
    }
    let persisted_contract = ContractRepo::find_by_hash(&state.pool, &req.contract_hash)
        .await
        .map_err(|e| {
            ProblemDetails::internal_error(
                "/router/route",
                &format!("failed to load contract anchor: {}", e),
            )
        })?;
    if persisted_contract.is_none() {
        return Err(ProblemDetails::mcl_compilation_error(
            "/router/route",
            "CONTRACT_ANCHOR_NOT_FOUND: compile the contract before routing",
        ));
    }

    let contract: coevo_core::contract::MCLSpec =
        serde_json::from_value(req.contract).map_err(|e| {
            ProblemDetails::mcl_compilation_error(
                "/router/route",
                &format!("invalid contract: {}", e),
            )
        })?;
    let computed_contract_hash = hash_contract(&contract).map_err(|e| {
        ProblemDetails::mcl_compilation_error(
            "/router/route",
            &format!("failed to hash contract: {}", e),
        )
    })?;
    if computed_contract_hash != req.contract_hash {
        return Err(ProblemDetails::mcl_compilation_error(
            "/router/route",
            "CONTRACT_HASH_MISMATCH: route request contract does not match the persisted contract_hash",
        ));
    }

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

    PlanRepo::insert_or_ignore(&state.pool, &result.plan, &req.contract_hash)
        .await
        .map_err(|e| {
            ProblemDetails::internal_error(
                "/router/route",
                &format!("failed to persist execution plan anchor: {}", e),
            )
        })?;

    Ok(Json(RouteResponse {
        plan: serde_json::to_value(&result.plan).unwrap(),
        plan_hash: result.plan_hash,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::extract::State;
    use coevo_core::contract::*;
    use coevo_store::{
        migrate::run_migrations,
        pool::create_test_pool,
        repos::{contract_repo::ContractRepo, plan_repo::PlanRepo},
    };

    fn contract() -> MCLSpec {
        MCLSpec {
            mcl_version: "1.0".to_string(),
            mcl_state: ContractState::DraftContract,
            parent_contract_hash: "0".repeat(64),
            goal_tree: GoalTree {
                root: GoalNode {
                    id: "root".to_string(),
                    description: "route persistence test".to_string(),
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
                max_risk_score: 0.3,
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
        }
    }

    #[tokio::test]
    async fn route_plan_persists_execution_plan_anchor() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
        let contract = contract();
        let contract_hash = hash_contract(&contract).unwrap();
        ContractRepo::insert(&pool, &contract, &contract_hash)
            .await
            .unwrap();

        let Json(response) = route_plan(
            State(state),
            Json(RouteRequest {
                contract_hash: contract_hash.clone(),
                contract: serde_json::to_value(&contract).unwrap(),
                agent_ids: vec!["agent-founder-01".to_string()],
            }),
        )
        .await
        .unwrap();

        let stored = PlanRepo::find_by_hash(&pool, &response.plan_hash)
            .await
            .unwrap();
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().contract_hash, contract_hash);
    }

    #[tokio::test]
    async fn route_plan_rejects_missing_contract_anchor() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());
        let missing_hash = "f".repeat(64);
        let err = route_plan(
            State(state),
            Json(RouteRequest {
                contract_hash: missing_hash,
                contract: serde_json::to_value(contract()).unwrap(),
                agent_ids: vec!["agent-founder-01".to_string()],
            }),
        )
        .await
        .unwrap_err();

        assert!(err.detail.contains("CONTRACT_ANCHOR_NOT_FOUND"));
    }

    #[tokio::test]
    async fn route_plan_rejects_contract_hash_mismatch() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());
        let contract_a = contract();
        let contract_hash = hash_contract(&contract_a).unwrap();
        ContractRepo::insert(&pool, &contract_a, &contract_hash)
            .await
            .unwrap();
        let mut contract_b = contract();
        contract_b.goal_tree.root.description = "different route request".to_string();

        let err = route_plan(
            State(state),
            Json(RouteRequest {
                contract_hash,
                contract: serde_json::to_value(contract_b).unwrap(),
                agent_ids: vec!["agent-founder-01".to_string()],
            }),
        )
        .await
        .unwrap_err();

        assert!(err.detail.contains("CONTRACT_HASH_MISMATCH"));
    }
}
