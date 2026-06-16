//! OpenAI-compatible Model Gateway — real HTTP calls, requires API key.

use crate::gateway::ModelGateway;
use crate::types::*;
use async_trait::async_trait;
use reqwest::StatusCode;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

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
        let url = format!("{}/models", config.base_url.trim_end_matches('/'));
        let client = crate::http::short_call_client_for(&url);
        let start = std::time::Instant::now();
        let resp = client
            .get(url)
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
            reasoning_content: None,
            tool_calls: vec![],
        })
    }

    async fn discover_models(
        &self,
        config: &ModelProviderConfig,
    ) -> Result<ModelDiscoveryResponse, ModelError> {
        if config.api_key.is_empty() {
            return Err(ModelError::MissingApiKey);
        }
        let url = format!("{}/models", config.base_url.trim_end_matches('/'));
        let client = crate::http::short_call_client_for(&url);
        let resp = client
            .get(url)
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
        if request.stream {
            let mut sink = no_op_event_handler;
            return self.stream(request, None, &mut sink).await;
        }
        if allow_test_mock_routing(request) {
            let mut response = crate::mock::MockModelGateway.chat(request).await?;
            response.usage = deterministic_test_usage(request, &response.content);
            return Ok(ModelResponse {
                model: request.model.clone(),
                provider_kind: request.config.kind,
                ..response
            });
        }
        if request.config.api_key.is_empty() {
            return Err(ModelError::MissingApiKey);
        }
        chat_once_or_retry(request).await
    }

    async fn structured(
        &self,
        request: &ModelRequest,
        schema: &serde_json::Value,
    ) -> Result<ModelResponse, ModelError> {
        if request.stream {
            let mut sink = no_op_event_handler;
            return self.stream(request, Some(schema), &mut sink).await;
        }
        if allow_test_mock_routing(request) {
            let json = match request.role {
                ModelRole::AgentReasoning => deterministic_agent_reasoning_json(request),
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
                    "risk_assessment": "LOW"
                }),
                _ => crate::mock::deterministic_structured_output_json_for_tests(request),
            };
            let content = serde_json::to_string(&json).unwrap();
            return Ok(ModelResponse {
                content: content.clone(),
                json: Some(json),
                usage: deterministic_test_usage(request, &content),
                latency_ms: 1,
                model: request.model.clone(),
                finish_reason: "stop".into(),
                provider_kind: request.config.kind,
                reasoning_content: None,
                tool_calls: vec![],
            });
        }
        if request.config.api_key.is_empty() {
            return Err(ModelError::MissingApiKey);
        }
        let body = build_chat_completions_body(request, Some(schema), false);
        let url = format!(
            "{}/chat/completions",
            request.config.base_url.trim_end_matches('/')
        );
        let client = crate::http::streaming_client_for(&url);
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
        let parsed: serde_json::Value = serde_json::from_str(
            &extract_structured_json_text(&content).unwrap_or_else(|| content.trim().to_string()),
        )
        .map_err(|e| ModelError::JsonSchemaViolation(e.to_string()))?;
        let usage = json.get("usage").and_then(parse_usage).unwrap_or_default();
        Ok(ModelResponse {
            content,
            json: Some(parsed),
            usage,
            latency_ms: latency,
            model: request.model.clone(),
            finish_reason: json["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("stop")
                .into(),
            provider_kind: request.config.kind,
            reasoning_content: json["choices"][0]["message"]["reasoning_content"]
                .as_str()
                .map(|value| value.to_string()),
            tool_calls: vec![],
        })
    }

    async fn stream(
        &self,
        request: &ModelRequest,
        schema_json: Option<&serde_json::Value>,
        on_event: &mut crate::gateway::ModelStreamHandler<'_>,
    ) -> Result<ModelResponse, ModelError> {
        if allow_test_mock_routing(request) {
            let mut non_stream_request = request.clone();
            non_stream_request.stream = false;
            let response = match schema_json {
                Some(schema) => self.structured(&non_stream_request, schema).await?,
                None => self.chat(&non_stream_request).await?,
            };
            emit_fallback_stream(response, on_event).await
        } else {
            stream_chat_completions(request, schema_json, on_event).await
        }
    }
}

/// Test-only mock routing for the `desktop-test` provider.
///
/// Routing to the mock requires ALL of:
/// 1. `provider_id == "desktop-test"`
/// 2. `api_key == "sk-test"`
/// 3. a debug build (`cfg!(debug_assertions)`), OR the explicit opt-in env
///    var `COEVO_ENABLE_TEST_MODEL_GATEWAY=1`.
///
/// Release builds without the env var never route to the mock.
fn allow_test_mock_routing(request: &ModelRequest) -> bool {
    let test_gateway_enabled = cfg!(debug_assertions)
        || std::env::var("COEVO_ENABLE_TEST_MODEL_GATEWAY").as_deref() == Ok("1");
    test_gateway_enabled
        && request.config.provider_id == "desktop-test"
        && request.config.api_key == "sk-test"
}

async fn chat_once_or_retry(request: &ModelRequest) -> Result<ModelResponse, ModelError> {
    let mut first_attempt = request.clone();
    match execute_non_stream_chat(&first_attempt).await {
        Ok(response) => Ok(response),
        Err(ModelError::InvalidResponse(message))
            if should_retry_empty_length_response(&message, &first_attempt) =>
        {
            first_attempt.max_tokens = retry_max_tokens(&first_attempt);
            execute_non_stream_chat(&first_attempt).await
        }
        Err(error) => Err(error),
    }
}

