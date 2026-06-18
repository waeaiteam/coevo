use crate::agent_harness::RunAuthorization;
use crate::r#loop::proposal::ActionProposal;
use crate::tool_policy::ToolPolicyEngine;
use crate::types::Tool;
use coevo_core::contract::{
    ActionMode, ApprovalMode, ContractState, EvidenceRequirement, GoalNode, GoalStatus, GoalTree,
    HumanApprovalPolicy, MCLSpec, ResponsibilityAnchorPolicy, RiskToleranceProfile,
    TerminationPolicy,
};
use coevo_core::decision::{ActionProposalSpec, GateDecision};
use coevo_policy::mock::MockPolicyEngine;
use coevo_policy::traits::{PolicyEngine, PolicyEngineError, PolicyResult, PolicyViolation};
use coevo_risk::decision_tree::RiskGate;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    Allow,
    Deny {
        reason: String,
    },
    NeedApproval {
        reason: String,
        action_digest: String,
    },
}

pub struct GovernGate {
    risk_gate: RiskGate,
    contract: MCLSpec,
}

impl GovernGate {
    pub fn new(risk_gate: RiskGate, contract: MCLSpec) -> Self {
        Self {
            risk_gate,
            contract,
        }
    }

    pub fn default_for_authorization(auth: &RunAuthorization) -> Self {
        let policy_engine: Box<dyn PolicyEngine> = if mock_policy_engine_enabled() {
            Box::new(MockPolicyEngine::default())
        } else {
            Box::new(DenyAllPolicyEngine)
        };
        Self::new(
            RiskGate::new(policy_engine),
            risk_contract_from_authorization(auth),
        )
    }

    pub async fn adjudicate(
        &self,
        proposal: &ActionProposal,
        auth: &RunAuthorization,
        tools: &[Tool],
    ) -> GateOutcome {
        let (tool, tool_policy) = match self.tool_policy_outcome(proposal, auth, tools) {
            Ok(value) => value,
            Err(outcome) => return outcome,
        };
        if let Some(outcome) = tool_policy {
            return outcome;
        }

        let Some(tool) = tool else {
            return GateOutcome::Allow;
        };
        let risk_proposal = action_proposal_spec(proposal, tool);
        let risk_decision = self
            .risk_gate
            .evaluate(
                &risk_proposal,
                &self.contract,
                &[1.0],
                &[1.0],
                &[],
                &[],
                0.0,
                0.0,
                0.0,
                false,
            )
            .await;

        match risk_decision.decision {
            GateDecision::Allow | GateDecision::AllowWithLease => GateOutcome::Allow,
            GateDecision::RequireHumanApproval => GateOutcome::NeedApproval {
                reason: format!("RiskGate requires approval: {}", risk_decision.reason),
                action_digest: action_digest(proposal),
            },
            GateDecision::Deny
            | GateDecision::DeferForMoreEvidence
            | GateDecision::EscalateToResolution => GateOutcome::Deny {
                reason: format!("RiskGate blocked action: {}", risk_decision.reason),
            },
        }
    }

    pub fn adjudicate_readonly(
        proposal: &ActionProposal,
        auth: &RunAuthorization,
        tools: &[Tool],
    ) -> GateOutcome {
        match proposal {
            ActionProposal::Finish { .. } => GateOutcome::Allow,
            // Subagent spawn is a delegation, not a resource action: the harness enforces
            // the skill-authorization guard. The gate allows it to proceed.
            ActionProposal::SpawnSubagent { .. } => GateOutcome::Allow,
            ActionProposal::AskHuman { question, .. } => GateOutcome::NeedApproval {
                reason: format!("Human input required: {question}"),
                action_digest: "ask-human".to_string(),
            },
            ActionProposal::CallExecutor { executor_id, .. } => {
                let Some(tool) = tools.iter().find(|tool| tool.tool_id == *executor_id) else {
                    return GateOutcome::Deny {
                        reason: format!("Executor {executor_id} is not registered for this run"),
                    };
                };
                Self::from_tool_policy(tool, auth)
            }
            ActionProposal::CallTool { tool_id, .. } => {
                let Some(tool) = tools.iter().find(|tool| tool.tool_id == *tool_id) else {
                    return GateOutcome::Deny {
                        reason: format!("Tool {tool_id} is not registered for this run"),
                    };
                };
                Self::from_tool_policy(tool, auth)
            }
        }
    }

