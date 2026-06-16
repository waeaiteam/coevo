//! Native Anthropic Messages API gateway.
//!
//! Speaks `POST {base}/v1/messages` directly (default base
//! `https://api.anthropic.com`, overridable via the provider config's
//! `base_url`) with `x-api-key` + `anthropic-version: 2023-06-01` headers.
//! Supports non-streaming chat, structured output (JSON instruction +
//! tolerant extraction), and SSE streaming mapped onto the crate's
//! `ModelStreamEvent` variants.

use crate::gateway::ModelGateway;
use crate::openai::{
    extract_sse_data, extract_structured_json_text, provider_http_error, split_next_sse_event,
};
use crate::types::*;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

pub struct AnthropicGateway;

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

#[async_trait]
impl ModelGateway for AnthropicGateway {
    async fn test_connection(
        &self,
        config: &ModelProviderConfig,
    ) -> Result<ModelResponse, ModelError> {
        if config.api_key.is_empty() {
            return Err(ModelError::MissingApiKey);
        }
        let url = format!("{}/v1/models?limit=1", anthropic_base(config));
        let client = crate::http::short_call_client_for(&url);
        let start = std::time::Instant::now();
        let resp = client
            .get(url)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
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
        let url = format!("{}/v1/models?limit=1000", anthropic_base(config));
        let client = crate::http::short_call_client_for(&url);
        let resp = client
            .get(url)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
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
                    let display_name = item
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(id);
                    models.push(enrich_anthropic_model(id, display_name));
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
        execute_messages_call(request, None).await
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
        execute_messages_call(request, Some(schema)).await
    }

    async fn stream(
        &self,
        request: &ModelRequest,
        schema_json: Option<&serde_json::Value>,
        on_event: &mut crate::gateway::ModelStreamHandler<'_>,
    ) -> Result<ModelResponse, ModelError> {
        stream_messages(request, schema_json, on_event).await
    }
}

fn no_op_event_handler(
    _event: ModelStreamEvent,
) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>> {
    Box::pin(async { Ok(()) })
}

/// Normalize the provider `base_url` into the Anthropic API origin.
/// Empty configs fall back to the public endpoint; OpenAI-style configs that
/// store a trailing `/v1` are tolerated so `/v1/messages` is not doubled.
fn anthropic_base(config: &ModelProviderConfig) -> String {
    let raw = config.base_url.trim().trim_end_matches('/');
    if raw.is_empty() {
        return DEFAULT_BASE_URL.to_string();
    }
    raw.strip_suffix("/v1").unwrap_or(raw).to_string()
}

fn enrich_anthropic_model(id: &str, display_name: &str) -> DiscoveredModel {
    let lower = id.to_lowercase();
    let supports_reasoning = !lower.contains("claude-3-5") && !lower.contains("claude-3-haiku");
    DiscoveredModel {
        id: id.to_string(),
        display_name: display_name.to_string(),
        max_context_tokens: Some(200_000),
        max_output_tokens: Some(if lower.contains("-4") { 64_000 } else { 8_192 }),
        supports_json: true,
        supports_reasoning,
    }
}

const JSON_ONLY_INSTRUCTION: &str = "You must respond with a single valid JSON object and nothing else: no prose, no markdown fences, no explanations.";

