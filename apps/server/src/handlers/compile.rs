use axum::{extract::State, Json};
use coevo_core::metadata::CommonMetadataHeader;
use coevo_core::problem::ProblemDetails;
use coevo_mcl::compiler::MCLCompiler;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CompileRequest {
    pub user_intent: String,
    pub requested_mode: String,
    pub parent_contract_hash: Option<String>,
}

#[derive(Debug, Serialize)]
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

    Ok(Json(CompileResponse {
        contract: serde_json::to_value(&result.contract).unwrap(),
        contract_hash: result.contract_hash,
        ambiguity_score: result.ambiguity_score,
        compile_warnings: result.compile_warnings,
    }))
}
