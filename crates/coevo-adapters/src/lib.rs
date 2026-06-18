//! coevo-adapters: Adapters for external protocols (A2A, MCP, Identity).
//!
//! `mcp_client` is the real MCP client (JSON-RPC 2.0 over stdio / Streamable
//! HTTP); `mcp` keeps the legacy mock adapter used by tests and the
//! env-gated mock path.
pub mod a2a;
pub mod a2a_router;
pub mod identity;
pub mod identity_ed25519;
pub mod mcp;
pub mod mcp_client;
pub mod traits;

pub use a2a_router::{DeliveredMessage, InProcessA2aRouter};
pub use identity_ed25519::{identity_challenge, Ed25519IdentityProvider};
pub use mcp_client::{
    integrity_hash, make_tool_urn, parse_tool_urn, shared_mcp_client_manager, McpClient,
    McpClientManager, McpContent, McpServerConfig, McpServerInfo, McpServerRow, McpToolInfo,
    McpToolOutput, McpTransportConfig, RealMcpClient, TransportKind, DEFAULT_TOOL_TIMEOUT,
};