    fn tool_policy_outcome<'a>(
        &self,
        proposal: &ActionProposal,
        auth: &RunAuthorization,
        tools: &'a [Tool],
    ) -> Result<(Option<&'a Tool>, Option<GateOutcome>), GateOutcome> {
        match proposal {
            ActionProposal::Finish { .. } => Ok((None, None)),
            // Subagent spawn polices no tool; the harness applies the skill guard.
            ActionProposal::SpawnSubagent { .. } => Ok((None, None)),
            ActionProposal::AskHuman { question, .. } => Ok((
                None,
                Some(GateOutcome::NeedApproval {
                    reason: format!("Human input required: {question}"),
                    action_digest: action_digest(proposal),
                }),
            )),
            ActionProposal::CallExecutor { executor_id, .. } => {
                let Some(tool) = tools.iter().find(|tool| tool.tool_id == *executor_id) else {
                    return Err(GateOutcome::Deny {
                        reason: format!("Executor {executor_id} is not registered for this run"),
                    });
                };
                Ok((Some(tool), Self::tool_policy_decision(tool, auth)))
            }
            ActionProposal::CallTool { tool_id, .. } => {
                let Some(tool) = tools.iter().find(|tool| tool.tool_id == *tool_id) else {
                    return Err(GateOutcome::Deny {
                        reason: format!("Tool {tool_id} is not registered for this run"),
                    });
                };
                Ok((Some(tool), Self::tool_policy_decision(tool, auth)))
            }
        }
    }

    fn tool_policy_decision(tool: &Tool, auth: &RunAuthorization) -> Option<GateOutcome> {
        let decision = ToolPolicyEngine::evaluate(
            tool,
            &auth.track,
            &auth.allowed_actions,
            &auth.restricted_actions,
        );
        if !decision.allowed {
            return Some(GateOutcome::Deny {
                reason: decision.reason,
            });
        }
        if decision.required_approval && auth.approval_receipt.is_none() {
            return Some(GateOutcome::NeedApproval {
                reason: decision.reason,
                action_digest: action_digest_for_tool(tool),
            });
        }
        None
    }

    fn from_tool_policy(tool: &Tool, auth: &RunAuthorization) -> GateOutcome {
        if let Some(outcome) = Self::tool_policy_decision(tool, auth) {
            return outcome;
        }
        GateOutcome::Allow
    }
}

fn mock_policy_engine_enabled() -> bool {
    // `cfg!(test)` covers this crate's own unit tests. Integration/acceptance suites
    // and dev runs opt in explicitly via COEVO_ENABLE_MOCK_POLICY_ENGINE=1. A plain
    // (non-release) debug build of the *binary* must NOT silently authorize work —
    // mirrors the fail-closed default in coevo-policy / coevo-tracks.
    cfg!(test)
        || matches!(
            std::env::var("COEVO_ENABLE_MOCK_POLICY_ENGINE"),
            Ok(value) if value == "1"
        )
}

fn action_proposal_spec(proposal: &ActionProposal, tool: &Tool) -> ActionProposalSpec {
    let (action_name, parameters) = match proposal {
        ActionProposal::CallTool { input, .. } => (
            input
                .get("action")
                .and_then(|value| value.as_str())
                .unwrap_or("call_tool")
                .to_string(),
            input.clone(),
        ),
        ActionProposal::CallExecutor { task, .. } => ("call_executor".to_string(), task.clone()),
        ActionProposal::SpawnSubagent { skill_id, task, .. } => (
            "spawn_subagent".to_string(),
            serde_json::json!({ "skill_id": skill_id, "task": task }),
        ),
        ActionProposal::Finish { result, .. } => ("finish".to_string(), result.clone()),
        ActionProposal::AskHuman { .. } => ("ask_human".to_string(), serde_json::json!({})),
    };
    let (br, ir, es, rv) = risk_factors_for_tool(tool);
    ActionProposalSpec {
        action_urn: format!("urn:coevo:action:{}:{action_name}", tool.tool_id),
        target_environment: "workspace".to_string(),
        parameters,
        emergency_mode: false,
        blast_radius: br,
        irreversibility: ir,
        environment_sensitivity: es,
        reversibility: rv,
    }
}

