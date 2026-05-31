use crate::agent_harness::RunAuthorization;
use crate::r#loop::{ActionProposal, GateOutcome, GovernGate, NetworkPolicy, SandboxProfile};
use crate::types::{Tool, ToolType};
use async_trait::async_trait;
use coevo_core::cognitive::CognitiveLayer;

#[derive(Debug, Clone)]
pub struct ExternalAgentTask {
    pub executor_id: String,
    pub task: serde_json::Value,
    pub sandbox_profile: SandboxProfile,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EgressAttempt {
    pub endpoint: String,
    pub allowed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExternalProducedItem {
    pub title: String,
    pub content: String,
    pub provenance: String,
    pub cognitive_layer: CognitiveLayer,
}

#[derive(Debug, Clone)]
pub struct ExternalAgentRunResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub produced_items: Vec<ExternalProducedItem>,
    pub side_effects: Vec<ActionProposal>,
    pub egress_log: Vec<EgressAttempt>,
    pub self_reported_trace: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SideEffectDecision {
    pub proposal: ActionProposal,
    pub outcome: GateOutcome,
}

#[derive(Debug, Clone)]
pub struct ExternalReturnFlowDecision {
    pub produced_items: Vec<ExternalProducedItem>,
    pub side_effects: Vec<SideEffectDecision>,
    pub egress_log: Vec<EgressAttempt>,
}

#[async_trait]
pub trait ExternalAgentAdapter: Send + Sync {
    fn executor_id(&self) -> &str;

    async fn run_in_sandbox(
        &self,
        task: ExternalAgentTask,
    ) -> Result<ExternalAgentRunResult, crate::error::WorkerError>;
}

pub struct ExternalAgentBoundary;

impl ExternalAgentBoundary {
    pub async fn adjudicate_return_flow(
        result: ExternalAgentRunResult,
        auth: &RunAuthorization,
        tools: &[Tool],
        govern_gate: &GovernGate,
    ) -> ExternalReturnFlowDecision {
        let produced_items = result
            .produced_items
            .into_iter()
            .map(|mut item| {
                item.cognitive_layer = CognitiveLayer::Hypothesis;
                item
            })
            .collect();
        let mut side_effects = Vec::with_capacity(result.side_effects.len());
        for proposal in result.side_effects {
            let outcome = govern_gate.adjudicate(&proposal, auth, tools).await;
            side_effects.push(SideEffectDecision { proposal, outcome });
        }
        ExternalReturnFlowDecision {
            produced_items,
            side_effects,
            egress_log: result.egress_log,
        }
    }

    pub fn egress_attempt(profile: &SandboxProfile, endpoint: &str) -> EgressAttempt {
        match &profile.network {
            NetworkPolicy::Blocked => EgressAttempt {
                endpoint: endpoint.to_string(),
                allowed: false,
                reason: "Network egress blocked by sandbox profile".to_string(),
            },
            NetworkPolicy::Proxied { allowed_endpoints } => {
                let allowed = allowed_endpoints.iter().any(|allowed| endpoint.starts_with(allowed));
                EgressAttempt {
                    endpoint: endpoint.to_string(),
                    allowed,
                    reason: if allowed {
                        "Allowed by sandbox egress allowlist".to_string()
                    } else {
                        "Endpoint not in sandbox egress allowlist".to_string()
                    },
                }
            }
            NetworkPolicy::Open => EgressAttempt {
                endpoint: endpoint.to_string(),
                allowed: true,
                reason: "Network egress open for this sandbox profile".to_string(),
            },
        }
    }
}

pub fn external_executor_tool(executor_id: &str) -> Tool {
    Tool {
        tool_id: executor_id.to_string(),
        name: executor_id.to_string(),
        tool_type: ToolType::ExternalExecutor,
        risk_ceiling: 0.6,
        supported_actions: vec!["read".to_string(), "execute".to_string()],
        permission_boundary_json: serde_json::json!({}),
        requires_credential: false,
        credential_ref: None,
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::r#loop::SandboxProfile;

    fn auth(restricted_actions: Vec<String>) -> RunAuthorization {
        RunAuthorization {
            work_order_id: "wo-external".to_string(),
            agent_id: "agent-founder-01".to_string(),
            worker_id: "worker-agent-founder-01".to_string(),
            session_id: "session-wo-external".to_string(),
            run_id: "run-external".to_string(),
            track: "green".to_string(),
            allowed_actions: vec!["read".to_string()],
            restricted_actions,
            approval_receipt: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            sandbox_profile: SandboxProfile::from_track("green", None),
        }
    }

    #[tokio::test]
    async fn external_side_effect_passes_govern_gate() {
        let result = ExternalAgentRunResult {
            success: true,
            output: serde_json::json!({}),
            produced_items: vec![],
            side_effects: vec![ActionProposal::CallTool {
                tool_id: "file-readonly".to_string(),
                input: serde_json::json!({"action":"ReadFile"}),
                rationale: "reported side effect".to_string(),
            }],
            egress_log: vec![],
            self_reported_trace: serde_json::json!([]),
        };
        let tool = Tool {
            tool_id: "file-readonly".to_string(),
            name: "File Readonly".to_string(),
            tool_type: ToolType::FileReadonly,
            risk_ceiling: 0.3,
            supported_actions: vec!["ReadFile".to_string()],
            permission_boundary_json: serde_json::json!({}),
            requires_credential: false,
            credential_ref: None,
            enabled: true,
        };

        let auth = auth(vec!["file-readonly".to_string()]);
        let gate = GovernGate::default_for_authorization(&auth);
        let decision = ExternalAgentBoundary::adjudicate_return_flow(
            result,
            &auth,
            &[tool],
            &gate,
        )
        .await;

        assert!(matches!(
            decision.side_effects[0].outcome,
            GateOutcome::Deny { .. }
        ));
    }

    #[tokio::test]
    async fn external_produced_items_remain_hypothesis_until_customs() {
        let result = ExternalAgentRunResult {
            success: true,
            output: serde_json::json!({}),
            produced_items: vec![ExternalProducedItem {
                title: "Claim".to_string(),
                content: "External agent claim".to_string(),
                provenance: "external-agent:self-report".to_string(),
                cognitive_layer: CognitiveLayer::Fact,
            }],
            side_effects: vec![],
            egress_log: vec![],
            self_reported_trace: serde_json::json!([]),
        };

        let auth = auth(vec![]);
        let gate = GovernGate::default_for_authorization(&auth);
        let decision =
            ExternalAgentBoundary::adjudicate_return_flow(result, &auth, &[], &gate).await;

        assert_eq!(
            decision.produced_items[0].cognitive_layer,
            CognitiveLayer::Hypothesis
        );
    }

    #[test]
    fn external_agent_egress_confined_by_empty_proxy_allowlist() {
        let profile = SandboxProfile::workspace_write(None);
        let attempt = ExternalAgentBoundary::egress_attempt(&profile, "https://example.com");

        assert!(!attempt.allowed);
        assert!(attempt.reason.contains("allowlist"));
    }
}
