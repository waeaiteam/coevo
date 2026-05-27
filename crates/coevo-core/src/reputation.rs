//! Reputation vector types — Reputation v1 Profile.
//! Per coevo whitepaper Section 6.

use serde::{Deserialize, Serialize};

/// Four-dimensional reputation vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationVector {
    pub agent_id: String,
    /// Task domain competence: historical success rate in a specific sub-domain.
    pub task_domain_competence: f64,
    /// Uncertainty honesty: probability of reporting errors when out-of-depth.
    pub uncertainty_honesty: f64,
    /// Policy compliance: frequency of boundary violations or risk gate triggers.
    pub policy_compliance: f64,
    /// Resource efficiency: token/latency deviation from industry baseline.
    pub resource_efficiency: f64,
    /// When this vector was last updated (Unix ms).
    pub last_updated_ms: u64,
    /// Number of tasks evaluated.
    pub task_count: u64,
    /// Consecutive high-difficulty tasks avoided (for decay).
    pub high_difficulty_avoidance_count: u32,
}

impl ReputationVector {
    /// Create a default neutral vector for a new agent.
    pub fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            task_domain_competence: 0.5,
            uncertainty_honesty: 0.5,
            policy_compliance: 1.0,
            resource_efficiency: 0.5,
            last_updated_ms: 0,
            task_count: 0,
            high_difficulty_avoidance_count: 0,
        }
    }

    /// Apply a penalty to a specific dimension (clamped to [0.0, 1.0]).
    pub fn penalize(&mut self, dimension: ReputationDimension, penalty_pct: f64) {
        let target = match dimension {
            ReputationDimension::TaskDomainCompetence => &mut self.task_domain_competence,
            ReputationDimension::UncertaintyHonesty => &mut self.uncertainty_honesty,
            ReputationDimension::PolicyCompliance => &mut self.policy_compliance,
            ReputationDimension::ResourceEfficiency => &mut self.resource_efficiency,
        };
        *target = (*target - penalty_pct).max(0.0);
    }

    /// Apply a reward to a specific dimension (clamped to [0.0, 1.0]).
    pub fn reward(&mut self, dimension: ReputationDimension, reward_pct: f64) {
        let target = match dimension {
            ReputationDimension::TaskDomainCompetence => &mut self.task_domain_competence,
            ReputationDimension::UncertaintyHonesty => &mut self.uncertainty_honesty,
            ReputationDimension::PolicyCompliance => &mut self.policy_compliance,
            ReputationDimension::ResourceEfficiency => &mut self.resource_efficiency,
        };
        *target = (*target + reward_pct).min(1.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReputationDimension {
    TaskDomainCompetence,
    UncertaintyHonesty,
    PolicyCompliance,
    ResourceEfficiency,
}
