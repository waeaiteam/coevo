//! The MCP client proper: lifecycle (`initialize` / `notifications/initialized`)
//! and the tools API (`tools/list`, `tools/call`) over any supported transport.

use super::http::{HttpOutcome, HttpTransport};
use super::stream::{NotificationState, StdioTransport, StreamPeer};
use super::types::{McpServerInfo, McpToolInfo, McpToolOutput};
use crate::traits::AdapterError;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, warn};

pub(crate) const PROTOCOL_VERSION_LATEST: &str = "2025-03-26";
pub(crate) const PROTOCOL_VERSION_FALLBACK: &str = "2024-11-05";

enum ClientTransport {
    /// Newline-delimited JSON over an arbitrary byte stream (tests / embedding).
    Stream(StreamPeer),
    /// Child process over stdio.
    Stdio(StdioTransport),
    /// Streamable HTTP.
    Http(HttpTransport),
}

#[derive(Debug, Clone)]
struct NegotiatedState {
    protocol_version: String,
    server_name: String,
    server_version: String,
    capabilities: Value,
}

/// A single connected MCP server. Created via [`McpClientManager`] in
/// production; `over_stream` exists for deterministic in-memory testing.
pub struct McpClient {
    id: String,
    name: String,
    transport: ClientTransport,
    notifications: Arc<NotificationState>,
    negotiated: StdMutex<Option<NegotiatedState>>,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl McpClient {
    /// Spawn a stdio MCP server (direct exec, no shell) and wrap it.
    /// Does NOT run the `initialize` handshake; call [`McpClient::initialize`].
    pub async fn connect_stdio(
        id: impl Into<String>,
        name: impl Into<String>,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, AdapterError> {
        let id = id.into();
        let notifications = Arc::new(NotificationState::default());
        let transport =
            StdioTransport::spawn(command, args, env, Arc::clone(&notifications), id.clone())
                .await?;
        Ok(Self {
            id,
            name: name.into(),
            transport: ClientTransport::Stdio(transport),
            notifications,
            negotiated: StdMutex::new(None),
        })
    }

    /// Wrap a Streamable HTTP endpoint.
    /// Does NOT run the `initialize` handshake; call [`McpClient::initialize`].
    pub fn connect_http(
        id: impl Into<String>,
        name: impl Into<String>,
        url: impl Into<String>,
        headers: HashMap<String, String>,
    ) -> Result<Self, AdapterError> {
        let id = id.into();
        let notifications = Arc::new(NotificationState::default());
        let transport =
            HttpTransport::new(url.into(), headers, Arc::clone(&notifications), id.clone())?;
        Ok(Self {
            id,
            name: name.into(),
            transport: ClientTransport::Http(transport),
            notifications,
            negotiated: StdMutex::new(None),
        })
    }

    /// Wrap an arbitrary byte stream speaking newline-delimited JSON-RPC.
    /// Used by tests (over `tokio::io::duplex`) and useful for embedding.
    pub fn over_stream<R, W>(
        id: impl Into<String>,
        name: impl Into<String>,
        read: R,
        write: W,
    ) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let id = id.into();
        let notifications = Arc::new(NotificationState::default());
        let peer = StreamPeer::new(read, write, Arc::clone(&notifications), id.clone());
        Self {
            id,
            name: name.into(),
            transport: ClientTransport::Stream(peer),
            notifications,
            negotiated: StdMutex::new(None),
        }
    }

    // ---- lifecycle ----

    /// Run the MCP `initialize` handshake (with protocol-version fallback) and
    /// send `notifications/initialized`. Idempotent: re-running re-negotiates.
    pub async fn initialize(&self) -> Result<McpServerInfo, AdapterError> {
        let result = match self.try_initialize(PROTOCOL_VERSION_LATEST).await {
            Ok(value) => value,
            Err(AdapterError::McpRpc {
                code,
                message,
                data,
            }) if is_version_mismatch(code, &message) => {
                debug!(
                    server = %self.id,
                    code,
                    %message,
                    ?data,
                    "protocol version rejected; retrying with fallback"
                );
                self.try_initialize(PROTOCOL_VERSION_FALLBACK).await?
            }
            Err(e) => return Err(e),
        };

        let protocol_version = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(PROTOCOL_VERSION_FALLBACK)
            .to_string();
        let server_name = result
            .pointer("/serverInfo/name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let server_version = result
            .pointer("/serverInfo/version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let capabilities = result.get("capabilities").cloned().unwrap_or(json!({}));

        *self.negotiated.lock().expect("negotiated lock") = Some(NegotiatedState {
            protocol_version,
            server_name,
            server_version,
            capabilities,
        });

        self.notify("notifications/initialized", None).await?;
        Ok(self.server_info().expect("just negotiated"))
    }

    async fn try_initialize(&self, protocol_version: &str) -> Result<Value, AdapterError> {
        let params = json!({
            "protocolVersion": protocol_version,
            // Tools-only client: we offer no sampling/roots capabilities.
            "capabilities": {},
            "clientInfo": {
                "name": "coevo",
                "version": env!("CARGO_PKG_VERSION"),
            },
        });
        // No re-init-on-session-expiry here: initialize IS the re-init path.
        match self.rpc_raw("initialize", Some(params)).await? {
            HttpOutcome::Response(result) => result,
            HttpOutcome::SessionExpired => Err(AdapterError::McpError(
                "MCP session rejected during initialize".to_string(),
            )),
        }
    }

    /// Negotiated server identity/capabilities, once `initialize` succeeded.
    pub fn server_info(&self) -> Option<McpServerInfo> {
        self.negotiated
            .lock()
            .expect("negotiated lock")
            .as_ref()
            .map(|s| McpServerInfo {
                id: self.id.clone(),
                name: self.name.clone(),
                protocol_version: s.protocol_version.clone(),
                server_name: s.server_name.clone(),
                server_version: s.server_version.clone(),
                capabilities: s.capabilities.clone(),
            })
    }

    /// True if the server announced `notifications/tools/list_changed` since
    /// the last `list_tools()` call.
    pub fn tools_changed(&self) -> bool {
        self.notifications.tools_dirty.load(Ordering::SeqCst)
    }

    /// Liveness probe: stdio child still running / stream still open.
    /// HTTP is stateless and always reports alive.
    pub async fn is_alive(&self) -> bool {
        match &self.transport {
            ClientTransport::Stream(peer) => peer.is_alive(),
            ClientTransport::Stdio(t) => t.is_alive().await,
            ClientTransport::Http(_) => true,
        }
    }

    /// Graceful shutdown (close stdin, wait, kill for stdio; no-op for HTTP).
    pub async fn shutdown(&self) {
        match &self.transport {
            ClientTransport::Stream(peer) => peer.close().await,
            ClientTransport::Stdio(t) => t.shutdown().await,
            ClientTransport::Http(_) => {}
        }
    }

    // ---- tools ----

    /// `tools/list`, following `nextCursor` pagination to exhaustion.
    pub async fn list_tools(&self) -> Result<Vec<McpToolInfo>, AdapterError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor.as_ref().map(|c| json!({ "cursor": c }));
            let result = self.rpc("tools/list", params).await?;
            let page = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    AdapterError::McpError("tools/list result missing 'tools' array".to_string())
                })?;
            for tool in page {
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AdapterError::McpError("tool entry missing 'name'".to_string()))?
                    .to_string();
                tools.push(McpToolInfo {
                    name,
                    description: tool
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    input_schema: tool.get("inputSchema").cloned().unwrap_or(json!({})),
                });
            }
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        self.notifications
            .tools_dirty
            .store(false, Ordering::SeqCst);
        Ok(tools)
    }

    /// `tools/call` with a hard timeout. A timeout maps to `AdapterError::Timeout`.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<McpToolOutput, AdapterError> {
        let params = json!({ "name": name, "arguments": arguments });
        let result = tokio::time::timeout(timeout, self.rpc("tools/call", Some(params)))
            .await
            .map_err(|_| AdapterError::Timeout)??;
        Ok(McpToolOutput::from_result(result))
    }

    // ---- plumbing ----

    /// Send a request; on HTTP, transparently re-initialize once if the
    /// server reports the session expired (404), then retry.
    async fn rpc(&self, method: &str, params: Option<Value>) -> Result<Value, AdapterError> {
        match self.rpc_raw(method, params.clone()).await? {
            HttpOutcome::Response(result) => result,
            HttpOutcome::SessionExpired => {
                warn!(server = %self.id, "MCP session expired; re-initializing once");
                self.initialize().await?;
                match self.rpc_raw(method, params).await? {
                    HttpOutcome::Response(result) => result,
                    HttpOutcome::SessionExpired => Err(AdapterError::McpError(
                        "MCP session expired again immediately after re-initialize".to_string(),
                    )),
                }
            }
        }
    }

    async fn rpc_raw(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<HttpOutcome, AdapterError> {
        match &self.transport {
            ClientTransport::Stream(peer) => {
                Ok(HttpOutcome::Response(peer.request(method, params).await))
            }
            ClientTransport::Stdio(t) => {
                Ok(HttpOutcome::Response(t.peer.request(method, params).await))
            }
            ClientTransport::Http(h) => h.request(method, params).await,
        }
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), AdapterError> {
        match &self.transport {
            ClientTransport::Stream(peer) => peer.notify(method, params).await,
            ClientTransport::Stdio(t) => t.peer.notify(method, params).await,
            ClientTransport::Http(h) => h.notify(method, params).await,
        }
    }
}

fn is_version_mismatch(code: i64, message: &str) -> bool {
    let msg = message.to_ascii_lowercase();
    code == -32602
        || (msg.contains("version") && (msg.contains("protocol") || msg.contains("unsupported")))
}
