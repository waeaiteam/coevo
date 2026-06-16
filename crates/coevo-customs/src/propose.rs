//! CognitiveCustoms.Propose — fact change proposals with optimistic concurrency control.
//! Per coevo whitepaper Section 2.3 and Section 8.

use coevo_core::cognitive::*;
use coevo_core::contract::EvidenceRequirement;
use coevo_core::metadata::CommonMetadataHeader;
use sqlx::SqlitePool;

use crate::blackboard::Blackboard;
use crate::dependency::{CognitiveDependencyGraph, EdgeType};
use crate::provenance::verify_provenance;

/// The CognitiveCustoms Propose interface.
pub struct CognitiveCustoms;

impl CognitiveCustoms {
    /// Receive a fact change proposal with optimistic concurrency control.
    /// Returns a commit receipt if successful.
    pub async fn propose(
        pool: &SqlitePool,
        target_key: &str,
        expected_version: u64,
        proposed_value: &serde_json::Value,
        cognitive_layer: CognitiveLayer,
        provenance_envelope: &ProvenanceEnvelope,
        metadata: &CommonMetadataHeader,
        evidence_requirement: &EvidenceRequirement,
        dependency_entry_ids: &[String],
    ) -> Result<CommitReceiptSpec, ProposeError> {
        // ---- Guard 1: Validate provenance envelope ----
        // Fact/Decision writes carry a cryptographically-verified provenance
        // envelope. The signature is checked against the proposed value so a
        // signature cannot be replayed onto different content.
        if cognitive_layer == CognitiveLayer::Fact || cognitive_layer == CognitiveLayer::Decision {
            let require_mcp = evidence_requirement.require_json_report;
            verify_provenance(provenance_envelope, proposed_value, require_mcp, None)
                .map_err(|e| ProposeError::ProvenanceValidationFailed(e.to_string()))?;
        }

        // ---- Guard 2: Cannot directly write to Fact layer without MCP verification ----
        if cognitive_layer == CognitiveLayer::Fact
            && provenance_envelope.verification_tool_urn.is_empty()
        {
            return Err(ProposeError::CognitiveBoundViolation {
                detail: "Cannot write directly to FACT layer without MCP-verified provenance"
                    .to_string(),
            });
        }

        // ---- Guard 3: Optimistic concurrency control ----
        // New key: expected_version must be 0.
        // Existing key: expected_version must match current version.
        let existing = Blackboard::read(pool, target_key).await?;
        if let Some(ref entry) = existing {
            if entry.version != expected_version {
                return Err(ProposeError::VersionMismatch {
                    expected: expected_version,
                    actual: entry.version,
                });
            }
        } else {
            // New key — only expected_version 0 is valid
            if expected_version != 0 {
                return Err(ProposeError::VersionMismatch {
                    expected: expected_version,
                    actual: 0,
                });
            }
        }

        // ---- Guard 4: Write ----
        let ttl_ms = if cognitive_layer == CognitiveLayer::Fact {
            Some(provenance_envelope.ttl_seconds * 1000)
        } else {
            None
        };

        let (entry_id, new_version) = Blackboard::write(
            pool,
            target_key,
            proposed_value,
            cognitive_layer,
            provenance_envelope,
            &metadata.contract_hash,
            ttl_ms,
        )
        .await?;

        // ---- Add dependency edges if this entry depends on others ----
        for dep_entry_id in dependency_entry_ids {
            let edge_type = match cognitive_layer {
                CognitiveLayer::Hypothesis => EdgeType::HypothesisDependsOnFact,
                CognitiveLayer::Suggestion => EdgeType::SuggestionDependsOnHypothesis,
                CognitiveLayer::Decision => EdgeType::DecisionDependsOnSuggestion,
                _ => EdgeType::DecisionDependsOnFact,
            };
            CognitiveDependencyGraph::add_edge(pool, &entry_id, dep_entry_id, edge_type).await?;
        }

        Ok(CommitReceiptSpec {
            commit_index: new_version,
            new_version,
            key: target_key.to_string(),
            committed_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProposeError {
    #[error("provenance validation failed: {0}")]
    ProvenanceValidationFailed(String),
    #[error("cognitive boundary violation: {detail}")]
    CognitiveBoundViolation { detail: String },
    #[error("version mismatch: expected {expected}, actual {actual}")]
    VersionMismatch { expected: u64, actual: u64 },
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("blackboard error: {0}")]
    Blackboard(#[from] super::blackboard::BlackboardError),
    #[error("dependency graph error: {0}")]
    Dependency(#[from] super::dependency::DependencyGraphError),
}
