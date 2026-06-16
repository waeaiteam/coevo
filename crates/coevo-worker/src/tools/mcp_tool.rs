//! Worker-side handler that executes a Model Context Protocol tool through the
//! real [`coevo_adapters`] MCP client. Tools are advertised from the cached
//! discovery in the `mcp_servers` table; the connection to the server is made
//! lazily, only when a tool is actually invoked.

use async_trait::async_trait;
use coevo_adapters::{McpClientManager, McpServerConfig, DEFAULT_TOOL_TIMEOUT};

use crate::error::WorkerError;
use crate::tools::github_readonly::ToolHandler;

/// Handler for one MCP tool on one configured server.
pub struct McpToolHandler {
    config: McpServerConfig,
    tool_name: String,
}

impl McpToolHandler {
    pub fn new(config: McpServerConfig, tool_name: impl Into<String>) -> Self {
        Self {
            config,
            tool_name: tool_name.into(),
        }
    }

    async fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let manager = McpClientManager::new();
        manager.connect(self.config.clone()).await.map_err(|e| {
            WorkerError::ToolUnavailable(format!(
                "MCP server '{}' connect failed: {e}",
                self.config.name
            ))
        })?;
        let output = manager
            .call_tool(
                &self.config.id,
                &self.tool_name,
                input,
                DEFAULT_TOOL_TIMEOUT,
            )
            .await
            .map_err(|e| {
                WorkerError::Internal(format!(
                    "MCP tool '{}' on '{}' failed: {e}",
                    self.tool_name, self.config.name
                ))
            })?;

        let text = output
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(serde_json::json!({
            "is_error": output.is_error,
            "text": text,
            "structured": output.structured,
        }))
    }
}

#[async_trait]
impl ToolHandler for McpToolHandler {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> {
        Ok(serde_json::json!({ "server": self.config.name, "tool": self.tool_name }))
    }

    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        // Advertise the call shape without contacting the server.
        Ok(serde_json::json!({
            "dry_run": true,
            "server": self.config.name,
            "tool": self.tool_name,
            "input": input,
        }))
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        self.call(input).await
    }
}