fn risk_factors_for_tool(tool: &Tool) -> (u8, u8, u8, u8) {
    match tool.tool_type {
        crate::types::ToolType::GitHubReadonly | crate::types::ToolType::FileReadonly => {
            (0, 0, 0, 0)
        }
        crate::types::ToolType::BrowserMock | crate::types::ToolType::MCPMock => (1, 1, 1, 1),
        crate::types::ToolType::Mcp | crate::types::ToolType::Browser => (1, 1, 1, 1),
        crate::types::ToolType::LocalProcessSandbox | crate::types::ToolType::ExternalExecutor => {
            (2, 2, 1, 2)
        }
    }
}

fn action_digest(proposal: &ActionProposal) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(proposal).unwrap_or_default());
    hex::encode(hasher.finalize())
}

fn action_digest_for_tool(tool: &Tool) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool.tool_id.as_bytes());
    hex::encode(hasher.finalize())
}

fn risk_contract_from_authorization(auth: &RunAuthorization) -> MCLSpec {
    MCLSpec {
        mcl_version: "1.0".to_string(),
        mcl_state: ContractState::ActiveContract,
        parent_contract_hash: auth.contract_hash.clone(),
        goal_tree: GoalTree {
            root: GoalNode {
                id: auth.work_order_id.clone(),
                description: format!("Worker run {}", auth.run_id),
                status: GoalStatus::InProgress,
                children: vec![],
                depends_on: vec![],
            },
        },
        institution_policy_hash: auth.plan_hash.clone(),
        data_boundary: vec!["urn:coevo:data:workspace".to_string()],
        allowed_action_modes: allowed_action_modes(&auth.allowed_actions),
        human_approval_policy: HumanApprovalPolicy {
            approval_mode: ApprovalMode::ExplicitApproval,
            authorized_roles: vec!["founder".to_string()],
            negative_consent_timeout_secs: 0,
            mfa_auth_url: None,
        },
        evidence_requirement: EvidenceRequirement {
            minimum_level: "self_report".to_string(),
            require_json_report: false,
        },
        risk_tolerance_profile: RiskToleranceProfile {
            max_risk_score: match auth.track.as_str() {
                "red" => 0.0,
                "yellow" => 0.6,
                _ => 0.3,
            },
            allow_emergency_lease: false,
        },
        termination_policy: TerminationPolicy {
            max_token_budget: 64_000,
            max_hops: 16,
            max_latency_ms: 120_000,
            max_stance_rounds: 16,
        },
        responsibility_anchor_policy: ResponsibilityAnchorPolicy {
            required_human_roles: vec!["founder".to_string()],
            agent_forbidden_actions: auth.restricted_actions.clone(),
        },
    }
}

fn allowed_action_modes(allowed_actions: &[String]) -> Vec<ActionMode> {
    let mut modes = vec![ActionMode::DraftOnly];
    if allowed_actions.iter().any(|action| {
        let lower = action.to_ascii_lowercase();
        lower.contains("write") || lower.contains("mutate")
    }) {
        modes.push(ActionMode::MutableWrite);
    }
    if allowed_actions.iter().any(|action| {
        let lower = action.to_ascii_lowercase();
        lower.contains("commit") || lower.contains("execute")
    }) {
        modes.push(ActionMode::CommitReady);
    }
    modes
}

struct DenyAllPolicyEngine;

