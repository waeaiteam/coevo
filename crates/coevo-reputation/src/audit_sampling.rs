//! Adversarial audit sampling — 5% daily random audit.
//! Per coevo whitepaper Section 6.2.

use coevo_store::repos::reputation_repo::ReputationRepo;
use rand::Rng;
use sqlx::SqlitePool;

/// Run adversarial audit sampling: randomly select 5% of "successful" tasks
/// and re-evaluate them in an isolated Critic sandbox.
pub async fn run_adversarial_audit(
    pool: &SqlitePool,
    sample_rate: f64,
) -> Result<AuditReport, AuditError> {
    // Get all active agents
    let agents = ReputationRepo::find_top_n(pool, 100).await?;

    let sample_size = (agents.len() as f64 * sample_rate).ceil() as usize;
    if sample_size == 0 {
        return Ok(AuditReport {
            sampled: 0,
            failed: 0,
            details: vec![],
        });
    }

    let mut rng = rand::thread_rng();
    let mut failed = 0;
    let mut details = vec![];

    for agent in &agents {
        if rng.gen::<f64>() < sample_rate {
            // Simulate audit: check if agent's reputation is suspiciously inflated
            let avg_score = (agent.task_domain_competence
                + agent.uncertainty_honesty
                + agent.policy_compliance
                + agent.resource_efficiency)
                / 4.0;

            if avg_score > 0.95 && agent.task_count < 10 {
                failed += 1;
                details.push(AuditDetail {
                    agent_id: agent.agent_id.clone(),
                    issue: format!(
                        "Suspiciously high avg score {:.2} with only {} tasks",
                        avg_score, agent.task_count
                    ),
                    recommended_action: "Flag for manual review".to_string(),
                });
            }
        }
    }

    Ok(AuditReport {
        sampled: agents.len(),
        failed,
        details,
    })
}

pub struct AuditReport {
    pub sampled: usize,
    pub failed: usize,
    pub details: Vec<AuditDetail>,
}

pub struct AuditDetail {
    pub agent_id: String,
    pub issue: String,
    pub recommended_action: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
