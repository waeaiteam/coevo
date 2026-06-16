//! Provenance envelope validation and signing.
//! Per coevo whitepaper Section 8.1.
//!
//! A provenance envelope is the cryptographic vouch attached to any fact-layer
//! write. Prior to production this was validated by a non-empty-string check
//! only; it is now verified with real Ed25519 signatures.
//!
//! # Canonical signing payload
//!
//! The `cryptographic_signature` is an Ed25519 signature (lowercase hex) over
//! the canonical bytes of:
//!
//! ```json
//! {
//!   "content_hash": "<sha256 hex of the proposed value's canonical bytes>",
//!   "created_at":   "<RFC3339 UTC>",
//!   "purpose":      "coevo:provenance:v1",
//!   "source_agent_id": "<agent id>",
//!   "verification_tool_urn": "<urn>"
//! }
//! ```
//!
//! Keys are sorted lexicographically (see
//! [`coevo_core::crypto::canonical_bytes`]). The payload is constructed by
//! [`coevo_core::cognitive::ProvenanceEnvelope::signing_payload`], so signer and
//! verifier cannot drift.
//!
//! The signature is verified against the platform public key by default, or
//! against a per-agent key supplied by an optional resolver.

use coevo_core::cognitive::ProvenanceEnvelope;
use coevo_core::crypto;

/// Resolves the verifying (public) key for a given `source_agent_id`.
///
/// Return the agent's Ed25519 public key as lowercase hex, or `None` to fall
/// back to the platform public key. Defaulting to the platform key matches the
/// Alpha deployment where all envelopes are signed by the platform signer.
pub type AgentKeyResolver<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// Validate a provenance envelope for fact-layer writes, **including real
/// Ed25519 signature verification** over the canonical payload bound to
/// `content` (the value being written).
///
/// `key_resolver` lets callers map `source_agent_id` to a per-agent public key;
/// when it is `None` (or returns `None`) the platform public key is used.
pub fn verify_provenance(
    envelope: &ProvenanceEnvelope,
    content: &serde_json::Value,
    require_mcp_verification: bool,
    key_resolver: Option<&AgentKeyResolver<'_>>,
) -> Result<(), ProvenanceError> {
    // ---- Structural checks ----
    if envelope.source_agent_id.is_empty() {
        return Err(ProvenanceError::MissingField("source_agent_id"));
    }
    if envelope.verification_tool_urn.is_empty() {
        return Err(ProvenanceError::MissingField("verification_tool_urn"));
    }
    if envelope.cryptographic_signature.is_empty() {
        return Err(ProvenanceError::MissingField("cryptographic_signature"));
    }
    if envelope.ttl_seconds <= 0 {
        return Err(ProvenanceError::InvalidTtl(envelope.ttl_seconds));
    }
    if require_mcp_verification && envelope.verification_report.is_none() {
        return Err(ProvenanceError::MissingMcpVerification);
    }

    // ---- Cryptographic verification ----
    let content_hash = crypto::sha256_hex(&crypto::canonical_bytes(content));
    let signing_bytes = envelope.signing_bytes(&content_hash);

    let public_key = key_resolver
        .and_then(|resolve| resolve(&envelope.source_agent_id))
        .unwrap_or_else(crypto::platform_public_key_hex);

    if !crypto::verify(
        &public_key,
        &signing_bytes,
        &envelope.cryptographic_signature,
    ) {
        return Err(ProvenanceError::SignatureVerificationFailed);
    }

    Ok(())
}

/// Backwards-compatible structural-only validation (no content / signature
/// cryptography). Retained for callers that only need field presence checks;
/// fact-layer writes MUST use [`verify_provenance`].
pub fn validate_provenance(
    envelope: &ProvenanceEnvelope,
    require_mcp_verification: bool,
) -> Result<(), ProvenanceError> {
    if envelope.source_agent_id.is_empty() {
        return Err(ProvenanceError::MissingField("source_agent_id"));
    }
    if envelope.verification_tool_urn.is_empty() {
        return Err(ProvenanceError::MissingField("verification_tool_urn"));
    }
    if envelope.cryptographic_signature.is_empty() {
        return Err(ProvenanceError::MissingField("cryptographic_signature"));
    }
    if envelope.ttl_seconds <= 0 {
        return Err(ProvenanceError::InvalidTtl(envelope.ttl_seconds));
    }
    if require_mcp_verification && envelope.verification_report.is_none() {
        return Err(ProvenanceError::MissingMcpVerification);
    }
    Ok(())
}

/// Produces correctly-signed [`ProvenanceEnvelope`]s using the platform signing
/// key. This is the canonical way for track runners and agents to mint
/// provenance that will pass [`verify_provenance`].
///
/// # Example
/// ```no_run
/// use coevo_customs::provenance::ProvenanceSigner;
/// use coevo_core::cognitive::{Environment, EnvironmentalScope};
///
/// let value = serde_json::json!({"result": "ok"});
/// let envelope = ProvenanceSigner::new("agent-1", "urn:mcp:tool:unit-test-runner")
///     .with_scope(EnvironmentalScope {
///         environment: Environment::Development,
///         tenant_id: "tenant-1".into(),
///     })
///     .with_ttl_seconds(3600)
///     .with_verification_report(serde_json::json!({"passed": true}))
///     .sign(&value);
/// ```
pub struct ProvenanceSigner {
    source_agent_id: String,
    verification_tool_urn: String,
    scope: coevo_core::cognitive::EnvironmentalScope,
    ttl_seconds: i64,
    verification_report: Option<serde_json::Value>,
}