/// Build the `/v1/messages` request body from a `ModelRequest`.
///
/// - `system`-role messages are lifted into the top-level `system` field.
/// - assistant messages carrying tool calls become `tool_use` content blocks.
/// - `tool`-role messages become user-role `tool_result` blocks.
/// - structured output is forced via a JSON-only system instruction (plus
///   the schema, when provided) and tolerant extraction on the way out.
fn build_messages_body(
    request: &ModelRequest,
    schema_json: Option<&serde_json::Value>,
    stream: bool,
) -> serde_json::Value {
    let mut system_parts: Vec<String> = Vec::new();
    let mut messages: Vec<serde_json::Value> = Vec::new();

    for message in &request.messages {
        match message.role.as_str() {
            "system" => {
                if !message.content.is_empty() {
                    system_parts.push(message.content.clone());
                }
            }
            "assistant" if !message.tool_calls.is_empty() => {
                let mut blocks: Vec<serde_json::Value> = Vec::new();
                if !message.content.is_empty() {
                    blocks.push(serde_json::json!({
                        "type": "text",
                        "text": message.content,
                    }));
                }
                for tool_call in &message.tool_calls {
                    let input: serde_json::Value = serde_json::from_str(&tool_call.arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": tool_call
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("toolu_{}", tool_call.index)),
                        "name": tool_call.name,
                        "input": input,
                    }));
                }
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": blocks,
                }));
            }
            "tool" => {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
                        "content": message.content,
                    }],
                }));
            }
            role => {
                messages.push(serde_json::json!({
                    "role": if role == "assistant" { "assistant" } else { "user" },
                    "content": message.content,
                }));
            }
        }
    }

    let structured =
        schema_json.is_some() || matches!(request.response_format, ResponseFormat::Json);
    if structured {
        let mut instruction = JSON_ONLY_INSTRUCTION.to_string();
        if let Some(schema) = schema_json {
            instruction.push_str("\nThe JSON object must conform to this JSON Schema:\n");
            instruction.push_str(&schema.to_string());
        }
        system_parts.push(instruction);
    }

    let mut body = serde_json::json!({
        "model": request.model,
        "max_tokens": request.max_tokens.max(1),
        "temperature": request.temperature.clamp(0.0, 1.0),
        "messages": messages,
        "stream": stream,
    });
    if !system_parts.is_empty() {
        body["system"] = serde_json::Value::String(system_parts.join("\n\n"));
    }
    if !request.tools.is_empty() {
        body["tools"] = serde_json::Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    let mut entry = serde_json::json!({
                        "name": tool.name,
                        "input_schema": tool.parameters_json,
                    });
                    if let Some(description) = &tool.description {
                        entry["description"] = serde_json::Value::String(description.clone());
                    }
                    entry
                })
                .collect(),
        );
    }
    if let Some(tool_choice) = map_tool_choice(request.tool_choice.as_ref()) {
        body["tool_choice"] = tool_choice;
    }
    body
}

/// Map OpenAI-style `tool_choice` values onto Anthropic's equivalents.
fn map_tool_choice(tool_choice: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let value = tool_choice?;
    if let Some(text) = value.as_str() {
        return match text {
            "auto" => Some(serde_json::json!({"type": "auto"})),
            "none" => Some(serde_json::json!({"type": "none"})),
            "required" | "any" => Some(serde_json::json!({"type": "any"})),
            _ => None,
        };
    }
    // {"type":"function","function":{"name":...}} → {"type":"tool","name":...}
    if let Some(name) = value
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(|name| name.as_str())
    {
        return Some(serde_json::json!({"type": "tool", "name": name}));
    }
    if value.get("type").is_some() {
        // Already Anthropic-shaped ({"type":"auto"|"any"|"none"|"tool",...}).
        return Some(value.clone());
    }
    None
}

fn map_stop_reason(reason: &str) -> String {
    match reason {
        "end_turn" | "stop_sequence" => "stop".to_string(),
        "tool_use" => "tool_calls".to_string(),
        "max_tokens" => "length".to_string(),
        other => other.to_string(),
    }
}

fn parse_anthropic_usage(value: Option<&serde_json::Value>) -> (Option<u64>, Option<u64>) {
    let Some(usage) = value else {
        return (None, None);
    };
    (
        usage.get("input_tokens").and_then(|v| v.as_u64()),
        usage.get("output_tokens").and_then(|v| v.as_u64()),
    )
}

