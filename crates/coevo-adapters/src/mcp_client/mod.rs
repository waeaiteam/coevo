//! Real Model Context Protocol (MCP) client.
//!
//! This module is the production replacement for `MockMcpAdapter`. It speaks
//! JSON-RPC 2.0 per the MCP spec (protocol version `2024-11-05`, negotiating up
//! to `2025-03-26` when the server supports it) over two transports:
//!
//! * **stdio** — spawn a child process and exchange newline-delimited JSON-RPC
//!   on its stdin/stdout (see [`stream`]). One JSON message per line.
//! * **streamable HTTP** — POST each JSON-RPC request to a base URL; the
//!   response is a JSON body or an SSE stream, and an `Mcp-Session-Id` header is
//!   carried across requests (see [`http`]).
//!
//! ## Layers
//!
//! * [`jsonrpc`] — JSON-RPC 2.0 message types + framing.
//! * [`types`] — public data types ([`McpToolInfo`], [`McpContent`],
//!   [`McpToolOutput`], [`McpServerInfo`]).
//! * [`config`] — [`McpServerConfig`] and its mapping to/from the `mcp_servers`
//!   table row ([`McpServerRow`]).
//! * [`client::McpClient`] — one connected server: `initialize` handshake,
//!   `tools/list` (paginated), `tools/call` (with timeout), shutdown.
//! * [`McpClientManager`] — a registry of [`client::McpClient`]s keyed by
//!   `McpServerConfig::id`.
//! * [`RealMcpClient`] — the [`McpProvider`] implementation the server/worker
//!   use: it wraps a [`McpClientManager`] and routes a tool **URN** to a server.
//!
//! ## URN / tool-id scheme
//!
//! Tools are addressed by a fully-qualified URN:
//!
//! ```text
//! urn:mcp:{server_name}:{tool_name}
//! ```
//!
//! `server_name` is [`McpServerConfig::name`] (validated free of `:` and
//! whitespace); `tool_name` is the bare name the server reports and may itself
//! contain `:` — the URN is split only on the first `:` after `urn:mcp:`, so the
//! remainder is the tool name verbatim. [`RealMcpClient::list_tools`] returns
//! these URNs so a later [`RealMcpClient::call_tool`] routes back to the right
//! server.
//!
//! ## Integrity hash (NOT a signature)
//!
//! [`McpToolResult::verification_signature`](crate::traits::McpToolResult)
//! carries a `sha256:`-prefixed digest of the canonical result payload. It is an
//! **integrity/audit digest** for tamper-detection and de-duplication — it is
//! *not* a cryptographic signature and asserts nothing about server identity
//! (MCP servers do not sign results). [`RealMcpClient::verify_result`]
//! recomputes the digest and checks `success` is the negation of the reported
//! `isError`. This replaces the mock's signature theater.

mod client;
mod config;
mod http;
mod jsonrpc;
mod stream;
mod types;

#[cfg(test)]
mod tests;

pub use client::McpClient;
pub use config::{McpServerConfig, McpServerRow, McpTransportConfig, TransportKind};
pub use types::{McpContent, McpServerInfo, McpToolInfo, McpToolOutput};

use crate::traits::{AdapterError, McpProvider, McpToolCall, McpToolResult};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

const URN_PREFIX: &str = "urn:mcp:";

/// Default per-call timeout for `tools/call` when the caller does not specify
/// one (used by the [`McpProvider`] trait impl, whose signature has no timeout).
pub const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(60);

/// Build the fully-qualified URN for a tool on a server.
pub fn make_tool_urn(server_name: &str, tool_name: &str) -> String {
    format!("{URN_PREFIX}{server_name}:{tool_name}")
}

/// Split a tool URN into `(server_name, tool_name)`. Splits on the first `:`
/// after the `urn:mcp:` prefix; the tool name keeps any remaining `:`.
pub fn parse_tool_urn(urn: &str) -> Result<(&str, &str), AdapterError> {
    let rest = urn.strip_prefix(URN_PREFIX).ok_or_else(|| {
        AdapterError::McpError(format!("tool URN '{urn}' missing '{URN_PREFIX}' prefix"))
    })?;
    let (server, tool) = rest.split_once(':').ok_or_else(|| {
        AdapterError::McpError(format!(
            "tool URN '{urn}' must be '{URN_PREFIX}{{server}}:{{tool}}'"
        ))
    })?;
    if server.is_empty() || tool.is_empty() {
        return Err(AdapterError::McpError(format!(
            "tool URN '{urn}' has an empty server or tool segment"
        )));
    }
    Ok((server, tool))
}

