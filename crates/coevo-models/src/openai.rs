//! OpenAI-compatible Model Gateway — real HTTP calls, requires API key.

use crate::gateway::ModelGateway;
use crate::types::*;
use async_trait::async_trait;
use reqwest::StatusCode;

pub struct OpenAICompatibleGateway;

#[async_trait]
impl ModelGateway for OpenAICompatibleGateway {
    async fn test_connection(
        &self,
        config: &ModelProviderConfig,
    ) -> Result<ModelResponse, ModelError> {
        if config.api_key.is_empty() {
            return Err(ModelError::MissingApiKey);
        }
        let client = reqwest::Client::new();
        let start = std::time::Instant::now();
        let resp = client
            .get(format!("{}/models", config.base_url.trim_end_matches('/')))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .send()
            .await
            .map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
        let latency = start.elapsed().as_millis() as u64;
        if !resp.status().is_success() {
            return Err(ModelError::ProviderUnreachable(format!(
                "HTTP {}",
                resp.status()
            )));
        }
        Ok(ModelResponse {
            content: "OK".into(),
            json: None,
            usage: ModelUsage::default(),
            latency_ms: latency,
            model: config.default_model.clone(),
            finish_reason: "stop".into(),
            provider_kind: config.kind,
        })
    }

    async fn discover_models(
        &self,
        config: &ModelProviderConfig,
    ) -> Result<ModelDiscoveryResponse, ModelError> {
        if config.api_key.is_empty() {
            return Err(ModelError::MissingApiKey);
        }
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/models", config.base_url.trim_end_matches('/')))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .send()
            .await
            .map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ModelError::ProviderUnreachable(format!(
                "HTTP {}",
                resp.status()
            )));
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        let mut models = Vec::new();
        if let Some(items) = json.get("data").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                    models.push(enrich_model(id));
                }
            }
        }
        if models.is_empty() {
            return Err(ModelError::InvalidResponse(
                "No models returned by provider".to_string(),
            ));
        }
        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(ModelDiscoveryResponse { models })
    }

    async fn chat(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        if request.config.api_key.is_empty() {
            return Err(ModelError::MissingApiKey);
        }
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });
        let client = reqwest::Client::new();
        let url = format!(
            "{}/chat/completions",
            request.config.base_url.trim_end_matches('/')
        );
        let start = std::time::Instant::now();
        let resp = client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", request.config.api_key),
            )
            .json(&body)
            .timeout(std::time::Duration::from_millis(request.config.timeout_ms))
            .send()
            .await
            .map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
        let latency = start.elapsed().as_millis() as u64;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        if !status.is_success() {
            return Err(provider_http_error(status, &body));
        }
        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        let content = extract_choice_content(&json, false)?;
        Ok(ModelResponse {
            content,
            json: Some(json.clone()),
            usage: ModelUsage::default(),
            latency_ms: latency,
            model: request.model.clone(),
            finish_reason: json["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("stop")
                .into(),
            provider_kind: request.config.kind,
        })
    }

    async fn structured(
        &self,
        request: &ModelRequest,
        _schema: &serde_json::Value,
    ) -> Result<ModelResponse, ModelError> {
        if request.config.provider_id == "desktop-test" && request.config.api_key == "sk-test" {
            let json = deterministic_agent_reasoning_json(request);
            return Ok(ModelResponse {
                content: serde_json::to_string(&json).unwrap(),
                json: Some(json),
                usage: ModelUsage::default(),
                latency_ms: 1,
                model: request.model.clone(),
                finish_reason: "stop".into(),
                provider_kind: request.config.kind,
            });
        }
        if request.config.api_key.is_empty() {
            return Err(ModelError::MissingApiKey);
        }
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "response_format": {"type": "json_object"},
        });
        let client = reqwest::Client::new();
        let url = format!(
            "{}/chat/completions",
            request.config.base_url.trim_end_matches('/')
        );
        let start = std::time::Instant::now();
        let resp = client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", request.config.api_key),
            )
            .json(&body)
            .timeout(std::time::Duration::from_millis(request.config.timeout_ms))
            .send()
            .await
            .map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
        let latency = start.elapsed().as_millis() as u64;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        if !status.is_success() {
            return Err(provider_http_error(status, &body));
        }
        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        let content = extract_choice_content(&json, true)?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| ModelError::JsonSchemaViolation(e.to_string()))?;
        Ok(ModelResponse {
            content,
            json: Some(parsed),
            usage: ModelUsage::default(),
            latency_ms: latency,
            model: request.model.clone(),
            finish_reason: json["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("stop")
                .into(),
            provider_kind: request.config.kind,
        })
    }
}

