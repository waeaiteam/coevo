use crate::agent_harness::{AgentRunContract, RunAuthorization};
use crate::error::WorkerError;
use crate::types::{MemoryContext, Tool};
use async_trait::async_trait;
use coevo_models::types::ModelMessage;
use sha2::{Digest, Sha256};

pub struct LoopContext<'a> {
    pub run_contract: &'a AgentRunContract,
    pub authorization: &'a RunAuthorization,
    pub memory_context: &'a MemoryContext,
    pub allowed_tools: &'a [&'a Tool],
    pub observation: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct PromptBundle {
    pub stable_prefix: Vec<ModelMessage>,
    pub volatile_suffix: Vec<ModelMessage>,
    pub prefix_fingerprint: String,
    pub estimated_tokens: u32,
}

impl PromptBundle {
    pub fn messages(&self) -> Vec<ModelMessage> {
        self.stable_prefix
            .iter()
            .chain(self.volatile_suffix.iter())
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct CompactedHistory {
    pub summary: ModelMessage,
    pub provenance: Vec<String>,
    pub dropped_message_count: usize,
}

#[async_trait]
pub trait ContextEngine: Send + Sync {
    async fn build_prompt(&self, ctx: &LoopContext<'_>) -> Result<PromptBundle, WorkerError>;

    async fn maybe_compact(
        &self,
        history: &[ModelMessage],
        token_budget: u32,
    ) -> Result<Option<CompactedHistory>, WorkerError> {
        let estimated = estimate_tokens(&[], history);
        if history.is_empty() || estimated <= token_budget {
            return Ok(None);
        }
        let content = history
            .iter()
            .map(|message| format!("{}: {}", message.role, message.content))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Some(CompactedHistory {
            summary: ModelMessage {
                role: "system".to_string(),
                content: format!(
                    "Compacted governed history summary ({} messages): {}",
                    history.len(),
                    content.chars().take(2000).collect::<String>()
                ),
            },
            provenance: vec!["compaction:memory-budget-v1".to_string()],
            dropped_message_count: history.len(),
        }))
    }

    fn engine_version(&self) -> String;
}

pub struct MemoryBudgetContextEngine;

#[async_trait]
impl ContextEngine for MemoryBudgetContextEngine {
    async fn build_prompt(&self, ctx: &LoopContext<'_>) -> Result<PromptBundle, WorkerError> {
        let governance_prefix = serde_json::json!({
            "principle": "Freedom in Reason, Governance in Action. The model proposes actions; GovernGate authorizes them.",
            "track": ctx.authorization.track,
            "allowed_actions": ctx.authorization.allowed_actions,
            "restricted_actions": ctx.authorization.restricted_actions,
            "approval_receipt_present": ctx.authorization.approval_receipt.is_some(),
            "contract_hash": ctx.authorization.contract_hash,
            "plan_hash": ctx.authorization.plan_hash,
            "sandbox_profile": ctx.authorization.sandbox_profile,
        });
        let tool_manifest = ctx
            .allowed_tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "tool_id": tool.tool_id,
                    "name": tool.name,
                    "tool_type": tool.tool_type,
                    "risk_ceiling": tool.risk_ceiling,
                    "supported_actions": tool.supported_actions,
                    "permission_boundary": tool.permission_boundary_json,
                })
            })
            .collect::<Vec<_>>();
        let memory_summary = serde_json::json!({
            "user_profile_loaded": ctx.memory_context.user_profile.is_some(),
            "company_profile": ctx.memory_context.company_profile,
            "company_memory": ctx.memory_context.company_memory,
            "agent_memory": ctx.memory_context.agent_memory,
            "task_memory": ctx.memory_context.task_memory,
            "excluded_revoked_count": ctx.memory_context.excluded_revoked_count,
            "excluded_fact_without_provenance": ctx.memory_context.fact_without_provenance,
        });
        let mut user_payload = serde_json::json!({
            "work_order_id": ctx.run_contract.work_order_id,
            "mission_intent": ctx.run_contract.mission_intent,
            "required_skills": ctx.run_contract.required_skills,
            "available_tools": tool_manifest,
            "memory_context": memory_summary,
            "required_output": "Return JSON matching ReasoningOutput. Choose exactly one proposal: call_tool, call_executor, ask_human, or finish.",
        });
        if let Some(observation) = ctx.observation {
            user_payload["previous_observation"] = serde_json::json!(observation);
        }
        let stable_prefix = vec![ModelMessage {
            role: "system".to_string(),
            content: governance_prefix.to_string(),
        }];
        let volatile_suffix = vec![ModelMessage {
            role: "user".to_string(),
            content: user_payload.to_string(),
        }];
        let prefix_fingerprint = fingerprint_messages(&stable_prefix);
        let estimated_tokens = estimate_tokens(&stable_prefix, &volatile_suffix);
        Ok(PromptBundle {
            stable_prefix,
            volatile_suffix,
            prefix_fingerprint,
            estimated_tokens,
        })
    }

    fn engine_version(&self) -> String {
        "memory-budget-v1".to_string()
    }
}

