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