fn should_retry_empty_length_response(message: &str, request: &ModelRequest) -> bool {
    message.contains("empty completion content")
        && message.contains("finish_reason: length")
        && request.max_tokens < request.config.max_tokens
}

fn retry_max_tokens(request: &ModelRequest) -> u32 {
    request
        .config
        .max_tokens
        .max(request.max_tokens.saturating_mul(2))
}

async fn execute_non_stream_chat(request: &ModelRequest) -> Result<ModelResponse, ModelError> {
    let body = build_chat_completions_body(request, None, false);
    let url = format!(
        "{}/chat/completions",
        request.config.base_url.trim_end_matches('/')
    );
    let client = crate::http::streaming_client_for(&url);
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
    let usage = json.get("usage").and_then(parse_usage).unwrap_or_default();
    Ok(ModelResponse {
        content,
        json: Some(json.clone()),
        usage,
        latency_ms: latency,
        model: request.model.clone(),
        finish_reason: json["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("stop")
            .into(),
        provider_kind: request.config.kind,
        reasoning_content: None,
        tool_calls: vec![],
    })
}

pub(crate) fn provider_http_error(status: StatusCode, body: &str) -> ModelError {
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

fn deterministic_test_usage(request: &ModelRequest, content: &str) -> ModelUsage {
    fn estimate_tokens(text: &str) -> u64 {
        let chars = text.chars().count() as u64;
        chars.div_ceil(4).max(1)
    }

    let prompt_tokens = request
        .messages
        .iter()
        .map(|message| estimate_tokens(&message.content))
        .sum::<u64>()
        .max(1);
    let completion_tokens = estimate_tokens(content);
    ModelUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
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

fn no_op_event_handler(
    _event: ModelStreamEvent,
) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>> {
    Box::pin(async { Ok(()) })
}

pub fn extract_structured_json_text(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return Some(trimmed.to_string());
    }
    if let Some(fenced) = extract_last_fenced_json_block(trimmed) {
        return Some(fenced);
    }
    extract_first_json_value(trimmed)
}

fn extract_last_fenced_json_block(content: &str) -> Option<String> {
    let mut in_fence = false;
    let mut current = Vec::new();
    let mut last_valid = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if !in_fence {
            if trimmed.starts_with("```") {
                in_fence = true;
                current.clear();
            }
            continue;
        }

        if trimmed == "```" {
            let body = current.join("\n");
            let body = body.trim();
            if !body.is_empty() && serde_json::from_str::<serde_json::Value>(body).is_ok() {
                last_valid = Some(body.to_string());
            }
            in_fence = false;
            current.clear();
            continue;
        }

        current.push(line);
    }

    last_valid
}

fn extract_first_json_value(content: &str) -> Option<String> {
    let start = content.find(['{', '['])?;
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaping = false;
    let mut started = false;
    let mut begin = None;

    for (idx, ch) in chars.into_iter().skip_while(|(idx, _)| *idx < start) {
        if !started {
            started = true;
            begin = Some(idx);
        }
        if in_string {
            if escaping {
                escaping = false;
                continue;
            }
            match ch {
                '\\' => escaping = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    let begin = begin?;
                    let candidate = content[begin..=idx].trim();
                    if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                        return Some(candidate.to_string());
                    }
                    return None;
                }
            }
            _ => {}
        }
    }
    None
}

fn build_chat_completions_body(
    request: &ModelRequest,
    _schema_json: Option<&serde_json::Value>,
    stream: bool,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.model,
        "messages": request
            .messages
            .iter()
            .map(model_message_to_json)
            .collect::<Vec<_>>(),
        "temperature": request.temperature,
        "max_tokens": request.max_tokens,
        "stream": stream,
    });
    if stream {
        body["stream_options"] = serde_json::json!({
            "include_usage": true
        });
    }
    if matches!(request.response_format, ResponseFormat::Json) {
        body["response_format"] = serde_json::json!({"type": "json_object"});
    }
    if !request.tools.is_empty() {
        body["tools"] = serde_json::Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    let mut function = serde_json::json!({
                        "name": tool.name,
                        "parameters": tool.parameters_json,
                    });
                    if let Some(description) = &tool.description {
                        function["description"] = serde_json::Value::String(description.clone());
                    }
                    serde_json::json!({
                        "type": "function",
                        "function": function,
                    })
                })
                .collect(),
        );
    }
    if let Some(tool_choice) = &request.tool_choice {
        body["tool_choice"] = tool_choice.clone();
    }
    body
}

fn model_message_to_json(message: &ModelMessage) -> serde_json::Value {
    let mut json = serde_json::json!({
        "role": message.role,
        "content": message.content,
    });
    if let Some(obj) = json.as_object_mut() {
        if let Some(reasoning_content) = &message.reasoning_content {
            obj.insert(
                "reasoning_content".to_string(),
                serde_json::Value::String(reasoning_content.clone()),
            );
        }
        if !message.tool_calls.is_empty() {
            obj.insert(
                "tool_calls".to_string(),
                serde_json::Value::Array(
                    message
                        .tool_calls
                        .iter()
                        .map(|tool_call| {
                            serde_json::json!({
                                "id": tool_call.id,
                                "type": "function",
                                "function": {
                                    "name": tool_call.name,
                                    "arguments": tool_call.arguments,
                                }
                            })
                        })
                        .collect(),
                ),
            );
        }
        if let Some(tool_call_id) = &message.tool_call_id {
            obj.insert(
                "tool_call_id".to_string(),
                serde_json::Value::String(tool_call_id.clone()),
            );
        }
    }
    json
}

