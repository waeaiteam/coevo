use axum::{extract::State, Json};
use coevo_core::cognitive::*;
use coevo_core::contract::EvidenceRequirement;
use coevo_core::metadata::CommonMetadataHeader;
use coevo_core::problem::ProblemDetails;
use coevo_customs::propose::CognitiveCustoms;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ProposeRequest {
    pub target_key: String,
    pub expected_version: u64,
    pub proposed_value: serde_json::Value,
    pub cognitive_layer: CognitiveLayer,
    pub provenance_envelope: ProvenanceEnvelope,
    pub dependency_entry_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProposeResponse {
    pub receipt: CommitReceiptSpec,
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

    let evidence_req = EvidenceRequirement {
        minimum_level: "unit_tests_passing".to_string(),
        require_json_report: true,
    };

    let receipt = CognitiveCustoms::propose(
        &state.pool,
        &req.target_key,
        req.expected_version,
        &req.proposed_value,
        req.cognitive_layer,
        &req.provenance_envelope,
        &meta,
        &evidence_req,
        &req.dependency_entry_ids,
    )
    .await
    .map_err(|e| match e {
        coevo_customs::propose::ProposeError::VersionRequired => {
            ProblemDetails::version_required("/customs/propose", "expected_version must be provided")
        }
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

    Ok(Json(ProposeResponse { receipt }))
}