async fn execute_messages_call(
    request: &ModelRequest,
    schema_json: Option<&serde_json::Value>,
) -> Result<ModelResponse, ModelError> {
    if request.config.api_key.is_empty() {
        return Err(ModelError::MissingApiKey);
    }
    let url = format!("{}/v1/messages", anthropic_base(&request.config));
    let body = build_messages_body(request, schema_json, false);
    let client = crate::http::streaming_client_for(&url);
    let start = std::time::Instant::now();
    let resp = client
        .post(&url)
        .header("x-api-key", &request.config.api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .timeout(std::time::Duration::from_millis(request.config.timeout_ms))
        .send()
        .await
        .map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
    let latency = start.elapsed().as_millis() as u64;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
    if !status.is_success() {
        return Err(provider_http_error(status, &text));
    }
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
    parse_messages_response(&json, request, latency, schema_json.is_some())
}

fn parse_messages_response(
    json: &serde_json::Value,
    request: &ModelRequest,
    latency_ms: u64,
    structured: bool,
) -> Result<ModelResponse, ModelError> {
    let mut content = String::new();
    let mut reasoning = String::new();
    let mut tool_calls: Vec<ModelToolCall> = Vec::new();

    let Some(blocks) = json.get("content").and_then(|v| v.as_array()) else {
        return Err(ModelError::InvalidResponse(
            "Anthropic response has no content array".to_string(),
        ));
    };
    for block in blocks {
        match block.get("type").and_then(|v| v.as_str()) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    content.push_str(text);
                }
            }
            Some("thinking") => {
                if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
                    reasoning.push_str(thinking);
                }
            }
            Some("tool_use") => {
                let arguments = block
                    .get("input")
                    .map(|input| input.to_string())
                    .unwrap_or_else(|| "{}".to_string());
                tool_calls.push(ModelToolCall {
                    index: tool_calls.len(),
                    id: block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|id| id.to_string()),
                    name: block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    arguments,
                });
            }
            _ => {}
        }
    }

    let content = content.trim().to_string();
    if content.is_empty() && reasoning.is_empty() && tool_calls.is_empty() {
        let reason = json
            .get("stop_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(if structured {
            ModelError::JsonSchemaViolation(format!(
                "Provider returned empty structured content (stop_reason: {})",
                reason
            ))
        } else {
            ModelError::InvalidResponse(format!(
                "Provider returned empty completion content (stop_reason: {})",
                reason
            ))
        });
    }

    let parsed_json = if structured && tool_calls.is_empty() {
        Some(
            serde_json::from_str(
                &extract_structured_json_text(&content)
                    .unwrap_or_else(|| content.trim().to_string()),
            )
            .map_err(|e| ModelError::JsonSchemaViolation(e.to_string()))?,
        )
    } else {
        None
    };

    let (prompt_tokens, completion_tokens) = parse_anthropic_usage(json.get("usage"));
    let prompt_tokens = prompt_tokens.unwrap_or(0);
    let completion_tokens = completion_tokens.unwrap_or(0);
    let finish_reason = json
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .map(map_stop_reason)
        .unwrap_or_else(|| "stop".to_string());

    Ok(ModelResponse {
        content,
        json: parsed_json,
        usage: ModelUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
        latency_ms,
        model: request.model.clone(),
        finish_reason,
        provider_kind: request.config.kind,
        reasoning_content: if reasoning.is_empty() {
            None
        } else {
            Some(reasoning)
        },
        tool_calls,
    })
}

/// Aggregation state for an Anthropic SSE stream.
///
/// Anthropic indexes content blocks (text + tool_use interleaved); the
/// crate's `ModelToolCall.index` is the tool-call ordinal, so we keep a map
/// from content-block index → tool ordinal.
#[derive(Debug)]
struct AnthropicStreamState {
    content: String,
    reasoning: String,
    tool_calls: BTreeMap<usize, ModelToolCall>,
    block_to_tool: BTreeMap<usize, usize>,
    prompt_tokens: u64,
    completion_tokens: u64,
    finish_reason: Option<String>,
}

impl AnthropicStreamState {
    fn new() -> Self {
        Self {
            content: String::new(),
            reasoning: String::new(),
            tool_calls: BTreeMap::new(),
            block_to_tool: BTreeMap::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            finish_reason: None,
        }
    }

    fn usage(&self) -> ModelUsage {
        ModelUsage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.prompt_tokens + self.completion_tokens,
        }
    }

    fn finish(
        self,
        request: &ModelRequest,
        latency_ms: u64,
        structured: bool,
    ) -> Result<ModelResponse, ModelError> {
        if self.content.is_empty() && self.reasoning.is_empty() && self.tool_calls.is_empty() {
            return Err(if structured {
                ModelError::JsonSchemaViolation(
                    "Provider returned empty structured stream".to_string(),
                )
            } else {
                ModelError::InvalidResponse("Provider returned empty stream".to_string())
            });
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
        let usage = self.usage();
        Ok(ModelResponse {
            content: self.content,
            json,
            usage,
            latency_ms,
            model: request.model.clone(),
            finish_reason: self.finish_reason.unwrap_or_else(|| "stop".to_string()),
            provider_kind: request.config.kind,
            reasoning_content: if self.reasoning.is_empty() {
                None
            } else {
                Some(self.reasoning)
            },
            tool_calls: self.tool_calls.into_values().collect(),
        })
    }
}