/// Compute the `sha256:`-prefixed integrity digest of a tool result payload.
///
/// Deterministic for a given [`serde_json::Value`] because coevo does not enable
/// serde_json's `preserve_order` feature, so object keys serialize sorted.
pub fn integrity_hash(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

// ---------------------------------------------------------------------------
// McpClientManager: registry of connected servers, keyed by config id.
// ---------------------------------------------------------------------------

/// A registry of connected [`McpClient`]s, keyed by [`McpServerConfig::id`].
///
/// Cheap to clone (shares the inner map via `Arc`). Thread-safe.
///
/// Reconnect-on-demand: `list_tools` / `call_tool` probe liveness first (stdio
/// child exited, stream closed) and attempt **one** transparent reconnect from
/// the stored config before failing; a call that fails with
/// [`AdapterError::Unavailable`] mid-flight is likewise retried once on a
/// fresh connection.
#[derive(Clone, Default)]
pub struct McpClientManager {
    clients: Arc<RwLock<HashMap<String, ManagedConnection>>>,
}

struct ManagedConnection {
    /// Config used to (re)connect. `None` for test-injected clients, which
    /// therefore cannot be reconnected.
    config: Option<McpServerConfig>,
    client: Arc<McpClient>,
}

impl McpClientManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Connect to `config`'s server and run the `initialize` handshake. The
    /// connected client is stored under `config.id`, replacing any prior client
    /// with that id (whose connection is shut down).
    pub async fn connect(&self, config: McpServerConfig) -> Result<McpServerInfo, AdapterError> {
        let client = Self::build_client(&config).await?;
        let info = client
            .server_info()
            .ok_or_else(|| AdapterError::McpError("initialize produced no server info".into()))?;
        let previous = {
            let mut guard = self.clients.write().await;
            guard.insert(
                config.id.clone(),
                ManagedConnection {
                    config: Some(config),
                    client,
                },
            )
        };
        if let Some(old) = previous {
            old.client.shutdown().await;
        }
        Ok(info)
    }

    /// Build + initialize a client from a config (shared by connect/reconnect).
    async fn build_client(config: &McpServerConfig) -> Result<Arc<McpClient>, AdapterError> {
        config.validate().map_err(AdapterError::McpError)?;
        let client = match &config.transport {
            McpTransportConfig::Stdio { command, args, env } => {
                McpClient::connect_stdio(config.id.clone(), config.name.clone(), command, args, env)
                    .await?
            }
            McpTransportConfig::Http { url, headers } => McpClient::connect_http(
                config.id.clone(),
                config.name.clone(),
                url.clone(),
                headers.clone(),
            )?,
        };
        if let Err(e) = client.initialize().await {
            client.shutdown().await;
            return Err(e);
        }
        Ok(Arc::new(client))
    }

    /// Disconnect and remove the server with the given id.
    pub async fn disconnect(&self, id: &str) -> Result<(), AdapterError> {
        let entry = self.clients.write().await.remove(id);
        match entry {
            Some(entry) => {
                entry.client.shutdown().await;
                Ok(())
            }
            None => Err(AdapterError::McpError(format!(
                "no connected MCP server with id '{id}'"
            ))),
        }
    }

    /// Disconnect every server.
    pub async fn disconnect_all(&self) {
        let clients: Vec<Arc<McpClient>> = {
            let mut guard = self.clients.write().await;
            guard.drain().map(|(_, entry)| entry.client).collect()
        };
        for c in clients {
            c.shutdown().await;
        }
    }

    /// True when the server is registered AND its connection is alive
    /// (stdio child running / stream open; HTTP is stateless = alive).
    pub async fn is_connected(&self, id: &str) -> bool {
        let client = {
            let guard = self.clients.read().await;
            guard.get(id).map(|entry| Arc::clone(&entry.client))
        };
        match client {
            Some(client) => client.is_alive().await,
            None => false,
        }
    }

    /// Ids of all registered servers (sorted for stable output).
    pub async fn connected_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.clients.read().await.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Negotiated info for a connected server, if present.
    pub async fn server_info(&self, id: &str) -> Option<McpServerInfo> {
        let client = {
            let guard = self.clients.read().await;
            guard.get(id).map(|entry| Arc::clone(&entry.client))
        };
        client.and_then(|c| c.server_info())
    }

    /// `tools/list` for one connected server (one transparent reconnect on a
    /// dead connection).
    pub async fn list_tools(&self, id: &str) -> Result<Vec<McpToolInfo>, AdapterError> {
        let client = self.live_client(id).await?;
        match client.list_tools().await {
            Err(AdapterError::Unavailable) => {
                let client = self.reconnect(id).await?;
                client.list_tools().await
            }
            other => other,
        }
    }

    /// `tools/call` for one connected server, bounded by `timeout` (one
    /// transparent reconnect on a dead connection).
    pub async fn call_tool(
        &self,
        id: &str,
        name: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<McpToolOutput, AdapterError> {
        let client = self.live_client(id).await?;
        match client.call_tool(name, arguments.clone(), timeout).await {
            Err(AdapterError::Unavailable) => {
                let client = self.reconnect(id).await?;
                client.call_tool(name, arguments, timeout).await
            }
            other => other,
        }
    }

    /// Fetch the client for `id`, reconnecting up-front if it is already dead.
    async fn live_client(&self, id: &str) -> Result<Arc<McpClient>, AdapterError> {
        let client = {
            let guard = self.clients.read().await;
            guard
                .get(id)
                .map(|entry| Arc::clone(&entry.client))
                .ok_or_else(|| {
                    AdapterError::McpError(format!("no connected MCP server with id '{id}'"))
                })?
        };
        if client.is_alive().await {
            Ok(client)
        } else {
            self.reconnect(id).await
        }
    }

    /// Rebuild the connection for `id` from its stored config.
    async fn reconnect(&self, id: &str) -> Result<Arc<McpClient>, AdapterError> {
        let config = {
            let guard = self.clients.read().await;
            let entry = guard.get(id).ok_or_else(|| {
                AdapterError::McpError(format!("no connected MCP server with id '{id}'"))
            })?;
            entry.config.clone().ok_or_else(|| {
                AdapterError::McpError(format!(
                    "MCP server '{id}' connection is dead and has no stored config to reconnect"
                ))
            })?
        };
        tracing::warn!(server = %id, "MCP connection dead; attempting reconnect");
        let client = Self::build_client(&config).await?;
        let previous = {
            let mut guard = self.clients.write().await;
            guard.insert(
                id.to_string(),
                ManagedConnection {
                    config: Some(config),
                    client: Arc::clone(&client),
                },
            )
        };
        if let Some(old) = previous {
            old.client.shutdown().await;
        }
        Ok(client)
    }

    /// Insert an already-built [`McpClient`] under `id` (test-only: lets tests
    /// register an in-memory `over_stream` server without spawning a child).
    #[cfg(test)]
    pub(crate) async fn insert_client(&self, id: impl Into<String>, client: McpClient) {
        self.clients.write().await.insert(
            id.into(),
            ManagedConnection {
                config: None,
                client: Arc::new(client),
            },
        );
    }
}

// ---------------------------------------------------------------------------
// RealMcpClient: the production McpProvider, routing tool URNs to servers.
// ---------------------------------------------------------------------------

/// The production [`McpProvider`]. Wraps a [`McpClientManager`] and routes a
/// tool URN (`urn:mcp:{server_name}:{tool_name}`) to the owning server.
///
/// Construct it from a set of [`McpServerConfig`]s with
/// [`RealMcpClient::connect_all`], or wrap an existing manager with
/// [`RealMcpClient::from_manager`] (e.g. to share one manager between the
/// CRUD API and the worker tool path).
#[derive(Clone)]
pub struct RealMcpClient {
    manager: McpClientManager,
    /// server_name -> server_id, kept in step with the manager's connections.
    name_to_id: Arc<RwLock<HashMap<String, String>>>,
    tool_timeout: Duration,
}

impl RealMcpClient {
    /// Connect to every server in `configs`. Fails on the first server that
    /// cannot be connected. Server names and ids must each be unique.
    pub async fn connect_all(configs: &[McpServerConfig]) -> Result<Self, AdapterError> {
        let client = Self {
            manager: McpClientManager::new(),
            name_to_id: Arc::new(RwLock::new(HashMap::new())),
            tool_timeout: DEFAULT_TOOL_TIMEOUT,
        };
        for cfg in configs {
            client.add(cfg.clone()).await?;
        }
        Ok(client)
    }

    /// Wrap an existing manager. The name->id index is rebuilt from the
    /// manager's current connections.
    pub async fn from_manager(manager: McpClientManager) -> Self {
        let mut index = HashMap::new();
        for id in manager.connected_ids().await {
            if let Some(info) = manager.server_info(&id).await {
                index.insert(info.name, id);
            }
        }
        Self {
            manager,
            name_to_id: Arc::new(RwLock::new(index)),
            tool_timeout: DEFAULT_TOOL_TIMEOUT,
        }
    }

    /// Override the per-call `tools/call` timeout used by the trait impl.
    pub fn with_tool_timeout(mut self, timeout: Duration) -> Self {
        self.tool_timeout = timeout;
        self
    }

    /// The underlying manager (for sharing with a CRUD layer).
    pub fn manager(&self) -> &McpClientManager {
        &self.manager
    }

    /// Connect (or reconnect) one server and index its name.
    pub async fn add(&self, config: McpServerConfig) -> Result<McpServerInfo, AdapterError> {
        let name = config.name.clone();
        {
            let index = self.name_to_id.read().await;
            if let Some(existing_id) = index.get(&name) {
                if existing_id != &config.id {
                    return Err(AdapterError::McpError(format!(
                        "MCP server name '{name}' already used by id '{existing_id}'"
                    )));
                }
            }
        }
        let info = self.manager.connect(config.clone()).await?;
        self.name_to_id
            .write()
            .await
            .insert(name, config.id.clone());
        Ok(info)
    }

    /// Disconnect one server by id and drop its name from the index.
    pub async fn remove(&self, id: &str) -> Result<(), AdapterError> {
        self.manager.disconnect(id).await?;
        self.name_to_id.write().await.retain(|_, v| v != id);
        Ok(())
    }

    async fn id_for_name(&self, name: &str) -> Result<String, AdapterError> {
        self.name_to_id
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| {
                AdapterError::McpError(format!("no connected MCP server named '{name}'"))
            })
    }

    /// List tools across all servers as `(urn, McpToolInfo)` pairs.
    pub async fn list_tools_detailed(&self) -> Result<Vec<(String, McpToolInfo)>, AdapterError> {
        let index = self.name_to_id.read().await.clone();
        let mut out = Vec::new();
        for (name, id) in index {
            let tools = self.manager.list_tools(&id).await?;
            for tool in tools {
                out.push((make_tool_urn(&name, &tool.name), tool));
            }
        }
        Ok(out)
    }

    /// Invoke a tool addressed by URN and return the raw [`McpToolOutput`].
    pub async fn call_tool_raw(
        &self,
        urn: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<McpToolOutput, AdapterError> {
        let (server, tool) = parse_tool_urn(urn)?;
        let id = self.id_for_name(server).await?;
        self.manager.call_tool(&id, tool, arguments, timeout).await
    }

    /// Disconnect every server.
    pub async fn close(&self) {
        self.manager.disconnect_all().await;
        self.name_to_id.write().await.clear();
    }

    /// Map a raw tool output to the trait-level [`McpToolResult`], computing the
    /// integrity digest over the structured result payload.
    fn to_tool_result(urn: String, output: McpToolOutput) -> McpToolResult {
        let result = serde_json::json!({
            "isError": output.is_error,
            "content": output.content,
            "structuredContent": output.structured,
            "text": output.text(),
        });
        let digest = integrity_hash(&result);
        McpToolResult {
            tool_urn: urn,
            result,
            success: !output.is_error,
            verification_signature: Some(digest),
        }
    }
}

#[async_trait]
impl McpProvider for RealMcpClient {
    async fn call_tool(&self, call: McpToolCall) -> Result<McpToolResult, AdapterError> {
        let output = self
            .call_tool_raw(&call.tool_urn, call.parameters, self.tool_timeout)
            .await?;
        Ok(Self::to_tool_result(call.tool_urn, output))
    }

    async fn list_tools(&self) -> Result<Vec<String>, AdapterError> {
        let detailed = self.list_tools_detailed().await?;
        Ok(detailed.into_iter().map(|(urn, _)| urn).collect())
    }

    /// Honest verification: recompute the integrity digest and confirm `success`
    /// is the negation of the embedded `isError`. No signature theater.
    async fn verify_result(&self, result: &McpToolResult) -> Result<bool, AdapterError> {
        let embedded_is_error = result
            .result
            .get("isError")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if result.success == embedded_is_error {
            return Ok(false);
        }
        match &result.verification_signature {
            Some(sig) => Ok(*sig == integrity_hash(&result.result)),
            None => Ok(false),
        }
    }
}
