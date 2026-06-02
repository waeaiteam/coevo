//! Model gateway types: configs, requests, responses, errors.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderKind {
    Mock,
    OpenAICompatible,
    OpenAI,
    Anthropic,
    Gemini,
    DeepSeek,
    Ollama,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderConfig {
    pub provider_id: String,
    pub kind: ModelProviderKind,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub fast_model: String,
    pub reasoning_model: String,
    pub structured_output_model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub timeout_ms: u64,
    pub max_cost_per_task_usd: f64,
}

impl ModelProviderConfig {
    pub fn mock() -> Self {
        Self {
            provider_id: "mock".into(),
            kind: ModelProviderKind::Mock,
            base_url: String::new(),
            api_key: String::new(),
            default_model: "mock-model".into(),
            fast_model: "mock-model".into(),
            reasoning_model: "mock-model".into(),
            structured_output_model: "mock-model".into(),
            max_tokens: 4096,
            temperature: 0.7,
            timeout_ms: 30000,
            max_cost_per_task_usd: 0.0,
        }
    }
    pub fn mask_key(&self) -> String {
        if self.api_key.len() <= 8 {
            "****".into()
        } else {
            format!(
                "{}****{}",
                &self.api_key[..4],
                &self.api_key[self.api_key.len() - 4..]
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelRole {
    MissionDraft,
    AgentReasoning,
    Critic,
    Synthesizer,
    SkillGenerator,
    StructuredOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequest {
    pub config: ModelProviderConfig,
    pub role: ModelRole,
    pub model: String,
    pub messages: Vec<ModelMessage>,
    pub temperature: f64,
    pub max_tokens: u32,
    pub response_format: ResponseFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub content: String,
    pub json: Option<serde_json::Value>,
    pub usage: ModelUsage,
    pub latency_ms: u64,
    pub model: String,
    pub finish_reason: String,
    pub provider_kind: ModelProviderKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    pub id: String,
    pub display_name: String,
    pub max_context_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub supports_json: bool,
    pub supports_reasoning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDiscoveryResponse {
    pub models: Vec<DiscoveredModel>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("API key is required but missing")]
    MissingApiKey,
    #[error("Provider unreachable: {0}")]
    ProviderUnreachable(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("JSON schema violation: {0}")]
    JsonSchemaViolation(String),
    #[error("Request timed out")]
    Timeout,
    #[error("Budget exceeded")]
    BudgetExceeded,
    #[error("Model disabled")]
    Disabled,
}
