//! Mock Model Gateway — returns structured, testable content. No API key needed.

use crate::gateway::ModelGateway;
use crate::types::*;
use async_trait::async_trait;

pub struct MockModelGateway;

#[async_trait]
impl ModelGateway for MockModelGateway {
    async fn test_connection(
        &self,
        _config: &ModelProviderConfig,
    ) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            content: "OK".into(),
            json: None,
            usage: ModelUsage::default(),
            latency_ms: 1,
            model: "mock-model".into(),
            finish_reason: "stop".into(),
            provider_kind: ModelProviderKind::Mock,
        })
    }

    async fn discover_models(
        &self,
        _config: &ModelProviderConfig,
    ) -> Result<ModelDiscoveryResponse, ModelError> {
        Ok(ModelDiscoveryResponse {
            models: vec![DiscoveredModel {
                id: "mock-model".into(),
                display_name: "Mock Model".into(),
                max_context_tokens: Some(4096),
                max_output_tokens: Some(4096),
                supports_json: true,
                supports_reasoning: false,
            }],
        })
    }

    async fn chat(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        let content = match request.role {
            ModelRole::Synthesizer => "This task was executed by coevo-opc on Green Track. Founder Assistant and Synthesizer participated. Mock OpenClaw Executor completed dry-run/execute. Results were written to Task Memory. No approval was required. The governance mesh preserved full audit trace.".into(),
            _ => "Mock chat response for role.".into(),
        };
        Ok(ModelResponse {
            content,
            json: None,
            usage: ModelUsage::default(),
            latency_ms: 1,
            model: "mock-model".into(),
            finish_reason: "stop".into(),
            provider_kind: ModelProviderKind::Mock,
        })
    }

    async fn structured(
        &self,
        request: &ModelRequest,
        _schema: &serde_json::Value,
    ) -> Result<ModelResponse, ModelError> {
        let json = match request.role {
            ModelRole::AgentReasoning => deterministic_agent_reasoning_json(request),
            ModelRole::MissionDraft => serde_json::json!({
                "goal_summary": "Summarize coevo-opc progress and propose next roadmap",
                "suggested_track": "Green",
                "reasoning": "Read-only planning task with no external write action",
                "recommended_agents": ["founder-assistant-agent", "synthesizer-agent"],
                "recommended_skills": ["mission-drafting-skill", "report-writing-skill"],
                "recommended_executors": ["mock-openclaw-executor"],
                "allowed_actions": ["read", "summarize", "write_task_memory"],
                "restricted_actions": ["production_write", "payment", "delete_data"],
                "questions_for_user": []
            }),
            ModelRole::SkillGenerator => serde_json::json!({
                "diagnosis": "The previous task lacked clear employee selection explanation.",
                "proposed_changes": "Add a step to explain selected AI Employees and executor boundaries.",
                "generated_tests": ["should explain selected employees", "should mention executor risk ceiling"],
                "guardrails": ["must not elevate permissions", "must not bypass RiskGate"],
                "risk_assessment": "LOW"
            }),
            _ => serde_json::json!({"mock": true, "role": format!("{:?}", request.role)}),
        };
        Ok(ModelResponse {
            content: serde_json::to_string(&json).unwrap(),
            json: Some(json),
            usage: ModelUsage::default(),
            latency_ms: 1,
            model: "mock-model".into(),
            finish_reason: "stop".into(),
            provider_kind: ModelProviderKind::Mock,
        })
    }
}

fn deterministic_agent_reasoning_json(request: &ModelRequest) -> serde_json::Value {
    let prompt = request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    if prompt.contains("previous_observation") || prompt.contains("governance denied") {
        return serde_json::json!({
            "thought": "I have enough governed observations to finish.",
            "proposal": {
                "kind": "finish",
                "summary": "Mock reasoning completed under governance.",
                "result": {"mock": true}
            },
            "confidence": 0.8
        });
    }

    if let Some(path) = deterministic_file_target(&prompt) {
        return serde_json::json!({
            "thought": "The mission asks for local evidence that can be read through the file readonly tool.",
            "proposal": {
                "kind": "call_tool",
                "tool_id": "file-readonly",
                "input": {
                    "action": "ReadFile",
                    "path": path,
                    "allowed_paths": deterministic_allowed_paths(),
                    "max_bytes": 5000
                },
                "rationale": "Green Track permits read-only file evidence through GovernGate."
            },
            "confidence": 0.78
        });
    }

    serde_json::json!({
        "thought": "No tool evidence is required for this governed dry run.",
        "proposal": {
            "kind": "finish",
            "summary": "Mock reasoning finished without external action.",
            "result": {"mock": true}
        },
        "confidence": 0.7
    })
}

fn deterministic_allowed_paths() -> Vec<String> {
    if let Ok(root) = std::env::var("COEVO_WORKSPACE_DIR") {
        return vec![root];
    }
    std::env::current_dir()
        .map(|path| vec![path.to_string_lossy().to_string()])
        .unwrap_or_default()
}

fn deterministic_file_target(prompt: &str) -> Option<String> {
    let roots = deterministic_allowed_paths();
    for name in ["mission-notes.md", "README.md", "README.zh-CN.md"] {
        if prompt.contains(&name.to_lowercase()) {
            for root in &roots {
                let path = std::path::Path::new(root).join(name);
                if path.is_file() {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}
