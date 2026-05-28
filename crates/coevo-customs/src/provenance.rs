//! Provenance envelope validation.
//! Per coevo whitepaper Section 8.1.

use coevo_core::cognitive::ProvenanceEnvelope;

/// Validate a provenance envelope for fact-layer writes.
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

#[derive(Debug, thiserror::Error)]
pub enum ProvenanceError {
    #[error("missing required provenance field: {0}")]
    MissingField(&'static str),
    #[error("TTL must be positive, got {0}")]
    InvalidTtl(i64),
    #[error("MCP verification report required but missing")]
    MissingMcpVerification,
}
