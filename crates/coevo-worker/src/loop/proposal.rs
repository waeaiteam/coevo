use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionProposal {
    CallTool {
        tool_id: String,
        input: serde_json::Value,
        rationale: String,
    },
    CallExecutor {
        executor_id: String,
        task: serde_json::Value,
        rationale: String,
    },
    /// A department head temporarily assembling a sub-agent to handle a focused piece of
    /// work that needs a specific skill. The sub-agent runs a bounded, governed sub-loop
    /// and hands its result back; it is ephemeral and cannot itself spawn further agents.
    SpawnSubagent {
        /// Skill the head wants the sub-agent to apply (must exist & be within the head's ceiling).
        skill_id: String,
        /// The focused goal handed to the sub-agent.
        task: String,
        rationale: String,
    },
    Finish {
        summary: String,
        #[serde(default = "empty_json_object")]
        result: serde_json::Value,
    },
    AskHuman {
        question: String,
        blocking: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReasoningOutput {
    pub thought: String,
    pub proposal: ActionProposal,
    pub confidence: f64,
}

fn empty_json_object() -> serde_json::Value {
    serde_json::json!({})
}
