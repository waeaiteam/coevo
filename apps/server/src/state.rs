//! Application state — dependency injection container.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use coevo_adapters::a2a::MockA2aAdapter;
use coevo_adapters::a2a_router::InProcessA2aRouter;
use coevo_adapters::identity::MockIdentityProvider;
use coevo_adapters::identity_ed25519::Ed25519IdentityProvider;
use coevo_adapters::mcp::MockMcpAdapter;
use coevo_adapters::mcp_client::{McpClientManager, RealMcpClient};
use coevo_adapters::mcp_client::{McpServerConfig, McpServerRow};
use coevo_adapters::shared_mcp_client_manager;
use coevo_adapters::traits::{
    A2aMessage, A2aProvider, A2aResponse, AdapterError, IdentityClaims, IdentityProvider,
    McpProvider, McpToolCall, McpToolResult,
};
use coevo_policy::config::ConfigDrivenPolicyEngine;
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
        // In server tests the worker crate is a dependency compiled WITHOUT its own
        // `cfg!(test)`, so its GovernGate would otherwise fall back to the fail-closed
        // ConfigDrivenPolicyEngine and enforce production policy rules. Opt the worker into the
        // keyword MockPolicyEngine here, the same way acceptance suites do. Never set
        // in release/dev binaries — gated on the server crate's own test cfg.
        #[cfg(test)]
        if std::env::var("COEVO_ENABLE_MOCK_POLICY_ENGINE").is_err() {
            std::env::set_var("COEVO_ENABLE_MOCK_POLICY_ENGINE", "1");
        }
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
        // Production: a real in-process message bus for manager-to-manager (A2A) delivery.
        // Heads register on demand; cross-system A2A would layer a transport over this.
        Arc::new(InProcessA2aRouter::new(vec![]))
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
        // Production: a real Ed25519 verifier seeded from an optional agent-keys file
        // (COEVO_IDENTITY_KEYS, JSON: {"agent_id": {"public_key_hex","roles","tenant_id"}}).
        // With no file the registry is empty, so verify_proof fails closed for every agent
        // exactly like the previous DenyAll provider — but the verification path is real
        // and ready to accept registered keys, instead of being a hard stub.
        Arc::new(load_ed25519_identity_provider())
    }
}

fn load_ed25519_identity_provider() -> Ed25519IdentityProvider {
    let mut provider = Ed25519IdentityProvider::new();
    let Ok(path) = std::env::var("COEVO_IDENTITY_KEYS") else {
        return provider;
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return provider;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return provider;
    };
    if let Some(map) = parsed.as_object() {
        for (agent_id, entry) in map {
            let public_key_hex = entry
                .get("public_key_hex")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if public_key_hex.is_empty() {
                continue;
            }
            let roles = entry
                .get("roles")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|r| r.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let tenant_id = entry
                .get("tenant_id")
                .and_then(|v| v.as_str())
                .unwrap_or("coevo-default-tenant")
                .to_string();
            provider = provider.with_agent(agent_id.clone(), public_key_hex, roles, tenant_id);
        }
    }
    provider
}

fn build_policy_engine(use_mocks: bool) -> Arc<dyn PolicyEngine> {
    if use_mocks {
        Arc::new(MockPolicyEngine::new())
    } else {
        Arc::new(ConfigDrivenPolicyEngine::from_env_or_baseline())
    }
}

#[allow(dead_code)]
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
        let mut seed_complete = true;
        for record in records {
            match mcp_record_to_config(&record) {
                Ok(config) => {
                    if let Err(err) = self.manager.connect(config).await {
                        seed_complete = false;
                        tracing::warn!(opc_id = %record.opc_id, server = %record.id, error = %err, "failed to seed MCP server");
                    }
                }
                Err(err) => {
                    seed_complete = false;
                    tracing::warn!(opc_id = %record.opc_id, server = %record.id, error = %err, "invalid MCP server config during seed");
                }
            }
        }
        *seeded = seed_complete;
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
        id: format!("{}:{}", record.opc_id, record.id),
        name: record.name.clone(),
        transport: record.transport.clone(),
        command: record.command.clone(),
        args: Some(record.args_json.clone()),
        env: Some(record.env_json.clone()),
        url: record.url.clone(),
        headers: Some(record.headers_json.clone()),
    })
}

// Retained as the hard fail-closed alternative. Production now uses the real
// Ed25519IdentityProvider (empty registry = same fail-closed behavior, but verifiable).
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};

    fn bad_stdio_record() -> McpServerRecord {
        let now = chrono::Utc::now().to_rfc3339();
        McpServerRecord {
            opc_id: "default-opc".to_string(),
            id: "mcp-bad-seed".to_string(),
            name: "bad-seed".to_string(),
            transport: "stdio".to_string(),
            command: Some("this-command-does-not-exist-coevo-mcp".to_string()),
            args_json: "[]".to_string(),
            env_json: "{}".to_string(),
            url: None,
            headers_json: "{}".to_string(),
            enabled: true,
            status: "unknown".to_string(),
            last_error: None,
            tools_json: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn mcp_seed_failure_is_not_marked_complete() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        McpServerRepo::insert(&pool, &bad_stdio_record())
            .await
            .unwrap();
        let provider = DbBackedMcpProvider::new(pool, Arc::new(McpClientManager::new()));

        let tools = provider
            .list_tools()
            .await
            .expect("failed MCP seed should not make listing itself fail");

        assert!(tools.is_empty());
        assert!(
            !*provider.seeded.lock().await,
            "partial MCP seed failure must leave provider unseeded so a later call retries"
        );
    }
}
