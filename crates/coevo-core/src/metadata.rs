//! Common Metadata Header — every API request must carry these fields.
//! Per coevo whitepaper Section 1.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::track::Track;

/// The mandatory metadata header carried by every coevo control-plane request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonMetadataHeader {
    /// UUIDv4 idempotency key. Locked for 1800s in distributed cache.
    pub idempotency_key: String,
    /// W3C Trace Context format: version-TraceID-SpanID-TraceFlags.
    pub traceparent: String,
    /// Optional system-specific tracing context. PII must be pseudonymized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
    /// Ed25519 signature over request body, with KID for JWKS chain.
    /// Conditional: required in Red Track, Emergency Lease, cross-org A2A,
    /// high-risk fact promotion, Human Override, and ADR-A signing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_identity_proof: Option<String>,
    /// SHA256 of the current active MCL contract.
    pub contract_hash: String,
    /// SHA256 of the current Institution Policy version.
    pub policy_version: String,
    /// UUIDv4 tenant identifier for wire-level physical isolation.
    pub tenant_id: String,
    /// SHA256 of the current ExecutionPlan.
    pub execution_plan_hash: String,
    /// Actor role from compliance passport (Proposer, Critic, Synthesizer, etc.).
    pub actor_role: String,
    /// Request time-to-live in milliseconds.
    pub request_ttl_ms: u64,
    /// UUIDv4 of the upstream triggering parent event.
    pub causality_parent_id: String,
    /// Whether this is a dry-run, simulation replay, or shadow test.
    pub replay_mode: bool,
    /// Unix timestamp in milliseconds. Deviations >5000ms from gateway NTP are blocked.
    pub timestamp: u64,
}

impl CommonMetadataHeader {
    /// Create a new header with sensible defaults and generated UUIDs.
    pub fn new(
        contract_hash: String,
        policy_version: String,
        tenant_id: String,
        execution_plan_hash: String,
        actor_role: String,
    ) -> Self {
        let now = Utc::now().timestamp_millis() as u64;
        Self {
            idempotency_key: Uuid::new_v4().to_string(),
            traceparent: format!(
                "00-{}-{}-01",
                hex::encode(Uuid::new_v4().as_bytes()),
                hex::encode(&rand::random::<[u8; 8]>())
            ),
            tracestate: None,
            caller_identity_proof: None,
            contract_hash,
            policy_version,
            tenant_id,
            execution_plan_hash,
            actor_role,
            request_ttl_ms: 30_000,
            causality_parent_id: Uuid::new_v4().to_string(),
            replay_mode: false,
            timestamp: now,
        }
    }

    /// Validate that all required fields are present and well-formed.
    pub fn validate(&self) -> Result<(), HeaderValidationError> {
        if Uuid::parse_str(&self.idempotency_key).is_err() {
            return Err(HeaderValidationError::InvalidIdempotencyKey);
        }
        if self.contract_hash.len() != 64 || !self.contract_hash.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(HeaderValidationError::InvalidContractHash);
        }
        if self.policy_version.len() != 64
            || !self.policy_version.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(HeaderValidationError::InvalidPolicyVersion);
        }
        if Uuid::parse_str(&self.tenant_id).is_err() {
            return Err(HeaderValidationError::InvalidTenantId);
        }
        if self.execution_plan_hash.len() != 64
            || !self
                .execution_plan_hash
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        {
            return Err(HeaderValidationError::InvalidExecutionPlanHash);
        }
        if self.actor_role.is_empty() {
            return Err(HeaderValidationError::MissingActorRole);
        }
        let now = Utc::now().timestamp_millis() as u64;
        let drift = if self.timestamp > now {
            self.timestamp - now
        } else {
            now - self.timestamp
        };
        if drift > 5000 {
            return Err(HeaderValidationError::TimestampDriftExceeded { drift_ms: drift });
        }
        Ok(())
    }

    /// Per-track validation with additional constraints.
    /// Red Track MUST have caller_identity_proof.
    /// All tracks: traceparent must match W3C format, causality_parent_id must be valid UUID,
    /// request_ttl_ms must be positive.
    pub fn validate_for_track(&self, track: Track) -> Result<(), HeaderValidationError> {
        // Base validation first
        self.validate()?;

        // traceparent must match W3C format: 00-<trace-id>-<span-id>-<flags>
        let parts: Vec<&str> = self.traceparent.split('-').collect();
        if parts.len() != 4
            || parts[0] != "00"
            || parts[1].len() != 32
            || parts[2].len() != 16
            || parts[3].len() != 2
        {
            return Err(HeaderValidationError::InvalidTraceparent);
        }

        // causality_parent_id must be valid UUID
        if Uuid::parse_str(&self.causality_parent_id).is_err() {
            return Err(HeaderValidationError::InvalidCausalityParentId);
        }

        // request_ttl_ms must be positive
        if self.request_ttl_ms == 0 {
            return Err(HeaderValidationError::InvalidRequestTtl);
        }

        // Red Track: caller_identity_proof is mandatory
        if track == Track::Red {
            match &self.caller_identity_proof {
                None => return Err(HeaderValidationError::MissingCallerIdentityProof),
                Some(proof) if proof.is_empty() => {
                    return Err(HeaderValidationError::MissingCallerIdentityProof);
                }
                _ => {}
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HeaderValidationError {
    #[error("idempotency_key is not a valid UUIDv4")]
    InvalidIdempotencyKey,
    #[error("contract_hash must be a 64-char hex-encoded SHA256")]
    InvalidContractHash,
    #[error("policy_version must be a 64-char hex-encoded SHA256")]
    InvalidPolicyVersion,
    #[error("tenant_id is not a valid UUIDv4")]
    InvalidTenantId,
    #[error("execution_plan_hash must be a 64-char hex-encoded SHA256")]
    InvalidExecutionPlanHash,
    #[error("actor_role must not be empty")]
    MissingActorRole,
    #[error("timestamp drift {drift_ms}ms exceeds 5000ms gateway threshold")]
    TimestampDriftExceeded { drift_ms: u64 },
    #[error("traceparent must match W3C format: 00-<32hex>-<16hex>-<flags>")]
    InvalidTraceparent,
    #[error("causality_parent_id must be a valid UUIDv4")]
    InvalidCausalityParentId,
    #[error("request_ttl_ms must be positive")]
    InvalidRequestTtl,
    #[error("caller_identity_proof is required for Red Track")]
    MissingCallerIdentityProof,
}