fn fingerprint_messages(messages: &[ModelMessage]) -> String {
    let mut hasher = Sha256::new();
    for message in messages {
        hasher.update(message.role.as_bytes());
        hasher.update([0]);
        hasher.update(message.content.as_bytes());
        hasher.update([0xff]);
    }
    hex::encode(hasher.finalize())
}

fn estimate_tokens(stable_prefix: &[ModelMessage], volatile_suffix: &[ModelMessage]) -> u32 {
    stable_prefix
        .iter()
        .chain(volatile_suffix.iter())
        .map(|message| (message.content.len() as u32).div_ceil(4))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::r#loop::SandboxProfile;

    fn memory_context() -> MemoryContext {
        MemoryContext {
            user_profile: None,
            company_profile: vec![],
            company_memory: vec![],
            agent_memory: vec![],
            task_memory: vec![],
            relevant_skill_memory: vec![],
            stale_memory_ids: vec![],
            excluded_revoked_count: 0,
            context_budget_chars: 0,
            fact_without_provenance: 0,
        }
    }

    fn contract() -> AgentRunContract {
        AgentRunContract {
            work_order_id: "wo-context".to_string(),
            mission_intent: "Analyze evidence".to_string(),
            required_skills: vec![],
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
        }
    }

    fn auth() -> RunAuthorization {
        RunAuthorization {
            work_order_id: "wo-context".to_string(),
            agent_id: "agent-founder-01".to_string(),
            worker_id: "worker-agent-founder-01".to_string(),
            session_id: "session-wo-context".to_string(),
            run_id: "run-context".to_string(),
            track: "green".to_string(),
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec!["delete".to_string()],
            approval_receipt: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            sandbox_profile: SandboxProfile::from_track("green", None),
            model_preference: None,
        }
    }

    #[tokio::test]
    async fn context_prefix_contains_track_actions_and_sandbox() {
        let engine = MemoryBudgetContextEngine;
        let memory = memory_context();
        let contract = contract();
        let auth = auth();
        let allowed_tools = vec![];
        let prompt = engine
            .build_prompt(&LoopContext {
                run_contract: &contract,
                authorization: &auth,
                memory_context: &memory,
                allowed_tools: &allowed_tools,
                observation: None,
            })
            .await
            .unwrap();

        let prefix = &prompt.stable_prefix[0].content;
        assert!(prefix.contains("\"track\":\"green\""));
        assert!(prefix.contains("\"allowed_actions\":[\"read\"]"));
        assert!(prefix.contains("\"restricted_actions\":[\"delete\"]"));
        assert!(prefix.contains("\"tier\":\"read_only\""));
    }

    #[tokio::test]
    async fn prefix_fingerprint_stable_across_round_observations() {
        let engine = MemoryBudgetContextEngine;
        let memory = memory_context();
        let contract = contract();
        let auth = auth();
        let allowed_tools = vec![];
        let first = engine
            .build_prompt(&LoopContext {
                run_contract: &contract,
                authorization: &auth,
                memory_context: &memory,
                allowed_tools: &allowed_tools,
                observation: None,
            })
            .await
            .unwrap();
        let second = engine
            .build_prompt(&LoopContext {
                run_contract: &contract,
                authorization: &auth,
                memory_context: &memory,
                allowed_tools: &allowed_tools,
                observation: Some("Tool result"),
            })
            .await
            .unwrap();

        assert_eq!(first.prefix_fingerprint, second.prefix_fingerprint);
        assert_ne!(
            first.volatile_suffix[0].content,
            second.volatile_suffix[0].content
        );
    }

    #[tokio::test]
    async fn compaction_preserves_provenance() {
        let engine = MemoryBudgetContextEngine;
        let history = vec![ModelMessage {
            role: "user".to_string(),
            content: "x".repeat(200),
        }];

        let compacted = engine.maybe_compact(&history, 1).await.unwrap().unwrap();

        assert!(!compacted.provenance.is_empty());
        assert_eq!(compacted.dropped_message_count, 1);
        assert!(compacted
            .summary
            .content
            .contains("Compacted governed history"));
    }

    #[tokio::test]
    async fn compaction_does_not_change_prefix() {
        let engine = MemoryBudgetContextEngine;
        let memory = memory_context();
        let contract = contract();
        let auth = auth();
        let allowed_tools = vec![];
        let before = engine
            .build_prompt(&LoopContext {
                run_contract: &contract,
                authorization: &auth,
                memory_context: &memory,
                allowed_tools: &allowed_tools,
                observation: None,
            })
            .await
            .unwrap();
        let history = vec![ModelMessage {
            role: "assistant".to_string(),
            content: "large prior observation".repeat(40),
        }];
        let compacted = engine.maybe_compact(&history, 1).await.unwrap().unwrap();
        let after = engine
            .build_prompt(&LoopContext {
                run_contract: &contract,
                authorization: &auth,
                memory_context: &memory,
                allowed_tools: &allowed_tools,
                observation: Some(&compacted.summary.content),
            })
            .await
            .unwrap();

        assert_eq!(before.stable_prefix.len(), after.stable_prefix.len());
        assert_eq!(before.stable_prefix[0].role, after.stable_prefix[0].role);
        assert_eq!(
            before.stable_prefix[0].content,
            after.stable_prefix[0].content
        );
        assert_eq!(before.prefix_fingerprint, after.prefix_fingerprint);
    }
}
