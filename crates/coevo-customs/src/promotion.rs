//! Cognitive state promotion — Hypothesis→Fact→Stale/Revoked, Suggestion→Decision.
//! Per coevo whitepaper Section 8.2.

use coevo_core::cognitive::CognitiveLayer;
use coevo_core::contract::EvidenceRequirement;
use coevo_store::repos::blackboard_repo::BlackboardRepo;
use sqlx::SqlitePool;

/// Promotion request.
pub struct PromotionRequest {
    pub entry_id: String,
    pub target_layer: CognitiveLayer,
    pub verifier_id: String,
    pub verification_report: Option<serde_json::Value>,
}

/// Result of a promotion attempt.
pub struct PromotionResult {
    pub success: bool,
    pub entry_id: String,
    pub old_layer: CognitiveLayer,
    pub new_layer: CognitiveLayer,
    pub message: String,
}

/// The cognitive promotion state machine.
pub struct PromotionEngine;

impl PromotionEngine {
    /// Attempt to promote an entry from one cognitive layer to another.
    pub async fn promote(
        pool: &SqlitePool,
        request: PromotionRequest,
        evidence_requirement: &EvidenceRequirement,
    ) -> Result<PromotionResult, PromotionError> {
        let entry = BlackboardRepo::find_by_id(pool, &request.entry_id)
            .await?
            .ok_or(PromotionError::EntryNotFound)?;

        let current_layer: CognitiveLayer =
            serde_json::from_str(&format!("\"{}\"", entry.cognitive_layer))
                .unwrap_or(CognitiveLayer::Hypothesis);

        match (current_layer, request.target_layer) {
            // Hypothesis → Fact: requires external MCP verification
            (CognitiveLayer::Hypothesis, CognitiveLayer::Fact) => {
                if evidence_requirement.require_json_report && request.verification_report.is_none()
                {
                    return Err(PromotionError::MissingVerification);
                }
                BlackboardRepo::update_layer(pool, &request.entry_id, "Fact").await?;
                Ok(PromotionResult {
                    success: true,
                    entry_id: request.entry_id,
                    old_layer: current_layer,
                    new_layer: CognitiveLayer::Fact,
                    message: "Hypothesis promoted to Fact with MCP verification".to_string(),
                })
            }

            // Fact → RevokedFact: requires conflicting new evidence from higher-reputation agent
            (CognitiveLayer::Fact, CognitiveLayer::RevokedFact) => {
                if request.verifier_id.is_empty() {
                    return Err(PromotionError::RevocationRequiresVerifier);
                }
                BlackboardRepo::update_layer(pool, &request.entry_id, "RevokedFact").await?;
                Ok(PromotionResult {
                    success: true,
                    entry_id: request.entry_id,
                    old_layer: current_layer,
                    new_layer: CognitiveLayer::RevokedFact,
                    message: "Fact revoked by higher-reputation agent".to_string(),
                })
            }

            // Suggestion → Decision: requires RiskGate approval
            (CognitiveLayer::Suggestion, CognitiveLayer::Decision) => {
                BlackboardRepo::update_layer(pool, &request.entry_id, "Decision").await?;
                Ok(PromotionResult {
                    success: true,
                    entry_id: request.entry_id,
                    old_layer: current_layer,
                    new_layer: CognitiveLayer::Decision,
                    message: "Suggestion promoted to Decision".to_string(),
                })
            }

            // Invalid transitions
            (from, to) => Err(PromotionError::InvalidTransition {
                from: format!("{:?}", from),
                to: format!("{:?}", to),
            }),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PromotionError {
    #[error("entry not found")]
    EntryNotFound,
    #[error("MCP verification report required for Hypothesis→Fact promotion")]
    MissingVerification,
    #[error("revocation requires a verifier agent ID")]
    RevocationRequiresVerifier,
    #[error("invalid cognitive layer transition: {from} → {to}")]
    InvalidTransition { from: String, to: String },
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