impl ProvenanceSigner {
    /// Start a signer for the given source agent and verification tool URN.
    pub fn new(
        source_agent_id: impl Into<String>,
        verification_tool_urn: impl Into<String>,
    ) -> Self {
        Self {
            source_agent_id: source_agent_id.into(),
            verification_tool_urn: verification_tool_urn.into(),
            scope: coevo_core::cognitive::EnvironmentalScope {
                environment: coevo_core::cognitive::Environment::Development,
                tenant_id: String::new(),
            },
            ttl_seconds: 3600,
            verification_report: None,
        }
    }

    /// Set the environmental scope (environment + tenant).
    pub fn with_scope(mut self, scope: coevo_core::cognitive::EnvironmentalScope) -> Self {
        self.scope = scope;
        self
    }

    /// Set the freshness TTL in seconds.
    pub fn with_ttl_seconds(mut self, ttl_seconds: i64) -> Self {
        self.ttl_seconds = ttl_seconds;
        self
    }

    /// Attach an MCP verification report.
    pub fn with_verification_report(mut self, report: serde_json::Value) -> Self {
        self.verification_report = Some(report);
        self
    }

    /// Produce a signed envelope vouching for `content`.
    ///
    /// The `created_at` timestamp is fixed at signing time and included in the
    /// signed payload, so the returned envelope's signature verifies as-is.
    pub fn sign(self, content: &serde_json::Value) -> ProvenanceEnvelope {
        let created_at = chrono::Utc::now();
        let mut envelope = ProvenanceEnvelope {
            source_agent_id: self.source_agent_id,
            verification_tool_urn: self.verification_tool_urn,
            environmental_scope: self.scope,
            ttl_seconds: self.ttl_seconds,
            cryptographic_signature: String::new(),
            verification_report: self.verification_report,
            created_at,
        };
        let content_hash = crypto::sha256_hex(&crypto::canonical_bytes(content));
        envelope.cryptographic_signature = crypto::sign(&envelope.signing_bytes(&content_hash));
        envelope
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProvenanceError {
    #[error("missing required provenance field: {0}")]
    MissingField(&'static str),
    #[error("TTL must be positive, got {0}")]
    InvalidTtl(i64),
    #[error("MCP verification report required but missing")]
    MissingMcpVerification,
    #[error("provenance signature verification failed")]
    SignatureVerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_core::cognitive::{Environment, EnvironmentalScope};

    fn scope() -> EnvironmentalScope {
        EnvironmentalScope {
            environment: Environment::Development,
            tenant_id: "tenant-test".to_string(),
        }
    }

    #[test]
    fn signed_envelope_verifies() {
        let value = serde_json::json!({"a": 1, "b": [2, 3]});
        let envelope = ProvenanceSigner::new("agent-1", "urn:mcp:tool:test")
            .with_scope(scope())
            .with_verification_report(serde_json::json!({"passed": true}))
            .sign(&value);

        assert!(verify_provenance(&envelope, &value, true, None).is_ok());
    }

    #[test]
    fn tampered_content_fails_verification() {
        let value = serde_json::json!({"a": 1});
        let envelope = ProvenanceSigner::new("agent-1", "urn:mcp:tool:test")
            .with_scope(scope())
            .sign(&value);

        let tampered = serde_json::json!({"a": 2});
        let err = verify_provenance(&envelope, &tampered, false, None).unwrap_err();
        assert!(matches!(err, ProvenanceError::SignatureVerificationFailed));
    }

    #[test]
    fn empty_signature_is_rejected() {
        let value = serde_json::json!({"a": 1});
        let mut envelope = ProvenanceSigner::new("agent-1", "urn:mcp:tool:test")
            .with_scope(scope())
            .sign(&value);
        envelope.cryptographic_signature = String::new();
        let err = verify_provenance(&envelope, &value, false, None).unwrap_err();
        assert!(matches!(
            err,
            ProvenanceError::MissingField("cryptographic_signature")
        ));
    }

    #[test]
    fn resolver_with_wrong_key_fails() {
        let value = serde_json::json!({"a": 1});
        let envelope = ProvenanceSigner::new("agent-1", "urn:mcp:tool:test")
            .with_scope(scope())
            .sign(&value);

        // Resolver returns a different (invalid) key for the agent.
        let resolver = |_id: &str| Some("00".repeat(32));
        let err = verify_provenance(&envelope, &value, false, Some(&resolver)).unwrap_err();
        assert!(matches!(err, ProvenanceError::SignatureVerificationFailed));
    }
}
