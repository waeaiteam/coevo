//! Worker-side handler that executes a Model Context Protocol tool through the
//! real [`coevo_adapters`] MCP client. Tools are advertised from the cached
//! discovery in the `mcp_servers` table; the connection to the server is made
//! lazily, only when a tool is actually invoked.

use async_trait::async_trait;
#[cfg(test)]
use coevo_adapters::McpTransportConfig;
use coevo_adapters::{shared_mcp_client_manager, McpServerConfig, DEFAULT_TOOL_TIMEOUT};

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
        let manager = shared_mcp_client_manager();
        manager
            .ensure_connected(self.config.clone())
            .await
            .map_err(|e| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    fn write_stdio_test_server_script() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "coevo-mcp-test-server-{}.ps1",
            uuid::Uuid::new_v4()
        ));
        let script = r#"$ErrorActionPreference = 'Stop'
$stdin = [Console]::In
$stdout = [Console]::Out
while (($line = $stdin.ReadLine()) -ne $null) {
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $msg = $line | ConvertFrom-Json
    if ($null -eq $msg.id) {
        continue
    }
    switch ($msg.method) {
        'initialize' {
            $resp = @{
                jsonrpc = '2.0'
                id = $msg.id
                result = @{
                    protocolVersion = $msg.params.protocolVersion
                    capabilities = @{
                        tools = @{
                            listChanged = $false
                        }
                    }
                    serverInfo = @{
                        name = 'worker-mcp-test'
                        version = '1.0.0'
                    }
                }
            }
        }
        'tools/call' {
            $resp = @{
                jsonrpc = '2.0'
                id = $msg.id
                result = @{
                    content = @(
                        @{
                            type = 'text'
                            text = ($msg.params.arguments | ConvertTo-Json -Compress -Depth 32)
                        }
                    )
                    isError = $false
                    structuredContent = @{
                        echoed = $msg.params.arguments
                    }
                }
            }
        }
        default {
            $resp = @{
                jsonrpc = '2.0'
                id = $msg.id
                error = @{
                    code = -32601
                    message = 'method not found'
                }
            }
        }
    }
    $json = $resp | ConvertTo-Json -Compress -Depth 32
    $stdout.WriteLine($json)
    $stdout.Flush()
}
"#;
        fs::write(&path, script).expect("write test MCP server");
        path
    }

    #[tokio::test]
    async fn mcp_calls_respect_shared_manager_disconnect_state() {
        if !cfg!(windows) {
            return;
        }
        let manager = shared_mcp_client_manager();
        let server_id = format!("worker-mcp-test-{}", uuid::Uuid::new_v4());
        let script = write_stdio_test_server_script();
        let config = McpServerConfig {
            id: server_id.clone(),
            name: server_id.clone(),
            transport: McpTransportConfig::Stdio {
                command: "powershell".to_string(),
                args: vec![
                    "-NoLogo".to_string(),
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    "-File".to_string(),
                    script.to_string_lossy().to_string(),
                ],
                env: HashMap::new(),
            },
        };
        manager
            .connect(config.clone())
            .await
            .expect("shared manager should connect to the test MCP server");
        let handler = McpToolHandler::new(config, "echo");

        let first = handler
            .execute(serde_json::json!({"x": 1}))
            .await
            .expect("initial call should succeed");
        assert_eq!(first["is_error"], serde_json::json!(false));

        manager
            .disconnect(&server_id)
            .await
            .expect("shared manager should hold the connected server");

        let second = tokio::time::timeout(
            Duration::from_secs(10),
            handler.execute(serde_json::json!({"x": 2})),
        )
        .await
        .expect("second call timed out");
        assert!(
            second.is_err(),
            "worker call should fail once shared manager state is cleared"
        );

        let _ = fs::remove_file(&script);
        let _ = manager.disconnect(&server_id).await;
    }

    #[tokio::test]
    async fn mcp_calls_lazy_connect_into_shared_manager_when_unseeded() {
        if !cfg!(windows) {
            return;
        }
        let manager = shared_mcp_client_manager();
        let server_id = format!("worker-mcp-lazy-{}", uuid::Uuid::new_v4());
        let script = write_stdio_test_server_script();
        let config = McpServerConfig {
            id: server_id.clone(),
            name: server_id.clone(),
            transport: McpTransportConfig::Stdio {
                command: "powershell".to_string(),
                args: vec![
                    "-NoLogo".to_string(),
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    "-File".to_string(),
                    script.to_string_lossy().to_string(),
                ],
                env: HashMap::new(),
            },
        };
        let handler = McpToolHandler::new(config, "echo");

        let result = handler
            .execute(serde_json::json!({"x": 7}))
            .await
            .expect("handler should lazily connect an unseeded shared-manager server");

        assert_eq!(result["is_error"], serde_json::json!(false));
        assert!(manager.connected_ids().await.contains(&server_id));

        let _ = fs::remove_file(&script);
        let _ = manager.disconnect(&server_id).await;
    }
}
