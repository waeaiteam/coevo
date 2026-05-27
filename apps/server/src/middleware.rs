//! Axum middleware for CommonMetadataHeader validation.
//! Per coevo whitepaper Section 1.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use coevo_core::metadata::CommonMetadataHeader;
use coevo_core::problem::ProblemDetails;

/// Extract and validate the CommonMetadataHeader from request headers.
pub async fn validate_metadata(req: Request, next: Next) -> Response {
    let headers = req.headers();

    // Extract header fields from HTTP headers
    let idempotency_key = headers
        .get("x-coevo-idempotency-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let traceparent = headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let contract_hash = headers
        .get("x-coevo-contract-hash")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let policy_version = headers
        .get("x-coevo-policy-version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let tenant_id = headers
        .get("x-coevo-tenant-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let execution_plan_hash = headers
        .get("x-coevo-execution-plan-hash")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let actor_role = headers
        .get("x-coevo-actor-role")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let causality_parent_id = headers
        .get("x-coevo-causality-parent-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let caller_identity_proof = headers
        .get("x-coevo-caller-identity-proof")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let tracestate = headers
        .get("tracestate")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let request_ttl_ms: u64 = headers
        .get("x-coevo-request-ttl-ms")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(30000);

    let replay_mode = headers
        .get("x-coevo-replay-mode")
        .and_then(|v| v.to_str().ok())
        .map(|s| s == "true")
        .unwrap_or(false);

    let timestamp: u64 = headers
        .get("x-coevo-timestamp")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64);

    // If idempotency_key is empty, auto-generate for convenience
    let idempotency_key = if idempotency_key.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        idempotency_key
    };

    let traceparent = if traceparent.is_empty() {
        format!(
            "00-{}-{}-01",
            hex::encode(uuid::Uuid::new_v4().as_bytes()),
            hex::encode(&rand::random::<[u8; 8]>())
        )
    } else {
        traceparent
    };

    let metadata = CommonMetadataHeader {
        idempotency_key,
        traceparent,
        tracestate,
        caller_identity_proof,
        contract_hash,
        policy_version,
        tenant_id,
        execution_plan_hash,
        actor_role,
        request_ttl_ms,
        causality_parent_id: if causality_parent_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            causality_parent_id
        },
        replay_mode,
        timestamp,
    };

    // Validate
    if let Err(e) = metadata.validate() {
        let problem = ProblemDetails::forbidden("validate_metadata", &e.to_string());
        return (StatusCode::FORBIDDEN, axum::Json(problem)).into_response();
    }

    // Inject metadata into request extensions
    let mut req = req;
    req.extensions_mut().insert(metadata);
    next.run(req).await
}
