//! PCDT Router — computes optimal policy-constrained execution plans.
//! Per coevo whitepaper Section 7.

use coevo_core::contract::MCLSpec;
use coevo_core::plan::*;
use sha2::{Digest, Sha256};

/// PCDT routing result.
pub struct RoutingResult {
    pub plan: ExecutionPlanSpec,
    pub plan_hash: String,
}

/// The PCDT (Policy-Constrained Dynamic Topology) router.
pub struct PcdtRouter;

impl PcdtRouter {
    /// Compute an execution plan from a contract and available agent registry.
    pub fn compute(
        contract: &MCLSpec,
        agent_ids: Vec<String>,
        parent_plan_hash: Option<&str>,
    ) -> Result<RoutingResult, RoutingError> {
        if agent_ids.is_empty() {
            return Err(RoutingError::NoCompliantPath {
                blockers: vec!["No agents registered".to_string()],
            });
        }

        // Build a topological path (simplistic: sequential ordering for now; DAG in production)
        let primary_path: Vec<String> = agent_ids
            .iter()
            .take(contract.termination_policy.max_hops as usize)
            .cloned()
            .collect();

        if primary_path.is_empty() {
            return Err(RoutingError::NoCompliantPath {
                blockers: vec!["Path collapsed to zero nodes".to_string()],
            });
        }

        let parent = parent_plan_hash
            .unwrap_or("0000000000000000000000000000000000000000000000000000000000000000")
            .to_string();

        // Derive a stable per-plan scope id BEFORE building agent configs, so it
        // can prefix the blackboard output keys. Without this, every plan uses
        // bare `step-{i}-output` keys, which collide across re-routes: a
        // re-routed plan would overwrite the prior plan's step outputs on the
        // shared blackboard. The id is a hash of the inputs that define this
        // plan (contract + agent roster + parent plan), so two distinct
        // routings get distinct key namespaces while a deterministic re-compute
        // of the same plan stays stable. It is computed from inputs only (not
        // from the agent configs) to avoid a hash-includes-the-keys cycle.
        let plan_scope_id = Self::plan_scope_id(contract, &primary_path, &parent);
        let key_for = |step: usize| format!("{plan_scope_id}:step-{step}-output");

        // Build agent configs
        let agent_configs: Vec<AgentSlotConfig> = primary_path
            .iter()
            .enumerate()
            .map(|(i, id)| AgentSlotConfig {
                agent_id: id.clone(),
                role: if i == 0 {
                    "Proposer".to_string()
                } else if i == primary_path.len() - 1 {
                    "Synthesizer".to_string()
                } else {
                    "Critic".to_string()
                },
                step_index: i as u32,
                input_keys: if i == 0 { vec![] } else { vec![key_for(i - 1)] },
                output_keys: vec![key_for(i)],
                timeout_ms: 30_000,
            })
            .collect();

        // Build failback rules for each agent
        let failback_rules: Vec<FailbackRule> = primary_path
            .iter()
            .map(|agent_id| FailbackRule {
                primary_agent_id: agent_id.clone(),
                trigger: FailbackTrigger::Timeout,
                fallback: FailbackAction::Abort,
            })
            .collect();

        let plan = ExecutionPlanSpec {
            execution_plan_version: "1.0".to_string(),
            plan_hash: String::new(), // filled below
            parent_plan_hash: parent,
            primary_path_dag: primary_path,
            agent_configs,
            failback_routing_rules: failback_rules,
            hard_resource_ceilings: HardResourceCeilings {
                max_hops: contract.termination_policy.max_hops,
                max_node_execution_ms: 60_000,
                max_plan_duration_ms: contract.termination_policy.max_latency_ms,
            },
            exploration_budget_quota: 0.0,
        };

        // Hash the plan — include timestamp for uniqueness per execution
        let plan_json =
            serde_json::to_string(&plan).map_err(|e| RoutingError::Internal(e.to_string()))?;
        let mut hasher = Sha256::new();
        hasher.update(plan_json.as_bytes());
        hasher.update(chrono::Utc::now().timestamp_millis().to_le_bytes());
        let plan_hash = hex::encode(hasher.finalize());

        Ok(RoutingResult {
            plan: ExecutionPlanSpec {
                plan_hash: plan_hash.clone(),
                ..plan
            },
            plan_hash,
        })
    }

    /// Check if a plan exceeds budget constraints.
    pub fn check_budget(plan: &ExecutionPlanSpec, budget: u64) -> Result<(), RoutingError> {
        let estimated_tokens: u64 = plan.agent_configs.len() as u64 * 10_000; // rough estimate
        if estimated_tokens > budget {
            return Err(RoutingError::BudgetExceeded {
                budget,
                estimated: estimated_tokens,
            });
        }
        Ok(())
    }

    /// Stable, collision-resistant namespace for a plan's blackboard keys.
    ///
    /// Hashes the plan-defining inputs (contract identity + agent roster +
    /// parent plan hash) into a short hex id. Deterministic for identical
    /// inputs; distinct for any change, so re-routed plans never share a key
    /// namespace with a prior plan. Returns the first 16 hex chars (64 bits) —
    /// enough to make accidental collisions across plans negligible while
    /// keeping keys readable.
    fn plan_scope_id(
        contract: &MCLSpec,
        primary_path: &[String],
        parent_plan_hash: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        // Bind to the contract's institution policy hash (its stable identity in
        // routing) plus the goal-tree root id; both are part of what makes this
        // a distinct plan.
        hasher.update(contract.institution_policy_hash.as_bytes());
        hasher.update(b"|");
        hasher.update(contract.goal_tree.root.id.as_bytes());
        hasher.update(b"|");
        hasher.update(parent_plan_hash.as_bytes());
        hasher.update(b"|");
        for agent in primary_path {
            hasher.update(agent.as_bytes());
            hasher.update(b",");
        }
        let full = hex::encode(hasher.finalize());
        full[..16].to_string()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("no compliant routing path found: blockers={blockers:?}")]
    NoCompliantPath { blockers: Vec<String> },
    #[error("token budget exceeded: needed {estimated}, budget {budget}")]
    BudgetExceeded { budget: u64, estimated: u64 },
    #[error("internal routing error: {0}")]
    Internal(String),
}
