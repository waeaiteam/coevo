//! Provenance envelope and cognitive layer types.
//! Per coevo whitepaper Section 8.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Cognitive layer for blackboard state entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CognitiveLayer {
    Hypothesis,
    Fact,
    Suggestion,
    Decision,
    StaleFact,
    RevokedFact,
}

/// Error returned when a stored cognitive-layer string is not recognized.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown cognitive layer: {0:?}")]
pub struct UnknownCognitiveLayer(pub String);

impl CognitiveLayer {
    /// The canonical database/wire string for this layer (PascalCase).
    pub fn as_db_str(&self) -> &'static str {
        match self {
            CognitiveLayer::Hypothesis => "Hypothesis",
            CognitiveLayer::Fact => "Fact",
            CognitiveLayer::Suggestion => "Suggestion",
            CognitiveLayer::Decision => "Decision",
            CognitiveLayer::StaleFact => "StaleFact",
            CognitiveLayer::RevokedFact => "RevokedFact",
        }
    }
}

impl std::str::FromStr for CognitiveLayer {
    type Err = UnknownCognitiveLayer;

    /// Parse a cognitive layer from its canonical PascalCase string. Returns an
    /// error (never a silent default) on any unrecognized value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Hypothesis" => Ok(CognitiveLayer::Hypothesis),
            "Fact" => Ok(CognitiveLayer::Fact),
            "Suggestion" => Ok(CognitiveLayer::Suggestion),
            "Decision" => Ok(CognitiveLayer::Decision),
            "StaleFact" => Ok(CognitiveLayer::StaleFact),
            "RevokedFact" => Ok(CognitiveLayer::RevokedFact),
            other => Err(UnknownCognitiveLayer(other.to_string())),
        }
    }
}

/// Provenance envelope — mandatory metadata wrapping any fact-layer write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceEnvelope {
    /// Agent that generated this fact.
    pub source_agent_id: String,
    /// URN of the external deterministic verification tool/server.
    pub verification_tool_urn: String,
    /// Environment scope and tenant boundaries.
    pub environmental_scope: EnvironmentalScope,
    /// Freshness TTL in seconds. Fact auto-degrades on expiry.
    pub ttl_seconds: i64,
    /// Tamper-proof cryptographic signature.
    pub cryptographic_signature: String,
    /// MCP verification report (JSON), if required.
    pub verification_report: Option<serde_json::Value>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentalScope {
    pub environment: Environment,
    pub tenant_id: String,
}

impl ProvenanceEnvelope {
    /// Build the deterministic canonical bytes that the `cryptographic_signature`
    /// signs over.
    ///
    /// The signature binds the envelope to the **content** it vouches for plus
    /// the provenance identity fields, so a signature cannot be replayed onto a
    /// different value, agent, tool, or timestamp.
    ///
    /// Signed payload (canonical JSON, keys sorted lexicographically by
    /// [`crate::crypto::canonical_bytes`]):
    /// ```json
    /// {
    ///   "content_hash": "<sha256 hex of the entry value's canonical bytes>",
    ///   "created_at":   "<RFC3339 / ISO-8601 UTC>",
    ///   "purpose":      "coevo:provenance:v1",
    ///   "source_agent_id": "<agent id>",
    ///   "verification_tool_urn": "<urn>"
    /// }
    /// ```
    pub fn signing_payload(&self, content_hash: &str) -> serde_json::Value {
        serde_json::json!({
            "purpose": "coevo:provenance:v1",
            "content_hash": content_hash,
            "source_agent_id": self.source_agent_id,
            "verification_tool_urn": self.verification_tool_urn,
            "created_at": self.created_at.to_rfc3339(),
        })
    }

    /// Canonical signing bytes for this envelope over the given content hash.
    pub fn signing_bytes(&self, content_hash: &str) -> Vec<u8> {
        crate::crypto::canonical_bytes(&self.signing_payload(content_hash))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Development,
    Staging,
    Production,
}

/// A blackboard entry submitted via CognitiveCustoms.Propose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackboardEntry {
    /// The state key being written.
    pub key: String,
    /// Current version (for optimistic concurrency control).
    pub version: u64,
    /// The value being proposed.
    pub value: serde_json::Value,
    /// Target cognitive layer.
    pub layer: CognitiveLayer,
    /// Provenance envelope for fact-layer writes.
    pub provenance: ProvenanceEnvelope,
    /// Whether this entry is still valid (for dependency graph tracking).
    pub is_valid: bool,
    /// When this entry was created.
    pub created_at_ms: u64,
    /// When this entry expires (based on TTL).
    pub expires_at_ms: Option<u64>,
}

/// Commit receipt returned on successful Propose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitReceiptSpec {
    /// Unique commit index.
    pub commit_index: u64,
    /// New state version after commit.
    pub new_version: u64,
    /// The key that was written.
    pub key: String,
    /// Timestamp of commit.
    pub committed_at_ms: u64,
}