#[async_trait::async_trait]
impl PolicyEngine for DenyAllPolicyEngine {
    async fn validate_contract(
        &self,
        _contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError> {
        Ok(PolicyResult {
            passed: false,
            violations: vec![PolicyViolation {
                policy_urn: "urn:coevo:policy:unavailable".to_string(),
                description: "policy engine unavailable".to_string(),
                remediation: Some("set COEVO_ENABLE_MOCK_POLICY_ENGINE=1 for dev/test".to_string()),
            }],
            policy_version: "unavailable".to_string(),
            policies_checked: vec!["urn:coevo:policy:unavailable".to_string()],
        })
    }

    async fn dry_run(&self, contract: &MCLSpec) -> Result<PolicyResult, PolicyEngineError> {
        self.validate_contract(contract).await
    }

    fn policy_version(&self) -> String {
        "unavailable".to_string()
    }

    async fn evaluate_action(
        &self,
        action_urn: &str,
        _contract: &MCLSpec,
    ) -> Result<PolicyResult, PolicyEngineError> {
        Ok(PolicyResult {
            passed: false,
            violations: vec![PolicyViolation {
                policy_urn: action_urn.to_string(),
                description: "policy engine unavailable".to_string(),
                remediation: Some("set COEVO_ENABLE_MOCK_POLICY_ENGINE=1 for dev/test".to_string()),
            }],
            policy_version: "unavailable".to_string(),
            policies_checked: vec![action_urn.to_string()],
        })
    }

    async fn diff_policies(
        &self,
        _old_version: &str,
        _new_version: &str,
    ) -> Result<coevo_policy::traits::PolicyDiff, PolicyEngineError> {
        Err(PolicyEngineError::Internal(
            "policy engine unavailable".to_string(),
        ))
    }

    async fn health_check(&self) -> Result<bool, PolicyEngineError> {
        Ok(false)
    }

    async fn rollback(&mut self, _target_version: &str) -> Result<(), PolicyEngineError> {
        Err(PolicyEngineError::Internal(
            "policy engine unavailable".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::r#loop::SandboxProfile;
    use crate::types::{Tool, ToolType};

    fn auth(track: &str, approval_receipt: Option<String>) -> RunAuthorization {
        RunAuthorization {
            work_order_id: "wo-gate".to_string(),
            agent_id: "agent-founder-01".to_string(),
            worker_id: "worker-agent-founder-01".to_string(),
            session_id: "session-wo-gate".to_string(),
            run_id: "run-gate".to_string(),
            track: track.to_string(),
            allowed_actions: vec!["read".to_string(), "execute".to_string()],
            restricted_actions: vec![],
            approval_receipt,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            sandbox_profile: SandboxProfile::from_track(track, None),
            model_preference: None,
        }
    }

    fn external_tool() -> Tool {
        Tool {
            tool_id: "external-read".to_string(),
            name: "External Read".to_string(),
            tool_type: ToolType::ExternalExecutor,
            risk_ceiling: 0.8,
            supported_actions: vec!["execute".to_string()],
            permission_boundary_json: serde_json::json!({}),
            requires_credential: false,
            credential_ref: None,
            enabled: true,
        }
    }

    #[test]
    fn required_approval_without_receipt_returns_need_approval() {
        let proposal = ActionProposal::CallTool {
            tool_id: "external-read".to_string(),
            input: serde_json::json!({}),
            rationale: "external boundary".to_string(),
        };
        let outcome =
            GovernGate::adjudicate_readonly(&proposal, &auth("yellow", None), &[external_tool()]);

        assert!(matches!(outcome, GateOutcome::NeedApproval { .. }));
    }

    #[test]
    fn red_model_proposal_call_executor_is_denied() {
        let proposal = ActionProposal::CallExecutor {
            executor_id: "external-read".to_string(),
            task: serde_json::json!({}),
            rationale: "try executor".to_string(),
        };
        let outcome =
            GovernGate::adjudicate_readonly(&proposal, &auth("red", None), &[external_tool()]);

        assert!(matches!(outcome, GateOutcome::Deny { .. }));
    }

    #[tokio::test]
    async fn risk_gate_denies_forbidden_policy_action() {
        let proposal = ActionProposal::CallTool {
            tool_id: "external-read".to_string(),
            input: serde_json::json!({"action":"call_tool"}),
            rationale: "forbidden by institution policy".to_string(),
        };
        let auth = auth("green", Some("receipt".to_string()));
        let gate = GovernGate::new(
            RiskGate::new(Box::new(
                MockPolicyEngine::default().with_forbidden_actions(vec![
                    "urn:coevo:action:external-read:call_tool".to_string(),
                ]),
            )),
            risk_contract_from_authorization(&auth),
        );
        let outcome = gate.adjudicate(&proposal, &auth, &[external_tool()]).await;

        assert!(matches!(outcome, GateOutcome::Deny { .. }));
    }
}
