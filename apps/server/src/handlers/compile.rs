use axum::{extract::State, Json};
use coevo_core::metadata::CommonMetadataHeader;
use coevo_core::problem::ProblemDetails;
use coevo_mcl::compiler::MCLCompiler;
use coevo_store::repos::contract_repo::ContractRepo;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CompileRequest {
    pub user_intent: String,
    pub requested_mode: String,
    pub parent_contract_hash: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CompileResponse {
    pub contract: serde_json::Value,
    pub contract_hash: String,
    pub ambiguity_score: f64,
    pub compile_warnings: Vec<String>,
}

/// POST /mcl/compile
#[utoipa::path(
    post,
    path = "/mcl/compile",
    tag = "MCL",
    request_body = CompileRequest,
    responses(
        (status = 200, description = "Contract compiled successfully", body = CompileResponse),
        (status = 403, description = "Institution policy violation", body = ProblemDetails),
        (status = 422, description = "Compilation error", body = ProblemDetails)
    )
)]
pub async fn compile_contract(
    State(state): State<AppState>,
    Json(req): Json<CompileRequest>,
) -> Result<Json<CompileResponse>, ProblemDetails> {
    let meta = CommonMetadataHeader::new(
        "compiling".to_string(),
        state.policy_engine.policy_version(),
        "default-tenant".to_string(),
        "compiling".to_string(),
        "Synthesizer".to_string(),
    );

    let compiler = MCLCompiler::new();
    let result = compiler
        .compile(
            &req.user_intent,
            &req.requested_mode,
            req.parent_contract_hash.as_deref(),
            &meta,
        )
        .await
        .map_err(|e| match e {
            coevo_mcl::compiler::CompileError::InstitutionViolation { violations, .. } => {
                ProblemDetails::mcl_institution_violation(
                    "/mcl/compile",
                    &format!("policy violations: {:?}", violations),
                )
            }
            coevo_mcl::compiler::CompileError::AmbiguityTooHigh { score, detail } => {
                ProblemDetails::mcl_compilation_error(
                    "/mcl/compile",
                    &format!("ambiguity {:.2}: {}", score, detail),
                )
            }
            _ => ProblemDetails::mcl_compilation_error("/mcl/compile", &e.to_string()),
        })?;

    ContractRepo::insert_or_ignore(&state.pool, &result.contract, &result.contract_hash)
        .await
        .map_err(|e| {
            ProblemDetails::internal_error(
                "/mcl/compile",
                &format!("failed to persist contract anchor: {}", e),
            )
        })?;

    Ok(Json(CompileResponse {
        contract: serde_json::to_value(&result.contract).unwrap(),
        contract_hash: result.contract_hash,
        ambiguity_score: result.ambiguity_score,
        compile_warnings: result.compile_warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::extract::State;
    use coevo_store::{
        migrate::run_migrations, pool::create_test_pool, repos::contract_repo::ContractRepo,
    };

    #[tokio::test]
    async fn compile_contract_persists_contract_anchor() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone());

        let Json(response) = compile_contract(
            State(state),
            Json(CompileRequest {
                user_intent: "Analyze workspace launch readiness".to_string(),
                requested_mode: "DRAFT".to_string(),
                parent_contract_hash: None,
            }),
        )
        .await
        .unwrap();

        let stored = ContractRepo::find_by_hash(&pool, &response.contract_hash)
            .await
            .unwrap();
        assert!(stored.is_some());
    }
}
