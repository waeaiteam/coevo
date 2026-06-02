//! Evolution Scheduler: trigger evolution, verify, approve, reject.

use coevo_core::skills::*;

pub struct EvolutionScheduler;

impl EvolutionScheduler {
    /// Determine the next action for an evolution proposal.
    pub fn schedule(
        proposal: &SkillEvolutionProposal,
        eval: &SkillEvalResult,
    ) -> EvolutionProposalStatus {
        if !eval.passed {
            return EvolutionProposalStatus::NeedsHumanReview;
        }
        if proposal.risk_assessment.contains("HIGH") {
            return EvolutionProposalStatus::NeedsHumanReview;
        }
        if proposal.risk_assessment.contains("MEDIUM") && eval.score < 0.9 {
            return EvolutionProposalStatus::NeedsHumanReview;
        }
        // Green auto-apply
        EvolutionProposalStatus::Approved
    }
}
