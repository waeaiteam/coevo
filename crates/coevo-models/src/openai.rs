//! OpenAI-compatible Model Gateway — real HTTP calls, requires API key.

use async_trait::async_trait;
use crate::types::*;
use crate::gateway::ModelGateway;

pub struct OpenAICompatibleGateway;

#[async_trait]
impl ModelGateway for OpenAICompatibleGateway {
    async fn test_connection(&self, config: &ModelProviderConfig) -> Result<ModelResponse, ModelError> {
        if config.api_key.is_empty() { return Err(ModelError::MissingApiKey); }
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/models", config.base_url.trim_end_matches('/')))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .send().await.map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
        if !resp.status().is_success() { return Err(ModelError::ProviderUnreachable(format!("HTTP {}", resp.status()))); }
        Ok(ModelResponse { content: "OK".into(), json: None, usage: ModelUsage::default(), latency_ms: 1, model: config.default_model.clone(), finish_reason: "stop".into(), provider_kind: ModelProviderKind::OpenAICompatible })
    }

    async fn chat(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        if request.config.api_key.is_empty() { return Err(ModelError::MissingApiKey); }
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });
        let client = reqwest::Client::new();
        let url = format!("{}/chat/completions", request.config.base_url.trim_end_matches('/'));
        let start = std::time::Instant::now();
        let resp = client.post(&url).header("Authorization", format!("Bearer {}", request.config.api_key)).json(&body).timeout(std::time::Duration::from_millis(request.config.timeout_ms)).send().await.map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
        let latency = start.elapsed().as_millis() as u64;
        let json: serde_json::Value = resp.json().await.map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        let content = json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
        Ok(ModelResponse { content, json: Some(json.clone()), usage: ModelUsage::default(), latency_ms: latency, model: request.model.clone(), finish_reason: json["choices"][0]["finish_reason"].as_str().unwrap_or("stop").into(), provider_kind: ModelProviderKind::OpenAICompatible })
    }

    async fn structured(&self, request: &ModelRequest, _schema: &serde_json::Value) -> Result<ModelResponse, ModelError> {
        if request.config.api_key.is_empty() { return Err(ModelError::MissingApiKey); }
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "response_format": {"type": "json_object"},
        });
        let client = reqwest::Client::new();
        let url = format!("{}/chat/completions", request.config.base_url.trim_end_matches('/'));
        let start = std::time::Instant::now();
        let resp = client.post(&url).header("Authorization", format!("Bearer {}", request.config.api_key)).json(&body).timeout(std::time::Duration::from_millis(request.config.timeout_ms)).send().await.map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
        let latency = start.elapsed().as_millis() as u64;
        let json: serde_json::Value = resp.json().await.map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        let content = json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
        let parsed: serde_json::Value = serde_json::from_str(&content).map_err(|e| ModelError::JsonSchemaViolation(e.to_string()))?;
        Ok(ModelResponse { content, json: Some(parsed), usage: ModelUsage::default(), latency_ms: latency, model: request.model.clone(), finish_reason: json["choices"][0]["finish_reason"].as_str().unwrap_or("stop").into(), provider_kind: ModelProviderKind::OpenAICompatible })
    }
}
