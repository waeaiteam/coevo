use crate::error::WorkerError;
use crate::memory_context::MemoryContextBuilder;
use crate::reflection::ReflectionEngine;
use crate::r#loop::{
    external_executor_tool, ActionProposal, ContextEngine, ExternalAgentAdapter,
    ExternalAgentBoundary, ExternalAgentTask, GateOutcome, GovernGate, LoopContext,
    MemoryBudgetContextEngine, ReasoningOutput, SandboxProfile,
    SandboxFilesystemGuard,
};
use crate::self_upgrade::SelfUpgradeLoop;
use crate::skill_runtime::SkillRuntime;
use crate::tool_policy::ToolPolicyEngine;
use crate::tool_registry::ToolRegistry;
use crate::types::WorkerRun;
use coevo_core::cognitive::CognitiveLayer;
use coevo_core::opc::{MemoryRecord, MemoryScope, MemoryStatus};
use coevo_models::gateway::ModelGateway;
use coevo_models::router::{
    required_capabilities_for_step, ModelCapability, ModelProfile, ModelRouter, ModelRoutingDecision,
    ModelRoutingRequest, PrivacyLevel,
};
use coevo_models::types::{ModelMessage, ModelProviderConfig, ModelRequest, ResponseFormat};
use coevo_store::repos::worker_run_repo::{WorkerEventRepo, WorkerSkillUsageRepo, WorkerToolCallRepo};
use coevo_store::repos_opc::memory_repo;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct RunAuthorization {
    pub work_order_id: String,
    pub agent_id: String,
    pub worker_id: String,
    pub session_id: String,
    pub run_id: String,
    pub track: String,
    pub allowed_actions: Vec<String>,
    pub restricted_actions: Vec<String>,
    pub approval_receipt: Option<String>,
    pub contract_hash: String,
    pub plan_hash: String,
    pub sandbox_profile: SandboxProfile,
}

#[derive(Debug, Clone)]
pub struct AgentRunContract {
    pub work_order_id: String,
    pub mission_intent: String,
    pub required_skills: Vec<String>,
    pub user_id: String,
    pub opc_id: String,
}

pub struct AgentSubHarnessResult {
    pub final_status: String,
    pub summary: String,
    pub memory_ids: Vec<String>,
    pub reflection_id: Option<String>,
    pub proposal_id: Option<String>,
}

pub struct AgentSubHarness;

