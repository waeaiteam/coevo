//! Plan Revision Protocol — dynamic plan adjustment during execution.
//! Per coevo whitepaper Section 7.2.

use coevo_core::plan::ExecutionPlanSpec;
use sha2::{Digest, Sha256};

/// Entities authorized to initiate plan revision.
pub enum RevisionInitiator {
    CognitiveCustoms,
    RiskGate,
    AgentHealthMonitor,
    BudgetManager,
    HumanController,
    ResolutionEngine,
}

/// A revision request.
pub struct RevisionRequest {
    pub initiator: RevisionInitiator,
    pub reason: String,
    pub affected_agents: Vec<String>,
    pub completed_steps: Vec<u32>,
}

/// Result of a plan revision.
pub struct RevisedPlan {
    pub plan: ExecutionPlanSpec,
    pub plan_hash: String,
    pub preserved_steps: Vec<u32>,
    pub rebuilt_steps: Vec<u32>,
}

/// Plan revision handler.
pub struct PlanRevision;

impl PlanRevision {
    /// Revise an existing execution plan while preserving completed node states.
    pub fn revise(
        current_plan: &ExecutionPlanSpec,
        request: &RevisionRequest,
        available_agents: Vec<String>,
    ) -> Result<RevisedPlan, RevisionError> {
        // Preserve completed agent configs
        let completed_indices: Vec<u32> = request.completed_steps.clone();

        // Rebuild path for remaining steps
        let remaining_agents: Vec<String> = available_agents
            .into_iter()
            .filter(|a| !request.affected_agents.contains(a))
            .collect();

        if remaining_agents.is_empty() {
            return Err(RevisionError::NoAgentsAvailable);
        }

        // Build new agent configs for uncompleted steps
        let mut new_configs: Vec<_> = current_plan
            .agent_configs
            .iter()
            .filter(|c| completed_indices.contains(&c.step_index))
            .cloned()
            .collect();

        let remaining_start = new_configs.len() as u32;
        for (i, agent_id) in remaining_agents.iter().enumerate() {
            new_configs.push(coevo_core::plan::AgentSlotConfig {
                agent_id: agent_id.clone(),
                role: "Synthesizer".to_string(),
                step_index: remaining_start + i as u32,
                input_keys: vec![format!("step-{}-output", remaining_start as i32 - 1)],
                output_keys: vec![format!("step-{}-output", remaining_start + i as u32)],
                timeout_ms: 30_000,
            });
        }

        let mut new_plan = ExecutionPlanSpec {
            execution_plan_version: format!(
                "{}.{}",
                current_plan.execution_plan_version,
                chrono::Utc::now().timestamp()
            ),
            plan_hash: String::new(),
            parent_plan_hash: current_plan.plan_hash.clone(),
            primary_path_dag: remaining_agents,
            agent_configs: new_configs,
            failback_routing_rules: current_plan.failback_routing_rules.clone(),
            hard_resource_ceilings: current_plan.hard_resource_ceilings.clone(),
            exploration_budget_quota: current_plan.exploration_budget_quota,
        };

        let plan_json = serde_json::to_string(&new_plan).map_err(|e| RevisionError::Serialization(e.to_string()))?;
        let mut hasher = Sha256::new();
        hasher.update(plan_json.as_bytes());
        let plan_hash = hex::encode(hasher.finalize());
        new_plan.plan_hash = plan_hash.clone();

        Ok(RevisedPlan {
            plan_hash,
            plan: new_plan,
            preserved_steps: completed_indices,
            rebuilt_steps: (remaining_start..remaining_start + remaining_agents.len() as u32).collect(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RevisionError {
    #[error("no agents available for revision")]
    NoAgentsAvailable,
    #[error("serialization error: {0}")]
    Serialization(String),
}
