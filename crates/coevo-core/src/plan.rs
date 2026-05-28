//! Execution Plan specification — PCDT router output.
//! Per coevo whitepaper Section 7.

use serde::{Deserialize, Serialize};

/// The system execution plan produced by the PCDT router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlanSpec {
    pub execution_plan_version: String,
    /// SHA256 signature of this plan.
    pub plan_hash: String,
    /// SHA256 of parent plan for revision tracking.
    pub parent_plan_hash: String,
    /// Topologically sorted primary agent ID sequence (DAG).
    pub primary_path_dag: Vec<String>,
    /// Per-agent configuration along the primary path.
    pub agent_configs: Vec<AgentSlotConfig>,
    /// Fallback routing rules.
    pub failback_routing_rules: Vec<FailbackRule>,
    /// Hard resource ceilings.
    pub hard_resource_ceilings: HardResourceCeilings,
    /// Token budget reserved for exploration observers.
    pub exploration_budget_quota: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSlotConfig {
    pub agent_id: String,
    pub role: String,
    /// Step index in the primary path.
    pub step_index: u32,
    /// Input data keys from the blackboard.
    pub input_keys: Vec<String>,
    /// Output keys expected on the blackboard.
    pub output_keys: Vec<String>,
    /// Timeout for this agent step in milliseconds.
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailbackRule {
    /// The agent ID this rule applies to.
    pub primary_agent_id: String,
    /// Condition that triggers failback.
    pub trigger: FailbackTrigger,
    /// Fallback agent or action.
    pub fallback: FailbackAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailbackTrigger {
    Timeout,
    CircuitBreaker,
    QualityFailure,
    HeartbeatLost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum FailbackAction {
    /// Route to a backup agent.
    RerouteToAgent { agent_id: String },
    /// Degrade action mode.
    DegradeMode { new_mode: String },
    /// Abort the plan entirely.
    Abort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardResourceCeilings {
    /// Maximum agent hops (default 8-12).
    pub max_hops: u32,
    /// Maximum execution time per single node in milliseconds.
    pub max_node_execution_ms: u64,
    /// Maximum total plan execution time in milliseconds.
    pub max_plan_duration_ms: u64,
}

impl Default for HardResourceCeilings {
    fn default() -> Self {
        Self {
            max_hops: 10,
            max_node_execution_ms: 60_000,
            max_plan_duration_ms: 600_000,
        }
    }
}

impl ExecutionPlanSpec {
    /// Create a minimal execution plan for a single-agent green-track scenario.
    pub fn single_agent(plan_hash: String, parent_plan_hash: String, agent_id: String) -> Self {
        Self {
            execution_plan_version: "1.0".to_string(),
            plan_hash,
            parent_plan_hash,
            primary_path_dag: vec![agent_id.clone()],
            agent_configs: vec![AgentSlotConfig {
                agent_id,
                role: "Synthesizer".to_string(),
                step_index: 0,
                input_keys: vec![],
                output_keys: vec!["result".to_string()],
                timeout_ms: 30_000,
            }],
            failback_routing_rules: vec![],
            hard_resource_ceilings: HardResourceCeilings::default(),
            exploration_budget_quota: 0.0,
        }
    }
}