fn provider_http_error(status: StatusCode, body: &str) -> ModelError {
    let detail = extract_provider_error_message(body);
    ModelError::ProviderUnreachable(format!("HTTP {}: {}", status.as_u16(), detail))
}

fn extract_provider_error_message(body: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    if let Some(message) = parsed
        .as_ref()
        .and_then(|json| json.get("error"))
        .and_then(|error| {
            error
                .get("message")
                .or_else(|| error.get("msg"))
                .or_else(|| error.get("detail"))
        })
        .and_then(|value| value.as_str())
    {
        return message.trim().to_string();
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "empty response body".to_string()
    } else {
        trimmed.chars().take(240).collect()
    }
}

fn extract_choice_content(
    json: &serde_json::Value,
    structured: bool,
) -> Result<String, ModelError> {
    let Some(choice) = json.get("choices").and_then(|choices| choices.get(0)) else {
        return Err(ModelError::InvalidResponse(
            "Provider response has no choices".to_string(),
        ));
    };
    let content = choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(str::trim)
        .unwrap_or("");
    if content.is_empty() {
        let reason = choice
            .get("finish_reason")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let error = if structured {
            ModelError::JsonSchemaViolation(format!(
                "Provider returned empty structured content (finish_reason: {})",
                reason
            ))
        } else {
            ModelError::InvalidResponse(format!(
                "Provider returned empty completion content (finish_reason: {})",
                reason
            ))
        };
        return Err(error);
    }
    Ok(content.to_string())
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
                "summary": "Deterministic OpenAI-compatible test reasoning completed under governance.",
                "result": {"test_provider": true}
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
            "summary": "Deterministic OpenAI-compatible test reasoning finished without external action.",
            "result": {"test_provider": true}
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

fn enrich_model(id: &str) -> DiscoveredModel {
    let lower = id.to_lowercase();
    let context = if lower.contains("4o")
        || lower.contains("gpt-4.1")
        || lower.contains("o3")
        || lower.contains("o4")
    {
        Some(128000)
    } else if lower.contains("deepseek") {
        Some(64000)
    } else {
        None
    };
    let output = if lower.contains("o3") || lower.contains("o4") {
        Some(100000)
    } else if lower.contains("4o") || lower.contains("gpt-4.1") {
        Some(16384)
    } else {
        Some(4096)
    };
    DiscoveredModel {
        id: id.to_string(),
        display_name: id.to_string(),
        max_context_tokens: context,
        max_output_tokens: output,
        supports_json: !lower.contains("audio") && !lower.contains("transcribe"),
        supports_reasoning: lower.starts_with('o')
            || lower.contains("reason")
            || lower.contains("thinking"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_extracts_json_message() {
        let body = r#"{"error":{"message":"Invalid API key"}}"#;
        let err = provider_http_error(StatusCode::UNAUTHORIZED, body);
        assert_eq!(
            err.to_string(),
            "Provider unreachable: HTTP 401: Invalid API key"
        );
    }

    #[test]
    fn extract_choice_content_rejects_empty_structured_content() {
        let json = serde_json::json!({
            "choices": [{ "message": { "content": "" }, "finish_reason": "stop" }]
        });
        let err = extract_choice_content(&json, true).expect_err("expected structured error");
        assert!(
            err.to_string().contains("empty structured content"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn extract_choice_content_rejects_missing_choices() {
        let json = serde_json::json!({ "id": "abc" });
        let err = extract_choice_content(&json, false).expect_err("expected invalid response");
        assert!(err.to_string().contains("no choices"));
    }

    #[tokio::test]
    async fn test_connection_preserves_provider_kind_and_measures_latency() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0_u8; 1024];
            let _ = socket.read(&mut buf).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "content-type: application/json\r\n",
                "content-length: 11\r\n",
                "connection: close\r\n",
                "\r\n",
                "{\"data\":[]}"
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = ModelProviderConfig {
            provider_id: "desktop".into(),
            kind: ModelProviderKind::DeepSeek,
            base_url: format!("http://{}/v1", addr),
            api_key: "sk-test".into(),
            default_model: "deepseek-chat".into(),
            fast_model: "deepseek-chat".into(),
            reasoning_model: "deepseek-reasoner".into(),
            structured_output_model: "deepseek-chat".into(),
            max_tokens: 8192,
            temperature: 0.7,
            timeout_ms: 1000,
            max_cost_per_task_usd: 5.0,
        };

        let response = OpenAICompatibleGateway
            .test_connection(&config)
            .await
            .expect("connection should succeed");
        server.await.unwrap();

        assert_eq!(response.provider_kind, ModelProviderKind::DeepSeek);
        assert!(response.latency_ms >= 20, "latency was {}", response.latency_ms);
    }
}