async fn emit_fallback_stream(
    response: ModelResponse,
    on_event: &mut crate::gateway::ModelStreamHandler<'_>,
) -> Result<ModelResponse, ModelError> {
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
    for tool_call in &response.tool_calls {
        on_event(ModelStreamEvent::ToolCallDelta {
            index: tool_call.index,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            arguments_delta: tool_call.arguments.clone(),
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

async fn stream_chat_completions(
    request: &ModelRequest,
    schema_json: Option<&serde_json::Value>,
    on_event: &mut crate::gateway::ModelStreamHandler<'_>,
) -> Result<ModelResponse, ModelError> {
    if request.config.api_key.is_empty() {
        return Err(ModelError::MissingApiKey);
    }
    let body = build_chat_completions_body(request, schema_json, true);
    // Shared client without a total request timeout: SSE streams are
    // long-lived, and a whole-request timeout would abort them mid-stream.
    // Connection establishment is still bounded by the client's connect_timeout.
    let url = format!(
        "{}/chat/completions",
        request.config.base_url.trim_end_matches('/')
    );
    let client = crate::http::streaming_client_for(&url);
    let start = std::time::Instant::now();
    let mut resp = client
        .post(&url)
        .header(
            "Authorization",
            format!("Bearer {}", request.config.api_key),
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
    let latency = start.elapsed().as_millis() as u64;
    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        return Err(provider_http_error(status, &body));
    }

    let mut aggregate = StreamAggregate::new(request, latency);
    let mut buffer = String::new();
    let mut saw_done = false;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| ModelError::InvalidResponse(e.to_string()))?
    {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some((event, rest)) = split_next_sse_event(&buffer) {
            let event = event.to_string();
            let rest = rest.to_string();
            buffer = rest;
            if let Some(data) = extract_sse_data(&event) {
                if data == "[DONE]" {
                    saw_done = true;
                    break;
                }
                let json: serde_json::Value = serde_json::from_str(&data)
                    .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
                apply_stream_chunk(&json, &mut aggregate, on_event).await?;
            }
        }
        if saw_done {
            break;
        }
    }
    if !saw_done {
        return Err(ModelError::InvalidResponse(
            "stream ended before [DONE] sentinel".to_string(),
        ));
    }

    let response = aggregate.finish(matches!(request.response_format, ResponseFormat::Json))?;
    if response.usage.total_tokens > 0 {
        on_event(ModelStreamEvent::Usage(response.usage.clone())).await?;
    }
    on_event(ModelStreamEvent::Done {
        finish_reason: Some(response.finish_reason.clone()),
    })
    .await?;
    Ok(response)
}

pub(crate) fn split_next_sse_event(buffer: &str) -> Option<(&str, &str)> {
    if let Some(idx) = buffer.find("\r\n\r\n") {
        return Some((&buffer[..idx], &buffer[idx + 4..]));
    }
    buffer
        .find("\n\n")
        .map(|idx| (&buffer[..idx], &buffer[idx + 2..]))
}

pub(crate) fn extract_sse_data(event: &str) -> Option<String> {
    let mut payload = String::new();
    for line in event.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            if !payload.is_empty() {
                payload.push('\n');
            }
            payload.push_str(data.trim_start());
        }
    }
    if payload.is_empty() {
        None
    } else {
        Some(payload)
    }
}

async fn apply_stream_chunk(
    json: &serde_json::Value,
    aggregate: &mut StreamAggregate,
    on_event: &mut crate::gateway::ModelStreamHandler<'_>,
) -> Result<(), ModelError> {
    if let Some(choice) = json.get("choices").and_then(|choices| choices.get(0)) {
        if let Some(reasoning) = choice
            .get("delta")
            .and_then(|delta| delta.get("reasoning_content"))
            .and_then(|value| value.as_str())
        {
            aggregate.reasoning.push_str(reasoning);
            on_event(ModelStreamEvent::ReasoningDelta {
                delta: reasoning.to_string(),
            })
            .await?;
        }
        if let Some(content) = choice
            .get("delta")
            .and_then(|delta| delta.get("content"))
            .and_then(|value| value.as_str())
        {
            aggregate.content.push_str(content);
            on_event(ModelStreamEvent::ContentDelta {
                delta: content.to_string(),
            })
            .await?;
        }
        if let Some(tool_calls) = choice
            .get("delta")
            .and_then(|delta| delta.get("tool_calls"))
            .and_then(|value| value.as_array())
        {
            for tool_call in tool_calls {
                let index = tool_call
                    .get("index")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0) as usize;
                let entry = aggregate.tool_calls.entry(index).or_insert(ModelToolCall {
                    index,
                    id: None,
                    name: String::new(),
                    arguments: String::new(),
                });
                if let Some(id) = tool_call.get("id").and_then(|value| value.as_str()) {
                    entry.id = Some(id.to_string());
                }
                let mut emitted_name = None;
                if let Some(name) = tool_call
                    .get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(|value| value.as_str())
                {
                    entry.name = name.to_string();
                    emitted_name = Some(name.to_string());
                }
                let arguments_delta = tool_call
                    .get("function")
                    .and_then(|function| function.get("arguments"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if !arguments_delta.is_empty() {
                    entry.arguments.push_str(arguments_delta);
                }
                on_event(ModelStreamEvent::ToolCallDelta {
                    index,
                    id: entry.id.clone(),
                    name: emitted_name,
                    arguments_delta: arguments_delta.to_string(),
                })
                .await?;
            }
        }
        if let Some(finish_reason) = choice.get("finish_reason").and_then(|value| value.as_str()) {
            aggregate.finish_reason = Some(finish_reason.to_string());
        }
    }

    if let Some(usage) = json.get("usage").and_then(parse_usage) {
        aggregate.usage = usage;
    }

    Ok(())
}

fn parse_usage(value: &serde_json::Value) -> Option<ModelUsage> {
    Some(ModelUsage {
        prompt_tokens: value.get("prompt_tokens")?.as_u64()?,
        completion_tokens: value.get("completion_tokens")?.as_u64()?,
        total_tokens: value.get("total_tokens")?.as_u64()?,
    })
}

struct StreamAggregate {
    content: String,
    reasoning: String,
    tool_calls: BTreeMap<usize, ModelToolCall>,
    usage: ModelUsage,
    latency_ms: u64,
    model: String,
    finish_reason: Option<String>,
    provider_kind: ModelProviderKind,
}

impl StreamAggregate {
    fn new(request: &ModelRequest, latency_ms: u64) -> Self {
        Self {
            content: String::new(),
            reasoning: String::new(),
            tool_calls: BTreeMap::new(),
            usage: ModelUsage::default(),
            latency_ms,
            model: request.model.clone(),
            finish_reason: None,
            provider_kind: request.config.kind,
        }
    }

    fn finish(self, structured: bool) -> Result<ModelResponse, ModelError> {
        if self.content.is_empty() && self.reasoning.is_empty() && self.tool_calls.is_empty() {
            let error = if structured {
                ModelError::JsonSchemaViolation(
                    "Provider returned empty structured stream".to_string(),
                )
            } else {
                ModelError::InvalidResponse("Provider returned empty stream".to_string())
            };
            return Err(error);
        }
        let json = if structured && self.tool_calls.is_empty() && !self.content.trim().is_empty() {
            Some(
                serde_json::from_str(
                    &extract_structured_json_text(&self.content)
                        .unwrap_or_else(|| self.content.trim().to_string()),
                )
                .map_err(|e| ModelError::JsonSchemaViolation(e.to_string()))?,
            )
        } else {
            None
        };
        Ok(ModelResponse {
            content: self.content,
            json,
            usage: self.usage,
            latency_ms: self.latency_ms,
            model: self.model,
            finish_reason: self.finish_reason.unwrap_or_else(|| "stop".to_string()),
            provider_kind: self.provider_kind,
            reasoning_content: if self.reasoning.is_empty() {
                None
            } else {
                Some(self.reasoning)
            },
            tool_calls: self.tool_calls.into_values().collect(),
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
    fn test_mock_routing_requires_desktop_test_identity() {
        let mut request = ModelRequest {
            config: ModelProviderConfig {
                provider_id: "desktop-test".into(),
                kind: ModelProviderKind::OpenAICompatible,
                base_url: "https://api.openai.com/v1".into(),
                api_key: "sk-test".into(),
                default_model: "gpt-4o".into(),
                fast_model: "gpt-4o-mini".into(),
                reasoning_model: "o3-mini".into(),
                structured_output_model: "gpt-4o".into(),
                max_tokens: 256,
                temperature: 0.2,
                timeout_ms: 1000,
                max_cost_per_task_usd: 5.0,
            },
            role: ModelRole::Synthesizer,
            model: "gpt-4o-mini".into(),
            messages: vec![],
            temperature: 0.2,
            max_tokens: 128,
            response_format: ResponseFormat::Text,
            stream: false,
            tools: vec![],
            tool_choice: None,
        };
        // Tests build with debug_assertions, so the build-type gate is open;
        // identity must still match exactly.
        assert!(allow_test_mock_routing(&request));
        request.config.api_key = "sk-real".into();
        assert!(!allow_test_mock_routing(&request));
        request.config.api_key = "sk-test".into();
        request.config.provider_id = "desktop".into();
        assert!(!allow_test_mock_routing(&request));
    }

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

    #[test]
    fn structured_response_preserves_reasoning_content_field() {
        let json = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"accuracy\":1.0,\"relevance\":1.0}",
                    "reasoning_content": "The output exactly matches the expected answer."
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });
        let content = extract_choice_content(&json, true).expect("structured content");
        let parsed: serde_json::Value =
            serde_json::from_str(&extract_structured_json_text(&content).expect("json body"))
                .expect("parsed json");
        let response = ModelResponse {
            content,
            json: Some(parsed),
            usage: parse_usage(&json["usage"]).unwrap(),
            latency_ms: 1,
            model: "deepseek-v4-flash".to_string(),
            finish_reason: json["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("stop")
                .to_string(),
            provider_kind: ModelProviderKind::OpenAICompatible,
            reasoning_content: json["choices"][0]["message"]["reasoning_content"]
                .as_str()
                .map(|value| value.to_string()),
            tool_calls: vec![],
        };
        assert_eq!(
            response.reasoning_content.as_deref(),
            Some("The output exactly matches the expected answer.")
        );
    }

    #[test]
    fn extract_structured_json_text_accepts_fenced_json() {
        let content = "```json\n{\"transcript\":[{\"agent_id\":\"agent-founder-01\",\"stance\":\"support\",\"text\":\"Ship it.\"}],\"resolution_md\":\"Done\",\"responsibility_anchor\":\"founder\"}\n```";
        let extracted =
            extract_structured_json_text(content).expect("fenced JSON should be extracted");

        let parsed: serde_json::Value =
            serde_json::from_str(&extracted).expect("extracted content should be valid JSON");
        assert_eq!(parsed["resolution_md"], "Done");
    }

    #[test]
    fn extract_structured_json_text_prefers_final_fenced_json_over_earlier_inline_objects() {
        let content = r#"I investigated the request carefully.

The sandbox profile contains this policy evidence:
{"readonly_guards":[".git",".env"],"tier":"read_only"}

Final structured answer:
```json
{"thought":"The guard policy blocks access.","proposal":{"kind":"finish","summary":"The .env file exists but is guarded.","result":{"blocked":true}},"confidence":1.0}
```"#;
        let extracted =
            extract_structured_json_text(content).expect("final fenced JSON should be extracted");

        let parsed: serde_json::Value =
            serde_json::from_str(&extracted).expect("extracted content should be valid JSON");
        assert_eq!(parsed["proposal"]["kind"], "finish");
        assert_eq!(parsed["proposal"]["result"]["blocked"], true);
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
        assert!(
            response.latency_ms >= 20,
            "latency was {}",
            response.latency_ms
        );
    }

    #[tokio::test]
    async fn chat_stream_aggregates_content_reasoning_and_tool_calls() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = Vec::new();
            let mut chunk = [0_u8; 4096];
            let mut content_length = None;
            loop {
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(header_end) = text.find("\r\n\r\n") {
                    if content_length.is_none() {
                        let headers = &text[..header_end];
                        for line in headers.lines() {
                            let lower = line.to_ascii_lowercase();
                            if let Some(value) = lower.strip_prefix("content-length:") {
                                content_length = value.trim().parse::<usize>().ok();
                                break;
                            }
                        }
                    }
                    if let Some(expected) = content_length {
                        let body_len = buf.len().saturating_sub(header_end + 4);
                        if body_len >= expected {
                            break;
                        }
                    }
                }
            }
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Need evidence. \"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"file-readonly\",\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"README.md\\\"}\"}}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"world\"},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
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
        let request = ModelRequest {
            config,
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Inspect README.md".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 256,
            response_format: ResponseFormat::Text,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };

        let response = OpenAICompatibleGateway
            .chat(&request)
            .await
            .expect("stream response should aggregate");
        server.await.unwrap();

        assert_eq!(response.content, "Hello world");
        assert_eq!(response.finish_reason, "tool_calls");
        assert_eq!(response.usage.total_tokens, 18);
        assert_eq!(
            response.reasoning_content.as_deref(),
            Some("Need evidence. ")
        );
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(response.tool_calls[0].name, "file-readonly");
        assert_eq!(response.tool_calls[0].arguments, "{\"path\":\"README.md\"}");
    }

    #[tokio::test]
    async fn chat_stream_emits_single_final_usage_before_done() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = Vec::new();
            let mut chunk = [0_u8; 4096];
            let mut content_length = None;
            loop {
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(header_end) = text.find("\r\n\r\n") {
                    if content_length.is_none() {
                        let headers = &text[..header_end];
                        for line in headers.lines() {
                            let lower = line.to_ascii_lowercase();
                            if let Some(value) = lower.strip_prefix("content-length:") {
                                content_length = value.trim().parse::<usize>().ok();
                                break;
                            }
                        }
                    }
                    if let Some(expected) = content_length {
                        let body_len = buf.len().saturating_sub(header_end + 4);
                        if body_len >= expected {
                            break;
                        }
                    }
                }
            }
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1,\"total_tokens\":5}}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"world\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
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
        let request = ModelRequest {
            config,
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Say hello".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 64,
            response_format: ResponseFormat::Text,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };

        let mut events = Vec::new();
        let mut sink = |event: ModelStreamEvent| {
            events.push(event);
            Box::pin(async { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>
        };

        let response = OpenAICompatibleGateway
            .stream(&request, None, &mut sink)
            .await
            .expect("stream response should succeed");
        server.await.unwrap();

        let usage_events = events
            .iter()
            .filter_map(|event| match event {
                ModelStreamEvent::Usage(usage) => Some(usage.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(usage_events.len(), 1);
        assert_eq!(usage_events[0].total_tokens, 18);
        assert_eq!(response.usage.total_tokens, 18);

        let usage_index = events
            .iter()
            .position(|event| matches!(event, ModelStreamEvent::Usage(_)))
            .expect("Usage event should be emitted");
        let done_index = events
            .iter()
            .position(|event| matches!(event, ModelStreamEvent::Done { .. }))
            .expect("Done event should be emitted");
        assert!(usage_index < done_index);
    }

    #[tokio::test]
    async fn stream_rejects_missing_done_sentinel() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0_u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3,\"total_tokens\":8}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
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
            temperature: 0.2,
            timeout_ms: 1000,
            max_cost_per_task_usd: 5.0,
        };
        let request = ModelRequest {
            config,
            role: ModelRole::Synthesizer,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Say hello.".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 128,
            response_format: ResponseFormat::Text,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };

        let mut events = Vec::new();
        let mut sink = |event: ModelStreamEvent| {
            events.push(event);
            Box::pin(async { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>
        };

        let err = OpenAICompatibleGateway
            .stream(&request, None, &mut sink)
            .await
            .expect_err("stream without [DONE] sentinel should fail closed");
        server.await.unwrap();

        assert!(matches!(err, ModelError::InvalidResponse(message) if message.contains("[DONE]")));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, ModelStreamEvent::Done { .. })),
            "missing sentinel must not emit a synthetic Done event"
        );
    }

    #[tokio::test]
    async fn non_stream_chat_parses_usage_from_provider_response() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0_u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            let body = serde_json::json!({
                "id": "chatcmpl-test",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Hello from DeepSeek."
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 13,
                    "completion_tokens": 5,
                    "total_tokens": 18
                }
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
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
            temperature: 0.2,
            timeout_ms: 1000,
            max_cost_per_task_usd: 5.0,
        };
        let request = ModelRequest {
            config,
            role: ModelRole::Synthesizer,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Say hello.".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 128,
            response_format: ResponseFormat::Text,
            stream: false,
            tools: vec![],
            tool_choice: None,
        };

        let response = OpenAICompatibleGateway.chat(&request).await.unwrap();
        server.await.unwrap();

        assert_eq!(response.content, "Hello from DeepSeek.");
        assert_eq!(response.usage.prompt_tokens, 13);
        assert_eq!(response.usage.completion_tokens, 5);
        assert_eq!(response.usage.total_tokens, 18);
    }

    #[tokio::test]
    async fn non_stream_chat_retries_once_when_provider_stops_at_length_with_empty_content() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured_requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let captured_requests_server = captured_requests.clone();
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            for attempt in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0_u8; 4096];
                let mut content_length = None;
                loop {
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    if let Some(header_end) = text.find("\r\n\r\n") {
                        if content_length.is_none() {
                            let headers = &text[..header_end];
                            for line in headers.lines() {
                                let lower = line.to_ascii_lowercase();
                                if let Some(value) = lower.strip_prefix("content-length:") {
                                    content_length = value.trim().parse::<usize>().ok();
                                    break;
                                }
                            }
                        }
                        if let Some(expected) = content_length {
                            let body_len = buf.len().saturating_sub(header_end + 4);
                            if body_len >= expected {
                                break;
                            }
                        }
                    }
                }
                captured_requests_server
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).to_string());

                let body = if attempt == 0 {
                    serde_json::json!({
                        "id": "chatcmpl-empty",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": ""
                            },
                            "finish_reason": "length"
                        }],
                        "usage": {
                            "prompt_tokens": 13,
                            "completion_tokens": 128,
                            "total_tokens": 141
                        }
                    })
                    .to_string()
                } else {
                    serde_json::json!({
                        "id": "chatcmpl-retry",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": "Hello after retry."
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": {
                            "prompt_tokens": 13,
                            "completion_tokens": 9,
                            "total_tokens": 22
                        }
                    })
                    .to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
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
            max_tokens: 4096,
            temperature: 0.2,
            timeout_ms: 1000,
            max_cost_per_task_usd: 5.0,
        };
        let request = ModelRequest {
            config,
            role: ModelRole::Synthesizer,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Say hello.".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 128,
            response_format: ResponseFormat::Text,
            stream: false,
            tools: vec![],
            tool_choice: None,
        };

        let response = OpenAICompatibleGateway.chat(&request).await.unwrap();
        server.await.unwrap();

        assert_eq!(response.content, "Hello after retry.");
        let requests = captured_requests.lock().unwrap();
        assert_eq!(requests.len(), 2);

        let first_body = requests[0].split("\r\n\r\n").nth(1).unwrap_or("");
        let first_json: serde_json::Value = serde_json::from_str(first_body).unwrap();
        assert_eq!(first_json["max_tokens"], serde_json::json!(128));

        let second_body = requests[1].split("\r\n\r\n").nth(1).unwrap_or("");
        let second_json: serde_json::Value = serde_json::from_str(second_body).unwrap();
        assert_eq!(second_json["max_tokens"], serde_json::json!(4096));
    }

    #[tokio::test]
    async fn desktop_test_chat_returns_nonzero_usage_for_offline_acceptance_paths() {
        let request = ModelRequest {
            config: ModelProviderConfig {
                provider_id: "desktop-test".into(),
                kind: ModelProviderKind::OpenAICompatible,
                base_url: "https://api.openai.com/v1".into(),
                api_key: "sk-test".into(),
                default_model: "gpt-4o".into(),
                fast_model: "gpt-4o-mini".into(),
                reasoning_model: "o3-mini".into(),
                structured_output_model: "gpt-4o".into(),
                max_tokens: 256,
                temperature: 0.2,
                timeout_ms: 1000,
                max_cost_per_task_usd: 5.0,
            },
            role: ModelRole::Synthesizer,
            model: "gpt-4o-mini".into(),
            messages: vec![
                ModelMessage {
                    role: "system".into(),
                    content: "You are a helpful test model.".into(),
                    ..Default::default()
                },
                ModelMessage {
                    role: "user".into(),
                    content: "Summarize product discovery.".into(),
                    ..Default::default()
                },
            ],
            temperature: 0.2,
            max_tokens: 128,
            response_format: ResponseFormat::Text,
            stream: false,
            tools: vec![],
            tool_choice: None,
        };

        let response = OpenAICompatibleGateway.chat(&request).await.unwrap();

        assert!(response.usage.prompt_tokens > 0);
        assert!(response.usage.completion_tokens > 0);
        assert_eq!(
            response.usage.total_tokens,
            response.usage.prompt_tokens + response.usage.completion_tokens
        );
    }

    #[tokio::test]
    async fn stream_emits_done_event_after_done_sentinel() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0_u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
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
        let request = ModelRequest {
            config,
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Say hello".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 64,
            response_format: ResponseFormat::Text,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };

        let mut events = Vec::new();
        let mut sink = |event: ModelStreamEvent| {
            events.push(event);
            Box::pin(async { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>
        };

        let response = OpenAICompatibleGateway
            .stream(&request, None, &mut sink)
            .await
            .expect("stream response should succeed");
        server.await.unwrap();

        assert_eq!(response.finish_reason, "stop");
        assert!(matches!(
            events.last(),
            Some(ModelStreamEvent::Done {
                finish_reason: Some(reason)
            }) if reason == "stop"
        ));
    }

    #[tokio::test]
    async fn structured_stream_with_tools_requests_json_object_response_format() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_request_server = captured_request.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = Vec::new();
            let mut chunk = [0_u8; 4096];
            let mut content_length = None;
            loop {
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(header_end) = text.find("\r\n\r\n") {
                    if content_length.is_none() {
                        let headers = &text[..header_end];
                        for line in headers.lines() {
                            let lower = line.to_ascii_lowercase();
                            if let Some(value) = lower.strip_prefix("content-length:") {
                                content_length = value.trim().parse::<usize>().ok();
                                break;
                            }
                        }
                    }
                    if let Some(expected) = content_length {
                        let body_len = buf.len().saturating_sub(header_end + 4);
                        if body_len >= expected {
                            break;
                        }
                    }
                }
            }
            *captured_request_server.lock().unwrap() = String::from_utf8_lossy(&buf).to_string();
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"file-readonly\",\"arguments\":\"{\\\"action\\\":\\\"ReadFile\\\",\\\"path\\\":\\\"welcome.md\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
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
        let request = ModelRequest {
            config,
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Read welcome.md with the tool".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 128,
            response_format: ResponseFormat::Json,
            stream: true,
            tools: vec![ModelToolDefinition {
                name: "file-readonly".into(),
                description: Some("File Readonly (actions: ReadFile, ListDirectory)".into()),
                parameters_json: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": { "type": "string" },
                        "path": { "type": "string" }
                    },
                    "required": ["action", "path"],
                    "additionalProperties": true
                }),
            }],
            tool_choice: Some("auto".into()),
        };

        let mut sink = |_event: ModelStreamEvent| {
            Box::pin(async { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>
        };

        let response = OpenAICompatibleGateway
            .stream(
                &request,
                Some(&serde_json::json!({"type": "object"})),
                &mut sink,
            )
            .await
            .expect("tool calling stream should succeed");
        server.await.unwrap();

        let raw = captured_request.lock().unwrap().clone();
        let body = raw.split("\r\n\r\n").nth(1).unwrap_or("");
        let request_json: serde_json::Value =
            serde_json::from_str(body).expect("request body should be JSON");
        assert_eq!(
            request_json["response_format"]["type"],
            serde_json::json!("json_object"),
            "structured tool-calling request should request json_object response_format: {request_json}"
        );
        assert_eq!(response.finish_reason, "tool_calls");
        assert_eq!(response.tool_calls.len(), 1);
    }

    #[tokio::test]
    async fn structured_stream_with_tools_respects_text_response_format_when_requested() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_request_server = captured_request.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = Vec::new();
            let mut chunk = [0_u8; 4096];
            let mut content_length = None;
            loop {
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(header_end) = text.find("\r\n\r\n") {
                    if content_length.is_none() {
                        let headers = &text[..header_end];
                        for line in headers.lines() {
                            let lower = line.to_ascii_lowercase();
                            if let Some(value) = lower.strip_prefix("content-length:") {
                                content_length = value.trim().parse::<usize>().ok();
                                break;
                            }
                        }
                    }
                    if let Some(expected) = content_length {
                        let body_len = buf.len().saturating_sub(header_end + 4);
                        if body_len >= expected {
                            break;
                        }
                    }
                }
            }
            *captured_request_server.lock().unwrap() = String::from_utf8_lossy(&buf).to_string();
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
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
        let request = ModelRequest {
            config,
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Inspect README.md".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 128,
            response_format: ResponseFormat::Text,
            stream: true,
            tools: vec![ModelToolDefinition {
                name: "file-readonly".into(),
                description: Some("File Readonly (actions: ReadFile, ListDirectory)".into()),
                parameters_json: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": { "type": "string" },
                        "path": { "type": "string" }
                    },
                    "required": ["action", "path"],
                    "additionalProperties": true
                }),
            }],
            tool_choice: Some("auto".into()),
        };

        let mut sink = |_event: ModelStreamEvent| {
            Box::pin(async { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>
        };

        let response = OpenAICompatibleGateway
            .stream(
                &request,
                Some(&serde_json::json!({"type": "object"})),
                &mut sink,
            )
            .await
            .expect("tool calling stream should succeed");
        server.await.unwrap();

        let raw = captured_request.lock().unwrap().clone();
        let body = raw.split("\r\n\r\n").nth(1).unwrap_or("");
        let request_json: serde_json::Value =
            serde_json::from_str(body).expect("request body should be JSON");
        assert!(
            request_json.get("response_format").is_none(),
            "text response format should not be overwritten: {request_json}"
        );
        assert_eq!(response.finish_reason, "stop");
    }

    #[test]
    fn build_chat_completions_body_preserves_reasoning_and_tool_call_messages() {
        let request = ModelRequest {
            config: ModelProviderConfig::mock(),
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![
                ModelMessage {
                    role: "assistant".into(),
                    content: String::new(),
                    reasoning_content: Some("Need to inspect the file first.".into()),
                    tool_calls: vec![ModelToolCall {
                        index: 0,
                        id: Some("call_1".into()),
                        name: "file-readonly".into(),
                        arguments: "{\"action\":\"ReadFile\",\"path\":\"welcome.md\"}".into(),
                    }],
                    tool_call_id: None,
                },
                ModelMessage {
                    role: "tool".into(),
                    content: "{\"path\":\"welcome.md\",\"content\":\"hello\"}".into(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    tool_call_id: Some("call_1".into()),
                },
            ],
            temperature: 0.2,
            max_tokens: 128,
            response_format: ResponseFormat::Json,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };

        let body = build_chat_completions_body(
            &request,
            Some(&serde_json::json!({"type":"object"})),
            true,
        );
        let messages = body["messages"]
            .as_array()
            .expect("messages should be an array");

        assert_eq!(
            messages[0]["reasoning_content"],
            serde_json::json!("Need to inspect the file first.")
        );
        assert_eq!(
            messages[0]["tool_calls"][0]["id"],
            serde_json::json!("call_1")
        );
        assert_eq!(
            messages[0]["tool_calls"][0]["function"]["name"],
            serde_json::json!("file-readonly")
        );
        assert_eq!(messages[1]["tool_call_id"], serde_json::json!("call_1"));
    }

    #[test]
    fn build_chat_completions_body_requests_usage_in_stream_mode() {
        let request = ModelRequest {
            config: ModelProviderConfig::mock(),
            role: ModelRole::Synthesizer,
            model: "deepseek-chat".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Say hello.".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 64,
            response_format: ResponseFormat::Text,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };

        let body = build_chat_completions_body(&request, None, true);

        assert_eq!(body["stream"], serde_json::json!(true));
        assert_eq!(
            body["stream_options"]["include_usage"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn structured_finish_skips_json_parse_when_native_tool_calls_exist() {
        let request = ModelRequest {
            config: ModelProviderConfig::mock(),
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![],
            temperature: 0.0,
            max_tokens: 128,
            response_format: ResponseFormat::Json,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };
        let mut aggregate = StreamAggregate::new(&request, 321);
        aggregate.content = "Need to call the file tool first.".into();
        aggregate.usage = ModelUsage {
            prompt_tokens: 11,
            completion_tokens: 7,
            total_tokens: 18,
        };
        aggregate.finish_reason = Some("tool_calls".into());
        aggregate.tool_calls.insert(
            0,
            ModelToolCall {
                index: 0,
                id: Some("call_1".into()),
                name: "file-readonly".into(),
                arguments: "{\"path\":\"welcome.md\"}".into(),
            },
        );

        let response = aggregate
            .finish(true)
            .expect("tool-calling structured stream should not require JSON content");

        assert!(response.json.is_none());
        assert_eq!(response.finish_reason, "tool_calls");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "file-readonly");
    }

    #[test]
    fn structured_finish_still_parses_json_without_tool_calls() {
        let request = ModelRequest {
            config: ModelProviderConfig::mock(),
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![],
            temperature: 0.0,
            max_tokens: 128,
            response_format: ResponseFormat::Json,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };
        let mut aggregate = StreamAggregate::new(&request, 123);
        aggregate.content = "{\"proposal\":{\"kind\":\"finish\",\"summary\":\"done\"}}".into();

        let response = aggregate
            .finish(true)
            .expect("structured stream JSON should still parse");

        assert_eq!(
            response.json,
            Some(serde_json::json!({"proposal":{"kind":"finish","summary":"done"}}))
        );
    }

    #[test]
    fn structured_finish_extracts_json_from_fenced_block() {
        let request = ModelRequest {
            config: ModelProviderConfig::mock(),
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![],
            temperature: 0.0,
            max_tokens: 128,
            response_format: ResponseFormat::Json,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };
        let mut aggregate = StreamAggregate::new(&request, 456);
        aggregate.content =
            "```json\n{\"proposal\":{\"kind\":\"finish\",\"summary\":\"done\"}}\n```".into();

        let response = aggregate
            .finish(true)
            .expect("structured stream should parse fenced JSON");

        assert_eq!(
            response.json,
            Some(serde_json::json!({"proposal":{"kind":"finish","summary":"done"}}))
        );
    }

    #[test]
    fn structured_finish_extracts_json_from_prose_wrapped_content() {
        let request = ModelRequest {
            config: ModelProviderConfig::mock(),
            role: ModelRole::AgentReasoning,
            model: "deepseek-chat".into(),
            messages: vec![],
            temperature: 0.0,
            max_tokens: 128,
            response_format: ResponseFormat::Json,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };
        let mut aggregate = StreamAggregate::new(&request, 654);
        aggregate.content =
            "Here is the final answer as JSON:\n{\"proposal\":{\"kind\":\"finish\",\"summary\":\"done\"}}"
                .into();

        let response = aggregate
            .finish(true)
            .expect("structured stream should parse prose-wrapped JSON");

        assert_eq!(
            response.json,
            Some(serde_json::json!({"proposal":{"kind":"finish","summary":"done"}}))
        );
    }

    #[tokio::test]
    async fn mock_gateway_stream_falls_back_to_single_frame_aggregation() {
        let config = ModelProviderConfig::mock();
        let request = ModelRequest {
            config,
            role: ModelRole::AgentReasoning,
            model: "mock-model".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Analyze README.md".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 256,
            response_format: ResponseFormat::Json,
            stream: true,
            tools: vec![],
            tool_choice: None,
        };
        let mut events = Vec::new();
        let mut sink = |event: ModelStreamEvent| {
            events.push(event);
            Box::pin(async { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>
        };

        let response = crate::mock::MockModelGateway
            .stream(&request, None, &mut sink)
            .await
            .expect("mock gateway should still aggregate");

        assert!(!response.content.is_empty());
        assert_eq!(response.provider_kind, ModelProviderKind::Mock);
        assert!(matches!(events.last(), Some(ModelStreamEvent::Done { .. })));
    }
}
