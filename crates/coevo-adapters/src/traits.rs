//! Adapter traits for external protocol integrations.
//! Per coevo whitepaper: A2A, MCP, Identity.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---- A2A (Agent-to-Agent) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aMessage {
    pub from_agent: String,
    pub to_agent: String,
    pub payload: serde_json::Value,
    pub traceparent: String,
    pub contract_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aResponse {
    pub from_agent: String,
    pub payload: serde_json::Value,
    pub success: bool,
}

/// One delivered message sitting in a recipient's inbox on the in-process bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveredMessage {
    pub delivery_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub payload: serde_json::Value,
    pub traceparent: String,
    pub contract_hash: String,
}

#[async_trait]
pub trait A2aProvider: Send + Sync {
    async fn send_message(&self, msg: A2aMessage) -> Result<A2aResponse, AdapterError>;
    async fn discover_agents(&self) -> Result<Vec<String>, AdapterError>;
    async fn health_check(&self) -> Result<bool, AdapterError>;

    /// Register an agent so it can receive messages. Default no-op for transports
    /// that do not maintain a local registry.
    fn register(&self, _agent_id: &str) {}

    /// Unregister an agent and discard any queued local messages for it. Default
    /// no-op for stateless transports.
    fn unregister(&self, _agent_id: &str) {}

    /// Drain (take and clear) the recipient's inbox. Default empty for transports
    /// without inboxes; the in-process router returns real queued peer messages.
    fn drain_inbox(&self, _agent_id: &str) -> Vec<DeliveredMessage> {
        Vec::new()
    }
}

// ---- MCP (Model Context Protocol) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub tool_urn: String,
    pub parameters: serde_json::Value,
    pub traceparent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub tool_urn: String,
    pub result: serde_json::Value,
    pub success: bool,
    pub verification_signature: Option<String>,
}

#[async_trait]
pub trait McpProvider: Send + Sync {
    async fn call_tool(&self, call: McpToolCall) -> Result<McpToolResult, AdapterError>;
    async fn list_tools(&self) -> Result<Vec<String>, AdapterError>;
    async fn verify_result(&self, result: &McpToolResult) -> Result<bool, AdapterError>;
}

// ---- Identity (OIDC / mTLS) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityClaims {
    pub sub: String,
    pub agent_id: String,
    pub roles: Vec<String>,
    pub tenant_id: String,
    pub passport_hash: String,
}

#[async_trait]
pub trait IdentityProvider: Send + Sync {
    async fn verify_proof(
        &self,
        caller_identity_proof: &str,
    ) -> Result<IdentityClaims, AdapterError>;
    async fn verify_mfa(&self, token: &str, user_id: &str) -> Result<bool, AdapterError>;
    async fn issue_passport(
        &self,
        agent_id: &str,
        roles: Vec<String>,
    ) -> Result<String, AdapterError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("A2A transport error: {0}")]
    A2aError(String),
    #[error("MCP tool error: {0}")]
    McpError(String),
    /// A JSON-RPC error object returned by an MCP server.
    #[error("MCP JSON-RPC error (code {code}): {message}")]
    McpRpc {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
    #[error("Identity verification failed: {0}")]
    IdentityError(String),
    #[error("Adapter timeout")]
    Timeout,
    #[error("Adapter not available")]
    Unavailable,
}
