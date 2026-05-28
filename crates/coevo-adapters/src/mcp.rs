//! Mock MCP adapter — simulates tool calls and verification.

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

        // Generate a deterministic verification signature
        let mut hasher = Sha256::new();
        hasher.update(call.tool_urn.as_bytes());
        hasher.update(serde_json::to_string(&call.parameters).unwrap().as_bytes());
        let sig = hex::encode(hasher.finalize());

        // Simulate tool execution based on URN
        let result = match call.tool_urn.as_str() {
            "urn:mcp:tool:unit-test-runner" => serde_json::json!({
                "passed": true,
                "total": 42,
                "failures": 0,
                "report": "all tests passing"
            }),
            "urn:mcp:tool:deploy-production" => serde_json::json!({
                "status": "requires_approval",
                "environment": "production"
            }),
            "urn:mcp:tool:db-query" => serde_json::json!({
                "rows": 10,
                "query_time_ms": 45
            }),
            _ => serde_json::json!({
                "status": "ok",
                "output": "mock tool execution succeeded"
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
        // Mock verification: any result with a verification_signature is valid
        Ok(result.verification_signature.is_some() && result.success)
    }
}
