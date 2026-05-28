//! Resolution Engine — conflict resolution with ADR-A generation.
//! Per coevo whitepaper Section 10.

use coevo_core::decision::*;
use coevo_core::stance::*;
use coevo_store::repos::adr_repo::AdrRepo;
use sqlx::SqlitePool;

use crate::adr::AdrBuilder;
use crate::stopper::{ResolutionStopper, RuleBasedStopper};

/// The Resolution Engine processes stance matrices and produces verdicts + ADR-A records.
pub struct ResolutionEngine {
    stopper: Box<dyn ResolutionStopper>,
}

impl ResolutionEngine {
    pub fn new() -> Self {
        Self {
            stopper: Box::new(RuleBasedStopper::new(3)),
        }
    }

    pub fn with_stopper(mut self, stopper: Box<dyn ResolutionStopper>) -> Self {
        self.stopper = stopper;
        self
    }

    /// Process a stance matrix and return a resolution decision.
    pub async fn process(
        &self,
        pool: &SqlitePool,
        stance_matrix: &StanceMatrixSpec,
    ) -> Result<ResolutionDecisionSpec, ResolutionError> {
        // Separate support and opposition
        let supporters: Vec<_> = stance_matrix
            .stances
            .iter()
            .filter(|s| s.position == StancePosition::Support)
            .collect();
        let opposers: Vec<_> = stance_matrix
            .stances
            .iter()
            .filter(|s| s.position == StancePosition::Oppose)
            .collect();

        // Check for veto power among opposers
        let has_veto = opposers.iter().any(|s| s.has_veto);
        if has_veto {
            let veto_agents: Vec<String> = opposers
                .iter()
                .filter(|s| s.has_veto)
                .map(|s| s.agent_id.clone())
                .collect();

            let adr = AdrBuilder::new(
                &stance_matrix.issue,
                &stance_matrix.context_ref,
                supporters
                    .first()
                    .map(|s| s.agent_id.as_str())
                    .unwrap_or("unknown"),
            )
            .with_veto_blockers(veto_agents.clone())
            .with_rejected_alternatives(vec![RejectedAlternative {
                option_id: "vetoed".to_string(),
                description: "Proposal blocked by veto".to_string(),
                rejection_reason: format!("Veto by agents: {:?}", veto_agents),
                evidence_chain: vec![],
            }])
            .build();

            let adr_id = adr.decision_id.clone();
            AdrRepo::insert(pool, &adr)
                .await
                .map_err(|e| ResolutionError::StorageError(e.to_string()))?;

            return Ok(ResolutionDecisionSpec {
                decision: ResolutionVerdict::Deadlocked,
                resolved_path: None,
                blocking_nodes: veto_agents,
                adr: Some(adr),
                escalation: Some(EscalationAction {
                    target: "HumanArbitrator".to_string(),
                    reason: "Veto power exercised".to_string(),
                    requires_human_arbitration: true,
                }),
            });
        }

        // Calculate consensus
        let total_support: f64 = supporters.iter().map(|s| s.weight).sum();
        let total_opposition: f64 = opposers.iter().map(|s| s.weight).sum();
        let total_weight = total_support + total_opposition;

        let consensus_ratio = if total_weight > 0.0 {
            total_support / total_weight
        } else {
            0.0
        };

        // Check if the stopper says we should stop
        let max_round = stance_matrix
            .stances
            .iter()
            .map(|s| s.round)
            .max()
            .unwrap_or(0);

        let should_stop =
            self.stopper
                .should_stop(max_round, consensus_ratio, stance_matrix.max_rounds);

        if consensus_ratio >= 0.66 {
            // Consensus reached
            let adr = AdrBuilder::new(
                &stance_matrix.issue,
                &stance_matrix.context_ref,
                supporters
                    .first()
                    .map(|s| s.agent_id.as_str())
                    .unwrap_or("unknown"),
            )
            .with_consensus(ConflictStatus::Consensus, consensus_ratio)
            .with_rejected_alternatives(
                opposers
                    .iter()
                    .map(|s| RejectedAlternative {
                        option_id: s.agent_id.clone(),
                        description: "Opposition stance".to_string(),
                        rejection_reason: "Insufficient weight".to_string(),
                        evidence_chain: vec![],
                    })
                    .collect(),
            )
            .build();

            let adr_id = adr.decision_id.clone();
            AdrRepo::insert(pool, &adr)
                .await
                .map_err(|e| ResolutionError::StorageError(e.to_string()))?;

            Ok(ResolutionDecisionSpec {
                decision: ResolutionVerdict::Resolved,
                resolved_path: Some("consensus_path".to_string()),
                blocking_nodes: vec![],
                adr: Some(adr),
                escalation: None,
            })
        } else if should_stop && consensus_ratio >= 0.4 {
            // Compromise
            let compromises: Vec<_> = stance_matrix
                .stances
                .iter()
                .filter_map(|s| s.compromise_proposal.clone())
                .collect();

            let adr = AdrBuilder::new(
                &stance_matrix.issue,
                &stance_matrix.context_ref,
                supporters
                    .first()
                    .map(|s| s.agent_id.as_str())
                    .unwrap_or("unknown"),
            )
            .with_consensus(ConflictStatus::TradeOff, consensus_ratio)
            .with_rejected_alternatives(vec![])
            .build();

            let adr_id = adr.decision_id.clone();
            AdrRepo::insert(pool, &adr)
                .await
                .map_err(|e| ResolutionError::StorageError(e.to_string()))?;

            Ok(ResolutionDecisionSpec {
                decision: ResolutionVerdict::Compromised,
                resolved_path: compromises.first().cloned(),
                blocking_nodes: vec![],
                adr: Some(adr),
                escalation: None,
            })
        } else if should_stop {
            // Deadlock
            let adr = AdrBuilder::new(
                &stance_matrix.issue,
                &stance_matrix.context_ref,
                supporters
                    .first()
                    .map(|s| s.agent_id.as_str())
                    .unwrap_or("unknown"),
            )
            .with_consensus(ConflictStatus::Divergence, consensus_ratio)
            .build();

            let adr_id = adr.decision_id.clone();
            AdrRepo::insert(pool, &adr)
                .await
                .map_err(|e| ResolutionError::StorageError(e.to_string()))?;

            Ok(ResolutionDecisionSpec {
                decision: ResolutionVerdict::Deadlocked,
                resolved_path: None,
                blocking_nodes: opposers.iter().map(|s| s.agent_id.clone()).collect(),
                adr: Some(adr),
                escalation: Some(EscalationAction {
                    target: "HumanArbitrator".to_string(),
                    reason: "Irreconcilable divergence".to_string(),
                    requires_human_arbitration: true,
                }),
            })
        } else {
            // Continue debate
            Ok(ResolutionDecisionSpec {
                decision: ResolutionVerdict::Escalated,
                resolved_path: None,
                blocking_nodes: vec![],
                adr: None,
                escalation: Some(EscalationAction {
                    target: "ResolutionEngine".to_string(),
                    reason: format!(
                        "Continue debate, round {}/{}",
                        max_round + 1,
                        stance_matrix.max_rounds
                    ),
                    requires_human_arbitration: false,
                }),
            })
        }
    }
}

impl Default for ResolutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolutionError {
    #[error("storage error: {0}")]
    StorageError(String),
}
