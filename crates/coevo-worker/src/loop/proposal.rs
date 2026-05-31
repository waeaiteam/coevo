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
    Finish {
        summary: String,
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