impl AgentSubHarness {
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        pool: &SqlitePool,
        run_contract: &AgentRunContract,
        authorization: &RunAuthorization,
        model_profiles: &[ModelProfile],
        max_runtime_ms: Option<i64>,
        gateway: &dyn ModelGateway,
        provider_config: &ModelProviderConfig,
        external_agents: &[&dyn ExternalAgentAdapter],
    ) -> Result<AgentSubHarnessResult, WorkerError> {
        let now = || chrono::Utc::now().timestamp_millis();
        let mut steps: Vec<serde_json::Value> = vec![];
        let mut memory_ids: Vec<String> = vec![];

        let mem_ctx = MemoryContextBuilder::build(
            pool,
            &authorization.agent_id,
            &run_contract.user_id,
            &run_contract.opc_id,
            &run_contract.work_order_id,
            &authorization.contract_hash,
            &authorization.plan_hash,
        )
        .await?;
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "BuildContext",
            &serde_json::json!({
                "intent": run_contract.mission_intent,
                "sandbox_profile": authorization.sandbox_profile,
            }),
            None,
        )
        .await?;
        step_create(pool, &mut steps, &authorization.run_id, "LoadMemory", &serde_json::json!({
            "user_profile_loaded":mem_ctx.user_profile.is_some(),"company_profile_loaded":!mem_ctx.company_profile.is_empty(),
            "company_memory_count":mem_ctx.company_memory.len(),"agent_memory_count":mem_ctx.agent_memory.len(),
            "task_memory_count":mem_ctx.task_memory.len(),"stale_memory_ids":mem_ctx.stale_memory_ids.len(),
            "excluded_revoked_count":mem_ctx.excluded_revoked_count,
            "excluded_fact_without_provenance":mem_ctx.fact_without_provenance
        }), None).await?;

        let index = SkillRuntime::load_skill_index(pool, &authorization.agent_id).await?;
        let selected =
            SkillRuntime::select_relevant(&run_contract.mission_intent, &run_contract.required_skills, &index);
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "LoadSkillIndex",
            &serde_json::json!({"skills_found":index.len(),"selected":selected}),
            None,
        )
        .await?;
        for sid in &selected {
            if let Some(_full) = SkillRuntime::load_full(pool, sid).await? {
                step_create(
                    pool,
                    &mut steps,
                    &authorization.run_id,
                    "LoadSkillFull",
                    &serde_json::json!({"loaded_skill":sid}),
                    None,
                )
                .await?;
                WorkerSkillUsageRepo::create(
                    pool,
                    &format!("su-{}", uuid::Uuid::new_v4()),
                    &authorization.run_id,
                    sid,
                    "1.0.0",
                    "execution",
                    true,
                    0.9,
                    "",
                    now(),
                )
                .await
                .map_err(|e| WorkerError::Internal(e.to_string()))?;
            }
        }

        let registry = ToolRegistry::default_registry();
        let mut all_tools = registry.list().to_vec();
        all_tools.extend(
            external_agents
                .iter()
                .map(|adapter| external_executor_tool(adapter.executor_id())),
        );
        let allowed = ToolPolicyEngine::filter(
            &all_tools,
            &authorization.track,
            &authorization.allowed_actions,
            &authorization.restricted_actions,
        );
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "SelectTool",
            &serde_json::json!({
                "allowed_tools": allowed
                    .iter()
                    .map(|tool| serde_json::json!({
                        "tool_id": tool.tool_id,
                        "supported_actions": tool.supported_actions,
                    }))
                    .collect::<Vec<_>>()
            }),
            None,
        )
        .await?;

        let mut tool_failed = false;
        let mut last_tool_summary = String::new();
        let schema = serde_json::to_value(schemars::schema_for!(ReasoningOutput))
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let mut observation: Option<String> = None;
        let mut finished = false;
        let mut waiting_approval = false;
        let mut timed_out = false;
        let mut blocked = false;
        let mut consecutive_denials = 0usize;
        let max_rounds = 16usize;
        let context_engine = MemoryBudgetContextEngine;
        let govern_gate = GovernGate::default_for_authorization(authorization);
        let started_at_ms = now();
        let mut loop_history: Vec<ModelMessage> = vec![];
        if let Some(resume_observation) = load_resume_cursor(pool, authorization).await? {
            loop_history.push(ModelMessage {
                role: "system".to_string(),
                content: resume_observation.clone(),
            });
            observation = Some(resume_observation);
        }
        let mut total_prompt_tokens = 0u64;
        let mut total_completion_tokens = 0u64;
        let mut total_tokens = 0u64;

        for round in 0..max_rounds {
            if let Some(max_runtime_ms) = max_runtime_ms {
                if now().saturating_sub(started_at_ms) >= max_runtime_ms {
                    timed_out = true;
                    last_tool_summary = format!(
                        "Controlled ReAct loop reached max_runtime_ms={max_runtime_ms}"
                    );
                    break;
                }
            }
            let mut caps = required_capabilities_for_step("Think", &run_contract.mission_intent);
            caps.push(ModelCapability::StructuredJSON);
            caps.push(ModelCapability::ToolPlanning);
            caps.sort_by_key(|cap| format!("{cap:?}"));
            caps.dedup();
            let routing = route_for_step(
                run_contract,
                authorization,
                "ModelCall",
                caps,
                model_profiles,
                max_runtime_ms.map(|m| m as u64),
            );
            let prompt = context_engine
                .build_prompt(&LoopContext {
                    run_contract,
                    authorization,
                    memory_context: &mem_ctx,
                    allowed_tools: &allowed,
                    observation: observation.as_deref(),
                })
                .await?;
            let history_budget = provider_config
                .max_tokens
                .saturating_sub(prompt.estimated_tokens)
                .max(1);
            let compacted_history = context_engine.maybe_compact(&loop_history, history_budget).await?;
            let request_messages = if let Some(compacted) = &compacted_history {
                let mut messages = prompt.stable_prefix.clone();
                messages.push(compacted.summary.clone());
                messages.extend(prompt.volatile_suffix.clone());
                messages
            } else {
                prompt.messages()
            };
            let request = ModelRequest {
                config: provider_config.clone(),
                role: coevo_models::types::ModelRole::AgentReasoning,
                model: routing.selected_model_id.clone(),
                messages: request_messages,
                temperature: provider_config.temperature,
                max_tokens: provider_config.max_tokens,
                response_format: ResponseFormat::Json,
            };
            let response = gateway
                .structured(&request, &schema)
                .await
                .map_err(|e| WorkerError::Internal(e.to_string()))?;
            let reasoning: ReasoningOutput = serde_json::from_value(
                response
                    .json
                    .ok_or_else(|| WorkerError::Internal("structured response did not include json".into()))?,
            )
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
            total_prompt_tokens += response.usage.prompt_tokens;
            total_completion_tokens += response.usage.completion_tokens;
            total_tokens += response.usage.total_tokens;
            loop_history.push(ModelMessage {
                role: "assistant".to_string(),
                content: serde_json::to_string(&reasoning).unwrap_or_default(),
            });

            let gate = govern_gate
                .adjudicate(&reasoning.proposal, authorization, &all_tools)
                .await;
            let mut model_output = serde_json::to_value(&routing).unwrap_or_default();
            if let Some(obj) = model_output.as_object_mut() {
                obj.insert("round".into(), serde_json::json!(round));
                obj.insert("thought".into(), serde_json::json!(reasoning.thought));
                obj.insert("proposal".into(), serde_json::to_value(&reasoning.proposal).unwrap_or_default());
                obj.insert("confidence".into(), serde_json::json!(reasoning.confidence));
                obj.insert("usage".into(), serde_json::to_value(&response.usage).unwrap_or_default());
                obj.insert(
                    "usage_total".into(),
                    serde_json::json!({
                        "prompt_tokens": total_prompt_tokens,
                        "completion_tokens": total_completion_tokens,
                        "total_tokens": total_tokens,
                    }),
                );
                obj.insert(
                    "context".into(),
                    serde_json::json!({
                        "engine_version": context_engine.engine_version(),
                        "prefix_fingerprint": prompt.prefix_fingerprint,
                        "estimated_tokens": prompt.estimated_tokens,
                        "compaction": compacted_history.as_ref().map(|compacted| serde_json::json!({
                            "provenance": compacted.provenance,
                            "dropped_message_count": compacted.dropped_message_count,
                        })),
                    }),
                );
                obj.insert("gate".into(), gate_to_json(&gate));
            }
            step_create(
                pool,
                &mut steps,
                &authorization.run_id,
                "ModelCall",
                &serde_json::json!({"intent":run_contract.mission_intent,"round":round}),
                Some(&model_output),
            )
            .await?;

            match gate {
                GateOutcome::Deny { reason } => {
                    consecutive_denials += 1;
                    WorkerEventRepo::append(
                        pool,
                        &authorization.run_id,
                        "WorkerBlocked",
                        &serde_json::to_string(&serde_json::json!({
                            "round": round,
                            "reason": reason,
                        }))
                        .unwrap(),
                    )
                    .await
                    .map_err(|e| WorkerError::Internal(e.to_string()))?;
                    let next_observation = format!(
                        "Governance denied the previous proposal: {reason}. Choose a legal action."
                    );
                    loop_history.push(ModelMessage {
                        role: "system".to_string(),
                        content: next_observation.clone(),
                    });
                    observation = Some(next_observation);
                    if consecutive_denials >= 3 {
                        blocked = true;
                        last_tool_summary = format!(
                            "Governance blocked after {consecutive_denials} consecutive denied proposals: {reason}"
                        );
                        break;
                    }
                    continue;
                }
                GateOutcome::NeedApproval {
                    reason,
                    action_digest,
                } => {
                    persist_loop_cursor(pool, authorization, round, &reason, &action_digest).await?;
                    WorkerEventRepo::append(
                        pool,
                        &authorization.run_id,
                        "ApprovalRequired",
                        &serde_json::to_string(&serde_json::json!({
                            "round": round,
                            "reason": reason,
                            "action_digest": action_digest,
                        }))
                        .unwrap(),
                    )
                    .await
                    .map_err(|e| WorkerError::Internal(e.to_string()))?;
                    waiting_approval = true;
                    last_tool_summary = format!("Approval required: {reason}");
                    break;
                }
                GateOutcome::Allow => {
                    consecutive_denials = 0;
                    match reasoning.proposal {
                    ActionProposal::Finish { summary, result } => {
                        last_tool_summary = format!(
                            "Model finished: {}\nResult: {}",
                            summary,
                            serde_json::to_string(&result).unwrap_or_default()
                        );
                        finished = true;
                        break;
                    }
                    ActionProposal::CallTool { tool_id, input, .. } => {
                        WorkerEventRepo::append(
                            pool,
                            &authorization.run_id,
                            "ToolStart",
                            &serde_json::to_string(&serde_json::json!({"tool_id":tool_id,"round":round}))
                                .unwrap(),
                        )
                        .await
                        .map_err(|e| WorkerError::Internal(e.to_string()))?;

                        let tool_result = registry
                            .execute(&tool_id, input.clone())
                            .await
                            .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
                        let success = tool_result.get("error").is_none();
                        let output_str = serde_json::to_string(&tool_result).unwrap_or_default();
                        step_create(
                            pool,
                            &mut steps,
                            &authorization.run_id,
                            "CallTool",
                            &serde_json::json!({"tool_id":tool_id,"round":round,"input":input}),
                            Some(&tool_result),
                        )
                        .await?;
                        let tool_type = match tool_id.as_str() {
                            "github-readonly" => "GitHubReadonly",
                            "file-readonly" => "FileReadonly",
                            _ => tool_id.as_str(),
                        };
                        WorkerToolCallRepo::create(
                            pool,
                            &format!("tc-{}", uuid::Uuid::new_v4()),
                            &authorization.run_id,
                            &tool_id,
                            tool_type,
                            &format!("{} execution", tool_id),
                            &output_str.chars().take(500).collect::<String>(),
                            success,
                            0.5,
                            None,
                            now(),
                            Some(now()),
                        )
                        .await
                        .map_err(|e| WorkerError::Internal(e.to_string()))?;
                        WorkerEventRepo::append(
                            pool,
                            &authorization.run_id,
                            "ToolEnd",
                            &serde_json::to_string(&serde_json::json!({
                                "tool_id":tool_id,
                                "round":round,
                                "success":success
                            }))
                            .unwrap(),
                        )
                        .await
                        .map_err(|e| WorkerError::Internal(e.to_string()))?;

                        last_tool_summary = output_str.chars().take(1000).collect::<String>();
                        let next_observation = format!(
                            "Tool {tool_id} completed with success={success}. Observation: {}",
                            output_str.chars().take(2000).collect::<String>()
                        );
                        loop_history.push(ModelMessage {
                            role: "tool".to_string(),
                            content: next_observation.clone(),
                        });
                        observation = Some(next_observation);
                    }
                    ActionProposal::CallExecutor {
                        executor_id,
                        task,
                        ..
                    } => {
                        let Some(adapter) = external_agents
                            .iter()
                            .find(|adapter| adapter.executor_id() == executor_id)
                        else {
                            WorkerEventRepo::append(
                                pool,
                                &authorization.run_id,
                                "WorkerBlocked",
                                &serde_json::to_string(&serde_json::json!({
                                    "round": round,
                                    "reason": format!("External executor {executor_id} has no adapter bound")
                                }))
                                .unwrap(),
                            )
                            .await
                            .map_err(|e| WorkerError::Internal(e.to_string()))?;
                            let next_observation = format!(
                                "Governance could not execute external executor {executor_id}: no adapter bound."
                            );
                            loop_history.push(ModelMessage {
                                role: "system".to_string(),
                                content: next_observation.clone(),
                            });
                            observation = Some(next_observation);
                            continue;
                        };

                        WorkerEventRepo::append(
                            pool,
                            &authorization.run_id,
                            "ToolStart",
                            &serde_json::to_string(&serde_json::json!({
                                "tool_id": executor_id,
                                "round": round,
                                "sandbox_profile": authorization.sandbox_profile,
                            }))
                            .unwrap(),
                        )
                        .await
                        .map_err(|e| WorkerError::Internal(e.to_string()))?;

                        let external_task = ExternalAgentTask {
                            executor_id: executor_id.clone(),
                            task: task.clone(),
                            sandbox_profile: authorization.sandbox_profile.clone(),
                        };
                        let _sandbox_guard = SandboxFilesystemGuard::enter(&authorization.sandbox_profile)
                            .map_err(|e| WorkerError::Internal(format!("sandbox guard failed: {e}")))?;
                        let run_result = match adapter.run_in_sandbox(external_task).await {
                            Ok(result) => result,
                            Err(err) => {
                                let output = serde_json::json!({"error": err.to_string()});
                                step_create(
                                    pool,
                                    &mut steps,
                                    &authorization.run_id,
                                    "CallExecutor",
                                    &serde_json::json!({
                                        "executor_id": executor_id,
                                        "round": round,
                                        "task": task,
                                        "sandbox_profile": authorization.sandbox_profile,
                                    }),
                                    Some(&output),
                                )
                                .await?;
                                let next_observation = format!(
                                    "External executor {executor_id} failed: {err}. Choose a legal recovery action."
                                );
                                loop_history.push(ModelMessage {
                                    role: "tool".to_string(),
                                    content: next_observation.clone(),
                                });
                                observation = Some(next_observation);
                                continue;
                            }
                        };
                        let success = run_result.success;
                        let output = run_result.output.clone();
                        let self_reported_trace = run_result.self_reported_trace.clone();
                        let return_flow = ExternalAgentBoundary::adjudicate_return_flow(
                            run_result,
                            authorization,
                            &all_tools,
                            &govern_gate,
                        )
                        .await;

                        for item in &return_flow.produced_items {
                            let memory_id = format!("tm-{}", uuid::Uuid::new_v4());
                            let mem = MemoryRecord {
                                memory_id: memory_id.clone(),
                                scope: MemoryScope::Task,
                                owner_id: run_contract.work_order_id.clone(),
                                title: item.title.clone(),
                                content: item.content.clone(),
                                tags: vec!["external-agent".to_string()],
                                source: format!("external-agent:{executor_id}"),
                                provenance: item.provenance.clone(),
                                confidence: 0.5,
                                ttl_seconds: 86400,
                                created_at_ms: now() as u64,
                                updated_at_ms: now() as u64,
                                access_policy: String::new(),
                                status: MemoryStatus::Active,
                                cognitive_layer: CognitiveLayer::Hypothesis,
                                linked_contract_hash: Some(authorization.contract_hash.clone()),
                                linked_plan_hash: Some(authorization.plan_hash.clone()),
                                linked_adr_id: None,
                            };
                            memory_repo::MemoryRepo::create(pool, &mem)
                                .await
                                .map_err(|e| WorkerError::Internal(e.to_string()))?;
                            memory_ids.push(memory_id.clone());
                            WorkerEventRepo::append(
                                pool,
                                &authorization.run_id,
                                "MemoryWrite",
                                &serde_json::to_string(&serde_json::json!({
                                    "memory_id": memory_id,
                                    "source": "external-agent",
                                    "cognitive_layer": "Hypothesis"
                                }))
                                .unwrap(),
                            )
                            .await
                            .map_err(|e| WorkerError::Internal(e.to_string()))?;
                        }

                        let side_effects_json = return_flow
                            .side_effects
                            .iter()
                            .map(|decision| {
                                serde_json::json!({
                                    "proposal": decision.proposal,
                                    "outcome": gate_to_json(&decision.outcome),
                                })
                            })
                            .collect::<Vec<_>>();
                        let executor_output = serde_json::json!({
                            "success": success,
                            "output": output,
                            "egress_log": return_flow.egress_log.clone(),
                            "self_reported_trace": self_reported_trace,
                            "produced_items": return_flow.produced_items.clone(),
                            "side_effects": side_effects_json,
                        });
                        step_create(
                            pool,
                            &mut steps,
                            &authorization.run_id,
                            "CallExecutor",
                            &serde_json::json!({
                                "executor_id": executor_id,
                                "round": round,
                                "task": task,
                                "sandbox_profile": authorization.sandbox_profile,
                            }),
                            Some(&executor_output),
                        )
                        .await?;
                        WorkerToolCallRepo::create(
                            pool,
                            &format!("tc-{}", uuid::Uuid::new_v4()),
                            &authorization.run_id,
                            &executor_id,
                            "ExternalExecutor",
                            &format!("{} external execution", executor_id),
                            &serde_json::to_string(&executor_output)
                                .unwrap_or_default()
                                .chars()
                                .take(500)
                                .collect::<String>(),
                            success,
                            0.6,
                            None,
                            now(),
                            Some(now()),
                        )
                        .await
                        .map_err(|e| WorkerError::Internal(e.to_string()))?;

                        if let Some(decision) = return_flow
                            .side_effects
                            .iter()
                            .find(|decision| matches!(decision.outcome, GateOutcome::NeedApproval { .. }))
                        {
                            if let GateOutcome::NeedApproval {
                                reason,
                                action_digest,
                            } = &decision.outcome
                            {
                                persist_loop_cursor(pool, authorization, round, reason, action_digest).await?;
                                WorkerEventRepo::append(
                                    pool,
                                    &authorization.run_id,
                                    "ApprovalRequired",
                                    &serde_json::to_string(&serde_json::json!({
                                        "round": round,
                                        "reason": reason,
                                        "action_digest": action_digest,
                                        "source": "external-agent-return-flow",
                                    }))
                                    .unwrap(),
                                )
                                .await
                                .map_err(|e| WorkerError::Internal(e.to_string()))?;
                                waiting_approval = true;
                                last_tool_summary = format!("Approval required: {reason}");
                                break;
                            }
                        }
                        if let Some(decision) = return_flow
                            .side_effects
                            .iter()
                            .find(|decision| matches!(decision.outcome, GateOutcome::Deny { .. }))
                        {
                            if let GateOutcome::Deny { reason } = &decision.outcome {
                                WorkerEventRepo::append(
                                    pool,
                                    &authorization.run_id,
                                    "WorkerBlocked",
                                    &serde_json::to_string(&serde_json::json!({
                                        "round": round,
                                        "reason": reason,
                                        "source": "external-agent-return-flow",
                                    }))
                                    .unwrap(),
                                )
                                .await
                                .map_err(|e| WorkerError::Internal(e.to_string()))?;
                                let next_observation = format!(
                                    "External agent return-flow side effect was denied: {reason}. Choose a legal action."
                                );
                                loop_history.push(ModelMessage {
                                    role: "system".to_string(),
                                    content: next_observation.clone(),
                                });
                                observation = Some(next_observation);
                                continue;
                            }
                        }

                        WorkerEventRepo::append(
                            pool,
                            &authorization.run_id,
                            "ToolEnd",
                            &serde_json::to_string(&serde_json::json!({
                                "tool_id": executor_id,
                                "round": round,
                                "success": success
                            }))
                            .unwrap(),
                        )
                        .await
                        .map_err(|e| WorkerError::Internal(e.to_string()))?;
                        last_tool_summary = serde_json::to_string(&executor_output)
                            .unwrap_or_default()
                            .chars()
                            .take(1000)
                            .collect::<String>();
                        let next_observation = format!(
                            "External executor {executor_id} completed with success={success}. Return-flow governance passed. Observation: {}",
                            last_tool_summary
                        );
                        loop_history.push(ModelMessage {
                            role: "tool".to_string(),
                            content: next_observation.clone(),
                        });
                        observation = Some(next_observation);
                    }
                    ActionProposal::AskHuman { question, .. } => {
                        tool_failed = true;
                        last_tool_summary = format!("Human input required: {question}");
                        break;
                    }
                }
            }
        }
        }

        if !finished && !tool_failed && !waiting_approval && !blocked && !timed_out {
            timed_out = true;
            last_tool_summary = format!("Controlled ReAct loop reached max_rounds={max_rounds}");
        }

        let mem_id = format!("tm-{}", uuid::Uuid::new_v4());
        let mem = MemoryRecord {
            memory_id: mem_id.clone(),
            scope: MemoryScope::Task,
            owner_id: run_contract.work_order_id.clone(),
            title: format!("WorkerRun {}", &authorization.run_id),
            content: if last_tool_summary.is_empty() {
                format!("Harness: {}", run_contract.mission_intent)
            } else {
                format!(
                    "Harness: {}\nTool evidence: {}",
                    run_contract.mission_intent, last_tool_summary
                )
            },
            tags: vec![],
            source: "worker-harness".into(),
            provenance: format!("worker-run-{}", authorization.run_id),
            confidence: 0.9,
            ttl_seconds: 86400,
            created_at_ms: now() as u64,
            updated_at_ms: now() as u64,
            access_policy: String::new(),
            status: MemoryStatus::Active,
            cognitive_layer: CognitiveLayer::Hypothesis,
            linked_contract_hash: Some(authorization.contract_hash.clone()),
            linked_plan_hash: Some(authorization.plan_hash.clone()),
            linked_adr_id: None,
        };
        memory_repo::MemoryRepo::create(pool, &mem)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        memory_ids.push(mem_id.clone());
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "WriteMemory",
            &serde_json::json!({"memory_id":mem_id}),
            None,
        )
        .await?;
        WorkerEventRepo::append(
            pool,
            &authorization.run_id,
            "MemoryWrite",
            &serde_json::to_string(&serde_json::json!({"memory_id":mem_id})).unwrap(),
        )
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;

        let reflect_route = route_for_step(
            run_contract,
            authorization,
            "Reflect",
            required_capabilities_for_step("Reflect", &run_contract.mission_intent),
            model_profiles,
            None,
        );
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "ModelCall",
            &serde_json::json!({"purpose":"Reflect"}),
            Some(&serde_json::to_value(&reflect_route).unwrap()),
        )
        .await?;

        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "Reflect",
            &serde_json::json!({"type":"post-execution"}),
            None,
        )
        .await?;
        let reflection = ReflectionEngine::reflect(
            pool,
            &authorization.run_id,
            &run_contract.work_order_id,
            &authorization.agent_id,
            &authorization.worker_id,
            &steps,
            &[],
            &[],
        )
        .await?;
        let reflection_id = Some(reflection.reflection_id.clone());

        let skill_route = route_for_step(
            run_contract,
            authorization,
            "ProposeSkillUpdate",
            vec![
                coevo_models::router::ModelCapability::SkillGeneration,
                coevo_models::router::ModelCapability::StructuredJSON,
            ],
            model_profiles,
            None,
        );
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "ModelCall",
            &serde_json::json!({"purpose":"ProposeSkillUpdate"}),
            Some(&serde_json::to_value(&skill_route).unwrap()),
        )
        .await?;

        let mut proposal_id = None;
        if tool_failed
            || reflection.needs_human_review
            || !reflection
                .skill_to_update_json
                .as_array()
                .map(|a| a.is_empty())
                .unwrap_or(true)
        {
            let run = WorkerRun {
                run_id: authorization.run_id.clone(),
                work_order_id: run_contract.work_order_id.clone(),
                agent_id: authorization.agent_id.clone(),
                worker_id: authorization.worker_id.clone(),
                session_id: authorization.session_id.clone(),
                status: if tool_failed {
                    crate::types::WorkerRunStatus::Failed
                } else {
                    crate::types::WorkerRunStatus::Completed
                },
                result_json: serde_json::json!({}),
                memory_ids_json: serde_json::json!([]),
                errors_json: serde_json::json!([]),
                audit_ref: None,
                started_at_ms: now(),
                ended_at_ms: Some(now()),
            };
            proposal_id = SelfUpgradeLoop::run(pool, &run, &reflection, None).await?;
        }

        let final_status = if waiting_approval {
            "WaitingApproval"
        } else if timed_out {
            "TimedOut"
        } else if blocked {
            "Blocked"
        } else if tool_failed {
            "Failed"
        } else {
            "Completed"
        }
        .to_string();
        Ok(AgentSubHarnessResult {
            final_status: final_status.clone(),
            summary: format!("WorkerHarness {} execution.", final_status),
            memory_ids,
            reflection_id,
            proposal_id,
        })
    }
}

