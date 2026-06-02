//! ModelRouter v0 — cognitive resource scheduling for OPC workers.
//! Models are cognitive resources, NOT authorization sources.
//! Agent preferences are advisory; final selection respects governance.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelCapability {
    FastText,
    DeepReasoning,
    CodeGeneration,
    CodeReview,
    LongContext,
    VisionUnderstanding,
    ImageGeneration,
    SlideGeneration,
    ThreeDGeneration,
    Embedding,
    StructuredJSON,
    ToolPlanning,
    RiskCritique,
    Summarization,
    SkillGeneration,
    SkillVerification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyLevel {
    LocalOnly,
    PrivateCloud,
    PublicApi,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    pub capabilities: Vec<ModelCapability>,
    pub max_context_tokens: u32,
    pub cost_per_1k_input_usd: f64,
    pub cost_per_1k_output_usd: f64,
    pub avg_latency_ms: u32,
    pub supports_json: bool,
    pub supports_tools: bool,
    pub privacy_level: PrivacyLevel,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentModelPreference {
    pub agent_id: String,
    pub default_model_id: Option<String>,
    pub fast_model_id: Option<String>,
    pub reasoning_model_id: Option<String>,
    pub code_model_id: Option<String>,
    pub review_model_id: Option<String>,
    pub vision_model_id: Option<String>,
    pub image_model_id: Option<String>,
    pub embedding_model_id: Option<String>,
    pub fallback_model_ids: Vec<String>,
    pub max_cost_per_task_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoutingRequest {
    pub work_order_id: String,
    pub agent_id: String,
    pub worker_step_type: String,
    pub intent: String,
    pub required_capabilities: Vec<ModelCapability>,
    pub track: String,
    pub risk_score: f64,
    pub max_latency_ms: Option<u64>,
    pub max_cost_usd: Option<f64>,
    pub privacy_boundary: PrivacyLevel,
    pub preferred_model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoutingDecision {
    pub selected_provider_id: String,
    pub selected_model_id: String,
    pub selected_capabilities: Vec<ModelCapability>,
    pub reason: String,
    pub fallback_model_ids: Vec<String>,
    pub estimated_cost_usd: Option<f64>,
    pub estimated_latency_ms: Option<u64>,
    pub governance_notes: Vec<String>,
    pub decision_id: String,
    pub created_at_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelRoutingError {
    #[error("No model available for required capabilities")]
    NoModelAvailable,
    #[error("Red Track cannot use PublicApi models")]
    RedTrackPublicApiBlocked,
    #[error("Cost budget exceeded")]
    CostBudgetExceeded,
    #[error("Latency budget exceeded")]
    LatencyBudgetExceeded,
    #[error("Internal error: {0}")]
    Internal(String),
}

pub fn default_model_profiles() -> Vec<ModelProfile> {
    vec![
        ModelProfile {
            provider_id: "mock".into(),
            model_id: "mock-fast".into(),
            display_name: "Mock Fast".into(),
            capabilities: vec![
                ModelCapability::FastText,
                ModelCapability::Summarization,
                ModelCapability::StructuredJSON,
            ],
            max_context_tokens: 8000,
            cost_per_1k_input_usd: 0.0,
            cost_per_1k_output_usd: 0.0,
            avg_latency_ms: 1,
            supports_json: true,
            supports_tools: false,
            privacy_level: PrivacyLevel::LocalOnly,
            enabled: true,
        },
        ModelProfile {
            provider_id: "mock".into(),
            model_id: "mock-reasoning".into(),
            display_name: "Mock Reasoning".into(),
            capabilities: vec![
                ModelCapability::DeepReasoning,
                ModelCapability::ToolPlanning,
                ModelCapability::RiskCritique,
                ModelCapability::CodeReview,
                ModelCapability::StructuredJSON,
            ],
            max_context_tokens: 32000,
            cost_per_1k_input_usd: 0.0,
            cost_per_1k_output_usd: 0.0,
            avg_latency_ms: 1,
            supports_json: true,
            supports_tools: true,
            privacy_level: PrivacyLevel::LocalOnly,
            enabled: true,
        },
        ModelProfile {
            provider_id: "openai-compatible".into(),
            model_id: "gpt-4o".into(),
            display_name: "OpenAI GPT-4o".into(),
            capabilities: vec![
                ModelCapability::DeepReasoning,
                ModelCapability::ToolPlanning,
                ModelCapability::StructuredJSON,
                ModelCapability::CodeReview,
                ModelCapability::Summarization,
                ModelCapability::LongContext,
            ],
            max_context_tokens: 128000,
            cost_per_1k_input_usd: 0.005,
            cost_per_1k_output_usd: 0.015,
            avg_latency_ms: 800,
            supports_json: true,
            supports_tools: true,
            privacy_level: PrivacyLevel::PublicApi,
            enabled: true,
        },
        ModelProfile {
            provider_id: "openai-compatible".into(),
            model_id: "deepseek-coder".into(),
            display_name: "DeepSeek Coder".into(),
            capabilities: vec![
                ModelCapability::CodeGeneration,
                ModelCapability::FastText,
                ModelCapability::StructuredJSON,
            ],
            max_context_tokens: 16384,
            cost_per_1k_input_usd: 0.00014,
            cost_per_1k_output_usd: 0.00028,
            avg_latency_ms: 600,
            supports_json: true,
            supports_tools: false,
            privacy_level: PrivacyLevel::PublicApi,
            enabled: true,
        },
        ModelProfile {
            provider_id: "openai-compatible".into(),
            model_id: "dalle".into(),
            display_name: "DALL-E Image Gen".into(),
            capabilities: vec![ModelCapability::ImageGeneration],
            max_context_tokens: 0,
            cost_per_1k_input_usd: 0.0,
            cost_per_1k_output_usd: 0.04,
            avg_latency_ms: 5000,
            supports_json: false,
            supports_tools: false,
            privacy_level: PrivacyLevel::PublicApi,
            enabled: true,
        },
        ModelProfile {
            provider_id: "local".into(),
            model_id: "slide-renderer".into(),
            display_name: "Slide Renderer".into(),
            capabilities: vec![
                ModelCapability::SlideGeneration,
                ModelCapability::StructuredJSON,
            ],
            max_context_tokens: 8192,
            cost_per_1k_input_usd: 0.0,
            cost_per_1k_output_usd: 0.0,
            avg_latency_ms: 100,
            supports_json: true,
            supports_tools: false,
            privacy_level: PrivacyLevel::LocalOnly,
            enabled: true,
        },
        ModelProfile {
            provider_id: "openai-compatible".into(),
            model_id: "threed-provider".into(),
            display_name: "3D Provider".into(),
            capabilities: vec![ModelCapability::ThreeDGeneration],
            max_context_tokens: 0,
            cost_per_1k_input_usd: 0.01,
            cost_per_1k_output_usd: 0.05,
            avg_latency_ms: 15000,
            supports_json: false,
            supports_tools: false,
            privacy_level: PrivacyLevel::PublicApi,
            enabled: true,
        },
    ]
}

pub fn required_capabilities_for_step(step_type: &str, intent: &str) -> Vec<ModelCapability> {
    let mut caps = vec![];
    let lower = intent.to_lowercase();
    match step_type {
        "BuildContext" => {
            caps.push(ModelCapability::FastText);
            caps.push(ModelCapability::Summarization);
        }
        "Think" => {
            caps.push(ModelCapability::DeepReasoning);
        }
        "ModelCall" => {
            caps.push(ModelCapability::StructuredJSON);
        }
        "LoadSkillIndex" | "LoadSkillFull" => {
            caps.push(ModelCapability::FastText);
        }
        "Reflect" => {
            caps.push(ModelCapability::Summarization);
        }
        "ProposeSkillUpdate" => {
            caps.push(ModelCapability::SkillGeneration);
            caps.push(ModelCapability::StructuredJSON);
        }
        _ => {}
    }
    if lower.contains("code")
        || lower.contains("rust")
        || lower.contains("python")
        || lower.contains("bug")
        || lower.contains("compile")
    {
        caps.push(ModelCapability::CodeGeneration);
    }
    if lower.contains("review") || lower.contains("audit") || lower.contains("security") {
        caps.push(ModelCapability::CodeReview);
        caps.push(ModelCapability::RiskCritique);
    }
    if lower.contains("ppt") || lower.contains("deck") || lower.contains("slides") {
        caps.push(ModelCapability::SlideGeneration);
        caps.push(ModelCapability::StructuredJSON);
    }
    if lower.contains("image")
        || lower.contains("poster")
        || lower.contains("logo")
        || lower.contains("visual")
    {
        caps.push(ModelCapability::ImageGeneration);
    }
    if lower.contains("3d") || lower.contains("model") || lower.contains("asset") {
        caps.push(ModelCapability::ThreeDGeneration);
    }
    caps
}

pub struct ModelRouter;
impl ModelRouter {
    pub fn route(
        req: &ModelRoutingRequest,
        profiles: &[ModelProfile],
        pref: Option<&AgentModelPreference>,
    ) -> Result<ModelRoutingDecision, ModelRoutingError> {
        let mut candidates: Vec<&ModelProfile> = profiles.iter().filter(|p| p.enabled).collect();
        // Filter by required capabilities
        candidates.retain(|p| {
            req.required_capabilities
                .iter()
                .all(|c| p.capabilities.contains(c))
        });
        // Filter by privacy boundary
        candidates.retain(|p| match (req.privacy_boundary, p.privacy_level) {
            (PrivacyLevel::LocalOnly, PrivacyLevel::LocalOnly) => true,
            (_, PrivacyLevel::LocalOnly) => true,
            (PrivacyLevel::PrivateCloud, PrivacyLevel::PublicApi) => false,
            (PrivacyLevel::PublicApi, _) => true,
            (PrivacyLevel::Unknown, _) => true,
            _ => true,
        });
        // Red Track blocks PublicApi
        if req.track == "red" {
            candidates.retain(|p| p.privacy_level != PrivacyLevel::PublicApi);
        }
        // Filter by cost
        if let Some(max_cost) = req.max_cost_usd {
            candidates.retain(|p| {
                let est = (req.intent.len() as f64 / 1000.0 * p.cost_per_1k_input_usd)
                    + (500.0 * p.cost_per_1k_output_usd / 1000.0);
                est <= max_cost
            });
        }
        // Filter by latency
        if let Some(max_ms) = req.max_latency_ms {
            candidates.retain(|p| p.avg_latency_ms as u64 <= max_ms);
        }
        // StructuredJSON requires supports_json
        if req
            .required_capabilities
            .contains(&ModelCapability::StructuredJSON)
        {
            candidates.retain(|p| p.supports_json);
        }
        // ToolPlanning requires supports_tools or StructuredJSON
        if req
            .required_capabilities
            .contains(&ModelCapability::ToolPlanning)
        {
            candidates.retain(|p| p.supports_tools || p.supports_json);
        }
        if candidates.is_empty() {
            return Err(ModelRoutingError::NoModelAvailable);
        }

        // Prefer explicit request/agent preference if it matches and doesn't break governance.
        let chosen = if let Some(preferred_model_id) = req.preferred_model_id.as_ref() {
            candidates
                .iter()
                .find(|c| &c.model_id == preferred_model_id)
                .unwrap_or(&candidates[0])
        } else if let Some(pf) = pref {
            let by_pref = candidates.iter().find(|c| {
                pf.default_model_id.as_ref() == Some(&c.model_id)
                    || (req
                        .required_capabilities
                        .contains(&ModelCapability::CodeGeneration)
                        && pf.code_model_id.as_ref() == Some(&c.model_id))
                    || (req
                        .required_capabilities
                        .contains(&ModelCapability::DeepReasoning)
                        && pf.reasoning_model_id.as_ref() == Some(&c.model_id))
            });
            by_pref.unwrap_or(&candidates[0])
        } else {
            &candidates[0]
        };

        let now = chrono::Utc::now().timestamp_millis();
        Ok(ModelRoutingDecision{
            selected_provider_id: chosen.provider_id.clone(),
            selected_model_id: chosen.model_id.clone(),
            selected_capabilities: chosen.capabilities.clone(),
            reason: format!("Selected {} for capabilities: {:?}", chosen.display_name, req.required_capabilities),
            fallback_model_ids: candidates.iter().filter(|c| c.model_id != chosen.model_id).take(2).map(|c| c.model_id.clone()).collect(),
            estimated_cost_usd: Some((req.intent.len() as f64 / 1000.0 * chosen.cost_per_1k_input_usd) + (500.0 * chosen.cost_per_1k_output_usd / 1000.0)),
            estimated_latency_ms: Some(chosen.avg_latency_ms as u64),
            governance_notes: vec!["Model provides cognition, not authorization. WorkOrder, RiskGate, and ToolPolicy govern execution.".into()],
            decision_id: format!("mrd-{}", uuid::Uuid::new_v4()),
            created_at_ms: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_req(intent: &str, caps: Vec<ModelCapability>, track: &str) -> ModelRoutingRequest {
        ModelRoutingRequest {
            work_order_id: "wo-1".into(),
            agent_id: "a1".into(),
            worker_step_type: "ModelCall".into(),
            intent: intent.into(),
            required_capabilities: caps,
            track: track.into(),
            risk_score: 0.5,
            max_latency_ms: None,
            max_cost_usd: None,
            privacy_boundary: PrivacyLevel::PublicApi,
            preferred_model_id: None,
        }
    }
    #[test]
    fn selects_code_for_code_gen() {
        let caps = vec![
            ModelCapability::CodeGeneration,
            ModelCapability::StructuredJSON,
        ];
        let d = ModelRouter::route(
            &make_req("fix rust bug", caps, "green"),
            &default_model_profiles(),
            None,
        )
        .unwrap();
        assert_eq!(d.selected_model_id, "deepseek-coder");
    }
    #[test]
    fn selects_reasoning() {
        let caps = vec![ModelCapability::DeepReasoning];
        let d = ModelRouter::route(
            &make_req("analyze strategy", caps, "green"),
            &default_model_profiles(),
            None,
        )
        .unwrap();
        assert_eq!(d.selected_model_id, "mock-reasoning");
    }
    #[test]
    fn blocks_public_api_for_red() {
        let profiles: Vec<ModelProfile> = default_model_profiles()
            .into_iter()
            .filter(|p| p.enabled && p.privacy_level != PrivacyLevel::PublicApi)
            .collect();
        let d = ModelRouter::route(
            &make_req("delete db", vec![ModelCapability::RiskCritique], "red"),
            &profiles,
            None,
        );
        assert!(d.is_ok()); // LocalOnly models still available
    }
    #[test]
    fn respects_cost_budget() {
        let mut req = make_req("summarize", vec![ModelCapability::Summarization], "green");
        req.max_cost_usd = Some(0.0001);
        let d = ModelRouter::route(&req, &default_model_profiles(), None).unwrap();
        assert_eq!(d.selected_model_id, "mock-fast"); // Free mock
    }
    #[test]
    fn requires_json_for_structured() {
        let profiles = default_model_profiles();
        let d = ModelRouter::route(
            &make_req(
                "test",
                vec![
                    ModelCapability::StructuredJSON,
                    ModelCapability::ImageGeneration,
                ],
                "green",
            ),
            &profiles,
            None,
        );
        assert!(d.is_err()); // Image provider doesn't support JSON
    }
    #[test]
    fn explicit_preferred_model_wins_after_governance_filters() {
        let mut req = make_req("summarize", vec![ModelCapability::StructuredJSON], "green");
        req.preferred_model_id = Some("mock-reasoning".into());
        let d = ModelRouter::route(&req, &default_model_profiles(), None).unwrap();
        assert_eq!(d.selected_model_id, "mock-reasoning");
    }
}
