//! Reputation scoring engine — difficulty-adjusted, anti-hindsight.
//! Per coevo whitepaper Section 6.

use coevo_core::reputation::{ReputationDimension, ReputationVector};
use coevo_store::repos::reputation_repo::ReputationRepo;
use sqlx::SqlitePool;

/// Learning rate η for reputation updates.
const ETA: f64 = 0.05;

/// The Reputation Engine.
pub struct ReputationEngine;

impl ReputationEngine {
    /// Get or initialize a reputation vector for an agent.
    pub async fn get_or_init(
        pool: &SqlitePool,
        agent_id: &str,
    ) -> Result<ReputationVector, ReputationError> {
        let row = ReputationRepo::find_by_agent(pool, agent_id).await?;
        match row {
            Some(r) => Ok(ReputationVector {
                agent_id: r.agent_id,
                task_domain_competence: r.task_domain_competence,
                uncertainty_honesty: r.uncertainty_honesty,
                policy_compliance: r.policy_compliance,
                resource_efficiency: r.resource_efficiency,
                last_updated_ms: r.last_updated_ms as u64,
                task_count: r.task_count as u64,
                high_difficulty_avoidance_count: r.high_difficulty_avoidance_count as u32,
            }),
            None => {
                let rv = ReputationVector::new(agent_id.to_string());
                ReputationRepo::upsert(pool, &rv).await?;
                Ok(rv)
            }
        }
    }

    /// Update reputation using the difficulty-adjusted formula:
    /// Reputation_{t+1} = Reputation_t + η · D_t · (Outcome − ExpectedOutcome)
    pub async fn update(
        pool: &SqlitePool,
        agent_id: &str,
        difficulty: f64,
        outcome: f64,
        expected_outcome: f64,
    ) -> Result<ReputationVector, ReputationError> {
        let difficulty = difficulty.clamp(1.0, 5.0);
        let mut rv = Self::get_or_init(pool, agent_id).await?;

        let delta = ETA * difficulty * (outcome - expected_outcome);
        rv.task_domain_competence = (rv.task_domain_competence + delta).clamp(0.0, 1.0);
        rv.task_count += 1;

        // Track difficulty avoidance
        if difficulty < 3.0 {
            rv.high_difficulty_avoidance_count += 1;
        } else {
            rv.high_difficulty_avoidance_count = 0;
        }

        // Apply half-life decay if continuously avoiding high-difficulty tasks
        if rv.high_difficulty_avoidance_count > 10 {
            rv.task_domain_competence *= 0.95;
            rv.uncertainty_honesty *= 0.95;
        }

        rv.last_updated_ms = chrono::Utc::now().timestamp_millis() as u64;

        ReputationRepo::upsert(pool, &rv).await?;
        Ok(rv)
    }

    /// Apply graded penalty based on error severity.
    pub async fn penalize(
        pool: &SqlitePool,
        agent_id: &str,
        dimension: ReputationDimension,
        error_severity: ErrorSeverity,
    ) -> Result<ReputationVector, ReputationError> {
        let mut rv = Self::get_or_init(pool, agent_id).await?;

        let penalty_pct = match error_severity {
            ErrorSeverity::Minor => 0.03,    // 2-5%
            ErrorSeverity::Moderate => 0.15, // 10-20%
            ErrorSeverity::Severe => 0.40,   // 30-50%
        };

        rv.penalize(dimension, penalty_pct);

        if matches!(error_severity, ErrorSeverity::Severe) {
            // Also impact policy compliance for severe errors
            rv.penalize(ReputationDimension::PolicyCompliance, penalty_pct * 0.5);
        }

        rv.last_updated_ms = chrono::Utc::now().timestamp_millis() as u64;
        ReputationRepo::upsert(pool, &rv).await?;
        Ok(rv)
    }

    /// Apply anti-hindsight bias: if bad outcome was due to dirty tool data,
    /// penalize the verification tool instead of the agent.
    pub async fn apply_anti_hindsight(
        pool: &SqlitePool,
        agent_id: &str,
        tool_id: &str,
        actual_outcome: f64,
        available_confidence_at_decision: f64,
        was_tool_data_dirty: bool,
    ) -> Result<(), ReputationError> {
        if was_tool_data_dirty {
            // Penalize the MCP tool's reputation, not the agent's
            let mut tool_rv = Self::get_or_init(pool, tool_id).await?;
            tool_rv.penalize(ReputationDimension::TaskDomainCompetence, 0.05);
            ReputationRepo::upsert(pool, &tool_rv).await?;
        } else {
            // Normal update for the agent
            Self::update(
                pool,
                agent_id,
                3.0,
                actual_outcome,
                available_confidence_at_decision,
            )
            .await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorSeverity {
    /// Format errors, excessive latency, improper retries — 2-5% penalty.
    Minor,
    /// Fact misuse, missing key evidence, direct blackboard write — 10-20%.
    Moderate,
    /// Unauthorized access, forged tool credentials, high-confidence hallucination — 30-50%.
    Severe,
}

#[derive(Debug, thiserror::Error)]
pub enum ReputationError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
