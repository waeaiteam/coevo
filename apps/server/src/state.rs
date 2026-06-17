//! Application state — dependency injection container.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use coevo_adapters::a2a::MockA2aAdapter;
use coevo_adapters::identity::MockIdentityProvider;
use coevo_adapters::mcp::MockMcpAdapter;
use coevo_adapters::mcp_client::{McpClientManager, RealMcpClient};
use coevo_adapters::mcp_client::{McpServerConfig, McpServerRow};
use coevo_adapters::shared_mcp_client_manager;
use coevo_adapters::traits::{
    A2aMessage, A2aProvider, A2aResponse, AdapterError, IdentityClaims, IdentityProvider,
    McpProvider, McpToolCall, McpToolResult,
};
use coevo_policy::mock::MockPolicyEngine;
use coevo_policy::traits::PolicyEngine;
use coevo_store::company_workspace::CompanyWorkspaceManager;
use coevo_store::repos::mcp_server_repo::{McpServerRecord, McpServerRepo};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

/// Shared application state injected into all handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub coevo_home: PathBuf,
    pub company_workspace: Arc<CompanyWorkspaceManager>,
    pub mcp_manager: Arc<McpClientManager>,
    pub a2a: Arc<dyn A2aProvider>,
    pub mcp: Arc<dyn McpProvider>,
    pub identity: Arc<dyn IdentityProvider>,
    pub policy_engine: Arc<dyn PolicyEngine>,
}

impl AppState {
    pub fn new(pool: SqlitePool, coevo_home: PathBuf) -> Self {
        let company_workspace = Arc::new(CompanyWorkspaceManager::new(coevo_home.clone()));
        let mock_adapters_enabled = cfg!(test)
            || matches!(
                std::env::var("COEVO_ENABLE_MOCK_ADAPTERS"),
                Ok(value) if value == "1"
            );
        let mcp_manager = Arc::new(shared_mcp_client_manager());
        let provider_pool = pool.clone();
        Self {
            pool,
            coevo_home,
            company_workspace,
            mcp_manager: Arc::clone(&mcp_manager),
            a2a: build_a2a_provider(mock_adapters_enabled),
            mcp: build_mcp_provider(mock_adapters_enabled, provider_pool, mcp_manager),
            identity: build_identity_provider(mock_adapters_enabled),
            policy_engine: build_policy_engine(mock_adapters_enabled),
        }
    }
}

fn build_a2a_provider(use_mocks: bool) -> Arc<dyn A2aProvider> {
    if use_mocks {
        Arc::new(MockA2aAdapter::new())
    } else {
        Arc::new(DenyAllA2aProvider)
    }
}

fn build_mcp_provider(
    use_mocks: bool,
    pool: SqlitePool,
    manager: Arc<McpClientManager>,
) -> Arc<dyn McpProvider> {
    if use_mocks {
        Arc::new(MockMcpAdapter::new())
    } else {
        Arc::new(DbBackedMcpProvider::new(pool, manager))
    }
}

fn build_identity_provider(use_mocks: bool) -> Arc<dyn IdentityProvider> {
    if use_mocks {
        Arc::new(MockIdentityProvider::new())
    } else {
        Arc::new(DenyAllIdentityProvider)
    }
}

fn build_policy_engine(use_mocks: bool) -> Arc<dyn PolicyEngine> {
    if use_mocks {
        Arc::new(MockPolicyEngine::new())
    } else {
        Arc::new(DenyAllPolicyEngine)
    }
}

struct DenyAllA2aProvider;

#[async_trait]
impl A2aProvider for DenyAllA2aProvider {
    async fn send_message(&self, _msg: A2aMessage) -> Result<A2aResponse, AdapterError> {
        Err(AdapterError::Unavailable)
    }

    async fn discover_agents(&self) -> Result<Vec<String>, AdapterError> {
        Err(AdapterError::Unavailable)
    }

    async fn health_check(&self) -> Result<bool, AdapterError> {
        Err(AdapterError::Unavailable)
    }
}

struct DbBackedMcpProvider {
    pool: SqlitePool,
    manager: Arc<McpClientManager>,
    seeded: Mutex<bool>,
}

impl DbBackedMcpProvider {
    fn new(pool: SqlitePool, manager: Arc<McpClientManager>) -> Self {
        Self {
            pool,
            manager,
            seeded: Mutex::new(false),
        }
    }

    async fn ensure_seeded(&self) -> Result<(), AdapterError> {
        let mut seeded = self.seeded.lock().await;
        if *seeded {
            return Ok(());
        }
        let records = McpServerRepo::list_enabled(&self.pool)
            .await
            .map_err(|e| AdapterError::McpError(e.to_string()))?;
        for record in records {
            if let Ok(config) = mcp_record_to_config(&record) {
                if let Err(err) = self.manager.connect(config).await {
                    tracing::warn!(server = %record.id, error = %err, "failed to seed MCP server");
                }
            }
        }
        *seeded = true;
        Ok(())
    }
}

