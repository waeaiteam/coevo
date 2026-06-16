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
            reasoning_content: None,
            tool_calls: vec![],
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
        if request.stream {
            let mut sink = |_: ModelStreamEvent| {
                Box::pin(async { Ok(()) })
                    as std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<(), ModelError>> + Send>,
                    >
            };
            return self.stream(request, None, &mut sink).await;
        }
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
            reasoning_content: None,
            tool_calls: vec![],
        })
    }

    async fn structured(
        &self,
        request: &ModelRequest,
        schema: &serde_json::Value,
    ) -> Result<ModelResponse, ModelError> {
        if request.stream {
            let mut sink = |_: ModelStreamEvent| {
                Box::pin(async { Ok(()) })
                    as std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<(), ModelError>> + Send>,
                    >
            };
            return self.stream(request, Some(schema), &mut sink).await;
        }
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
                "expected_benefit": "Improve proposal specificity while keeping governance boundaries explicit.",
                "generated_tests": [
                    {
                        "description": "should explain selected employees",
                        "pass_criteria": [
                            "The generated proposal names the selected employees involved in the task."
                        ]
                    },
                    {
                        "description": "should mention executor risk ceiling",
                        "pass_criteria": [
                            "The generated proposal preserves executor governance and risk-ceiling instructions."
                        ]
                    }
                ],
                "guardrails": ["must not elevate permissions", "must not bypass RiskGate"],
                "risk_assessment": "LOW"
            }),
            ModelRole::StructuredOutput => deterministic_structured_output_json_for_tests(request),
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
            reasoning_content: None,
            tool_calls: vec![],
        })
    }

    async fn stream(
        &self,
        request: &ModelRequest,
        schema_json: Option<&serde_json::Value>,
        on_event: &mut crate::gateway::ModelStreamHandler<'_>,
    ) -> Result<ModelResponse, ModelError> {
        let mut non_stream_request = request.clone();
        non_stream_request.stream = false;
        let response = match schema_json {
            Some(schema) => self.structured(&non_stream_request, schema).await?,
            None => self.chat(&non_stream_request).await?,
        };

        if !response.content.is_empty() {
            on_event(ModelStreamEvent::ContentDelta {
                delta: response.content.clone(),
            })
            .await?;
        }
        if let Some(reasoning) = &response.reasoning_content {
            on_event(ModelStreamEvent::ReasoningDelta {
                delta: reasoning.clone(),
            })
            .await?;
        }
        if response.usage.total_tokens > 0 {
            on_event(ModelStreamEvent::Usage(response.usage.clone())).await?;
        }
        on_event(ModelStreamEvent::Done {
            finish_reason: Some(response.finish_reason.clone()),
        })
        .await?;

        Ok(response)
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

pub fn deterministic_structured_output_json_for_tests(request: &ModelRequest) -> serde_json::Value {
    let prompt = request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if prompt.contains("Return JSON with agent_id, stance, text for the current participant only.")
    {
        let topic =
            extract_labeled_line(&prompt, "Topic").unwrap_or_else(|| "the proposal".to_string());
        let current_agent = extract_labeled_line(&prompt, "Current participant")
            .unwrap_or_else(|| "agent-founder-01".to_string());
        let stance = extract_labeled_line(&prompt, "Assigned stance").unwrap_or_else(|| {
            if current_agent == "agent-critic-01" || current_agent == "agent-risk-01" {
                "oppose".to_string()
            } else {
                "support".to_string()
            }
        });
        let text = if current_agent == "agent-critic-01" || current_agent == "agent-risk-01" {
            format!(
                "{current_agent} opposes pushing {topic} immediately because rollout risk, migration exposure, and governance checkpoints are still under-defined."
            )
        } else if current_agent == "agent-pm-01" {
            format!(
                "{current_agent} supports {topic} because it sharpens the product roadmap, clarifies priorities, and improves team execution."
            )
        } else {
            format!(
                "{current_agent} supports {topic} with a staged plan, provided the next owner and review checkpoint are explicit."
            )
        };
        return serde_json::json!({
            "agent_id": current_agent,
            "stance": stance,
            "text": text
        });
    }

    if prompt.contains("Return JSON with resolution_md and responsibility_anchor.") {
        let topic =
            extract_labeled_line(&prompt, "Topic").unwrap_or_else(|| "the proposal".to_string());
        return serde_json::json!({
            "resolution_md": format!(
                "# Opinion Letter\n\nThe meeting on {topic} concluded with support for a staged rollout, while preserving the critic's risk concerns as required follow-up checkpoints."
            ),
            "responsibility_anchor": "agent-founder-01"
        });
    }

    if prompt.contains("Create a skill evolution proposal.") {
        let target_skill = extract_labeled_line(&prompt, "Target skill")
            .unwrap_or_else(|| "skill-mission-draft".to_string());
        let failure_category = extract_labeled_line(&prompt, "Failure category")
            .unwrap_or_else(|| "BadPromptProcedure".to_string());
        let root_cause = extract_labeled_line(&prompt, "Root cause")
            .unwrap_or_else(|| "The worker skipped an explicit validation checkpoint.".to_string());
        let proposed_changes = if failure_category.contains("BadPromptProcedure") {
            "Add an explicit validation checkpoint before the final response and require the worker to restate the evidence it used.".to_string()
        } else {
            "Add a governed recovery step that validates the failure signal before returning the final response.".to_string()
        };
        return serde_json::json!({
            "diagnosis": root_cause,
            "proposed_changes": proposed_changes,
            "expected_benefit": "Improves traceability and keeps the generated proposal grounded in the actual failure.",
            "risk_assessment": "LOW",
            "generated_tests": [{
                "description": "should require a validation checkpoint before returning",
                "pass_criteria": [format!(
                    "The proposal for {target_skill} requires an explicit validation checkpoint before the final response."
                )]
            }]
        });
    }

    if prompt.contains("Evaluate this output") && prompt.contains("Metrics:") {
        return serde_json::json!({
            "accuracy": 88,
            "relevance": 91,
            "judge_reasoning": "Mock judge: the output addresses the requested task with concrete supporting detail."
        });
    }

    serde_json::json!({"mock": true, "role": format!("{:?}", request.role)})
}

fn extract_labeled_line(prompt: &str, label: &str) -> Option<String> {
    let prefix = format!("{label}:");
    prompt.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|value| value.trim().to_string())
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
