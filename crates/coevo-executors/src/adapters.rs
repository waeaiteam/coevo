//! Mock External Executor adapters for Hermes, OpenClaw, MCP, 302AI, Browser, LocalProcess.
//! v1: mock implementations. Real adapters will connect to actual runtimes later.

use async_trait::async_trait;
use coevo_core::opc::*;
use crate::traits::*;

macro_rules! mock_executor {
    ($name:ident, $source_type:expr, $display_name:expr) => {
        pub struct $name {
            passport: ExternalExecutorPassport,
        }
        impl $name {
            pub fn new() -> Self {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                Self {
                    passport: ExternalExecutorPassport {
                        executor_id: uuid::Uuid::new_v4().to_string(),
                        display_name: $display_name.to_string(),
                        source_type: $source_type,
                        runtime_endpoint: "http://localhost:0/mock".to_string(),
                        capabilities: vec!["mock".to_string()],
                        required_credentials: vec![],
                        permission_boundary: PermissionBoundary {
                            max_risk_score: 0.5, can_write_fact: false, can_write_decision: false,
                            can_access_network: false, can_access_filesystem: false,
                            can_call_external_executor: false, can_propose_skill: false,
                        },
                        file_scope: vec![],
                        network_scope: vec![],
                        memory_scope: MemoryScope::Executor,
                        risk_ceiling: 0.5,
                        supported_actions: vec!["read".to_string()],
                        sandbox_level: SandboxLevel::None,
                        health_check_url: String::new(),
                        audit_callback_url: String::new(),
                        status: ExecutorStatus::Registered,
                        created_at_ms: now,
                        updated_at_ms: now,
                    },
                }
            }
        }
        impl Default for $name { fn default() -> Self { Self::new() } }

        #[async_trait]
        impl ExternalExecutorAdapter for $name {
            fn passport(&self) -> &ExternalExecutorPassport { &self.passport }
            async fn health_check(&self) -> Result<ExecutorHealth, ExecutorError> {
                Ok(ExecutorHealth { online: true, latency_ms: 1, version: "mock-1.0".into() })
            }
            async fn describe_capabilities(&self) -> Result<Vec<String>, ExecutorError> {
                Ok(self.passport.capabilities.clone())
            }
            async fn dry_run(&self, wo: &WorkOrder) -> Result<DryRunResult, ExecutorError> {
                if wo.track == "red" && self.passport.risk_ceiling < 0.8 {
                    return Err(ExecutorError::RiskCeilingExceeded { wo: 0.9, ceiling: self.passport.risk_ceiling });
                }
                Ok(DryRunResult { passed: true, estimated_cost_usd: 0.01, estimated_duration_ms: 100, warnings: vec![] })
            }
            async fn execute(&self, wo: &WorkOrder, _lease: Option<&coevo_core::lease::EmergencyLease>)
                -> Result<ExecutorResult, ExecutorError> {
                if self.passport.status != ExecutorStatus::Registered {
                    return Err(ExecutorError::NotRegistered);
                }
                Ok(ExecutorResult {
                    run_id: uuid::Uuid::new_v4().to_string(), success: true,
                    output: serde_json::json!({"mock": true, "executor": self.passport.display_name}),
                    audit_trace: format!("mock-trace-{}", uuid::Uuid::new_v4()), cost_usd: 0.01,
                })
            }
            async fn cancel(&self, _run_id: &str) -> Result<(), ExecutorError> { Ok(()) }
            async fn fetch_audit(&self, _run_id: &str) -> Result<String, ExecutorError> {
                Ok("mock-audit".to_string())
            }
        }
    };
}

mock_executor!(MockHermesAdapter, ExecutorSourceType::Hermes, "Hermes Runtime");
mock_executor!(MockOpenClawAdapter, ExecutorSourceType::OpenClaw, "OpenClaw Runtime");
mock_executor!(MockMcpAdapter, ExecutorSourceType::MCP, "MCP Runtime");
mock_executor!(MockLocal302AIAdapter, ExecutorSourceType::Local302AI, "302AI Runtime");
mock_executor!(MockBrowserAdapter, ExecutorSourceType::Browser, "Browser Runtime");
mock_executor!(MockLocalProcessAdapter, ExecutorSourceType::LocalProcess, "Local Process");
mock_executor!(MockDockerAdapter, ExecutorSourceType::Docker, "Docker Runtime");