fn route_for_step(
    run_contract: &AgentRunContract,
    authorization: &RunAuthorization,
    step_type: &str,
    required_capabilities: Vec<coevo_models::router::ModelCapability>,
    model_profiles: &[ModelProfile],
    max_latency_ms: Option<u64>,
) -> ModelRoutingDecision {
    let req = ModelRoutingRequest {
        work_order_id: run_contract.work_order_id.clone(),
        agent_id: authorization.agent_id.clone(),
        worker_step_type: step_type.to_string(),
        intent: run_contract.mission_intent.clone(),
        required_capabilities,
        track: authorization.track.clone(),
        risk_score: if authorization.track == "red" {
            0.9
        } else if authorization.track == "yellow" {
            0.6
        } else {
            0.3
        },
        max_latency_ms,
        max_cost_usd: None,
        privacy_boundary: PrivacyLevel::PublicApi,
        preferred_model_id: None,
    };
    ModelRouter::route(&req, model_profiles, None).unwrap_or_else(|_| ModelRoutingDecision {
        selected_provider_id: "unavailable".into(),
        selected_model_id: "unavailable".into(),
        selected_capabilities: vec![],
        reason: "NoModelAvailable for configured provider profiles".into(),
        fallback_model_ids: vec![],
        estimated_cost_usd: None,
        estimated_latency_ms: None,
        governance_notes: vec!["ModelRouter failed for configured provider profiles".into()],
        decision_id: format!("mrd-{}", uuid::Uuid::new_v4()),
        created_at_ms: chrono::Utc::now().timestamp_millis(),
    })
}