#[async_trait]
impl McpProvider for DbBackedMcpProvider {
    async fn call_tool(&self, call: McpToolCall) -> Result<McpToolResult, AdapterError> {
        self.ensure_seeded().await?;
        let real = RealMcpClient::from_manager((*self.manager).clone()).await;
        real.call_tool(call).await
    }

    async fn list_tools(&self) -> Result<Vec<String>, AdapterError> {
        self.ensure_seeded().await?;
        let real = RealMcpClient::from_manager((*self.manager).clone()).await;
        real.list_tools().await
    }

    async fn verify_result(&self, _result: &McpToolResult) -> Result<bool, AdapterError> {
        self.ensure_seeded().await?;
        let real = RealMcpClient::from_manager((*self.manager).clone()).await;
        real.verify_result(_result).await
    }
}

fn mcp_record_to_config(record: &McpServerRecord) -> Result<McpServerConfig, String> {
    McpServerConfig::from_row(McpServerRow {
        id: record.id.clone(),
        name: record.name.clone(),
        transport: record.transport.clone(),
        command: record.command.clone(),
        args: Some(record.args_json.clone()),
        env: Some(record.env_json.clone()),
        url: record.url.clone(),
        headers: Some(record.headers_json.clone()),
    })
}

struct DenyAllIdentityProvider;

#[async_trait]
impl IdentityProvider for DenyAllIdentityProvider {
    async fn verify_proof(
        &self,
        _caller_identity_proof: &str,
    ) -> Result<IdentityClaims, AdapterError> {
        Err(AdapterError::Unavailable)
    }

    async fn verify_mfa(&self, _token: &str, _user_id: &str) -> Result<bool, AdapterError> {
        Err(AdapterError::Unavailable)
    }

    async fn issue_passport(
        &self,
        _agent_id: &str,
        _roles: Vec<String>,
    ) -> Result<String, AdapterError> {
        Err(AdapterError::Unavailable)
    }
}

struct DenyAllPolicyEngine;

#[async_trait]
impl PolicyEngine for DenyAllPolicyEngine {
    async fn validate_contract(
        &self,
        _contract: &coevo_core::contract::MCLSpec,
    ) -> Result<coevo_policy::traits::PolicyResult, coevo_policy::traits::PolicyEngineError> {
        Ok(coevo_policy::traits::PolicyResult {
            passed: false,
            violations: vec![coevo_policy::traits::PolicyViolation {
                policy_urn: "urn:coevo:policy:unavailable".to_string(),
                description: "policy engine unavailable".to_string(),
                remediation: Some("set COEVO_ENABLE_MOCK_ADAPTERS=1 for dev/test".to_string()),
            }],
            policy_version: "unavailable".to_string(),
            policies_checked: vec!["urn:coevo:policy:unavailable".to_string()],
        })
    }

    async fn dry_run(
        &self,
        contract: &coevo_core::contract::MCLSpec,
    ) -> Result<coevo_policy::traits::PolicyResult, coevo_policy::traits::PolicyEngineError> {
        self.validate_contract(contract).await
    }

    fn policy_version(&self) -> String {
        "unavailable".to_string()
    }

    async fn evaluate_action(
        &self,
        action_urn: &str,
        _contract: &coevo_core::contract::MCLSpec,
    ) -> Result<coevo_policy::traits::PolicyResult, coevo_policy::traits::PolicyEngineError> {
        Ok(coevo_policy::traits::PolicyResult {
            passed: false,
            violations: vec![coevo_policy::traits::PolicyViolation {
                policy_urn: action_urn.to_string(),
                description: "policy engine unavailable".to_string(),
                remediation: Some("set COEVO_ENABLE_MOCK_ADAPTERS=1 for dev/test".to_string()),
            }],
            policy_version: "unavailable".to_string(),
            policies_checked: vec![action_urn.to_string()],
        })
    }

    async fn diff_policies(
        &self,
        _old_version: &str,
        _new_version: &str,
    ) -> Result<coevo_policy::traits::PolicyDiff, coevo_policy::traits::PolicyEngineError> {
        Err(coevo_policy::traits::PolicyEngineError::Internal(
            "policy engine unavailable".to_string(),
        ))
    }

    async fn health_check(&self) -> Result<bool, coevo_policy::traits::PolicyEngineError> {
        Ok(false)
    }

    async fn rollback(
        &mut self,
        _target_version: &str,
    ) -> Result<(), coevo_policy::traits::PolicyEngineError> {
        Err(coevo_policy::traits::PolicyEngineError::Internal(
            "policy engine unavailable".to_string(),
        ))
    }
}