/// Apply one parsed Anthropic SSE data payload to the stream state, emitting
/// the corresponding `ModelStreamEvent`s. Returns `true` on `message_stop`.
async fn apply_anthropic_stream_event(
    json: &serde_json::Value,
    state: &mut AnthropicStreamState,
    on_event: &mut crate::gateway::ModelStreamHandler<'_>,
) -> Result<bool, ModelError> {
    match json.get("type").and_then(|v| v.as_str()) {
        Some("message_start") => {
            let (prompt, completion) =
                parse_anthropic_usage(json.get("message").and_then(|m| m.get("usage")));
            if let Some(prompt) = prompt {
                state.prompt_tokens = prompt;
            }
            if let Some(completion) = completion {
                state.completion_tokens = completion;
            }
        }
        Some("content_block_start") => {
            let block_index = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let block = json.get("content_block").cloned().unwrap_or_default();
            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                let ordinal = state.tool_calls.len();
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|id| id.to_string());
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                state.block_to_tool.insert(block_index, ordinal);
                state.tool_calls.insert(
                    ordinal,
                    ModelToolCall {
                        index: ordinal,
                        id: id.clone(),
                        name: name.clone(),
                        arguments: String::new(),
                    },
                );
                on_event(ModelStreamEvent::ToolCallDelta {
                    index: ordinal,
                    id,
                    name: Some(name),
                    arguments_delta: String::new(),
                })
                .await?;
            }
        }
        Some("content_block_delta") => {
            let block_index = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let Some(delta) = json.get("delta") else {
                return Ok(false);
            };
            match delta.get("type").and_then(|v| v.as_str()) {
                Some("text_delta") => {
                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                        state.content.push_str(text);
                        on_event(ModelStreamEvent::ContentDelta {
                            delta: text.to_string(),
                        })
                        .await?;
                    }
                }
                Some("thinking_delta") => {
                    if let Some(thinking) = delta.get("thinking").and_then(|v| v.as_str()) {
                        state.reasoning.push_str(thinking);
                        on_event(ModelStreamEvent::ReasoningDelta {
                            delta: thinking.to_string(),
                        })
                        .await?;
                    }
                }
                Some("input_json_delta") => {
                    let partial = delta
                        .get("partial_json")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if let Some(&ordinal) = state.block_to_tool.get(&block_index) {
                        let entry = state
                            .tool_calls
                            .get_mut(&ordinal)
                            .expect("tool ordinal must exist");
                        entry.arguments.push_str(partial);
                        let id = entry.id.clone();
                        on_event(ModelStreamEvent::ToolCallDelta {
                            index: ordinal,
                            id,
                            name: None,
                            arguments_delta: partial.to_string(),
                        })
                        .await?;
                    }
                }
                _ => {}
            }
        }
        Some("message_delta") => {
            if let Some(stop_reason) = json
                .get("delta")
                .and_then(|delta| delta.get("stop_reason"))
                .and_then(|v| v.as_str())
            {
                state.finish_reason = Some(map_stop_reason(stop_reason));
            }
            let (prompt, completion) = parse_anthropic_usage(json.get("usage"));
            if let Some(prompt) = prompt {
                state.prompt_tokens = prompt;
            }
            if let Some(completion) = completion {
                state.completion_tokens = completion;
            }
        }
        Some("message_stop") => return Ok(true),
        Some("error") => {
            let message = json
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Anthropic stream error");
            return Err(ModelError::InvalidResponse(message.to_string()));
        }
        // "ping", "content_block_stop" and unknown future event types are
        // intentionally ignored.
        _ => {}
    }
    Ok(false)
}

