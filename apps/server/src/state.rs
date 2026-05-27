//! Application state — dependency injection container.

use coevo_adapters::a2a::MockA2aAdapter;
use coevo_adapters::identity::MockIdentityProvider;
use coevo_adapters::mcp::MockMcpAdapter;
use coevo_adapters::traits::*;
use coevo_policy::mock::MockPolicyEngine;
use coevo_policy::traits::PolicyEngine;
use sqlx::SqlitePool;

/// Shared application state injected into all handlers.
pub struct AppState {
    pub pool: SqlitePool,
    pub a2a: Box<dyn A2aProvider>,
    pub mcp: Box<dyn McpProvider>,
    pub identity: Box<dyn IdentityProvider>,
    pub policy_engine: Box<dyn PolicyEngine>,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            a2a: Box::new(MockA2aAdapter::new()),
            mcp: Box::new(MockMcpAdapter::new()),
            identity: Box::new(MockIdentityProvider::new()),
            policy_engine: Box::new(MockPolicyEngine::new()),
        }
    }
}
