use crate::error::WorkerError;
use crate::types::*;
use coevo_core::skills::*;
use coevo_store::repos_opc::{agent_memory_repo, skill_evolution_repo};
use sqlx::SqlitePool;

pub struct SelfUpgradeLoop;
impl SelfUpgradeLoop {
    pub async fn run(
        pool: &SqlitePool,
        run: &WorkerRun,
        reflection: &ReflectionRecord,
        feedback: Option<&str>,
    ) -> Result<Option<String>, WorkerError> {
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Write to AgentMemory
        if let Ok(Some(mut am)) = agent_memory_repo::AgentMemoryRepo::get(pool, &run.agent_id).await
        {
            if run.status == WorkerRunStatus::Completed {
                am.successful_patterns = vec!["worker harness completed".into()];
            } else {
                am.recurring_failures = vec!["worker harness failed".into()];
            }
            agent_memory_repo::AgentMemoryRepo::upsert(pool, &am)
                .await
                .map_err(|e| WorkerError::Internal(e.to_string()))?;
        }

        // Create SkillEvolutionProposal if needed
        let skill_updates: Vec<String> = reflection
            .skill_to_update_json
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let has_feedback = feedback.map(|f| !f.is_empty()).unwrap_or(false);

        if !skill_updates.is_empty() || has_feedback {
            let proposal = SkillEvolutionProposal {
                proposal_id: format!("evol-{}", uuid::Uuid::new_v4()),
                source_type: EvolutionSourceType::Failure,
                source_refs: vec![run.run_id.clone()],
                target_skill_id: "skill-mission-draft".into(),
                proposal_type: EvolutionProposalType::PatchSkill,
                diagnosis: if has_feedback {
                    feedback.unwrap_or("").into()
                } else {
                    format!("Skill update needed: {:?}", skill_updates)
                },
                proposed_changes: "Auto-patch from SelfUpgradeLoop".into(),
                expected_benefit: "Improve execution reliability".into(),
                risk_assessment: "LOW".into(),
                generated_tests: vec![],
                status: EvolutionProposalStatus::Draft,
                created_by_agent: run.agent_id.clone(),
                created_at_ms: now,
            };
            skill_evolution_repo::SkillEvolutionRepo::create_proposal(pool, &proposal)
                .await
                .map_err(|e| WorkerError::Internal(e.to_string()))?;
            return Ok(Some(proposal.proposal_id));
        }
        Ok(None)
    }
}