async fn stream_messages(
    request: &ModelRequest,
    schema_json: Option<&serde_json::Value>,
    on_event: &mut crate::gateway::ModelStreamHandler<'_>,
) -> Result<ModelResponse, ModelError> {
    if request.config.api_key.is_empty() {
        return Err(ModelError::MissingApiKey);
    }
    let url = format!("{}/v1/messages", anthropic_base(&request.config));
    let body = build_messages_body(request, schema_json, true);
    // Shared client without a total request timeout: SSE streams are
    // long-lived. Connection establishment is bounded by connect_timeout.
    let client = crate::http::streaming_client_for(&url);
    let start = std::time::Instant::now();
    let mut resp = client
        .post(&url)
        .header("x-api-key", &request.config.api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|e| ModelError::ProviderUnreachable(e.to_string()))?;
    let latency = start.elapsed().as_millis() as u64;
    let status = resp.status();
    if !status.is_success() {
        let text = resp
            .text()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        return Err(provider_http_error(status, &text));
    }

    let mut state = AnthropicStreamState::new();
    let mut buffer = String::new();
    let mut saw_stop = false;
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
                let json: serde_json::Value = serde_json::from_str(&data)
                    .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
                if apply_anthropic_stream_event(&json, &mut state, on_event).await? {
                    saw_stop = true;
                    break;
                }
            }
        }
        if saw_stop {
            break;
        }
    }

    let structured =
        schema_json.is_some() || matches!(request.response_format, ResponseFormat::Json);
    let response = state.finish(request, latency, structured)?;
    if response.usage.total_tokens > 0 {
        on_event(ModelStreamEvent::Usage(response.usage.clone())).await?;
    }
    on_event(ModelStreamEvent::Done {
        finish_reason: Some(response.finish_reason.clone()),
    })
    .await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ModelProviderConfig {
        ModelProviderConfig {
            provider_id: "anthropic".into(),
            kind: ModelProviderKind::Anthropic,
            base_url: String::new(),
            api_key: "sk-ant-test".into(),
            default_model: "claude-sonnet-4-6".into(),
            fast_model: "claude-haiku-4-5".into(),
            reasoning_model: "claude-opus-4-6".into(),
            structured_output_model: "claude-sonnet-4-6".into(),
            max_tokens: 4096,
            temperature: 0.7,
            timeout_ms: 30_000,
            max_cost_per_task_usd: 5.0,
        }
    }

    fn test_request(format: ResponseFormat) -> ModelRequest {
        ModelRequest {
            config: test_config(),
            role: ModelRole::AgentReasoning,
            model: "claude-sonnet-4-6".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "Hello".into(),
                ..Default::default()
            }],
            temperature: 0.2,
            max_tokens: 256,
            response_format: format,
            stream: true,
            tools: vec![],
            tool_choice: None,
        }
    }

    async fn run_fixture(
        fixture: &[&str],
        events: &mut Vec<ModelStreamEvent>,
    ) -> Result<(AnthropicStreamState, bool), ModelError> {
        let mut state = AnthropicStreamState::new();
        let mut stopped = false;
        let mut sink = |event: ModelStreamEvent| {
            events.push(event);
            Box::pin(async { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>
        };
        for data in fixture {
            let json: serde_json::Value = serde_json::from_str(data).expect("fixture JSON");
            if apply_anthropic_stream_event(&json, &mut state, &mut sink).await? {
                stopped = true;
                break;
            }
        }
        Ok((state, stopped))
    }

    /// Canned Anthropic SSE fixture: text + tool_use + usage, no network.
    const SSE_FIXTURE: &[&str] = &[
        r#"{"type":"message_start","message":{"id":"msg_01","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-6","usage":{"input_tokens":25,"output_tokens":1}}}"#,
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello "}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"world"}}"#,
        r#"{"type":"content_block_stop","index":0}"#,
        r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_01","name":"file-readonly","input":{}}}"#,
        r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#,
        r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"README.md\"}"}}"#,
        r#"{"type":"content_block_stop","index":1}"#,
        r#"{"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":17}}"#,
        r#"{"type":"message_stop"}"#,
    ];

    #[tokio::test]
    async fn sse_fixture_aggregates_text_tool_calls_and_usage() {
        let request = test_request(ResponseFormat::Text);
        let mut events = Vec::new();
        let (state, stopped) = run_fixture(SSE_FIXTURE, &mut events)
            .await
            .expect("fixture should apply");
        assert!(stopped, "message_stop should end the stream");

        let response = state
            .finish(&request, 5, false)
            .expect("aggregate should finish");
        assert_eq!(response.content, "Hello world");
        assert_eq!(response.finish_reason, "tool_calls");
        assert_eq!(response.usage.prompt_tokens, 25);
        assert_eq!(response.usage.completion_tokens, 17);
        assert_eq!(response.usage.total_tokens, 42);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].index, 0);
        assert_eq!(response.tool_calls[0].id.as_deref(), Some("toolu_01"));
        assert_eq!(response.tool_calls[0].name, "file-readonly");
        assert_eq!(response.tool_calls[0].arguments, "{\"path\":\"README.md\"}");

        // Event mapping: text_delta → ContentDelta, tool blocks → ToolCallDelta.
        let content: String = events
            .iter()
            .filter_map(|event| match event {
                ModelStreamEvent::ContentDelta { delta } => Some(delta.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(content, "Hello world");
        let tool_deltas: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ModelStreamEvent::ToolCallDelta {
                    index,
                    name,
                    arguments_delta,
                    ..
                } => Some((*index, name.clone(), arguments_delta.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(tool_deltas.len(), 3);
        assert_eq!(
            tool_deltas[0],
            (0, Some("file-readonly".to_string()), String::new())
        );
        assert_eq!(tool_deltas[1], (0, None, "{\"path\":".to_string()));
        assert_eq!(tool_deltas[2], (0, None, "\"README.md\"}".to_string()));
    }

    #[tokio::test]
    async fn sse_fixture_thinking_delta_maps_to_reasoning() {
        let fixture = [
            r#"{"type":"message_start","message":{"usage":{"input_tokens":9,"output_tokens":1}}}"#,
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me check."}}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Done."}}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}}"#,
            r#"{"type":"message_stop"}"#,
        ];
        let request = test_request(ResponseFormat::Text);
        let mut events = Vec::new();
        let (state, stopped) = run_fixture(&fixture, &mut events)
            .await
            .expect("fixture should apply");
        assert!(stopped);

        let response = state.finish(&request, 1, false).expect("finish");
        assert_eq!(response.content, "Done.");
        assert_eq!(response.reasoning_content.as_deref(), Some("Let me check."));
        assert_eq!(response.finish_reason, "stop");
        assert!(events
            .iter()
            .any(|event| matches!(event, ModelStreamEvent::ReasoningDelta { delta } if delta == "Let me check.")));
    }

    #[tokio::test]
    async fn sse_fixture_structured_stream_parses_json_content() {
        let fixture = [
            r#"{"type":"message_start","message":{"usage":{"input_tokens":12,"output_tokens":1}}}"#,
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"{\"proposal\":{\"kind\":\"finish\""}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":",\"summary\":\"done\"}}"}}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":11}}"#,
            r#"{"type":"message_stop"}"#,
        ];
        let request = test_request(ResponseFormat::Json);
        let mut events = Vec::new();
        let (state, _) = run_fixture(&fixture, &mut events)
            .await
            .expect("fixture should apply");
        let response = state.finish(&request, 1, true).expect("finish");
        assert_eq!(
            response.json,
            Some(serde_json::json!({"proposal":{"kind":"finish","summary":"done"}}))
        );
    }

    #[tokio::test]
    async fn sse_error_event_surfaces_as_invalid_response() {
        let fixture =
            [r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#];
        let mut events = Vec::new();
        let err = run_fixture(&fixture, &mut events)
            .await
            .expect_err("error event should fail the stream");
        assert!(err.to_string().contains("Overloaded"), "got: {err}");
    }

    #[tokio::test]
    async fn empty_stream_is_rejected() {
        let request = test_request(ResponseFormat::Text);
        let state = AnthropicStreamState::new();
        let err = state
            .finish(&request, 1, false)
            .expect_err("empty stream should error");
        assert!(err.to_string().contains("empty stream"));
    }

    #[test]
    fn build_body_lifts_system_messages_and_maps_tool_history() {
        let mut request = test_request(ResponseFormat::Text);
        request.messages = vec![
            ModelMessage {
                role: "system".into(),
                content: "You are a governed agent.".into(),
                ..Default::default()
            },
            ModelMessage {
                role: "user".into(),
                content: "Read welcome.md".into(),
                ..Default::default()
            },
            ModelMessage {
                role: "assistant".into(),
                content: String::new(),
                tool_calls: vec![ModelToolCall {
                    index: 0,
                    id: Some("toolu_01".into()),
                    name: "file-readonly".into(),
                    arguments: "{\"path\":\"welcome.md\"}".into(),
                }],
                ..Default::default()
            },
            ModelMessage {
                role: "tool".into(),
                content: "{\"content\":\"hello\"}".into(),
                tool_call_id: Some("toolu_01".into()),
                ..Default::default()
            },
        ];
        request.tools = vec![ModelToolDefinition {
            name: "file-readonly".into(),
            description: Some("Read files".into()),
            parameters_json: serde_json::json!({"type":"object"}),
        }];
        request.tool_choice = Some(serde_json::Value::String("auto".into()));

        let body = build_messages_body(&request, None, false);
        assert_eq!(
            body["system"],
            serde_json::json!("You are a governed agent.")
        );
        assert_eq!(body["stream"], serde_json::json!(false));
        assert_eq!(body["tool_choice"], serde_json::json!({"type":"auto"}));
        assert_eq!(body["tools"][0]["name"], serde_json::json!("file-readonly"));
        assert_eq!(
            body["tools"][0]["input_schema"],
            serde_json::json!({"type":"object"})
        );

        let messages = body["messages"].as_array().expect("messages array");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][0]["type"], "tool_use");
        assert_eq!(messages[1]["content"][0]["id"], "toolu_01");
        assert_eq!(
            messages[1]["content"][0]["input"],
            serde_json::json!({"path":"welcome.md"})
        );
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
        assert_eq!(messages[2]["content"][0]["tool_use_id"], "toolu_01");
    }

    #[test]
    fn build_body_appends_json_instruction_for_structured_requests() {
        let request = test_request(ResponseFormat::Json);
        let schema = serde_json::json!({"type":"object","required":["thought"]});
        let body = build_messages_body(&request, Some(&schema), true);
        let system = body["system"].as_str().expect("system instruction");
        assert!(system.contains("single valid JSON object"));
        assert!(system.contains("\"required\":[\"thought\"]"));
        assert_eq!(body["stream"], serde_json::json!(true));
        // Temperature must be clamped into Anthropic's [0,1] range.
        assert!(body["temperature"].as_f64().unwrap() <= 1.0);
    }

    #[test]
    fn tool_choice_mapping_covers_openai_and_anthropic_shapes() {
        assert_eq!(
            map_tool_choice(Some(&serde_json::json!("required"))),
            Some(serde_json::json!({"type":"any"}))
        );
        assert_eq!(
            map_tool_choice(Some(
                &serde_json::json!({"type":"function","function":{"name":"f"}})
            )),
            Some(serde_json::json!({"type":"tool","name":"f"}))
        );
        assert_eq!(
            map_tool_choice(Some(&serde_json::json!({"type":"any"}))),
            Some(serde_json::json!({"type":"any"}))
        );
        assert_eq!(map_tool_choice(None), None);
    }

    #[test]
    fn base_url_normalization_handles_default_and_v1_suffix() {
        let mut config = test_config();
        assert_eq!(anthropic_base(&config), "https://api.anthropic.com");
        config.base_url = "https://proxy.example.com/".into();
        assert_eq!(anthropic_base(&config), "https://proxy.example.com");
        config.base_url = "https://proxy.example.com/v1".into();
        assert_eq!(anthropic_base(&config), "https://proxy.example.com");
    }

    #[test]
    fn non_stream_response_parses_text_tool_use_and_usage() {
        let json = serde_json::json!({
            "id": "msg_01",
            "content": [
                {"type": "text", "text": "Reading the file now."},
                {"type": "tool_use", "id": "toolu_02", "name": "file-readonly",
                 "input": {"path": "README.md"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 30, "output_tokens": 12}
        });
        let request = test_request(ResponseFormat::Text);
        let response =
            parse_messages_response(&json, &request, 7, false).expect("response should parse");
        assert_eq!(response.content, "Reading the file now.");
        assert_eq!(response.finish_reason, "tool_calls");
        assert_eq!(response.usage.total_tokens, 42);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "file-readonly");
        assert_eq!(
            response.tool_calls[0].arguments,
            serde_json::json!({"path":"README.md"}).to_string()
        );
        assert_eq!(response.provider_kind, ModelProviderKind::Anthropic);
    }

    #[test]
    fn non_stream_structured_response_parses_fenced_json() {
        let json = serde_json::json!({
            "content": [{"type": "text", "text": "```json\n{\"ok\":true}\n```"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 3, "output_tokens": 9}
        });
        let request = test_request(ResponseFormat::Json);
        let response = parse_messages_response(&json, &request, 1, true).expect("structured parse");
        assert_eq!(response.json, Some(serde_json::json!({"ok":true})));
        assert_eq!(response.finish_reason, "stop");
    }

    #[test]
    fn non_stream_empty_response_is_rejected() {
        let json = serde_json::json!({
            "content": [],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 3, "output_tokens": 0}
        });
        let request = test_request(ResponseFormat::Text);
        let err = parse_messages_response(&json, &request, 1, false)
            .expect_err("empty content should error");
        assert!(err.to_string().contains("empty completion content"));
    }
}
