use axum::{extract::State, Json};
use coevo_core::contract::EvidenceRequirement;
use coevo_core::metadata::CommonMetadataHeader;
use coevo_core::problem::ProblemDetails;
use coevo_customs::propose::CognitiveCustoms;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ProposeRequest {
    pub target_key: String,
    pub expected_version: u64,
    pub proposed_value: serde_json::Value,
    /// CognitiveLayer as JSON (simplified for OpenAPI).
    pub cognitive_layer: serde_json::Value,
    /// ProvenanceEnvelope as JSON (simplified for OpenAPI).
    pub provenance_envelope: serde_json::Value,
    pub dependency_entry_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProposeResponse {
    pub commit_index: u64,
    pub new_version: u64,
    pub key: String,
    pub committed_at_ms: u64,
}

/// POST /customs/propose
#[utoipa::path(
    post,
    path = "/customs/propose",
    tag = "Customs",
    request_body = ProposeRequest,
    responses(
        (status = 200, description = "Proposal committed", body = ProposeResponse),
        (status = 403, description = "Cognitive boundary violation", body = ProblemDetails),
        (status = 409, description = "Write conflict", body = ProblemDetails),
        (status = 412, description = "Version mismatch", body = ProblemDetails),
        (status = 428, description = "Version required", body = ProblemDetails)
    )
)]
pub async fn propose_fact(
    State(state): State<AppState>,
    Json(req): Json<ProposeRequest>,
) -> Result<Json<ProposeResponse>, ProblemDetails> {
    let meta = CommonMetadataHeader::new(
        "in-flight".to_string(),
        state.policy_engine.policy_version(),
        "default-tenant".to_string(),
        "in-flight".to_string(),
        "Synthesizer".to_string(),
    );

    let cognitive_layer: coevo_core::cognitive::CognitiveLayer =
        serde_json::from_value(req.cognitive_layer).map_err(|e| {
            ProblemDetails::mcl_compilation_error(
                "/customs/propose",
                &format!("invalid cognitive_layer: {}", e),
            )
        })?;

    let provenance_envelope: coevo_core::cognitive::ProvenanceEnvelope =
        serde_json::from_value(req.provenance_envelope).map_err(|e| {
            ProblemDetails::mcl_compilation_error(
                "/customs/propose",
                &format!("invalid provenance_envelope: {}", e),
            )
        })?;

    let evidence_req = EvidenceRequirement {
        minimum_level: "unit_tests_passing".to_string(),
        require_json_report: true,
    };

    let receipt = CognitiveCustoms::propose(
        &state.pool,
        &req.target_key,
        req.expected_version,
        &req.proposed_value,
        cognitive_layer,
        &provenance_envelope,
        &meta,
        &evidence_req,
        &req.dependency_entry_ids,
    )
    .await
    .map_err(|e| match e {
        coevo_customs::propose::ProposeError::VersionMismatch { expected, actual } => {
            ProblemDetails::version_mismatch(
                "/customs/propose",
                &format!("expected {}, actual {}", expected, actual),
            )
        }
        coevo_customs::propose::ProposeError::CognitiveBoundViolation { detail } => {
            ProblemDetails::cognitive_bound_violation("/customs/propose", &detail)
        }
        _ => ProblemDetails::cognitive_write_conflict("/customs/propose", &e.to_string()),
    })?;

    Ok(Json(ProposeResponse {
        commit_index: receipt.commit_index,
        new_version: receipt.new_version,
        key: receipt.key,
        committed_at_ms: receipt.committed_at_ms,
    }))
}