fn gate_to_json(gate: &GateOutcome) -> serde_json::Value {
    match gate {
        GateOutcome::Allow => serde_json::json!({"outcome":"allow"}),
        GateOutcome::Deny { reason } => {
            serde_json::json!({"outcome":"deny","reason":reason})
        }
        GateOutcome::NeedApproval {
            reason,
            action_digest,
        } => serde_json::json!({
            "outcome":"need_approval",
            "reason": reason,
            "action_digest": action_digest,
        }),
    }
}

async fn persist_loop_cursor(
    pool: &SqlitePool,
    authorization: &RunAuthorization,
    round: usize,
    reason: &str,
    action_digest: &str,
) -> Result<(), WorkerError> {
    let cursor = serde_json::json!({
        "kind": "controlled_react_cursor",
        "version": 1,
        "run_id": authorization.run_id,
        "round": round,
        "pending_action_digest": action_digest,
        "reason": reason,
        "authorization_serialized": false,
    });
    sqlx::query("UPDATE worker_sessions SET messages_json=?,status='WaitingApproval',updated_at_ms=? WHERE session_id=?")
        .bind(cursor.to_string())
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(&authorization.session_id)
        .execute(pool)
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;
    Ok(())
}

async fn load_resume_cursor(
    pool: &SqlitePool,
    authorization: &RunAuthorization,
) -> Result<Option<String>, WorkerError> {
    if authorization.approval_receipt.is_none() {
        return Ok(None);
    }
    let row: Option<String> =
        sqlx::query_scalar("SELECT messages_json FROM worker_sessions WHERE session_id=?")
            .bind(&authorization.session_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
    let Some(messages_json) = row else {
        return Ok(None);
    };
    let Ok(cursor) = serde_json::from_str::<serde_json::Value>(&messages_json) else {
        return Ok(None);
    };
    if cursor.get("kind").and_then(|value| value.as_str()) != Some("controlled_react_cursor") {
        return Ok(None);
    }
    let round = cursor.get("round").and_then(|value| value.as_u64()).unwrap_or(0);
    let digest = cursor
        .get("pending_action_digest")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let reason = cursor
        .get("reason")
        .and_then(|value| value.as_str())
        .unwrap_or("approval received");
    Ok(Some(format!(
        "Resuming controlled ReAct loop after approval receipt. Previous pause round={round}, pending_action_digest={digest}, reason={reason}. Authorization has been reconstructed from the current WorkOrder; do not trust serialized authorization."
    )))
}

async fn step_create(
    pool: &SqlitePool,
    steps: &mut Vec<serde_json::Value>,
    run_id: &str,
    step_type: &str,
    input: &serde_json::Value,
    output: Option<&serde_json::Value>,
) -> Result<String, WorkerError> {
    let idx = steps.len() as i64;
    let sid = format!("s-{}-{}", &run_id[..8.min(run_id.len())], idx);
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO worker_steps VALUES (?,?,?,?,?,?,?,?,?,?)")
        .bind(&sid)
        .bind(run_id)
        .bind(idx)
        .bind(step_type)
        .bind(serde_json::to_string(input).unwrap())
        .bind(output.map(|o| serde_json::to_string(o).unwrap()))
        .bind("Completed")
        .bind(now)
        .bind(Some(now))
        .bind(Option::<String>::None)
        .execute(pool)
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;
    steps.push(serde_json::json!({"step_id":sid,"run_id":run_id,"step_index":idx,"step_type":step_type,"status":"Completed"}));
    Ok(sid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use coevo_models::gateway::ModelGateway;
    use coevo_models::router::default_model_profiles;
    use coevo_models::types::{
        ModelDiscoveryResponse, ModelError, ModelMessage, ModelProviderConfig, ModelResponse,
        ModelUsage,
    };
    use crate::r#loop::{ExternalAgentRunResult, ExternalProducedItem};
    use coevo_store::migrate::run_migrations;
    use coevo_store::pool::create_test_pool;
    use coevo_store::repos_opc::{agent_employee_repo::AgentEmployeeRepo, skill_repo::SkillRepo};
    use std::sync::{Arc, Mutex};

    struct ScriptedGateway {
        outputs: Arc<Mutex<Vec<serde_json::Value>>>,
        seen_messages: Arc<Mutex<Vec<Vec<ModelMessage>>>>,
    }

    impl ScriptedGateway {
        fn new(outputs: Vec<serde_json::Value>) -> Self {
            Self {
                outputs: Arc::new(Mutex::new(outputs)),
                seen_messages: Arc::new(Mutex::new(vec![])),
            }
        }
    }

    #[async_trait]
    impl ModelGateway for ScriptedGateway {
        async fn test_connection(
            &self,
            _config: &ModelProviderConfig,
        ) -> Result<ModelResponse, ModelError> {
            unreachable!("agent harness tests do not call test_connection")
        }

        async fn discover_models(
            &self,
            _config: &ModelProviderConfig,
        ) -> Result<ModelDiscoveryResponse, ModelError> {
            unreachable!("agent harness tests do not call discover_models")
        }

        async fn chat(&self, _request: &coevo_models::types::ModelRequest) -> Result<ModelResponse, ModelError> {
            unreachable!("agent harness tests do not call chat")
        }

        async fn structured(
            &self,
            request: &coevo_models::types::ModelRequest,
            _schema_json: &serde_json::Value,
        ) -> Result<ModelResponse, ModelError> {
            self.seen_messages.lock().unwrap().push(request.messages.clone());
            let next = self
                .outputs
                .lock()
                .unwrap()
                .remove(0);
            Ok(ModelResponse {
                content: serde_json::to_string(&next).unwrap(),
                json: Some(next),
                usage: ModelUsage {
                    prompt_tokens: 17,
                    completion_tokens: 11,
                    total_tokens: 28,
                },
                latency_ms: 1,
                model: request.model.clone(),
                finish_reason: "stop".to_string(),
                provider_kind: request.config.kind,
            })
        }
    }

    struct EchoExternalAgent;

    #[async_trait]
    impl ExternalAgentAdapter for EchoExternalAgent {
        fn executor_id(&self) -> &str {
            "external-echo"
        }

        async fn run_in_sandbox(
            &self,
            task: ExternalAgentTask,
        ) -> Result<ExternalAgentRunResult, WorkerError> {
            Ok(ExternalAgentRunResult {
                success: true,
                output: serde_json::json!({
                    "executor_id": task.executor_id,
                    "task": task.task,
                    "sandbox_tier": task.sandbox_profile.tier,
                }),
                produced_items: vec![ExternalProducedItem {
                    title: "External claim".to_string(),
                    content: "The external agent reported a claim.".to_string(),
                    provenance: "external-echo:self-report".to_string(),
                    cognitive_layer: CognitiveLayer::Fact,
                }],
                side_effects: vec![],
                egress_log: vec![ExternalAgentBoundary::egress_attempt(
                    &task.sandbox_profile,
                    "https://example.com",
                )],
                self_reported_trace: serde_json::json!([
                    {"step": "received_task"},
                    {"step": "produced_claim"}
                ]),
            })
        }
    }

    fn test_contract(work_order_id: &str, intent: &str) -> AgentRunContract {
        AgentRunContract {
            work_order_id: work_order_id.to_string(),
            mission_intent: intent.to_string(),
            required_skills: vec!["skill-mission-draft".to_string()],
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
        }
    }

    fn test_auth(work_order_id: &str, run_id: &str, restricted_actions: Vec<String>) -> RunAuthorization {
        RunAuthorization {
            work_order_id: work_order_id.to_string(),
            agent_id: "agent-founder-01".to_string(),
            worker_id: "worker-agent-founder-01".to_string(),
            session_id: format!("session-{work_order_id}"),
            run_id: run_id.to_string(),
            track: "green".to_string(),
            allowed_actions: vec!["read".to_string(), "analyze".to_string()],
            restricted_actions,
            approval_receipt: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            sandbox_profile: SandboxProfile::from_track("green", Some(std::env::temp_dir())),
        }
    }

    async fn migrated_pool() -> sqlx::SqlitePool {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        AgentEmployeeRepo::seed(&pool).await.unwrap();
        SkillRepo::seed_default(&pool).await.unwrap();
        pool
    }

    async fn create_worker_session(pool: &sqlx::SqlitePool, auth: &RunAuthorization) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO worker_sessions (
                session_id,
                worker_id,
                work_order_id,
                agent_id,
                channel,
                messages_json,
                context_memory_ids_json,
                loaded_skill_ids_json,
                tool_call_ids_json,
                status,
                created_at_ms,
                updated_at_ms
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&auth.session_id)
        .bind(&auth.worker_id)
        .bind(&auth.work_order_id)
        .bind(&auth.agent_id)
        .bind("MissionChat")
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("Running")
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn model_drives_tool_selection_without_keyword_trigger() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!("coevo-model-picks-tool-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("evidence.txt");
        std::fs::write(&evidence_path, "model selected the file tool").unwrap();
        let work_order_id = "wo-model-tool-selection";
        let run_id = "run-model-tool-selection";
        let gateway = ScriptedGateway::new(vec![
            serde_json::json!({
                "thought": "The user wants local evidence, so I should read the supplied evidence file.",
                "proposal": {
                    "kind": "call_tool",
                    "tool_id": "file-readonly",
                    "input": {
                        "action": "ReadFile",
                        "path": evidence_path.to_string_lossy().to_string(),
                        "allowed_paths": [root.to_string_lossy().to_string()],
                        "max_bytes": 5000
                    },
                    "rationale": "Read-only local evidence is within Green Track."
                },
                "confidence": 0.91
            }),
            serde_json::json!({
                "thought": "The file observation is enough to finish.",
                "proposal": {
                    "kind": "finish",
                    "summary": "Launch evidence read successfully.",
                    "result": {"ok": true}
                },
                "confidence": 0.92
            }),
        ]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(work_order_id, "Inspect the provided launch evidence and summarize it."),
            &test_auth(work_order_id, run_id, vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let file_tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id=? AND tool_id='file-readonly'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(file_tool_calls, 1);
    }

    #[tokio::test]
    async fn denied_action_feeds_back_and_model_retries() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!("coevo-denied-retry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("evidence.txt");
        std::fs::write(&evidence_path, "restricted evidence").unwrap();
        let work_order_id = "wo-denied-retry";
        let run_id = "run-denied-retry";
        let gateway = ScriptedGateway::new(vec![
            serde_json::json!({
                "thought": "I will try to read the local evidence first.",
                "proposal": {
                    "kind": "call_tool",
                    "tool_id": "file-readonly",
                    "input": {
                        "action": "ReadFile",
                        "path": evidence_path.to_string_lossy().to_string(),
                        "allowed_paths": [root.to_string_lossy().to_string()],
                        "max_bytes": 5000
                    },
                    "rationale": "Read the requested evidence."
                },
                "confidence": 0.8
            }),
            serde_json::json!({
                "thought": "The requested file tool is restricted, so I should finish with a governed explanation.",
                "proposal": {
                    "kind": "finish",
                    "summary": "The requested file read was blocked by governance.",
                    "result": {"blocked": true}
                },
                "confidence": 0.7
            }),
        ]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(work_order_id, "Review launch evidence."),
            &test_auth(work_order_id, run_id, vec!["file-readonly".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        assert_eq!(seen_messages.lock().unwrap().len(), 2);
        let second_round = seen_messages.lock().unwrap()[1]
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(second_round.contains("Tool in restricted actions"));
        let file_tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id=? AND tool_id='file-readonly'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(file_tool_calls, 0);
        let denied_model_steps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_steps WHERE run_id=? AND step_type='ModelCall' AND output_json LIKE '%Tool in restricted actions%'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(denied_model_steps, 1);
    }

    #[tokio::test]
    async fn loop_terminates_on_finish() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "No tool needed.",
            "proposal": {
                "kind": "finish",
                "summary": "Done",
                "result": {"done": true}
            },
            "confidence": 0.9
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-finish", "Summarize from existing context."),
            &test_auth("wo-finish", "run-finish", vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let tool_calls: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-finish'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tool_calls, 0);
    }

    #[tokio::test]
    async fn loop_terminates_on_max_rounds() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!("coevo-max-rounds-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let outputs = (0..16)
            .map(|_| {
                serde_json::json!({
                    "thought": "Keep inspecting the allowed directory without finishing.",
                    "proposal": {
                        "kind": "call_tool",
                        "tool_id": "file-readonly",
                        "input": {
                            "action": "ListDirectory",
                            "path": root.to_string_lossy().to_string(),
                            "allowed_paths": [root.to_string_lossy().to_string()]
                        },
                        "rationale": "This is a legal read-only action, but the model never finishes."
                    },
                    "confidence": 0.4
                })
            })
            .collect();
        let gateway = ScriptedGateway::new(outputs);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-max-rounds", "Keep trying an unavailable tool."),
            &test_auth("wo-max-rounds", "run-max-rounds", vec![]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "TimedOut");
        let model_steps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_steps WHERE run_id='run-max-rounds' AND step_type='ModelCall' AND output_json LIKE '%file-readonly%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(model_steps, 16);
    }

    #[tokio::test]
    async fn denied_proposals_block_after_three_rounds() {
        let pool = migrated_pool().await;
        let outputs = (0..3)
            .map(|_| {
                serde_json::json!({
                    "thought": "Try an unavailable tool again.",
                    "proposal": {
                        "kind": "call_tool",
                        "tool_id": "missing-tool",
                        "input": {},
                        "rationale": "This should be denied."
                    },
                    "confidence": 0.4
                })
            })
            .collect();
        let gateway = ScriptedGateway::new(outputs);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-blocked-denials", "Keep trying an unavailable tool."),
            &test_auth("wo-blocked-denials", "run-blocked-denials", vec![]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Blocked");
        let blocked_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_events WHERE run_id='run-blocked-denials' AND event_type='WorkerBlocked'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(blocked_events, 3);
    }

    #[tokio::test]
    async fn call_executor_runs_in_sandbox_and_return_flow_records_hypothesis() {
        let pool = migrated_pool().await;
        let work_order_id = "wo-external-executor";
        let run_id = "run-external-executor";
        let gateway = ScriptedGateway::new(vec![
            serde_json::json!({
                "thought": "Delegate bounded analysis to the external agent.",
                "proposal": {
                    "kind": "call_executor",
                    "executor_id": "external-echo",
                    "task": {"prompt": "inspect safely"},
                    "rationale": "The external agent is a registered boundary."
                },
                "confidence": 0.78
            }),
            serde_json::json!({
                "thought": "The external observation is enough to finish.",
                "proposal": {
                    "kind": "finish",
                    "summary": "External boundary completed and returned governed output.",
                    "result": {"ok": true}
                },
                "confidence": 0.88
            }),
        ]);
        let adapter = EchoExternalAgent;
        let root = std::env::temp_dir().join(format!("coevo-external-executor-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("seed.txt"), "sandbox seed").unwrap();
        let mut auth = test_auth(work_order_id, run_id, vec![]);
        auth.sandbox_profile = SandboxProfile::from_track("green", Some(root.clone()));

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(work_order_id, "Ask an external executor for bounded analysis."),
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[&adapter],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let executor_steps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_steps WHERE run_id=? AND step_type='CallExecutor' AND output_json LIKE '%external-echo%' AND output_json LIKE '%Hypothesis%'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(executor_steps, 1);
        let external_memories: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_records WHERE owner_id=? AND source='external-agent:external-echo' AND cognitive_layer='Hypothesis'",
        )
        .bind(work_order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(external_memories, 1);
    }

    #[tokio::test]
    async fn need_approval_persists_cursor_without_serializing_authorization() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I need human input before continuing.",
            "proposal": {
                "kind": "ask_human",
                "question": "Approve this external action?",
                "blocking": true
            },
            "confidence": 0.6
        })]);
        let auth = test_auth("wo-approval-cursor", "run-approval-cursor", vec![]);
        create_worker_session(&pool, &auth).await;

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-approval-cursor", "Ask before continuing."),
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "WaitingApproval");
        let messages_json: String = sqlx::query_scalar(
            "SELECT messages_json FROM worker_sessions WHERE session_id=?",
        )
        .bind(&auth.session_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let cursor: serde_json::Value = serde_json::from_str(&messages_json).unwrap();
        assert_eq!(cursor["kind"], "controlled_react_cursor");
        assert_eq!(cursor["authorization_serialized"], false);
        assert!(cursor.get("track").is_none());
    }

    #[tokio::test]
    async fn resume_with_approval_receipt_rehydrates_cursor_for_model() {
        let pool = migrated_pool().await;
        let pause_gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I need human input before continuing.",
            "proposal": {
                "kind": "ask_human",
                "question": "Approve this external action?",
                "blocking": true
            },
            "confidence": 0.6
        })]);
        let auth = test_auth("wo-resume-cursor", "run-resume-cursor-1", vec![]);
        create_worker_session(&pool, &auth).await;
        let paused = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-resume-cursor", "Pause and resume."),
            &auth,
            &default_model_profiles(),
            None,
            &pause_gateway,
            &ModelProviderConfig::mock(),
            &[],
        )
        .await
        .unwrap();
        assert_eq!(paused.final_status, "WaitingApproval");

        let resume_gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "Approval receipt is present; finish.",
            "proposal": {
                "kind": "finish",
                "summary": "Resumed after approval.",
                "result": {"resumed": true}
            },
            "confidence": 0.9
        })]);
        let seen_messages = resume_gateway.seen_messages.clone();
        let mut resumed_auth = auth.clone();
        resumed_auth.run_id = "run2-resume-cursor".to_string();
        resumed_auth.approval_receipt = Some("approval-receipt".to_string());
        let resumed = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-resume-cursor", "Pause and resume."),
            &resumed_auth,
            &default_model_profiles(),
            None,
            &resume_gateway,
            &ModelProviderConfig::mock(),
            &[],
        )
        .await
        .unwrap();

        assert_eq!(resumed.final_status, "Completed");
        let first_prompt = seen_messages.lock().unwrap()[0]
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(first_prompt.contains("Resuming controlled ReAct loop"));
        assert!(first_prompt.contains("pending_action_digest"));
        assert!(!first_prompt.contains("approval-receipt"));
    }
}
