//! Provenance envelope and cognitive layer types.
//! Per coevo whitepaper Section 8.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Cognitive layer for blackboard state entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CognitiveLayer {
    Hypothesis,
    Fact,
    Suggestion,
    Decision,
    StaleFact,
    RevokedFact,
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
