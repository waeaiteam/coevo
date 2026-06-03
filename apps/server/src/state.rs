//! Application state — dependency injection container.

use std::path::PathBuf;
use std::sync::Arc;

use coevo_adapters::a2a::MockA2aAdapter;
use coevo_adapters::identity::MockIdentityProvider;
use coevo_adapters::mcp::MockMcpAdapter;
use coevo_store::company_workspace::CompanyWorkspaceManager;
use coevo_adapters::traits::*;
use coevo_policy::mock::MockPolicyEngine;
use coevo_policy::traits::PolicyEngine;
use sqlx::SqlitePool;

/// Shared application state injected into all handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub coevo_home: PathBuf,
    pub company_workspace: Arc<CompanyWorkspaceManager>,
    pub a2a: Arc<dyn A2aProvider>,
    pub mcp: Arc<dyn McpProvider>,
    pub identity: Arc<dyn IdentityProvider>,
    pub policy_engine: Arc<dyn PolicyEngine>,
}

impl AppState {
    pub fn new(pool: SqlitePool, coevo_home: PathBuf) -> Self {
        let company_workspace = Arc::new(CompanyWorkspaceManager::new(coevo_home.clone()));
        Self {
            pool,
            coevo_home,
            company_workspace,
            a2a: Arc::new(MockA2aAdapter::new()),
            mcp: Arc::new(MockMcpAdapter::new()),
            identity: Arc::new(MockIdentityProvider::new()),
            policy_engine: Arc::new(MockPolicyEngine::new()),
        }
    }
}
