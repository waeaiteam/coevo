//! MCP adapter backed by an in-memory tool registry.

use crate::traits::*;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

pub struct MockMcpAdapter {
    registered_tools: Vec<String>,
}

impl MockMcpAdapter {
    pub fn new() -> Self {
        Self {
            registered_tools: vec![
                "urn:mcp:tool:file-read".to_string(),
                "urn:mcp:tool:file-write".to_string(),
                "urn:mcp:tool:db-query".to_string(),
                "urn:mcp:tool:http-request".to_string(),
                "urn:mcp:tool:unit-test-runner".to_string(),
                "urn:mcp:tool:deploy-staging".to_string(),
                "urn:mcp:tool:deploy-production".to_string(),
            ],
        }
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.registered_tools = tools;
        self
    }
}

impl Default for MockMcpAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl McpProvider for MockMcpAdapter {
    async fn call_tool(&self, call: McpToolCall) -> Result<McpToolResult, AdapterError> {
        if !self.registered_tools.contains(&call.tool_urn) {
            return Err(AdapterError::McpError(format!(
                "tool '{}' not registered",
                call.tool_urn
            )));
        }

        let mut hasher = Sha256::new();
        hasher.update(call.tool_urn.as_bytes());
        hasher.update(serde_json::to_string(&call.parameters).unwrap().as_bytes());
        let sig = hex::encode(hasher.finalize());
        let parameter_count = call
            .parameters
            .as_object()
            .map(|map| map.len())
            .unwrap_or(0);

        let result = match call.tool_urn.as_str() {
            "urn:mcp:tool:unit-test-runner" => serde_json::json!({
                "tool": call.tool_urn,
                "suite": call.parameters.get("suite").cloned().unwrap_or(serde_json::json!("default")),
                "passed": true,
                "failures": 0,
                "verification": {
                    "signature": sig,
                    "parameters": parameter_count,
                }
            }),
            "urn:mcp:tool:deploy-production" => serde_json::json!({
                "tool": call.tool_urn,
                "status": "requires_approval",
                "environment": "production",
                "verification": {
                    "signature": sig,
                    "parameters": parameter_count,
                }
            }),
            "urn:mcp:tool:db-query" => serde_json::json!({
                "tool": call.tool_urn,
                "rows": call.parameters.get("expected_rows").and_then(|v| v.as_u64()).unwrap_or(10),
                "query_time_ms": 45,
                "verification": {
                    "signature": sig,
                    "parameters": parameter_count,
                }
            }),
            _ => serde_json::json!({
                "tool": call.tool_urn,
                "status": "ok",
                "echo": call.parameters,
                "verification": {
                    "signature": sig,
                    "parameters": parameter_count,
                }
            }),
        };

        Ok(McpToolResult {
            tool_urn: call.tool_urn,
            result,
            success: true,
            verification_signature: Some(sig),
        })
    }

    async fn list_tools(&self) -> Result<Vec<String>, AdapterError> {
        Ok(self.registered_tools.clone())
    }

    async fn verify_result(&self, result: &McpToolResult) -> Result<bool, AdapterError> {
        let embedded_signature = result
            .result
            .get("verification")
            .and_then(|v| v.get("signature"))
            .and_then(|v| v.as_str());
        Ok(result.success && result.verification_signature.as_deref() == embedded_signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_tools_reflects_configured_registry() {
        let adapter = MockMcpAdapter::new().with_tools(vec![
            "urn:mcp:tool:alpha".into(),
            "urn:mcp:tool:beta".into(),
        ]);

        assert_eq!(
            adapter.list_tools().await.unwrap(),
            vec!["urn:mcp:tool:alpha", "urn:mcp:tool:beta"]
        );
    }

    #[tokio::test]
    async fn call_tool_embeds_request_metadata() {
        let adapter = MockMcpAdapter::new();
        let result = adapter
            .call_tool(McpToolCall {
                tool_urn: "urn:mcp:tool:db-query".into(),
                parameters: serde_json::json!({"expected_rows": 23}),
                traceparent: "00-abc-def-01".into(),
            })
            .await
            .unwrap();

        assert_eq!(result.result["rows"], 23);
        assert_eq!(result.result["verification"]["parameters"], 1);
        assert!(result.verification_signature.is_some());
        assert!(adapter.verify_result(&result).await.unwrap());
    }
}
