//! Skill Generator: produce SkillEvolutionProposal from failure analysis.
//! v1: rule-based mock. Structured for future LLM integration.

use coevo_core::skills::*;
use uuid::Uuid;

pub struct SkillGenerator;

impl SkillGenerator {
    pub fn generate_from_failure(
        analysis: &FailureAnalysis,
        target_skill_id: &str,
        created_by_agent: &str,
    ) -> SkillEvolutionProposal {
        let proposal_type = match analysis.category {
            FailureCategory::MissingCapability => EvolutionProposalType::CreateNewSkill,
            FailureCategory::BadPromptProcedure | FailureCategory::WrongToolUse => EvolutionProposalType::PatchSkill,
            FailureCategory::MemoryStale | FailureCategory::PolicyViolation => EvolutionProposalType::PatchSkill,
            _ => EvolutionProposalType::PatchSkill,
        };

        let now = chrono::Utc::now().timestamp_millis() as u64;

        SkillEvolutionProposal {
            proposal_id: format!("evol-{}", Uuid::new_v4()),
            source_type: EvolutionSourceType::Failure,
            source_refs: vec![],
            target_skill_id: target_skill_id.to_string(),
            proposal_type,
            diagnosis: format!("{:?}: {}", analysis.category, analysis.root_cause),
            proposed_changes: "Auto-generated patch based on failure analysis".to_string(),
            expected_benefit: "Prevent recurrence of diagnosed failure".to_string(),
            risk_assessment: self.assess_risk(analysis),
            generated_tests: vec![],
            status: EvolutionProposalStatus::Draft,
            created_by_agent: created_by_agent.to_string(),
            created_at_ms: now,
        }
    }

    fn assess_risk(&self, analysis: &FailureAnalysis) -> String {
        match analysis.category {
            FailureCategory::PolicyViolation | FailureCategory::ExternalExecutorFailure => {
                "HIGH — requires human review".to_string()
            }
            FailureCategory::HallucinatedFact | FailureCategory::OverConfidentDecision => {
                "MEDIUM — needs verifier validation".to_string()
            }
            _ => "LOW — auto-verifiable".to_string(),
        }
    }
}
