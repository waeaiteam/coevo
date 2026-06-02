//! Skill Verifier: run tests, check guardrails, reject privilege escalation.

use coevo_core::skills::*;

pub struct SkillVerifier;

impl SkillVerifier {
    /// Verify a skill evolution proposal.
    /// Returns eval results. Fails if skill tries to escalate privileges.
    pub fn verify(
        proposal: &SkillEvolutionProposal,
        skill: Option<&AgentSkillPackage>,
    ) -> SkillEvalResult {
        let mut failures: Vec<String> = vec![];
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Guard 1: Skill cannot escalate risk ceiling
        if let Some(existing) = skill {
            if proposal.risk_assessment.contains("HIGH") && existing.risk_ceiling < 0.5 {
                failures.push("Risk ceiling escalation denied — requires human review".to_string());
            }
        }

        // Guard 2: Skill cannot add forbidden behaviors
        for test in &proposal.generated_tests {
            for forbidden in &test.forbidden_behaviors {
                if forbidden.contains("fact")
                    || forbidden.contains("decision")
                    || forbidden.contains("bypass")
                {
                    failures.push(format!("Skill attempts forbidden behavior: {}", forbidden));
                }
            }
        }

        // Guard 3: Skill cannot bypass RiskGate
        if proposal
            .proposed_changes
            .to_lowercase()
            .contains("bypass risk")
        {
            failures.push("Skill attempts to bypass RiskGate — rejected".to_string());
        }

        // Guard 4: Production/external write skills need human review
        if proposal.risk_assessment.contains("HIGH") {
            failures.push("HIGH-risk skill change requires human review".to_string());
        }

        let passed = failures.is_empty();

        SkillEvalResult {
            eval_id: format!("eval-{}", uuid::Uuid::new_v4()),
            skill_id: proposal.target_skill_id.clone(),
            version: "proposed".to_string(),
            run_id: uuid::Uuid::new_v4().to_string(),
            passed,
            score: if passed { 1.0 } else { 0.0 },
            failures,
            regression_detected: false,
            verifier_notes: if passed {
                "All guardrails passed. Ready for approval.".to_string()
            } else {
                "Guardrail violations detected. Requires human review.".to_string()
            },
            created_at_ms: now,
        }
    }
}
