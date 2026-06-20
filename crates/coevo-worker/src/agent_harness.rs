use crate::error::WorkerError;
use crate::event_stream::WorkerEventStream;
use crate::memory_context::MemoryContextBuilder;
use crate::r#loop::{
    external_executor_tool, ActionProposal, ContextEngine, ExternalAgentAdapter,
    ExternalAgentBoundary, ExternalAgentTask, GateOutcome, GovernGate, LoopContext,
    MemoryBudgetContextEngine, ReasoningOutput, SandboxFilesystemGuard, SandboxProfile,
};
use crate::reflection::ReflectionEngine;
use crate::self_upgrade::SelfUpgradeLoop;
use crate::skill_runtime::SkillRuntime;
use crate::tool_policy::{parse_file_tool_policy, ToolPolicyEngine};
use crate::tool_registry::ToolRegistry;
use crate::types::WorkerRun;
use coevo_audit::logger::AuditLogger;
use coevo_core::cognitive::CognitiveLayer;
use coevo_core::contract::MCLSpec;
use coevo_core::opc::{MemoryRecord, MemoryScope, MemoryStatus};
use coevo_models::gateway::ModelGateway;
use coevo_models::openai::extract_structured_json_text;
use coevo_models::router::{
    required_capabilities_for_step, ModelCapability, ModelProfile, ModelRouter,
    ModelRoutingDecision, ModelRoutingRequest, PrivacyLevel,
};
use coevo_models::types::{
    ModelMessage, ModelProviderConfig, ModelRequest, ModelStreamEvent, ModelToolCall,
    ModelToolDefinition, ResponseFormat,
};
use coevo_store::company_workspace::CompanyWorkspaceManager;
use coevo_store::repos::risk_repo::RiskRepo;
use coevo_store::repos::worker_run_repo::{
    WorkerEventRepo, WorkerRunRepo, WorkerSkillUsageRepo, WorkerToolCallRepo,
};
use coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo;
use coevo_store::repos_opc::memory_repo;
use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};

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
    pub model_preference: Option<String>,
    pub execution_contract: Option<MCLSpec>,
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
    pub termination_reason: String,
    pub summary: String,
    pub memory_ids: Vec<String>,
    pub reflection_id: Option<String>,
    pub proposal_id: Option<String>,
    /// Execution metrics surfaced so the server layer can update employee
    /// reputation and growth history without re-parsing step JSON.
    pub agent_id: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub latency_ms: u64,
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
        preferred_tool_ids: &[String],
    ) -> Result<AgentSubHarnessResult, WorkerError> {
        Self::execute_with_opc_pool(
            pool,
            pool,
            std::env::var("COEVO_HOME")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(std::path::PathBuf::from)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(std::env::temp_dir),
            run_contract,
            authorization,
            model_profiles,
            max_runtime_ms,
            gateway,
            provider_config,
            external_agents,
            preferred_tool_ids,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_with_opc_pool(
        pool: &SqlitePool,
        opc_pool: &SqlitePool,
        coevo_home: std::path::PathBuf,
        run_contract: &AgentRunContract,
        authorization: &RunAuthorization,
        model_profiles: &[ModelProfile],
        max_runtime_ms: Option<i64>,
        gateway: &dyn ModelGateway,
        provider_config: &ModelProviderConfig,
        external_agents: &[&dyn ExternalAgentAdapter],
        preferred_tool_ids: &[String],
    ) -> Result<AgentSubHarnessResult, WorkerError> {
        let now = || chrono::Utc::now().timestamp_millis();
        let mut steps: Vec<serde_json::Value> = vec![];
        let mut memory_ids: Vec<String> = vec![];

        let mem_ctx = MemoryContextBuilder::build(
            opc_pool,
            &coevo_home,
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
            "company_memory_count":mem_ctx.company_memory.len(),"company_shared_count":mem_ctx.company_shared_files.len(),"agent_memory_count":mem_ctx.agent_memory.len(),
            "task_memory_count":mem_ctx.task_memory.len(),"stale_memory_ids":mem_ctx.stale_memory_ids.len(),
            "excluded_revoked_count":mem_ctx.excluded_revoked_count,
            "excluded_fact_without_provenance":mem_ctx.fact_without_provenance
        }), None).await?;

        let index = SkillRuntime::load_skill_index(opc_pool, &authorization.agent_id).await?;
        let selected = SkillRuntime::select_relevant(
            &run_contract.mission_intent,
            &run_contract.required_skills,
            &index,
        );
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "LoadSkillIndex",
            &serde_json::json!({"skills_found":index.len(),"selected":selected}),
            None,
        )
        .await?;
        // Load the assigned employee's system prompt (its working charter) so it
        // can be injected into every round's prompt. Empty for built-in employees
        // until customized via the Agent Workbench.
        let agent_system_prompt: String = AgentEmployeeRepo::list(opc_pool)
            .await
            .ok()
            .and_then(|emps| {
                emps.into_iter()
                    .find(|e| e.agent_id == authorization.agent_id)
                    .map(|e| e.system_prompt)
            })
            .unwrap_or_default();
        let agent_system_prompt = load_employee_system_prompt(
            &CompanyWorkspaceManager::new(coevo_home.clone()),
            &run_contract.opc_id,
            &authorization.agent_id,
            &agent_system_prompt,
        );

        let mut skill_directives: Vec<(String, String, String)> = Vec::new();
        // Skills loaded for this run; their usage success/score is reconciled from
        // the actual run outcome after execution (not fabricated at load time).
        let mut loaded_skills: Vec<(String, String)> = Vec::new();
        for sid in &selected {
            if let Some(full) = SkillRuntime::load_full(
                opc_pool,
                &coevo_home,
                &run_contract.opc_id,
                &authorization.agent_id,
                sid,
            )
            .await?
            {
                let version = full
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1.0.0")
                    .to_string();
                let prompt_template = full
                    .get("prompt_template")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                step_create(
                    pool,
                    &mut steps,
                    &authorization.run_id,
                    "LoadSkillFull",
                    &serde_json::json!({
                        "loaded_skill": sid,
                        "version": version,
                        "has_directive": !prompt_template.trim().is_empty(),
                    }),
                    None,
                )
                .await?;
                if !prompt_template.trim().is_empty() {
                    skill_directives.push((sid.clone(), version.clone(), prompt_template));
                }
                loaded_skills.push((sid.clone(), version.clone()));
            }
        }

        let mut registry = ToolRegistry::default_registry();
        // Inject Model Context Protocol tools from enabled, already-discovered MCP
        // servers (their tools were cached during connect/test). Each becomes a
        // governed tool the agent can call; the server connection is made lazily on
        // first invocation, so listing here costs no network.
        if let Ok(mcp_servers) =
            coevo_store::repos::mcp_server_repo::McpServerRepo::list_enabled_for_opc(
                pool,
                &run_contract.opc_id,
            )
            .await
        {
            for record in mcp_servers {
                if record.transport.trim().eq_ignore_ascii_case("stdio") {
                    continue;
                }
                let row = coevo_adapters::McpServerRow {
                    id: format!("{}:{}", record.opc_id, record.id),
                    name: record.name.clone(),
                    transport: record.transport.clone(),
                    command: record.command.clone(),
                    args: Some(record.args_json.clone()),
                    env: Some(record.env_json.clone()),
                    url: record.url.clone(),
                    headers: Some(record.headers_json.clone()),
                };
                let config = match coevo_adapters::McpServerConfig::from_row(row) {
                    Ok(cfg) => cfg,
                    Err(_) => continue,
                };
                let tools: Vec<coevo_adapters::McpToolInfo> =
                    serde_json::from_str(&record.tools_json).unwrap_or_default();
                for tool in tools {
                    let urn = coevo_adapters::make_tool_urn(&config.name, &tool.name);
                    registry.register(
                        crate::types::Tool {
                            tool_id: urn.clone(),
                            name: urn,
                            tool_type: crate::types::ToolType::Mcp,
                            risk_ceiling: 0.6,
                            supported_actions: vec!["read".into(), "execute".into()],
                            permission_boundary_json: serde_json::json!({}),
                            requires_credential: false,
                            credential_ref: None,
                            enabled: true,
                        },
                        Box::new(crate::tools::mcp_tool::McpToolHandler::new(
                            config.clone(),
                            tool.name,
                        )),
                    );
                }
            }
        }
        let mut all_tools = registry.list().to_vec();
        all_tools.extend(
            external_agents
                .iter()
                .map(|adapter| external_executor_tool(adapter.executor_id())),
        );
        let file_tool_policy = CompanyWorkspaceManager::new(coevo_home.clone())
            .read_company_employee_files(&run_contract.opc_id, &authorization.agent_id)
            .ok()
            .map(|files| parse_file_tool_policy(&files.tool_policy_json))
            .unwrap_or_default();
        let allowed = ToolPolicyEngine::filter_with_file_policy(
            &all_tools,
            &authorization.track,
            &authorization.allowed_actions,
            &authorization.restricted_actions,
            &file_tool_policy,
        );
        let allowed = order_tools_by_preference(allowed, preferred_tool_ids);
        let registry = Arc::new(registry);
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "SelectTool",
            &serde_json::json!({
                "file_tool_policy": {
                    "allowed_tools": file_tool_policy.allowed_tools,
                    "risk_ceiling": file_tool_policy.risk_ceiling,
                },
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
        let mut termination_reason = String::new();
        let schema = serde_json::to_value(schemars::schema_for!(ReasoningOutput))
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let mut observation: Option<String> = None;
        let mut saw_file_evidence_attempt = false;
        let mut finished = false;
        let mut waiting_approval = false;
        let mut timed_out = false;
        let mut blocked = false;
        let mut consecutive_denials = 0usize;
        let max_rounds = authorization
            .execution_contract
            .as_ref()
            .map(|contract| contract.termination_policy.max_hops.max(1) as usize)
            .unwrap_or(16usize);
        let effective_max_runtime_ms = max_runtime_ms.or_else(|| {
            authorization
                .execution_contract
                .as_ref()
                .and_then(|contract| i64::try_from(contract.termination_policy.max_latency_ms).ok())
        });
        let context_engine = MemoryBudgetContextEngine;
        let govern_gate = GovernGate::default_for_authorization(authorization);
        let started_at_ms = now();
        let mut loop_history: Vec<ModelMessage> = vec![];
        if let Some(resume_observation) = load_resume_cursor(pool, authorization).await? {
            loop_history.push(ModelMessage {
                role: "system".to_string(),
                content: resume_observation.clone(),
                ..Default::default()
            });
            observation = Some(resume_observation);
        }
        let mut total_prompt_tokens = 0u64;
        let mut total_completion_tokens = 0u64;
        let mut total_tokens = 0u64;
        let mut total_estimated_cost_usd = 0.0f64;

        for round in 0..max_rounds {
            if is_run_cancelled(pool, &authorization.run_id, &authorization.session_id).await? {
                termination_reason = "cancelled".to_string();
                last_tool_summary = "Run cancelled by server before a new round began".to_string();
                break;
            }
            if let Some(max_runtime_ms) = effective_max_runtime_ms {
                if now().saturating_sub(started_at_ms) >= max_runtime_ms {
                    timed_out = true;
                    termination_reason = "runtime_timeout".to_string();
                    last_tool_summary =
                        format!("Controlled ReAct loop reached max_runtime_ms={max_runtime_ms}");
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
                effective_max_runtime_ms.map(|m| m as u64),
            );
            if routing.selected_model_id == "unavailable" {
                return Err(WorkerError::Internal(
                    "MODEL_ROUTE_UNAVAILABLE: no routable model for required capabilities"
                        .to_string(),
                ));
            }
            let prompt = context_engine
                .build_prompt(&LoopContext {
                    run_contract,
                    authorization,
                    memory_context: &mem_ctx,
                    allowed_tools: &allowed,
                    observation: observation.as_deref(),
                    skill_directives: &skill_directives,
                    system_prompt: &agent_system_prompt,
                })
                .await?;
            let history_budget = provider_config
                .max_tokens
                .saturating_sub(prompt.estimated_tokens)
                .max(1);
            let compacted_history = if estimate_history_tokens(&loop_history) > history_budget {
                match compact_history_with_model(
                    gateway,
                    provider_config,
                    model_profiles,
                    run_contract,
                    authorization,
                    &loop_history,
                    &prompt,
                    history_budget,
                )
                .await
                {
                    Ok(Some(compacted)) => Some(compacted),
                    _ => {
                        context_engine
                            .maybe_compact(&loop_history, history_budget)
                            .await?
                    }
                }
            } else {
                None
            };
            let request_messages = if let Some(compacted) = &compacted_history {
                let mut messages = prompt.stable_prefix.clone();
                messages.push(compacted.summary.clone());
                messages.extend(prompt.volatile_suffix.clone());
                messages
            } else {
                let mut messages = prompt.stable_prefix.clone();
                messages.extend(loop_history.clone());
                messages.extend(prompt.volatile_suffix.clone());
                messages
            };
            let model_tools = allowed
                .iter()
                .map(|tool| model_tool_definition(tool))
                .collect::<Vec<_>>();
            let request = ModelRequest {
                config: provider_config.clone(),
                role: coevo_models::types::ModelRole::AgentReasoning,
                model: routing.selected_model_id.clone(),
                messages: request_messages,
                temperature: provider_config.temperature,
                max_tokens: provider_config.max_tokens,
                response_format: response_format_for_mission(
                    &run_contract.mission_intent,
                    saw_file_evidence_attempt,
                ),
                stream: true,
                tools: model_tools.clone(),
                tool_choice: initial_tool_choice_for_mission(
                    &run_contract.mission_intent,
                    saw_file_evidence_attempt,
                    provider_config.kind,
                    &model_tools,
                ),
            };
            let model_started_at_ms = now();
            let run_id = authorization.run_id.clone();
            let streamed_tool_calls = Arc::new(Mutex::new(Vec::<ModelToolCall>::new()));
            let saw_done_event = Arc::new(Mutex::new(false));
            let on_event = |event: ModelStreamEvent| {
                let pool = pool.clone();
                let run_id = run_id.clone();
                let streamed_tool_calls = Arc::clone(&streamed_tool_calls);
                let saw_done_event = Arc::clone(&saw_done_event);
                Box::pin(async move {
                    match event {
                        ModelStreamEvent::ReasoningDelta { delta } => {
                            WorkerEventStream::append(
                                &pool,
                                &run_id,
                                crate::types::WorkerEventType::ReasoningDelta,
                                serde_json::json!({ "delta": delta }),
                            )
                            .await?;
                        }
                        ModelStreamEvent::ContentDelta { delta } => {
                            WorkerEventStream::append(
                                &pool,
                                &run_id,
                                crate::types::WorkerEventType::ContentDelta,
                                serde_json::json!({ "delta": delta }),
                            )
                            .await?;
                        }
                        ModelStreamEvent::ToolCallDelta {
                            index,
                            id,
                            name,
                            arguments_delta,
                        } => {
                            {
                                let mut guard = streamed_tool_calls.lock().map_err(|_| {
                                    WorkerError::Internal("tool call stream lock poisoned".into())
                                })?;
                                merge_streamed_tool_call(
                                    &mut guard,
                                    index,
                                    id.clone(),
                                    name.clone(),
                                    &arguments_delta,
                                );
                            }
                            WorkerEventStream::append(
                                &pool,
                                &run_id,
                                crate::types::WorkerEventType::ToolCallDelta,
                                serde_json::json!({
                                    "index": index,
                                    "id": id,
                                    "name": name,
                                    "arguments_delta": arguments_delta,
                                }),
                            )
                            .await?;
                        }
                        ModelStreamEvent::Usage(usage) => {
                            WorkerEventStream::append(
                                &pool,
                                &run_id,
                                crate::types::WorkerEventType::Usage,
                                serde_json::to_value(&usage).unwrap_or_default(),
                            )
                            .await?;
                        }
                        ModelStreamEvent::Done { finish_reason } => {
                            *saw_done_event.lock().map_err(|_| {
                                WorkerError::Internal("done stream lock poisoned".into())
                            })? = true;
                            WorkerEventStream::append(
                                &pool,
                                &run_id,
                                crate::types::WorkerEventType::Done,
                                serde_json::json!({ "finish_reason": finish_reason }),
                            )
                            .await?;
                        }
                    }
                    Ok(())
                })
                    as std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<(), WorkerError>> + Send>,
                    >
            };
            let mut stream_event_handler = |event| -> coevo_models::gateway::ModelStreamFuture<'_> {
                let future = on_event(event);
                Box::pin(async move {
                    future.await.map_err(|e| {
                        coevo_models::types::ModelError::InvalidResponse(e.to_string())
                    })
                })
            };
            let stream_future = gateway.stream(&request, Some(&schema), &mut stream_event_handler);
            tokio::pin!(stream_future);
            let response = if let Some(cancellation_token) =
                crate::worker_cancel::token_for_run(&authorization.run_id)
            {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        termination_reason = "cancelled".to_string();
                        last_tool_summary = "Run cancelled by server during model response".to_string();
                        break;
                    }
                    response = &mut stream_future => {
                        response.map_err(|e| WorkerError::Internal(e.to_string()))?
                    }
                }
            } else {
                stream_future
                    .await
                    .map_err(|e| WorkerError::Internal(e.to_string()))?
            };
            let model_ended_at_ms = now();
            if is_run_cancelled(pool, &authorization.run_id, &authorization.session_id).await? {
                termination_reason = "cancelled".to_string();
                last_tool_summary = "Run cancelled by server after model response".to_string();
                break;
            }
            let streamed_tool_calls = streamed_tool_calls
                .lock()
                .map_err(|_| WorkerError::Internal("tool call stream lock poisoned".into()))?
                .clone();
            let saw_done_event = *saw_done_event
                .lock()
                .map_err(|_| WorkerError::Internal("done stream lock poisoned".into()))?;
            let reasoning_seed = if let Some(json) = response.json {
                json
            } else if !streamed_tool_calls.is_empty() {
                serde_json::json!({})
            } else {
                return Err(WorkerError::Internal(
                    "structured response did not include json".into(),
                ));
            };
            let reasoning = parse_reasoning_output(
                reasoning_seed,
                &streamed_tool_calls,
                mission_requires_file_evidence(&run_contract.mission_intent)
                    && !saw_file_evidence_attempt,
            )?;
            total_prompt_tokens += response.usage.prompt_tokens;
            total_completion_tokens += response.usage.completion_tokens;
            total_tokens += response.usage.total_tokens;
            total_estimated_cost_usd += actual_or_estimated_cost_usd(
                &routing,
                response.usage.prompt_tokens,
                response.usage.completion_tokens,
            );
            // Built-in cost control: stop the run as soon as the accumulated spend
            // crosses the provider's per-task ceiling (0 = unlimited).
            if provider_config.max_cost_per_task_usd > 0.0
                && total_estimated_cost_usd > provider_config.max_cost_per_task_usd
            {
                blocked = true;
                last_tool_summary = format!(
                    "Cost cap reached: ${:.4} exceeds ${:.4} per-task limit",
                    total_estimated_cost_usd, provider_config.max_cost_per_task_usd
                );
                WorkerEventRepo::append(
                    pool,
                    &authorization.run_id,
                    "WorkerBlocked",
                    &serde_json::to_string(&serde_json::json!({
                        "round": round,
                        "reason": last_tool_summary,
                        "cost_total_usd": total_estimated_cost_usd,
                        "cost_cap_usd": provider_config.max_cost_per_task_usd,
                    }))
                    .unwrap(),
                )
                .await
                .map_err(|e| WorkerError::Internal(e.to_string()))?;
                break;
            }
            let replayed_tool_calls = match &reasoning.proposal {
                ActionProposal::CallTool { tool_id, input, .. } => streamed_tool_calls
                    .iter()
                    .find(|call| {
                        call.name == *tool_id
                            && serde_json::from_str::<serde_json::Value>(&call.arguments)
                                .ok()
                                .is_some_and(|arguments| arguments == *input)
                    })
                    .cloned()
                    .into_iter()
                    .collect::<Vec<_>>(),
                ActionProposal::CallExecutor { .. }
                | ActionProposal::SpawnSubagent { .. }
                | ActionProposal::Finish { .. }
                | ActionProposal::AskHuman { .. } => vec![],
            };
            let streamed_native_gate = if streamed_tool_calls.len() > 1 {
                Some(
                    adjudicate_streamed_native_tool_calls(
                        &govern_gate,
                        &streamed_tool_calls,
                        &allowed,
                        &all_tools,
                        authorization,
                    )
                    .await,
                )
            } else {
                None
            };
            let allowed_native_calls = streamed_native_gate
                .as_ref()
                .map(|result| result.allowed_calls.clone())
                .unwrap_or_default();
            let assistant_history_message = ModelMessage {
                role: "assistant".to_string(),
                content: serde_json::to_string(&reasoning).unwrap_or_default(),
                reasoning_content: response.reasoning_content.clone(),
                tool_calls: vec![],
                tool_call_id: None,
            };

            let (mut gate, mut gate_audit) = govern_gate
                .adjudicate_with_audit(&reasoning.proposal, authorization, &all_tools)
                .await;
            if let Some(streamed_gate) = &streamed_native_gate {
                gate = streamed_gate.gate.clone();
                gate_audit = None;
            }
            if let Some(audit) = &gate_audit {
                let _ = RiskRepo::insert(
                    pool,
                    &uuid::Uuid::new_v4().to_string(),
                    &authorization.contract_hash,
                    &authorization.agent_id,
                    &audit.action_urn,
                    &audit.decision,
                    audit.required_confidence,
                    audit.available_confidence,
                    audit.action_risk,
                    audit.inaction_risk,
                    &audit.reason,
                )
                .await;
            }
            let _ = AuditLogger::log_json(
                pool,
                "worker.governance",
                Some(&authorization.contract_hash),
                Some(&authorization.agent_id),
                None,
                &run_contract.opc_id,
                &serde_json::json!({
                    "run_id": authorization.run_id,
                    "work_order_id": run_contract.work_order_id,
                    "round": round,
                    "proposal": serde_json::to_value(&reasoning.proposal).unwrap_or_default(),
                    "gate": gate_to_json(&gate),
                }),
            )
            .await;
            let mut model_output = serde_json::to_value(&routing).unwrap_or_default();
            if let Some(obj) = model_output.as_object_mut() {
                obj.insert("round".into(), serde_json::json!(round));
                obj.insert("thought".into(), serde_json::json!(reasoning.thought));
                obj.insert(
                    "proposal".into(),
                    serde_json::to_value(&reasoning.proposal).unwrap_or_default(),
                );
                obj.insert("confidence".into(), serde_json::json!(reasoning.confidence));
                obj.insert(
                    "usage".into(),
                    serde_json::to_value(&response.usage).unwrap_or_default(),
                );
                obj.insert(
                    "usage_total".into(),
                    serde_json::json!({
                        "prompt_tokens": total_prompt_tokens,
                        "completion_tokens": total_completion_tokens,
                        "total_tokens": total_tokens,
                    }),
                );
                obj.insert(
                    "cost_total_usd".into(),
                    serde_json::json!(total_estimated_cost_usd),
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
                if let Some(streamed_gate) = &streamed_native_gate {
                    obj.insert(
                        "streamed_tool_gate".into(),
                        serde_json::json!({
                            "gate": gate_to_json(&streamed_gate.gate),
                            "allowed_call_count": streamed_gate.allowed_calls.len(),
                        }),
                    );
                }
            }
            WorkerEventStream::append(
                pool,
                &authorization.run_id,
                crate::types::WorkerEventType::AssistantDelta,
                serde_json::json!({
                    "round": round,
                    "delta": serde_json::to_string(&reasoning).unwrap_or_default(),
                    "usage_delta": response.usage,
                    "usage_total": {
                        "prompt_tokens": total_prompt_tokens,
                        "completion_tokens": total_completion_tokens,
                        "total_tokens": total_tokens,
                    },
                    "cost_total_usd": total_estimated_cost_usd,
                    "tool_calls": streamed_tool_calls,
                    "done_emitted": saw_done_event,
                }),
            )
            .await?;
            step_create_timed(
                pool,
                &mut steps,
                &authorization.run_id,
                "ModelCall",
                &serde_json::json!({"intent":run_contract.mission_intent,"round":round}),
                Some(&model_output),
                (model_started_at_ms, model_ended_at_ms),
            )
            .await?;

            match gate {
                GateOutcome::Deny { reason } => {
                    consecutive_denials += 1;
                    loop_history.push(assistant_history_message.clone());
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
                        "Governance denied the previous proposal: {reason}. For file-evidence missions, inspect allowed evidence with a native file-readonly tool call before finishing."
                    );
                    loop_history.push(ModelMessage {
                        role: "system".to_string(),
                        content: next_observation.clone(),
                        ..Default::default()
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
                    let _ = AuditLogger::log_json(
                        pool,
                        "worker.approval.required",
                        Some(&authorization.contract_hash),
                        Some(&authorization.agent_id),
                        None,
                        &run_contract.opc_id,
                        &serde_json::json!({
                            "run_id": authorization.run_id,
                            "work_order_id": run_contract.work_order_id,
                            "round": round,
                            "reason": reason,
                            "action_digest": action_digest,
                        }),
                    )
                    .await;
                    persist_loop_cursor(pool, authorization, round, &reason, &action_digest)
                        .await?;
                    WorkerEventStream::append_approval_required(
                        pool,
                        &authorization.run_id,
                        round,
                        &reason,
                        &action_digest,
                        "governance",
                    )
                    .await?;
                    waiting_approval = true;
                    last_tool_summary = format!("Approval required: {reason}");
                    break;
                }
                GateOutcome::Allow => {
                    consecutive_denials = 0;
                    if !allowed_native_calls.is_empty() {
                        if is_run_cancelled(pool, &authorization.run_id, &authorization.session_id)
                            .await?
                        {
                            termination_reason = "cancelled".to_string();
                            last_tool_summary =
                                "Run cancelled by server before native tool execution".to_string();
                            break;
                        }
                        let mut assistant_history_message = assistant_history_message.clone();
                        assistant_history_message.tool_calls = allowed_native_calls.clone();
                        loop_history.push(assistant_history_message);

                        for tool_call in &allowed_native_calls {
                            WorkerEventRepo::append(
                                pool,
                                &authorization.run_id,
                                "ToolStart",
                                &serde_json::to_string(&serde_json::json!({
                                    "tool_id": tool_call.name,
                                    "round": round
                                }))
                                .unwrap(),
                            )
                            .await
                            .map_err(|e| WorkerError::Internal(e.to_string()))?;
                        }

                        let mut join_set = tokio::task::JoinSet::new();
                        for tool_call in allowed_native_calls.clone() {
                            let registry = Arc::clone(&registry);
                            join_set.spawn(async move {
                                execute_native_tool_call(
                                    &registry,
                                    tool_call.clone(),
                                    tool_call.name.clone(),
                                )
                                .await
                            });
                        }
                        let mut executed_calls = Vec::new();
                        while let Some(join_result) = join_set.join_next().await {
                            let executed =
                                join_result.map_err(|e| WorkerError::Internal(e.to_string()))??;
                            executed_calls.push(executed);
                        }
                        executed_calls.sort_by_key(|result| result.tool_call.index);
                        let mut combined_observation = Vec::new();
                        for executed in executed_calls {
                            let tool_id = executed.tool_call.name.clone();
                            let input = executed.input;
                            let output = executed.output;
                            let success = executed.success;
                            let output_str = serde_json::to_string(&output).unwrap_or_default();
                            if tool_id == "file-readonly" {
                                saw_file_evidence_attempt = true;
                            }
                            step_create(
                                pool,
                                &mut steps,
                                &authorization.run_id,
                                "CallTool",
                                &serde_json::json!({"tool_id":tool_id,"round":round,"input":input}),
                                Some(&output),
                            )
                            .await?;
                            let tool_type = all_tools
                                .iter()
                                .find(|t| t.tool_id == tool_id)
                                .map(|t| t.tool_type.db_value())
                                .unwrap_or_else(|| {
                                    if tool_id.starts_with("urn:mcp:") {
                                        "MCP"
                                    } else {
                                        "LocalProcessSandbox"
                                    }
                                });
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
                                    "tool_id": tool_id,
                                    "round": round,
                                    "success": success
                                }))
                                .unwrap(),
                            )
                            .await
                            .map_err(|e| WorkerError::Internal(e.to_string()))?;
                            let next_observation = format!(
                                "Tool {tool_id} completed with success={success}. Observation: {}",
                                output_str.chars().take(2000).collect::<String>()
                            );
                            combined_observation.push(next_observation.clone());
                            loop_history.push(observation_history_message(
                                next_observation.clone(),
                                executed.tool_call.id.clone().or_else(|| {
                                    Some(format!("tool-call-{}", executed.tool_call.index))
                                }),
                            ));
                        }
                        last_tool_summary = combined_observation.join("\n");
                        observation = Some(last_tool_summary.clone());
                        continue;
                    }
                    if matches!(reasoning.proposal, ActionProposal::Finish { .. })
                        && mission_requires_file_evidence(&run_contract.mission_intent)
                        && !saw_file_evidence_attempt
                    {
                        consecutive_denials += 1;
                        let reason =
                            "must inspect allowed file evidence before finishing".to_string();
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
                            "Governance denied the previous proposal: {reason}. For file-evidence missions, inspect allowed evidence with a native file-readonly tool call before finishing."
                        );
                        loop_history.push(ModelMessage {
                            role: "system".to_string(),
                            content: next_observation.clone(),
                            ..Default::default()
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
                    consecutive_denials = 0;
                    match reasoning.proposal {
                        ActionProposal::Finish { summary, result } => {
                            loop_history.push(assistant_history_message.clone());
                            last_tool_summary = format!(
                                "Model finished: {}\nResult: {}",
                                summary,
                                serde_json::to_string(&result).unwrap_or_default()
                            );
                            termination_reason = "completed".to_string();
                            finished = true;
                            break;
                        }
                        ActionProposal::CallTool { tool_id, input, .. } => {
                            if is_run_cancelled(
                                pool,
                                &authorization.run_id,
                                &authorization.session_id,
                            )
                            .await?
                            {
                                termination_reason = "cancelled".to_string();
                                last_tool_summary =
                                    "Run cancelled by server before tool execution".to_string();
                                break;
                            }
                            let _ = AuditLogger::log_json(
                                pool,
                                "worker.tool.start",
                                Some(&authorization.contract_hash),
                                Some(&authorization.agent_id),
                                None,
                                &run_contract.opc_id,
                                &serde_json::json!({
                                    "run_id": authorization.run_id,
                                    "work_order_id": run_contract.work_order_id,
                                    "round": round,
                                    "tool_id": tool_id,
                                    "input": input,
                                }),
                            )
                            .await;
                            let mut assistant_history_message = assistant_history_message.clone();
                            assistant_history_message.tool_calls = replayed_tool_calls.clone();
                            loop_history.push(assistant_history_message);
                            if tool_id == "file-readonly" {
                                saw_file_evidence_attempt = true;
                            }
                            WorkerEventRepo::append(
                                pool,
                                &authorization.run_id,
                                "ToolStart",
                                &serde_json::to_string(
                                    &serde_json::json!({"tool_id":tool_id,"round":round}),
                                )
                                .unwrap(),
                            )
                            .await
                            .map_err(|e| WorkerError::Internal(e.to_string()))?;

                            let tool_result = registry
                                .execute(&tool_id, input.clone())
                                .await
                                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
                            let success = tool_result.get("error").is_none();
                            let output_str =
                                serde_json::to_string(&tool_result).unwrap_or_default();
                            step_create(
                                pool,
                                &mut steps,
                                &authorization.run_id,
                                "CallTool",
                                &serde_json::json!({"tool_id":tool_id,"round":round,"input":input}),
                                Some(&tool_result),
                            )
                            .await?;
                            // Derive the persisted tool_type from the registered
                            // tool so it always matches the worker_tool_calls CHECK
                            // constraint (MCP tools �?"MCP"); never write a raw id.
                            let tool_type = all_tools
                                .iter()
                                .find(|t| t.tool_id == tool_id)
                                .map(|t| t.tool_type.db_value())
                                .unwrap_or_else(|| {
                                    if tool_id.starts_with("urn:mcp:") {
                                        "MCP"
                                    } else {
                                        "LocalProcessSandbox"
                                    }
                                });
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
                            let _ = AuditLogger::log_json(
                                pool,
                                "worker.tool.end",
                                Some(&authorization.contract_hash),
                                Some(&authorization.agent_id),
                                None,
                                &run_contract.opc_id,
                                &serde_json::json!({
                                    "run_id": authorization.run_id,
                                    "work_order_id": run_contract.work_order_id,
                                    "round": round,
                                    "tool_id": tool_id,
                                    "success": success,
                                }),
                            )
                            .await;
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
                            let executed_tool_call_id = replayed_tool_calls
                                .first()
                                .and_then(|call| call.id.clone())
                                .or_else(|| {
                                    streamed_tool_calls.first().and_then(|call| call.id.clone())
                                });
                            loop_history.push(observation_history_message(
                                next_observation.clone(),
                                executed_tool_call_id,
                            ));
                            observation = Some(next_observation);
                            termination_reason = "tool_executed".to_string();
                        }
                        ActionProposal::CallExecutor {
                            executor_id, task, ..
                        } => {
                            if is_run_cancelled(
                                pool,
                                &authorization.run_id,
                                &authorization.session_id,
                            )
                            .await?
                            {
                                termination_reason = "cancelled".to_string();
                                last_tool_summary =
                                    "Run cancelled by server before executor execution".to_string();
                                break;
                            }
                            let _ = AuditLogger::log_json(
                                pool,
                                "worker.tool.start",
                                Some(&authorization.contract_hash),
                                Some(&authorization.agent_id),
                                None,
                                &run_contract.opc_id,
                                &serde_json::json!({
                                    "run_id": authorization.run_id,
                                    "work_order_id": run_contract.work_order_id,
                                    "round": round,
                                    "tool_id": executor_id,
                                    "input": task,
                                }),
                            )
                            .await;
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
                                blocked = true;
                                last_tool_summary =
                                    format!("External executor {executor_id} has no adapter bound");
                                break;
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
                            let _sandbox_guard =
                                SandboxFilesystemGuard::enter(&authorization.sandbox_profile)
                                    .map_err(|e| {
                                        WorkerError::Internal(format!("sandbox guard failed: {e}"))
                                    })?;
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
                                    loop_history.push(observation_history_message(
                                        next_observation.clone(),
                                        streamed_tool_calls
                                            .first()
                                            .and_then(|call| call.id.clone()),
                                    ));
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
                                memory_repo::MemoryRepo::create(opc_pool, &mem)
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
                            let _ = AuditLogger::log_json(
                                pool,
                                "worker.tool.end",
                                Some(&authorization.contract_hash),
                                Some(&authorization.agent_id),
                                None,
                                &run_contract.opc_id,
                                &serde_json::json!({
                                    "run_id": authorization.run_id,
                                    "work_order_id": run_contract.work_order_id,
                                    "round": round,
                                    "tool_id": executor_id,
                                    "success": success,
                                }),
                            )
                            .await;

                            if let Some(decision) =
                                return_flow.side_effects.iter().find(|decision| {
                                    matches!(decision.outcome, GateOutcome::NeedApproval { .. })
                                })
                            {
                                if let GateOutcome::NeedApproval {
                                    reason,
                                    action_digest,
                                } = &decision.outcome
                                {
                                    persist_loop_cursor(
                                        pool,
                                        authorization,
                                        round,
                                        reason,
                                        action_digest,
                                    )
                                    .await?;
                                    WorkerEventStream::append_approval_required(
                                        pool,
                                        &authorization.run_id,
                                        round,
                                        &reason,
                                        &action_digest,
                                        "external-agent-return-flow",
                                    )
                                    .await?;
                                    waiting_approval = true;
                                    last_tool_summary = format!("Approval required: {reason}");
                                    break;
                                }
                            }
                            if let Some(decision) =
                                return_flow.side_effects.iter().find(|decision| {
                                    matches!(decision.outcome, GateOutcome::Deny { .. })
                                })
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
                                        ..Default::default()
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
                            loop_history.push(assistant_history_message.clone());
                            let next_observation = format!(
                            "External executor {executor_id} completed with success={success}. Return-flow governance passed. Observation: {}",
                            last_tool_summary
                        );
                            loop_history.push(observation_history_message(
                                next_observation.clone(),
                                streamed_tool_calls.first().and_then(|call| call.id.clone()),
                            ));
                            observation = Some(next_observation);
                            termination_reason = "executor_executed".to_string();
                        }
                        ActionProposal::SpawnSubagent { skill_id, task, .. } => {
                            if is_run_cancelled(
                                pool,
                                &authorization.run_id,
                                &authorization.session_id,
                            )
                            .await?
                            {
                                termination_reason = "cancelled".to_string();
                                last_tool_summary =
                                    "Run cancelled by server before subagent spawn".to_string();
                                break;
                            }
                            loop_history.push(assistant_history_message.clone());
                            // Governance guard: a department head may only delegate a skill it
                            // actually holds for this run. This keeps subagent creation inside
                            // the head's authorized skill envelope (no privilege escalation).
                            let skill_authorized =
                                loaded_skills.iter().any(|(sid, _)| sid == &skill_id);
                            if !skill_authorized {
                                WorkerEventRepo::append(
                                    pool,
                                    &authorization.run_id,
                                    "WorkerBlocked",
                                    &serde_json::to_string(&serde_json::json!({
                                        "round": round,
                                        "reason": format!(
                                            "Subagent spawn denied: skill '{skill_id}' is not in the head's authorized skill set"
                                        ),
                                    }))
                                    .unwrap(),
                                )
                                .await
                                .map_err(|e| WorkerError::Internal(e.to_string()))?;
                                let next_observation = format!(
                                    "Subagent spawn for skill '{skill_id}' was denied by governance (skill not authorized). Proceed yourself or finish."
                                );
                                loop_history.push(observation_history_message(
                                    next_observation.clone(),
                                    streamed_tool_calls.first().and_then(|call| call.id.clone()),
                                ));
                                observation = Some(next_observation);
                                continue;
                            }
                            // Record the governed delegation. The ephemeral sub-agent is a
                            // bounded, single-skill helper created under this head; it cannot
                            // itself spawn further agents (no recursion).
                            let subagent_id = format!(
                                "sub-{}-{}",
                                authorization.agent_id,
                                &uuid::Uuid::new_v4().to_string()[..8]
                            );
                            let _ = AuditLogger::log_json(
                                pool,
                                "worker.subagent.spawn",
                                Some(&authorization.contract_hash),
                                Some(&authorization.agent_id),
                                None,
                                &run_contract.opc_id,
                                &serde_json::json!({
                                    "run_id": authorization.run_id,
                                    "work_order_id": run_contract.work_order_id,
                                    "round": round,
                                    "supervisor_agent_id": authorization.agent_id,
                                    "subagent_id": subagent_id,
                                    "skill_id": skill_id,
                                    "task": task,
                                }),
                            )
                            .await;
                            WorkerEventRepo::append(
                                pool,
                                &authorization.run_id,
                                "SubagentSpawned",
                                &serde_json::to_string(&serde_json::json!({
                                    "round": round,
                                    "subagent_id": subagent_id,
                                    "skill_id": skill_id,
                                    "task": task,
                                }))
                                .unwrap(),
                            )
                            .await
                            .map_err(|e| WorkerError::Internal(e.to_string()))?;
                            // Actually run the bounded, governed sub-agent: a single-skill,
                            // read-only reasoning helper that produces a real contribution.
                            let skill_directive = skill_directives
                                .iter()
                                .find(|(sid, _, _)| sid == &skill_id)
                                .map(|(_, _, directive)| directive.clone())
                                .unwrap_or_default();
                            let subagent_outcome = run_governed_subagent(
                                gateway,
                                provider_config,
                                model_profiles,
                                run_contract,
                                authorization,
                                &subagent_id,
                                &skill_id,
                                &skill_directive,
                                &task,
                            )
                            .await;
                            let next_observation = match subagent_outcome {
                                Ok(contribution) => {
                                    WorkerEventRepo::append(
                                        pool,
                                        &authorization.run_id,
                                        "SubagentCompleted",
                                        &serde_json::to_string(&serde_json::json!({
                                            "round": round,
                                            "subagent_id": subagent_id,
                                            "skill_id": skill_id,
                                            "contribution": contribution,
                                        }))
                                        .unwrap(),
                                    )
                                    .await
                                    .map_err(|e| WorkerError::Internal(e.to_string()))?;
                                    format!(
                                        "Sub-agent {subagent_id} (skill '{skill_id}') completed the delegated task '{task}' and reports:\n{contribution}\nIncorporate this into your next step."
                                    )
                                }
                                Err(err) => {
                                    WorkerEventRepo::append(
                                        pool,
                                        &authorization.run_id,
                                        "SubagentFailed",
                                        &serde_json::to_string(&serde_json::json!({
                                            "round": round,
                                            "subagent_id": subagent_id,
                                            "skill_id": skill_id,
                                            "error": err.to_string(),
                                        }))
                                        .unwrap(),
                                    )
                                    .await
                                    .map_err(|e| WorkerError::Internal(e.to_string()))?;
                                    format!(
                                        "Sub-agent {subagent_id} could not complete '{task}' ({err}). Proceed yourself or finish."
                                    )
                                }
                            };
                            loop_history.push(observation_history_message(
                                next_observation.clone(),
                                streamed_tool_calls.first().and_then(|call| call.id.clone()),
                            ));
                            observation = Some(next_observation);
                            termination_reason = "subagent_spawned".to_string();
                        }
                        ActionProposal::AskHuman { question, .. } => {
                            loop_history.push(assistant_history_message.clone());
                            tool_failed = true;
                            last_tool_summary = format!("Human input required: {question}");
                            termination_reason = "human_input_required".to_string();
                            break;
                        }
                    }
                }
            }
        }

        if termination_reason != "cancelled"
            && !finished
            && !tool_failed
            && !waiting_approval
            && !blocked
            && !timed_out
        {
            timed_out = true;
            termination_reason = "max_rounds_exhausted".to_string();
            last_tool_summary = format!("Controlled ReAct loop reached max_rounds={max_rounds}");
        }
        if termination_reason.is_empty() {
            termination_reason = if finished {
                "completed".to_string()
            } else if waiting_approval {
                "waiting_approval".to_string()
            } else if blocked {
                "blocked".to_string()
            } else if timed_out {
                "runtime_timeout".to_string()
            } else if tool_failed {
                "human_input_required".to_string()
            } else {
                "unknown".to_string()
            };
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
        memory_repo::MemoryRepo::create(opc_pool, &mem)
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
            proposal_id = SelfUpgradeLoop::run(pool, opc_pool, &run, &reflection, None).await?;
        }

        let final_status = if termination_reason == "cancelled" {
            "Cancelled"
        } else if waiting_approval {
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
        // Reconcile skill-usage with the real run outcome (replaces the old
        // fabricated success=true/score=0.9 written at load time).
        {
            let skill_succeeded = final_status == "Completed";
            let skill_score = match final_status.as_str() {
                "Completed" => 1.0,
                "WaitingApproval" => 0.5,
                _ => 0.0,
            };
            for (sid, version) in &loaded_skills {
                let _ = WorkerSkillUsageRepo::create(
                    pool,
                    &format!("su-{}", uuid::Uuid::new_v4()),
                    &authorization.run_id,
                    sid,
                    version,
                    "execution",
                    skill_succeeded,
                    skill_score,
                    "",
                    now(),
                )
                .await;
            }
        }
        let latency_ms = now().saturating_sub(started_at_ms).max(0) as u64;
        // Persist queryable execution-summary columns for the growth page.
        WorkerRunRepo::record_summary(
            pool,
            &authorization.run_id,
            total_prompt_tokens as i64,
            total_completion_tokens as i64,
            total_tokens as i64,
            total_estimated_cost_usd,
            latency_ms as i64,
        )
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let summary = format!(
            "WorkerHarness {} execution ({}).",
            final_status, termination_reason
        );
        Ok(AgentSubHarnessResult {
            final_status: final_status.clone(),
            termination_reason,
            summary,
            memory_ids,
            reflection_id,
            proposal_id,
            agent_id: authorization.agent_id.clone(),
            prompt_tokens: total_prompt_tokens,
            completion_tokens: total_completion_tokens,
            total_tokens,
            total_cost_usd: total_estimated_cost_usd,
            latency_ms,
        })
    }
}

fn load_employee_system_prompt(
    workspace: &CompanyWorkspaceManager,
    opc_id: &str,
    agent_id: &str,
    db_system_prompt: &str,
) -> String {
    const PERSONA_SECTION_CHAR_LIMIT: usize = 4000;
    let prompt_path = workspace.company_employee_prompt_path(opc_id, agent_id);
    let employee_dir = workspace.company_employee_dir(opc_id, agent_id);
    let prompt_body =
        read_nonempty_markdown(&prompt_path).unwrap_or_else(|| db_system_prompt.trim().to_string());
    let mut sections = Vec::new();
    if !prompt_body.is_empty() {
        sections.push(prompt_body);
    }
    for (label, path) in [
        ("identity.md", employee_dir.join("identity.md")),
        ("soul.md", employee_dir.join("soul.md")),
        ("agents.md", employee_dir.join("agents.md")),
        ("owner.md", employee_dir.join("owner.md")),
        ("tools.md", employee_dir.join("tools.md")),
    ] {
        if !path.exists() {
            sections.push(format!("[{label}]\n(MISSING)"));
            continue;
        }
        if let Some(content) = read_nonempty_markdown(&path) {
            let content = truncate_markdown_for_prompt(&content, PERSONA_SECTION_CHAR_LIMIT);
            sections.push(format!("[{label}]\n{content}"));
        }
    }
    sections.join("\n\n")
}

fn read_nonempty_markdown(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

fn truncate_markdown_for_prompt(content: &str, limit: usize) -> String {
    let total_chars = content.chars().count();
    if total_chars <= limit {
        return content.to_string();
    }
    let truncated = content.chars().take(limit).collect::<String>();
    format!("{truncated}\n\n[TRUNCATED: {total_chars} chars total]")
}

fn merge_streamed_tool_call(
    tool_calls: &mut Vec<ModelToolCall>,
    index: usize,
    id: Option<String>,
    name: Option<String>,
    arguments_delta: &str,
) {
    let Some(existing) = tool_calls.iter_mut().find(|call| call.index == index) else {
        tool_calls.push(ModelToolCall {
            index,
            id: id.or_else(|| Some(format!("tool-call-{index}"))),
            name: name.unwrap_or_default(),
            arguments: arguments_delta.to_string(),
        });
        return;
    };
    if let Some(id) = id {
        existing.id = Some(id);
    } else if existing.id.is_none() {
        existing.id = Some(format!("tool-call-{}", existing.index));
    }
    if let Some(name) = name {
        existing.name = name;
    }
    if !arguments_delta.is_empty() {
        existing.arguments.push_str(arguments_delta);
    }
}

fn observation_history_message(content: String, tool_call_id: Option<String>) -> ModelMessage {
    match tool_call_id {
        Some(tool_call_id) => ModelMessage {
            role: "tool".to_string(),
            content,
            tool_call_id: Some(tool_call_id),
            ..Default::default()
        },
        None => ModelMessage {
            role: "system".to_string(),
            content,
            ..Default::default()
        },
    }
}

fn model_tool_definition(tool: &crate::types::Tool) -> ModelToolDefinition {
    ModelToolDefinition {
        name: tool.tool_id.clone(),
        description: Some(format!(
            "{} (actions: {})",
            tool.name,
            tool.supported_actions.join(", ")
        )),
        parameters_json: model_tool_parameters_schema(tool),
    }
}

fn model_tool_parameters_schema(tool: &crate::types::Tool) -> serde_json::Value {
    match tool.tool_id.as_str() {
        "github-readonly" => serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["ReadRepositoryMetadata", "ReadReadme", "ListRecentCommits"]
                },
                "repo_url": { "type": "string" },
                "max_bytes": { "type": "integer", "minimum": 1 }
            },
            "required": ["action", "repo_url"],
            "additionalProperties": true
        }),
        "file-readonly" => serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["ReadFile", "ListDirectory"]
                },
                "path": { "type": "string" },
                "allowed_paths": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "denied_paths": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "max_bytes": { "type": "integer", "minimum": 1 }
            },
            "required": ["action", "path"],
            "additionalProperties": true
        }),
        "http-get" => serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_bytes": { "type": "integer", "minimum": 1 }
            },
            "required": ["url"],
            "additionalProperties": true
        }),
        "workspace-write-file" => serde_json::json!({
            "type": "object",
            "properties": {
                "workspace_root": { "type": "string" },
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["workspace_root", "path", "content"],
            "additionalProperties": true
        }),
        "workspace-shell" => serde_json::json!({
            "type": "object",
            "properties": {
                "workspace_root": { "type": "string" },
                "command": { "type": "string" }
            },
            "required": ["workspace_root", "command"],
            "additionalProperties": true
        }),
        _ if tool.tool_type == crate::types::ToolType::ExternalExecutor => serde_json::json!({
            "type": "object",
            "properties": {
                "task": { "type": "object" }
            },
            "required": ["task"],
            "additionalProperties": true
        }),
        _ => serde_json::json!({
            "type": "object",
            "additionalProperties": true
        }),
    }
}

fn actual_or_estimated_cost_usd(
    routing: &ModelRoutingDecision,
    prompt_tokens: u64,
    completion_tokens: u64,
) -> f64 {
    // Prefer the shared, broadly-covering price book (OpenAI / Anthropic /
    // DeepSeek / Gemini / Qwen / GLM / Kimi / local). Known-free local models
    // return Some(0.0); unknown models fall back to the router's estimate.
    match coevo_models::pricing::estimate_cost_usd(
        &routing.selected_model_id,
        prompt_tokens,
        completion_tokens,
    ) {
        Some(cost) if cost > 0.0 => cost,
        Some(_) => 0.0,
        None => routing.estimated_cost_usd.unwrap_or(0.0),
    }
}

fn mission_requires_file_evidence(mission_intent: &str) -> bool {
    let lower = mission_intent.to_ascii_lowercase();
    (lower.contains("read") || lower.contains("inspect") || lower.contains("review"))
        && (lower.contains("file")
            || lower.contains("workspace")
            || lower.contains(".md")
            || lower.contains(".txt")
            || lower.contains("evidence"))
}

fn initial_tool_choice_for_mission(
    mission_intent: &str,
    saw_file_evidence_attempt: bool,
    _provider_kind: coevo_models::types::ModelProviderKind,
    model_tools: &[coevo_models::types::ModelToolDefinition],
) -> Option<serde_json::Value> {
    if model_tools.is_empty() {
        return None;
    }
    if mission_requires_file_evidence(mission_intent) && !saw_file_evidence_attempt {
        if model_tools.iter().any(|tool| tool.name == "file-readonly") {
            return Some(serde_json::json!({
                "type": "function",
                "function": {
                    "name": "file-readonly"
                }
            }));
        }
    }
    Some(serde_json::json!("auto"))
}

fn response_format_for_mission(
    mission_intent: &str,
    saw_file_evidence_attempt: bool,
) -> ResponseFormat {
    let _ = (mission_intent, saw_file_evidence_attempt);
    ResponseFormat::Json
}

fn route_for_step(
    run_contract: &AgentRunContract,
    authorization: &RunAuthorization,
    step_type: &str,
    required_capabilities: Vec<coevo_models::router::ModelCapability>,
    model_profiles: &[ModelProfile],
    max_latency_ms: Option<u64>,
) -> ModelRoutingDecision {
    let preferred_model_id =
        preferred_model_id_for_role(authorization.model_preference.as_deref(), model_profiles);
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
        preferred_model_id,
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

fn parse_reasoning_output(
    mut value: serde_json::Value,
    streamed_tool_calls: &[ModelToolCall],
    prefer_streamed_tool_call: bool,
) -> Result<ReasoningOutput, WorkerError> {
    if let Some(text) = value.as_str() {
        let extracted =
            extract_structured_json_text(text).unwrap_or_else(|| text.trim().to_string());
        value =
            serde_json::from_str(&extracted).map_err(|e| WorkerError::Internal(e.to_string()))?;
    }
    maybe_promote_native_tool_call(&mut value, streamed_tool_calls, prefer_streamed_tool_call)?;
    if value.get("thought").is_none() {
        let fallback_thought = value
            .get("summary")
            .cloned()
            .or_else(|| value.pointer("/proposal/question").cloned())
            .or_else(|| value.pointer("/proposal/summary").cloned())
            .unwrap_or_else(|| serde_json::json!("Model returned a structured action."));
        if let Some(obj) = value.as_object_mut() {
            obj.insert("thought".to_string(), fallback_thought);
        }
    }
    if value.get("confidence").is_none() {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("confidence".to_string(), serde_json::json!(0.5));
        }
    }
    let proposal_kind = value
        .get("proposal")
        .and_then(|proposal| proposal.as_str())
        .map(str::to_string);
    if let Some(kind) = proposal_kind {
        let normalized = match kind.as_str() {
            "call_tool" => match (
                value.get("tool_id").and_then(|value| value.as_str()),
                value.get("input").cloned(),
            ) {
                (Some(tool_id), Some(input)) => serde_json::json!({
                    "kind": "call_tool",
                    "tool_id": tool_id,
                    "input": input,
                    "rationale": proposal_rationale(&value, "call_tool"),
                }),
                _ => finish_for_incomplete_string_proposal(&kind, &["tool_id", "input"]),
            },
            "call_executor" => match (
                value.get("executor_id").and_then(|value| value.as_str()),
                value.get("task").cloned(),
            ) {
                (Some(executor_id), Some(task)) => serde_json::json!({
                    "kind": "call_executor",
                    "executor_id": executor_id,
                    "task": task,
                    "rationale": proposal_rationale(&value, "call_executor"),
                }),
                _ => finish_for_incomplete_string_proposal(&kind, &["executor_id", "task"]),
            },
            "spawn_subagent" => match (
                value.get("skill_id").and_then(|value| value.as_str()),
                value.get("task").and_then(|value| value.as_str()),
            ) {
                (Some(skill_id), Some(task)) => serde_json::json!({
                    "kind": "spawn_subagent",
                    "skill_id": skill_id,
                    "task": task,
                    "rationale": proposal_rationale(&value, "spawn_subagent"),
                }),
                _ => finish_for_incomplete_string_proposal(&kind, &["skill_id", "task"]),
            },
            "finish" => serde_json::json!({
                "kind": "finish",
                "summary": value
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .or_else(|| value.get("thought").and_then(|v| v.as_str()))
                    .unwrap_or("Done"),
                "result": value
                    .get("result")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
            }),
            "ask_human" => serde_json::json!({
                "kind": "ask_human",
                "question": value
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .or_else(|| value.get("thought").and_then(|v| v.as_str()))
                    .unwrap_or("Done"),
                "blocking": true,
            }),
            other => {
                return Err(WorkerError::Internal(format!(
                    "structured response proposal string is not supported: {other}"
                )));
            }
        };
        if let Some(proposal) = value.get_mut("proposal") {
            *proposal = normalized;
        }
    } else {
        let fallback_text = value
            .get("summary")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("thought").and_then(|v| v.as_str()))
            .unwrap_or("Done")
            .to_string();
        if let Some(proposal) = value
            .get_mut("proposal")
            .and_then(|proposal| proposal.as_object_mut())
        {
            let kind = proposal
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            match kind {
                "finish" => {
                    if !proposal.contains_key("summary") {
                        proposal.insert("summary".to_string(), serde_json::json!(fallback_text));
                    }
                    proposal
                        .entry("result".to_string())
                        .or_insert_with(|| serde_json::json!({}));
                }
                "ask_human" => {
                    if !proposal.contains_key("question") {
                        proposal.insert("question".to_string(), serde_json::json!(fallback_text));
                    }
                    proposal
                        .entry("blocking".to_string())
                        .or_insert_with(|| serde_json::json!(true));
                }
                _ => {}
            }
        }
        let fallback_summary = value
            .get("summary")
            .cloned()
            .or_else(|| value.get("thought").cloned())
            .unwrap_or_else(|| serde_json::json!("Done"));
        let fallback_result = value
            .get("result")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if value.get("proposal").is_none() {
            if let Some(obj) = value.as_object_mut() {
                obj.insert(
                    "proposal".to_string(),
                    serde_json::json!({
                        "kind": "finish",
                        "summary": fallback_summary,
                        "result": fallback_result,
                    }),
                );
            }
        } else if let Some(proposal) = value.get_mut("proposal") {
            if let Some(obj) = proposal.as_object_mut() {
                normalize_incomplete_action_object(obj, &fallback_summary, &fallback_result);
                if !obj.contains_key("kind") {
                    let inferred = if obj.contains_key("tool_id") {
                        "call_tool"
                    } else if obj.contains_key("executor_id") {
                        "call_executor"
                    } else if obj
                        .get("question")
                        .and_then(|value| value.as_str())
                        .is_some_and(|question| !is_generic_structured_placeholder(question))
                    {
                        "ask_human"
                    } else {
                        "finish"
                    };
                    obj.insert("kind".to_string(), serde_json::json!(inferred));
                    match inferred {
                        "finish" => {
                            obj.entry("summary".to_string())
                                .or_insert_with(|| fallback_summary.clone());
                            obj.entry("result".to_string())
                                .or_insert_with(|| fallback_result.clone());
                        }
                        "ask_human" => {
                            obj.entry("question".to_string())
                                .or_insert_with(|| fallback_summary.clone());
                            obj.entry("blocking".to_string())
                                .or_insert_with(|| serde_json::json!(true));
                        }
                        _ => {}
                    }
                    normalize_incomplete_action_object(obj, &fallback_summary, &fallback_result);
                }
            }
        }
    }
    serde_json::from_value(value).map_err(|e| WorkerError::Internal(e.to_string()))
}

fn maybe_promote_native_tool_call(
    value: &mut serde_json::Value,
    streamed_tool_calls: &[ModelToolCall],
    prefer_streamed_tool_call: bool,
) -> Result<(), WorkerError> {
    if streamed_tool_calls.is_empty() {
        return Ok(());
    }
    let explicit_kind = value
        .get("proposal")
        .and_then(|proposal| proposal.get("kind"))
        .and_then(|kind| kind.as_str());
    let should_promote = match explicit_kind {
        Some("finish") => prefer_streamed_tool_call,
        Some("call_tool" | "call_executor" | "ask_human") => false,
        Some(_) => false,
        None => true,
    };
    if !should_promote {
        return Ok(());
    }

    let tool_call = &streamed_tool_calls[0];
    if tool_call.name.is_empty() {
        return Ok(());
    }
    let input: serde_json::Value = serde_json::from_str(&tool_call.arguments).map_err(|e| {
        WorkerError::Internal(format!(
            "native tool call arguments were not valid JSON: {e}"
        ))
    })?;
    let summary = format!("Execute native tool call via {}.", tool_call.name);
    let thought = value
        .get("thought")
        .cloned()
        .unwrap_or_else(|| serde_json::json!("Model emitted a native tool call."));
    let confidence = value
        .get("confidence")
        .cloned()
        .unwrap_or_else(|| serde_json::json!(0.5));
    *value = serde_json::json!({
        "thought": thought,
        "confidence": confidence,
        "proposal": {
            "kind": "call_tool",
            "tool_id": tool_call.name,
            "input": input,
            "rationale": summary,
        }
    });
    Ok(())
}

fn normalize_incomplete_action_object(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    fallback_summary: &serde_json::Value,
    fallback_result: &serde_json::Value,
) {
    let kind = obj.get("kind").and_then(|value| value.as_str());
    let incomplete = matches!(kind, Some("call_tool")) && !obj.contains_key("input")
        || matches!(kind, Some("call_executor")) && !obj.contains_key("task");
    if incomplete {
        obj.clear();
        obj.insert("kind".to_string(), serde_json::json!("finish"));
        obj.insert("summary".to_string(), fallback_summary.clone());
        obj.insert("result".to_string(), fallback_result.clone());
    }
}

fn proposal_rationale(value: &serde_json::Value, fallback_kind: &str) -> String {
    value
        .get("rationale")
        .and_then(|value| value.as_str())
        .or_else(|| value.get("summary").and_then(|value| value.as_str()))
        .or_else(|| value.get("thought").and_then(|value| value.as_str()))
        .unwrap_or(fallback_kind)
        .to_string()
}

fn finish_for_incomplete_string_proposal(kind: &str, missing_fields: &[&str]) -> serde_json::Value {
    let missing_fields_text = missing_fields.join(", ");
    serde_json::json!({
        "kind": "finish",
        "summary": format!(
            "Invalid structured response: proposal \"{kind}\" is missing required fields: {missing_fields_text}."
        ),
        "result": {
            "error": format!(
                "proposal \"{kind}\" is missing required fields: {missing_fields_text}"
            ),
            "proposal": kind,
            "missing_fields": missing_fields,
        },
    })
}

fn is_generic_structured_placeholder(text: &str) -> bool {
    let normalized = text.trim().trim_matches('"').to_ascii_lowercase();
    normalized.is_empty()
        || normalized == "model returned a structured action."
        || normalized == "model returned a structured action"
}

fn preferred_model_id_for_role(
    model_preference: Option<&str>,
    model_profiles: &[ModelProfile],
) -> Option<String> {
    let label = match model_preference {
        Some("fast") => "fast",
        Some("standard") | Some("default") => "default",
        Some("reasoning") => "reasoning",
        _ => return None,
    };
    model_profiles
        .iter()
        .find(|profile| profile.display_name.to_ascii_lowercase().contains(label))
        .map(|profile| profile.model_id.clone())
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

fn order_tools_by_preference<'a>(
    tools: Vec<&'a crate::types::Tool>,
    preferred_tool_ids: &[String],
) -> Vec<&'a crate::types::Tool> {
    let mut tools = tools;
    tools.sort_by(|left, right| {
        let left_rank = preferred_tool_ids
            .iter()
            .position(|tool_id| tool_id == &left.tool_id)
            .unwrap_or(usize::MAX);
        let right_rank = preferred_tool_ids
            .iter()
            .position(|tool_id| tool_id == &right.tool_id)
            .unwrap_or(usize::MAX);
        left_rank
            .cmp(&right_rank)
            .then_with(|| left.tool_id.cmp(&right.tool_id))
    });
    tools
}

fn estimate_history_tokens(history: &[ModelMessage]) -> u32 {
    history
        .iter()
        .map(|message| (message.content.len() as u32).div_ceil(4))
        .sum()
}

async fn is_run_cancelled(
    pool: &SqlitePool,
    run_id: &str,
    session_id: &str,
) -> Result<bool, WorkerError> {
    if crate::worker_cancel::is_run_cancelled(run_id) {
        return Ok(true);
    }
    let run_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM worker_runs WHERE run_id=?")
            .bind(run_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
    if matches!(run_status.as_deref(), Some("Cancelled")) {
        return Ok(true);
    }
    let session_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM worker_sessions WHERE session_id=?")
            .bind(session_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
    Ok(matches!(session_status.as_deref(), Some("Cancelled")))
}

#[derive(Debug)]
struct StreamedNativeToolGate {
    gate: GateOutcome,
    allowed_calls: Vec<ModelToolCall>,
}

async fn adjudicate_streamed_native_tool_calls(
    govern_gate: &GovernGate,
    streamed_tool_calls: &[ModelToolCall],
    visible_tools: &[&crate::types::Tool],
    all_tools: &[crate::types::Tool],
    authorization: &RunAuthorization,
) -> StreamedNativeToolGate {
    let mut allowed_calls = Vec::new();
    for call in streamed_tool_calls {
        if !visible_tools.iter().any(|tool| tool.tool_id == call.name) {
            return StreamedNativeToolGate {
                gate: GateOutcome::Deny {
                    reason: format!("Tool {} is not permitted for this run", call.name),
                },
                allowed_calls: Vec::new(),
            };
        }
        let input = match serde_json::from_str::<serde_json::Value>(&call.arguments) {
            Ok(value) => value,
            Err(e) => {
                return StreamedNativeToolGate {
                    gate: GateOutcome::Deny {
                        reason: format!("Tool {} arguments are not valid JSON: {e}", call.name),
                    },
                    allowed_calls: Vec::new(),
                };
            }
        };
        let proposal = ActionProposal::CallTool {
            tool_id: call.name.clone(),
            input,
            rationale: "streamed model tool call".to_string(),
        };
        match govern_gate
            .adjudicate(&proposal, authorization, all_tools)
            .await
        {
            GateOutcome::Allow => allowed_calls.push(call.clone()),
            other => {
                return StreamedNativeToolGate {
                    gate: other,
                    allowed_calls: Vec::new(),
                };
            }
        }
    }
    StreamedNativeToolGate {
        gate: GateOutcome::Allow,
        allowed_calls,
    }
}
struct ExecutedNativeToolCall {
    tool_call: ModelToolCall,
    input: serde_json::Value,
    output: serde_json::Value,
    success: bool,
}

async fn execute_native_tool_call(
    registry: &Arc<ToolRegistry>,
    tool_call: ModelToolCall,
    tool_id: String,
) -> Result<ExecutedNativeToolCall, WorkerError> {
    let input: serde_json::Value = match serde_json::from_str(&tool_call.arguments) {
        Ok(input) => input,
        Err(e) => {
            let output = serde_json::json!({
                "error": format!("native tool call arguments were not valid JSON: {e}")
            });
            return Ok(ExecutedNativeToolCall {
                tool_call,
                input: serde_json::json!({}),
                output,
                success: false,
            });
        }
    };
    let output = registry
        .execute(&tool_id, input.clone())
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    let success = output.get("error").is_none();
    Ok(ExecutedNativeToolCall {
        tool_call,
        input,
        output,
        success,
    })
}

async fn compact_history_with_model(
    gateway: &dyn ModelGateway,
    provider_config: &ModelProviderConfig,
    model_profiles: &[ModelProfile],
    run_contract: &AgentRunContract,
    authorization: &RunAuthorization,
    history: &[ModelMessage],
    prompt: &crate::r#loop::PromptBundle,
    history_budget: u32,
) -> Result<Option<crate::r#loop::CompactedHistory>, WorkerError> {
    if history.is_empty() {
        return Ok(None);
    }
    let mut caps = vec![
        ModelCapability::Summarization,
        ModelCapability::StructuredJSON,
        ModelCapability::LongContext,
    ];
    caps.sort_by_key(|capability| format!("{capability:?}"));
    caps.dedup();
    let routing = route_for_step(
        run_contract,
        authorization,
        "ContextCompaction",
        caps,
        model_profiles,
        None,
    );
    if routing.selected_model_id == "unavailable" {
        return Ok(None);
    }
    let mut messages = prompt.stable_prefix.clone();
    messages.push(ModelMessage {
        role: "system".to_string(),
        content: "Summarize the governed history into one compact system message that preserves tool outcomes, approvals, denials, and unresolved follow-up actions. Return JSON with summary and provenance fields.".to_string(),
        ..Default::default()
    });
    messages.push(ModelMessage {
        role: "user".to_string(),
        content: serde_json::json!({
            "mission_intent": run_contract.mission_intent,
            "work_order_id": run_contract.work_order_id,
            "track": authorization.track,
            "history_budget": history_budget,
            "history": history,
        })
        .to_string(),
        ..Default::default()
    });
    let request = ModelRequest {
        config: provider_config.clone(),
        role: coevo_models::types::ModelRole::StructuredOutput,
        model: routing.selected_model_id.clone(),
        messages,
        temperature: 0.2,
        max_tokens: provider_config.max_tokens.min(1024),
        response_format: ResponseFormat::Json,
        stream: false,
        tools: vec![],
        tool_choice: None,
    };
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "summary": {"type": "string"},
            "provenance": {"type": "array", "items": {"type": "string"}},
            "dropped_message_count": {"type": "integer", "minimum": 0}
        },
        "required": ["summary"],
        "additionalProperties": true
    });
    let response = match gateway.structured(&request, &schema).await {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };
    let summary_text = response
        .json
        .as_ref()
        .and_then(|value| value.get("summary"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            let extracted = extract_structured_json_text(&response.content).unwrap_or_default();
            serde_json::from_str::<serde_json::Value>(&extracted)
                .ok()
                .and_then(|value| {
                    value
                        .get("summary")
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim().to_string())
                })
        });
    let Some(summary_text) = summary_text else {
        return Ok(None);
    };
    let provenance = response
        .json
        .as_ref()
        .and_then(|value| value.get("provenance"))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(|value| value.to_string()))
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| vec!["compaction:model-summary-v1".to_string()]);
    let dropped_message_count = response
        .json
        .as_ref()
        .and_then(|value| value.get("dropped_message_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(history.len() as u64) as usize;
    Ok(Some(crate::r#loop::CompactedHistory {
        summary: ModelMessage {
            role: "system".to_string(),
            content: format!(
                "Compacted governed history summary ({} messages): {}",
                history.len(),
                summary_text
            ),
            ..Default::default()
        },
        provenance,
        dropped_message_count,
    }))
}

/// Run a bounded, governed sub-agent on behalf of a department head.
///
/// The sub-agent is an ephemeral, single-skill, read-only *reasoning* helper: it
/// performs one focused model call using the delegated skill's directive and
/// returns a concrete contribution that the head folds into its next step. It has
/// no tool, executor, or spawn authority of its own (so there is no governance
/// bypass and no recursion), and it inherits the head's model routing. Returns the
/// sub-agent's contribution text, or an error the caller surfaces as an observation.
#[allow(clippy::too_many_arguments)]
async fn run_governed_subagent(
    gateway: &dyn ModelGateway,
    provider_config: &ModelProviderConfig,
    model_profiles: &[ModelProfile],
    run_contract: &AgentRunContract,
    authorization: &RunAuthorization,
    subagent_id: &str,
    skill_id: &str,
    skill_directive: &str,
    task: &str,
) -> Result<String, WorkerError> {
    let mut caps = vec![
        ModelCapability::StructuredJSON,
        ModelCapability::DeepReasoning,
    ];
    caps.sort_by_key(|capability| format!("{capability:?}"));
    caps.dedup();
    let routing = route_for_step(
        run_contract,
        authorization,
        "SubagentReasoning",
        caps,
        model_profiles,
        None,
    );
    if routing.selected_model_id == "unavailable" {
        return Err(WorkerError::Internal(
            "no model available for subagent".to_string(),
        ));
    }
    let directive = if skill_directive.trim().is_empty() {
        format!("Apply your '{skill_id}' skill.")
    } else {
        skill_directive.trim().to_string()
    };
    let system = format!(
        "You are sub-agent {subagent_id}, an ephemeral single-skill helper created by a \
         department head. Your only skill is '{skill_id}'. You reason and analyze only: you \
         have NO authority to call tools, executors, or spawn further agents. Produce a \
         focused, concrete contribution for the delegated task that the head can act on. \
         Skill directive: {directive}"
    );
    let messages = vec![
        ModelMessage {
            role: "system".to_string(),
            content: system,
            ..Default::default()
        },
        ModelMessage {
            role: "user".to_string(),
            content: serde_json::json!({
                "mission_intent": run_contract.mission_intent,
                "delegated_task": task,
                "skill_id": skill_id,
            })
            .to_string(),
            ..Default::default()
        },
    ];
    let request = ModelRequest {
        config: provider_config.clone(),
        role: coevo_models::types::ModelRole::StructuredOutput,
        model: routing.selected_model_id.clone(),
        messages,
        temperature: 0.2,
        max_tokens: provider_config.max_tokens.min(800),
        response_format: ResponseFormat::Json,
        stream: false,
        tools: vec![],
        tool_choice: None,
    };
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "contribution": {"type": "string"},
            "key_findings": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["contribution"],
        "additionalProperties": true
    });
    let response = gateway
        .structured(&request, &schema)
        .await
        .map_err(|e| WorkerError::Internal(format!("subagent model call failed: {e}")))?;
    let contribution = response
        .json
        .as_ref()
        .and_then(|value| value.get("contribution"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            let extracted = extract_structured_json_text(&response.content).unwrap_or_default();
            serde_json::from_str::<serde_json::Value>(&extracted)
                .ok()
                .and_then(|value| {
                    value
                        .get("contribution")
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim().to_string())
                })
        })
        .filter(|value| !value.is_empty());
    contribution.ok_or_else(|| {
        WorkerError::Internal("subagent returned no usable contribution".to_string())
    })
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

fn parse_approval_receipt_proof(proof: &str) -> (&str, Option<&str>) {
    match proof.split_once(':') {
        Some((receipt_id, digest))
            if !receipt_id.trim().is_empty() && !digest.trim().is_empty() =>
        {
            (receipt_id.trim(), Some(digest.trim()))
        }
        _ => (proof.trim(), None),
    }
}

async fn load_resume_cursor(
    pool: &SqlitePool,
    authorization: &RunAuthorization,
) -> Result<Option<String>, WorkerError> {
    let Some(approval_receipt) = authorization.approval_receipt.as_deref() else {
        return Ok(None);
    };
    let (_, provided_digest) = parse_approval_receipt_proof(approval_receipt);
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
    let round = cursor
        .get("round")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let digest = cursor
        .get("pending_action_digest")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    match provided_digest {
        Some(candidate) if candidate == digest => {}
        _ => return Err(WorkerError::YellowApprovalRequired),
    }
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
    let now = chrono::Utc::now().timestamp_millis();
    step_create_timed(pool, steps, run_id, step_type, input, output, (now, now)).await
}

async fn step_create_timed(
    pool: &SqlitePool,
    steps: &mut Vec<serde_json::Value>,
    run_id: &str,
    step_type: &str,
    input: &serde_json::Value,
    output: Option<&serde_json::Value>,
    timing_ms: (i64, i64),
) -> Result<String, WorkerError> {
    let idx = steps.len() as i64;
    let sid = format!("s-{}-{}", &run_id[..8.min(run_id.len())], idx);
    let (started_at_ms, ended_at_ms) = timing_ms;
    sqlx::query("INSERT INTO worker_steps VALUES (?,?,?,?,?,?,?,?,?,?)")
        .bind(&sid)
        .bind(run_id)
        .bind(idx)
        .bind(step_type)
        .bind(serde_json::to_string(input).unwrap())
        .bind(output.map(|o| serde_json::to_string(o).unwrap()))
        .bind("Completed")
        .bind(started_at_ms)
        .bind(Some(ended_at_ms))
        .bind(Option::<String>::None)
        .execute(pool)
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;
    steps.push(serde_json::json!({
        "step_id": sid,
        "run_id": run_id,
        "step_index": idx,
        "step_type": step_type,
        "input_json": input,
        "output_json": output.cloned(),
        "status": "Completed",
        "started_at_ms": started_at_ms,
        "ended_at_ms": ended_at_ms,
        "error": null
    }));
    Ok(sid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::r#loop::{ExternalAgentRunResult, ExternalProducedItem};
    use async_trait::async_trait;
    use coevo_models::gateway::ModelGateway;
    use coevo_models::router::default_model_profiles;
    use coevo_models::types::{
        ModelDiscoveryResponse, ModelError, ModelMessage, ModelProviderConfig, ModelResponse,
        ModelStreamEvent, ModelUsage,
    };
    use coevo_store::company_workspace::CompanyWorkspaceManager;
    use coevo_store::migrate::run_migrations;
    use coevo_store::pool::create_test_pool;
    use coevo_store::repos::worker_run_repo::WorkerRunRepo;
    use coevo_store::repos_opc::{agent_employee_repo::AgentEmployeeRepo, skill_repo::SkillRepo};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct ScriptedGateway {
        outputs: Arc<Mutex<Vec<serde_json::Value>>>,
        seen_messages: Arc<Mutex<Vec<Vec<ModelMessage>>>>,
        seen_requests: Arc<Mutex<Vec<coevo_models::types::ModelRequest>>>,
        streamed_tool_calls: Arc<Mutex<Vec<Vec<ModelToolCall>>>>,
        stream_json: Arc<Mutex<Vec<Option<serde_json::Value>>>>,
        stream_reasoning: Arc<Mutex<Vec<Option<String>>>>,
        stream_completion_hook: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    }

    impl ScriptedGateway {
        fn new(outputs: Vec<serde_json::Value>) -> Self {
            Self {
                outputs: Arc::new(Mutex::new(outputs)),
                seen_messages: Arc::new(Mutex::new(vec![])),
                seen_requests: Arc::new(Mutex::new(vec![])),
                streamed_tool_calls: Arc::new(Mutex::new(vec![])),
                stream_json: Arc::new(Mutex::new(vec![])),
                stream_reasoning: Arc::new(Mutex::new(vec![])),
                stream_completion_hook: Arc::new(Mutex::new(None)),
            }
        }

        fn with_streamed_tool_calls(self, tool_calls: Vec<Vec<ModelToolCall>>) -> Self {
            *self.streamed_tool_calls.lock().unwrap() = tool_calls;
            self
        }

        fn with_stream_json(self, payloads: Vec<Option<serde_json::Value>>) -> Self {
            *self.stream_json.lock().unwrap() = payloads;
            self
        }

        fn with_stream_reasoning(self, payloads: Vec<Option<String>>) -> Self {
            *self.stream_reasoning.lock().unwrap() = payloads;
            self
        }

        fn with_stream_completion_hook<F>(self, hook: F) -> Self
        where
            F: Fn() + Send + Sync + 'static,
        {
            *self.stream_completion_hook.lock().unwrap() = Some(Arc::new(hook));
            self
        }
    }

    struct FailingGateway;

    #[derive(Clone)]
    struct CancellingDuringStreamGateway {
        run_id: String,
        seen_requests: Arc<Mutex<Vec<coevo_models::types::ModelRequest>>>,
    }

    impl CancellingDuringStreamGateway {
        fn new(run_id: impl Into<String>) -> Self {
            Self {
                run_id: run_id.into(),
                seen_requests: Arc::new(Mutex::new(vec![])),
            }
        }
    }

    #[async_trait]
    impl ModelGateway for FailingGateway {
        async fn test_connection(
            &self,
            _config: &ModelProviderConfig,
        ) -> Result<ModelResponse, ModelError> {
            Err(ModelError::InvalidResponse(
                "test_connection failed".to_string(),
            ))
        }

        async fn discover_models(
            &self,
            _config: &ModelProviderConfig,
        ) -> Result<ModelDiscoveryResponse, ModelError> {
            Err(ModelError::InvalidResponse(
                "discover_models failed".to_string(),
            ))
        }

        async fn chat(
            &self,
            _request: &coevo_models::types::ModelRequest,
        ) -> Result<ModelResponse, ModelError> {
            Err(ModelError::InvalidResponse("chat failed".to_string()))
        }

        async fn structured(
            &self,
            _request: &coevo_models::types::ModelRequest,
            _schema_json: &serde_json::Value,
        ) -> Result<ModelResponse, ModelError> {
            Err(ModelError::InvalidResponse("structured failed".to_string()))
        }

        async fn stream(
            &self,
            _request: &coevo_models::types::ModelRequest,
            _schema_json: Option<&serde_json::Value>,
            _on_event: &mut coevo_models::gateway::ModelStreamHandler<'_>,
        ) -> Result<ModelResponse, ModelError> {
            Err(ModelError::InvalidResponse("stream failed".to_string()))
        }
    }

    #[async_trait]
    impl ModelGateway for CancellingDuringStreamGateway {
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

        async fn chat(
            &self,
            _request: &coevo_models::types::ModelRequest,
        ) -> Result<ModelResponse, ModelError> {
            unreachable!("agent harness tests do not call chat")
        }

        async fn structured(
            &self,
            _request: &coevo_models::types::ModelRequest,
            _schema_json: &serde_json::Value,
        ) -> Result<ModelResponse, ModelError> {
            unreachable!("agent harness tests do not call structured")
        }

        async fn stream(
            &self,
            request: &coevo_models::types::ModelRequest,
            _schema_json: Option<&serde_json::Value>,
            on_event: &mut coevo_models::gateway::ModelStreamHandler<'_>,
        ) -> Result<ModelResponse, ModelError> {
            self.seen_requests.lock().unwrap().push(request.clone());
            on_event(ModelStreamEvent::ContentDelta {
                delta: "{\"thought\":\"still thinking\"}".to_string(),
            })
            .await?;
            let _ = crate::worker_cancel::cancel_run(&self.run_id);
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            on_event(ModelStreamEvent::Usage(ModelUsage {
                prompt_tokens: 17,
                completion_tokens: 11,
                total_tokens: 28,
            }))
            .await?;
            on_event(ModelStreamEvent::Done {
                finish_reason: Some("stop".to_string()),
            })
            .await?;
            Ok(ModelResponse {
                content: String::new(),
                json: Some(serde_json::json!({
                    "thought": "The response should be cancelled before it is used.",
                    "proposal": {
                        "kind": "finish",
                        "summary": "cancelled",
                        "result": {"ok": true}
                    },
                    "confidence": 0.1
                })),
                usage: ModelUsage {
                    prompt_tokens: 17,
                    completion_tokens: 11,
                    total_tokens: 28,
                },
                latency_ms: 1,
                model: request.model.clone(),
                finish_reason: "stop".to_string(),
                provider_kind: request.config.kind,
                reasoning_content: None,
                tool_calls: vec![],
            })
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

        async fn chat(
            &self,
            _request: &coevo_models::types::ModelRequest,
        ) -> Result<ModelResponse, ModelError> {
            unreachable!("agent harness tests do not call chat")
        }

        async fn structured(
            &self,
            request: &coevo_models::types::ModelRequest,
            _schema_json: &serde_json::Value,
        ) -> Result<ModelResponse, ModelError> {
            self.seen_requests.lock().unwrap().push(request.clone());
            self.seen_messages
                .lock()
                .unwrap()
                .push(request.messages.clone());
            let next = {
                let mut outputs = self.outputs.lock().unwrap();
                if outputs.is_empty() {
                    serde_json::json!({
                        "thought": "No scripted output was queued for this test gateway call.",
                        "proposal": {
                            "kind": "finish",
                            "summary": "Default scripted gateway finish.",
                            "result": {"ok": true}
                        },
                        "confidence": 0.5
                    })
                } else {
                    outputs.remove(0)
                }
            };
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
                reasoning_content: None,
                tool_calls: vec![],
            })
        }

        async fn stream(
            &self,
            request: &coevo_models::types::ModelRequest,
            schema_json: Option<&serde_json::Value>,
            on_event: &mut coevo_models::gateway::ModelStreamHandler<'_>,
        ) -> Result<ModelResponse, ModelError> {
            self.seen_requests.lock().unwrap().push(request.clone());
            let tool_calls = {
                let mut guard = self.streamed_tool_calls.lock().unwrap();
                if guard.is_empty() {
                    vec![]
                } else {
                    guard.remove(0)
                }
            };
            for tool_call in &tool_calls {
                if let Some(name) = (!tool_call.name.is_empty()).then_some(tool_call.name.clone()) {
                    on_event(ModelStreamEvent::ToolCallDelta {
                        index: tool_call.index,
                        id: tool_call.id.clone(),
                        name: Some(name),
                        arguments_delta: tool_call.arguments.clone(),
                    })
                    .await?;
                }
            }
            let response = if self.stream_json.lock().unwrap().is_empty() {
                self.structured(request, schema_json.unwrap_or(&serde_json::json!({})))
                    .await?
            } else {
                self.seen_messages
                    .lock()
                    .unwrap()
                    .push(request.messages.clone());
                let json = self.stream_json.lock().unwrap().remove(0);
                let reasoning_content = if self.stream_reasoning.lock().unwrap().is_empty() {
                    None
                } else {
                    self.stream_reasoning.lock().unwrap().remove(0)
                };
                ModelResponse {
                    content: json
                        .as_ref()
                        .map(|value| serde_json::to_string(value).unwrap())
                        .unwrap_or_default(),
                    json,
                    usage: ModelUsage {
                        prompt_tokens: 17,
                        completion_tokens: 11,
                        total_tokens: 28,
                    },
                    latency_ms: 1,
                    model: request.model.clone(),
                    finish_reason: if tool_calls.is_empty() {
                        "stop".to_string()
                    } else {
                        "tool_calls".to_string()
                    },
                    provider_kind: request.config.kind,
                    reasoning_content,
                    tool_calls: tool_calls.clone(),
                }
            };
            on_event(ModelStreamEvent::ContentDelta {
                delta: response.content.clone(),
            })
            .await?;
            if let Some(reasoning_content) = &response.reasoning_content {
                on_event(ModelStreamEvent::ReasoningDelta {
                    delta: reasoning_content.clone(),
                })
                .await?;
            }
            on_event(ModelStreamEvent::Usage(response.usage.clone())).await?;
            on_event(ModelStreamEvent::Done {
                finish_reason: Some(response.finish_reason.clone()),
            })
            .await?;
            let hook = self.stream_completion_hook.lock().unwrap().clone();
            if let Some(hook) = hook {
                hook();
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Ok(response)
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

    fn test_mcl_contract(work_order_id: &str) -> MCLSpec {
        MCLSpec {
            mcl_version: "1.0".to_string(),
            mcl_state: coevo_core::contract::ContractState::ActiveContract,
            parent_contract_hash: "0".repeat(64),
            goal_tree: coevo_core::contract::GoalTree {
                root: coevo_core::contract::GoalNode {
                    id: work_order_id.to_string(),
                    description: "Test governed work".to_string(),
                    status: coevo_core::contract::GoalStatus::InProgress,
                    children: vec![],
                    depends_on: vec![],
                },
            },
            institution_policy_hash: "b".repeat(64),
            data_boundary: vec!["urn:coevo:data:workspace".to_string()],
            allowed_action_modes: vec![coevo_core::contract::ActionMode::DraftOnly],
            human_approval_policy: coevo_core::contract::HumanApprovalPolicy {
                approval_mode: coevo_core::contract::ApprovalMode::ExplicitApproval,
                authorized_roles: vec!["founder".to_string()],
                negative_consent_timeout_secs: 0,
                mfa_auth_url: None,
            },
            evidence_requirement: coevo_core::contract::EvidenceRequirement {
                minimum_level: "self_report".to_string(),
                require_json_report: false,
            },
            risk_tolerance_profile: coevo_core::contract::RiskToleranceProfile {
                max_risk_score: 0.3,
                allow_emergency_lease: false,
            },
            termination_policy: coevo_core::contract::TerminationPolicy {
                max_token_budget: 10_000,
                max_hops: 16,
                max_latency_ms: 60_000,
                max_stance_rounds: 16,
            },
            responsibility_anchor_policy: coevo_core::contract::ResponsibilityAnchorPolicy {
                required_human_roles: vec!["founder".to_string()],
                agent_forbidden_actions: vec![],
            },
        }
    }

    fn memory_context() -> crate::types::MemoryContext {
        crate::types::MemoryContext {
            user_profile: None,
            company_profile: vec![],
            company_memory: vec![],
            company_shared_files: vec![],
            employee_persona_files: vec![],
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
        test_contract("wo-context", "Analyze evidence")
    }

    fn test_auth(
        work_order_id: &str,
        run_id: &str,
        restricted_actions: Vec<String>,
    ) -> RunAuthorization {
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
            model_preference: None,
            execution_contract: None,
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
            execution_contract: None,
        }
    }

    fn harness_tool(
        id: &str,
        tool_type: crate::types::ToolType,
        risk_ceiling: f64,
        actions: Vec<&str>,
    ) -> crate::types::Tool {
        crate::types::Tool {
            tool_id: id.to_string(),
            name: id.to_string(),
            tool_type,
            risk_ceiling,
            supported_actions: actions.into_iter().map(ToString::to_string).collect(),
            permission_boundary_json: serde_json::json!({}),
            requires_credential: false,
            credential_ref: None,
            enabled: true,
        }
    }

    fn streamed_call(index: usize, name: &str, arguments: serde_json::Value) -> ModelToolCall {
        ModelToolCall {
            index,
            id: Some(format!("call-{index}")),
            name: name.to_string(),
            arguments: serde_json::to_string(&arguments).unwrap(),
        }
    }

    #[tokio::test]
    async fn streamed_native_tool_calls_fail_closed_when_any_call_is_not_visible() {
        let auth = test_auth("wo-multi-tool-gate", "run-multi-tool-gate", vec![]);
        let gate = GovernGate::default_for_authorization(&auth);
        let read_tool = harness_tool(
            "file-readonly",
            crate::types::ToolType::FileReadonly,
            0.3,
            vec!["ReadFile"],
        );
        let shell_tool = harness_tool(
            "workspace-shell",
            crate::types::ToolType::LocalProcessSandbox,
            0.6,
            vec!["RunShell"],
        );
        let all_tools = vec![read_tool.clone(), shell_tool];
        let visible_tools = vec![&read_tool];
        let calls = vec![
            streamed_call(0, "file-readonly", serde_json::json!({"path":"README.md"})),
            streamed_call(
                1,
                "workspace-shell",
                serde_json::json!({"command":"whoami"}),
            ),
        ];

        let result =
            adjudicate_streamed_native_tool_calls(&gate, &calls, &visible_tools, &all_tools, &auth)
                .await;

        assert!(matches!(result.gate, GateOutcome::Deny { .. }));
        assert!(result.allowed_calls.is_empty());
    }
    async fn migrated_pool() -> sqlx::SqlitePool {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        AgentEmployeeRepo::seed(&pool).await.unwrap();
        SkillRepo::seed_default(&pool).await.unwrap();
        pool
    }

    async fn configure_active_openai_compatible(pool: &sqlx::SqlitePool) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("desktop-test")
            .bind("OpenAICompatible")
            .bind("https://api.openai.com/v1")
            .bind("sk-test")
            .bind("sk-t****test")
            .bind("gpt-4o")
            .bind("gpt-4o-mini")
            .bind("o3-mini")
            .bind("gpt-4o")
            .bind(16384)
            .bind(0.2)
            .bind(30000)
            .bind(5.0)
            .bind(1)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
    }

    fn test_coevo_home(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("coevo-worker-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    async fn create_worker_session(pool: &sqlx::SqlitePool, auth: &RunAuthorization) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO worker_sessions (
                session_id,
                opc_id,
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
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&auth.session_id)
        .bind("default-opc")
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
        let root =
            std::env::temp_dir().join(format!("coevo-model-picks-tool-{}", uuid::Uuid::new_v4()));
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
            &test_contract(
                work_order_id,
                "Inspect the provided launch evidence and summarize it.",
            ),
            &test_auth(work_order_id, run_id, vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
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
    async fn employee_skill_file_overrides_company_and_db_prompt_template() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("employee-skill-override");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-skill-override";
        std::fs::create_dir_all(workspace.company_dir(opc_id)).unwrap();
        std::fs::create_dir_all(workspace.company_skills_dir(opc_id)).unwrap();
        workspace
            .ensure_company_employee_skeleton(opc_id, "agent-founder-01")
            .unwrap();

        let mut skill = SkillRepo::get(&pool, "skill-mission-draft", None)
            .await
            .unwrap()
            .unwrap();
        skill.prompt_template = "db fallback directive".to_string();
        SkillRepo::upsert(&pool, &skill).await.unwrap();

        std::fs::create_dir_all(workspace.company_skill_dir(opc_id, "skill-mission-draft"))
            .unwrap();
        std::fs::write(
            workspace.company_skill_markdown_path(opc_id, "skill-mission-draft"),
            "company directive",
        )
        .unwrap();
        std::fs::create_dir_all(workspace.company_employee_skill_dir(
            opc_id,
            "agent-founder-01",
            "skill-mission-draft",
        ))
        .unwrap();
        std::fs::write(
            workspace.company_employee_skill_markdown_path(
                opc_id,
                "agent-founder-01",
                "skill-mission-draft",
            ),
            "employee directive",
        )
        .unwrap();

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish immediately.",
            "proposal": {
                "kind": "finish",
                "summary": "Done.",
                "result": {"ok": true}
            },
            "confidence": 0.95
        })]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &pool,
            coevo_home.clone(),
            &AgentRunContract {
                opc_id: opc_id.to_string(),
                ..test_contract("wo-skill-file-override", "Use the selected skill.")
            },
            &test_auth(
                "wo-skill-file-override",
                "run-skill-file-override",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(coevo_home).ok();

        assert_eq!(result.final_status, "Completed");
        let first_prompt = seen_messages.lock().unwrap()[0]
            .iter()
            .find(|message| {
                message.role == "system" && message.content.contains("active_skill_directives")
            })
            .expect("skill directives should be injected")
            .content
            .clone();
        assert!(first_prompt.contains("employee directive"));
        assert!(!first_prompt.contains("company directive"));
        assert!(!first_prompt.contains("db fallback directive"));
    }

    #[tokio::test]
    async fn employee_scoped_skill_record_wins_over_company_skill_with_same_skill_id() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("employee-skill-record-priority");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-skill-record-priority";
        std::fs::create_dir_all(workspace.company_dir(opc_id)).unwrap();
        std::fs::create_dir_all(workspace.company_skills_dir(opc_id)).unwrap();
        workspace
            .ensure_company_employee_skeleton(opc_id, "agent-founder-01")
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let company_skill = coevo_core::skills::AgentSkillPackage {
            skill_id: "skill-layered-priority".to_string(),
            version: "1.0.0".to_string(),
            name: "Company skill".to_string(),
            owner_agent_id: "agent-founder-01".to_string(),
            department: "FounderOffice".to_string(),
            description: "Company shared version".to_string(),
            trigger_patterns: vec!["layered".to_string()],
            applicable_domains: vec![],
            required_tools: vec![],
            required_model_profile: None,
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            prompt_template: "company db directive".to_string(),
            procedure_steps: vec![],
            guardrails: vec![],
            examples: vec![],
            tests: vec![],
            evals: vec![],
            permissions_required: vec![],
            allowed_cognitive_layers: vec![],
            allowed_action_modes: vec![],
            risk_ceiling: 0.3,
            provenance: "seed".to_string(),
            status: coevo_core::skills::SkillStatus::Active,
            created_at_ms: now,
            updated_at_ms: now,
        };
        let employee_skill = coevo_core::skills::AgentSkillPackage {
            skill_id: "skill-layered-priority".to_string(),
            version: "2.0.0".to_string(),
            name: "Employee skill".to_string(),
            owner_agent_id: "agent-founder-01".to_string(),
            department: "FounderOffice".to_string(),
            description: "Employee evolved version".to_string(),
            trigger_patterns: vec!["layered".to_string()],
            applicable_domains: vec![],
            required_tools: vec![],
            required_model_profile: None,
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            prompt_template: "employee db directive".to_string(),
            procedure_steps: vec![],
            guardrails: vec![],
            examples: vec![],
            tests: vec![],
            evals: vec![],
            permissions_required: vec![],
            allowed_cognitive_layers: vec![],
            allowed_action_modes: vec![],
            risk_ceiling: 0.3,
            provenance: "skill-evolution-proposal-priority".to_string(),
            status: coevo_core::skills::SkillStatus::Active,
            created_at_ms: now,
            updated_at_ms: now,
        };
        let mut newer_company_skill = company_skill.clone();
        newer_company_skill.updated_at_ms = now + 10;
        newer_company_skill.created_at_ms = now + 10;
        SkillRepo::upsert(&pool, &company_skill).await.unwrap();
        SkillRepo::upsert(&pool, &employee_skill).await.unwrap();
        SkillRepo::upsert(&pool, &newer_company_skill)
            .await
            .unwrap();

        let loaded = SkillRuntime::load_full(
            &pool,
            workspace.root(),
            opc_id,
            "agent-founder-01",
            "skill-layered-priority",
        )
        .await
        .unwrap()
        .expect("skill should load");

        std::fs::remove_dir_all(coevo_home).ok();

        assert_eq!(loaded["version"], "2.0.0");
        assert_eq!(loaded["provenance"], "skill-evolution-proposal-priority");
    }

    #[tokio::test]
    async fn db_only_skill_is_materialized_to_company_markdown_before_runtime_reads_it() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("db-skill-materialize");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-db-skill-materialize";
        std::fs::create_dir_all(workspace.company_dir(opc_id)).unwrap();
        std::fs::create_dir_all(workspace.company_skills_dir(opc_id)).unwrap();

        let mut skill = SkillRepo::get(&pool, "skill-mission-draft", None)
            .await
            .unwrap()
            .unwrap();
        skill.prompt_template = "db only directive".to_string();
        SkillRepo::upsert(&pool, &skill).await.unwrap();

        let skill_path = workspace.company_skill_markdown_path(opc_id, "skill-mission-draft");
        if skill_path.exists() {
            std::fs::remove_file(&skill_path).unwrap();
        }

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish immediately.",
            "proposal": {
                "kind": "finish",
                "summary": "Done.",
                "result": {"ok": true}
            },
            "confidence": 0.95
        })]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &pool,
            coevo_home.clone(),
            &AgentRunContract {
                opc_id: opc_id.to_string(),
                ..test_contract("wo-db-skill-materialize", "Use the selected skill.")
            },
            &test_auth(
                "wo-db-skill-materialize",
                "run-db-skill-materialize",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        assert!(
            skill_path.exists(),
            "expected runtime to materialize skill markdown"
        );
        let skill_markdown = std::fs::read_to_string(&skill_path).unwrap();
        assert!(skill_markdown.contains("db only directive"));
        let first_prompt = seen_messages.lock().unwrap()[0]
            .iter()
            .find(|message| {
                message.role == "system" && message.content.contains("active_skill_directives")
            })
            .expect("skill directives should be injected")
            .content
            .clone();
        assert!(first_prompt.contains("db only directive"));

        std::fs::remove_dir_all(coevo_home).ok();
    }

    #[tokio::test]
    async fn selected_skill_markdown_is_injected_into_memory_context() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("skill-memory-context");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-skill-memory-context";
        std::fs::create_dir_all(workspace.company_dir(opc_id)).unwrap();
        std::fs::create_dir_all(workspace.company_skills_dir(opc_id)).unwrap();

        let mut skill = SkillRepo::get(&pool, "skill-mission-draft", None)
            .await
            .unwrap()
            .unwrap();
        skill.prompt_template = "memory context directive".to_string();
        SkillRepo::upsert(&pool, &skill).await.unwrap();

        std::fs::create_dir_all(workspace.company_skill_dir(opc_id, "skill-mission-draft"))
            .unwrap();
        std::fs::write(
            workspace.company_skill_markdown_path(opc_id, "skill-mission-draft"),
            "memory context directive",
        )
        .unwrap();

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish immediately.",
            "proposal": {
                "kind": "finish",
                "summary": "Done.",
                "result": {"ok": true}
            },
            "confidence": 0.95
        })]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &pool,
            coevo_home.clone(),
            &AgentRunContract {
                opc_id: opc_id.to_string(),
                ..test_contract("wo-skill-memory-context", "Use the selected skill.")
            },
            &test_auth(
                "wo-skill-memory-context",
                "run-skill-memory-context",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(coevo_home).ok();

        assert_eq!(result.final_status, "Completed");
        let payload = seen_messages.lock().unwrap()[0]
            .iter()
            .find(|message| message.role == "user")
            .map(|message| serde_json::from_str::<serde_json::Value>(&message.content).unwrap())
            .expect("user payload should be present");
        let relevant = payload["memory_context"]["relevant_skill_memory"]
            .as_array()
            .expect("relevant_skill_memory should be an array");
        assert!(
            relevant.iter().any(|item| {
                item["skill_id"] == "skill-mission-draft"
                    && item["content_md"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("memory context directive")
            }),
            "selected skill markdown should be reflected in memory_context: {payload}"
        );
    }

    #[tokio::test]
    async fn employee_persona_markdown_files_are_injected_into_system_prompt() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("employee-persona-injection");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-persona-injection";
        workspace
            .ensure_company_employee_skeleton(opc_id, "agent-founder-01")
            .unwrap();
        let employee_dir = workspace.company_employee_dir(opc_id, "agent-founder-01");
        std::fs::write(employee_dir.join("prompt.md"), "prompt file body").unwrap();
        std::fs::write(employee_dir.join("identity.md"), "identity file body").unwrap();
        std::fs::write(employee_dir.join("soul.md"), "soul file body").unwrap();
        std::fs::write(employee_dir.join("agents.md"), "agents file body").unwrap();
        std::fs::write(employee_dir.join("owner.md"), "owner file body").unwrap();
        std::fs::write(employee_dir.join("tools.md"), "tools file body").unwrap();

        let mut employee = AgentEmployeeRepo::get(&pool, "agent-founder-01")
            .await
            .unwrap()
            .unwrap();
        employee.system_prompt = "db system prompt body".to_string();
        AgentEmployeeRepo::upsert(&pool, &employee).await.unwrap();

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish immediately.",
            "proposal": {
                "kind": "finish",
                "summary": "Done.",
                "result": {"ok": true}
            },
            "confidence": 0.95
        })]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &pool,
            coevo_home.clone(),
            &AgentRunContract {
                opc_id: opc_id.to_string(),
                ..test_contract("wo-persona-injection", "Use the current employee persona.")
            },
            &test_auth(
                "wo-persona-injection",
                "run-persona-injection",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(coevo_home).ok();

        assert_eq!(result.final_status, "Completed");
        let first_system_messages = seen_messages.lock().unwrap()[0]
            .iter()
            .filter(|message| message.role == "system")
            .map(|message| message.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");
        assert!(first_system_messages.contains("prompt file body"));
        assert!(first_system_messages.contains("[identity.md]\nidentity file body"));
        assert!(first_system_messages.contains("[soul.md]\nsoul file body"));
        assert!(first_system_messages.contains("[agents.md]\nagents file body"));
        assert!(first_system_messages.contains("[owner.md]\nowner file body"));
        assert!(first_system_messages.contains("[tools.md]\ntools file body"));
        assert!(!first_system_messages.contains("db system prompt body"));
    }

    #[tokio::test]
    async fn employee_persona_markdown_files_are_injected_into_memory_context() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("employee-persona-memory-context");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-persona-memory-context";
        workspace
            .ensure_company_employee_skeleton(opc_id, "agent-founder-01")
            .unwrap();
        let employee_dir = workspace.company_employee_dir(opc_id, "agent-founder-01");
        std::fs::write(employee_dir.join("prompt.md"), "prompt file body").unwrap();
        std::fs::write(employee_dir.join("identity.md"), "identity file body").unwrap();
        std::fs::write(employee_dir.join("soul.md"), "soul file body").unwrap();
        std::fs::write(employee_dir.join("agents.md"), "agents file body").unwrap();
        std::fs::write(employee_dir.join("owner.md"), "owner file body").unwrap();
        std::fs::write(employee_dir.join("tools.md"), "tools file body").unwrap();

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish immediately.",
            "proposal": {
                "kind": "finish",
                "summary": "Done.",
                "result": {"ok": true}
            },
            "confidence": 0.95
        })]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &pool,
            coevo_home.clone(),
            &AgentRunContract {
                opc_id: opc_id.to_string(),
                ..test_contract(
                    "wo-persona-memory-context",
                    "Use the current employee persona from memory context.",
                )
            },
            &test_auth(
                "wo-persona-memory-context",
                "run-persona-memory-context",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(coevo_home).ok();

        assert_eq!(result.final_status, "Completed");
        let payload = seen_messages.lock().unwrap()[0]
            .iter()
            .find(|message| message.role == "user")
            .map(|message| serde_json::from_str::<serde_json::Value>(&message.content).unwrap())
            .expect("user payload should be present");
        let persona = payload["memory_context"]["employee_persona_files"]
            .as_array()
            .expect("employee_persona_files should be an array");
        assert!(
            persona.iter().any(|item| {
                item["path"] == "employees/agent-founder-01/identity.md"
                    && item["content_md"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("identity file body")
            }),
            "identity.md should be reflected in memory_context: {payload}"
        );
        assert!(
            persona.iter().any(|item| {
                item["path"] == "employees/agent-founder-01/tools.md"
                    && item["content_md"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("tools file body")
            }),
            "tools.md should be reflected in memory_context: {payload}"
        );
    }

    #[tokio::test]
    async fn file_backed_tool_policy_filters_runtime_available_tools() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("tool-policy-runtime-filter");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-tool-policy-runtime-filter";
        workspace
            .ensure_company_employee_skeleton(opc_id, "agent-founder-01")
            .unwrap();
        let employee_dir = workspace.company_employee_dir(opc_id, "agent-founder-01");
        std::fs::write(
            employee_dir.join("tool_policy.json"),
            serde_json::json!({
                "allowed_tools": ["file-readonly"],
                "risk_ceiling": 0.3
            })
            .to_string(),
        )
        .unwrap();

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish immediately.",
            "proposal": {
                "kind": "finish",
                "summary": "Done.",
                "result": {"ok": true}
            },
            "confidence": 0.95
        })]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &pool,
            coevo_home.clone(),
            &AgentRunContract {
                opc_id: opc_id.to_string(),
                ..test_contract(
                    "wo-tool-policy-runtime-filter",
                    "Use only whitelisted tools from file policy.",
                )
            },
            &test_auth(
                "wo-tool-policy-runtime-filter",
                "run-tool-policy-runtime-filter",
                vec![],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(coevo_home).ok();

        assert_eq!(result.final_status, "Completed");
        let payload = seen_messages.lock().unwrap()[0]
            .iter()
            .find(|message| message.role == "user")
            .map(|message| serde_json::from_str::<serde_json::Value>(&message.content).unwrap())
            .expect("user payload should be present");
        let available_tools = payload["available_tools"]
            .as_array()
            .expect("available_tools should be an array");
        assert!(
            available_tools
                .iter()
                .all(|tool| tool["tool_id"] != "github-readonly"),
            "github-readonly should be filtered out by file-backed tool policy: {payload}"
        );
        assert!(
            available_tools
                .iter()
                .any(|tool| tool["tool_id"] == "file-readonly"),
            "file-readonly should remain available under file-backed tool policy: {payload}"
        );
    }

    #[test]
    fn employee_persona_prompt_marks_missing_files_and_truncates_large_sections() {
        let coevo_home = test_coevo_home("employee-persona-shape");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-persona-shape";
        let agent_id = "agent-founder-01";
        workspace
            .ensure_company_employee_skeleton(opc_id, agent_id)
            .unwrap();
        let employee_dir = workspace.company_employee_dir(opc_id, agent_id);
        std::fs::write(employee_dir.join("prompt.md"), "prompt body").unwrap();
        std::fs::write(employee_dir.join("identity.md"), "x".repeat(4500)).unwrap();
        std::fs::write(employee_dir.join("soul.md"), "   ").unwrap();
        std::fs::write(employee_dir.join("agents.md"), "rules body").unwrap();

        let prompt = load_employee_system_prompt(&workspace, opc_id, agent_id, "");

        std::fs::remove_dir_all(coevo_home).ok();

        assert!(prompt.contains("prompt body"));
        assert!(prompt.contains("[identity.md]"));
        assert!(prompt.contains("[TRUNCATED: 4500 chars total]"));
        assert!(!prompt.contains("[soul.md]"));
        assert!(prompt.contains("[agents.md]\nrules body"));
        assert!(prompt.contains("[owner.md]\n(MISSING)"));
        assert!(prompt.contains("[tools.md]\n(MISSING)"));
    }

    #[tokio::test]
    async fn company_shared_markdown_files_are_injected_into_prompt_context() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("company-shared-injection");
        let workspace = CompanyWorkspaceManager::new(coevo_home.clone());
        let opc_id = "opc-shared-injection";
        let shared_dir = workspace
            .company_dir(opc_id)
            .join("shared")
            .join("playbooks");
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::write(
            shared_dir.join("launch.md"),
            "# Launch Playbook\n\nShared launch checklist from company workspace.",
        )
        .unwrap();

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish immediately.",
            "proposal": {
                "kind": "finish",
                "summary": "Done.",
                "result": {"ok": true}
            },
            "confidence": 0.95
        })]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &pool,
            coevo_home.clone(),
            &AgentRunContract {
                opc_id: opc_id.to_string(),
                ..test_contract(
                    "wo-company-shared-injection",
                    "Use company shared documentation.",
                )
            },
            &test_auth(
                "wo-company-shared-injection",
                "run-company-shared-injection",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(coevo_home).ok();

        assert_eq!(result.final_status, "Completed");
        let first_user_message = seen_messages.lock().unwrap()[0]
            .iter()
            .find(|message| message.role == "user")
            .expect("user payload should exist")
            .content
            .clone();
        assert!(first_user_message.contains("launch.md"));
        assert!(first_user_message.contains("Shared launch checklist from company workspace."));
    }

    #[tokio::test]
    async fn company_memory_records_are_injected_into_prompt_context() {
        let pool = migrated_pool().await;
        let coevo_home = test_coevo_home("company-memory-injection");
        let opc_id = "opc-memory-injection";
        let now = chrono::Utc::now().timestamp_millis() as u64;
        memory_repo::MemoryRepo::create(
            &pool,
            &coevo_core::opc::MemoryRecord {
                memory_id: "mem-company-launch".to_string(),
                scope: coevo_core::opc::MemoryScope::Company,
                owner_id: opc_id.to_string(),
                title: "Launch decision".to_string(),
                content: "Prefer staged rollout for the company launch.".to_string(),
                tags: vec!["launch".to_string(), "decision".to_string()],
                source: "founder-note".to_string(),
                provenance: "meeting-2026-06-07".to_string(),
                confidence: 0.91,
                ttl_seconds: 86400,
                created_at_ms: now,
                updated_at_ms: now,
                access_policy: "company".to_string(),
                status: coevo_core::opc::MemoryStatus::Active,
                cognitive_layer: coevo_core::cognitive::CognitiveLayer::Fact,
                linked_contract_hash: None,
                linked_plan_hash: None,
                linked_adr_id: None,
            },
        )
        .await
        .unwrap();

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish immediately.",
            "proposal": {
                "kind": "finish",
                "summary": "Done.",
                "result": {"ok": true}
            },
            "confidence": 0.95
        })]);
        let seen_messages = gateway.seen_messages.clone();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &pool,
            coevo_home.clone(),
            &AgentRunContract {
                opc_id: opc_id.to_string(),
                ..test_contract("wo-company-memory-injection", "Use company memory context.")
            },
            &test_auth(
                "wo-company-memory-injection",
                "run-company-memory-injection",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(coevo_home).ok();

        assert_eq!(result.final_status, "Completed");
        let first_user_message = seen_messages.lock().unwrap()[0]
            .iter()
            .find(|message| message.role == "user")
            .expect("user payload should exist")
            .content
            .clone();
        assert!(first_user_message.contains("company_memory"));
        assert!(first_user_message.contains("Launch decision"));
        assert!(first_user_message.contains("Prefer staged rollout for the company launch."));
        assert!(first_user_message.contains("meeting-2026-06-07"));
    }

    #[tokio::test]
    async fn skill_index_hides_other_agents_evolved_skills() {
        let pool = migrated_pool().await;
        let now = chrono::Utc::now().timestamp_millis() as u64;
        SkillRepo::upsert(
            &pool,
            &coevo_core::skills::AgentSkillPackage {
                skill_id: "skill-private-evolved".to_string(),
                version: "1.0.0".to_string(),
                name: "Private evolved".to_string(),
                owner_agent_id: "agent-engineer-01".to_string(),
                department: "Engineering".to_string(),
                description: String::new(),
                trigger_patterns: vec!["private".to_string()],
                applicable_domains: vec![],
                required_tools: vec![],
                required_model_profile: None,
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                prompt_template: "private directive".to_string(),
                procedure_steps: vec![],
                guardrails: vec![],
                examples: vec![],
                tests: vec![],
                evals: vec![],
                permissions_required: vec![],
                allowed_cognitive_layers: vec![],
                allowed_action_modes: vec![],
                risk_ceiling: 0.3,
                provenance: "skill-evolution-proposal-1".to_string(),
                status: coevo_core::skills::SkillStatus::Active,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .await
        .unwrap();

        let founder_index = SkillRuntime::load_skill_index(&pool, "agent-founder-01")
            .await
            .unwrap();
        assert!(founder_index
            .iter()
            .all(|row| row["skill_id"] != "skill-private-evolved"));

        let engineer_index = SkillRuntime::load_skill_index(&pool, "agent-engineer-01")
            .await
            .unwrap();
        assert!(engineer_index
            .iter()
            .any(|row| row["skill_id"] == "skill-private-evolved"));
    }

    #[tokio::test]
    async fn skill_index_prefers_employee_scoped_winner_for_duplicate_skill_id() {
        let pool = migrated_pool().await;
        let now = chrono::Utc::now().timestamp_millis() as u64;

        let company_skill = coevo_core::skills::AgentSkillPackage {
            skill_id: "skill-layered-index".to_string(),
            version: "1.0.0".to_string(),
            name: "Company index skill".to_string(),
            owner_agent_id: "agent-founder-01".to_string(),
            department: "FounderOffice".to_string(),
            description: "Company shared version".to_string(),
            trigger_patterns: vec!["layered-index".to_string()],
            applicable_domains: vec![],
            required_tools: vec![],
            required_model_profile: None,
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            prompt_template: "company index directive".to_string(),
            procedure_steps: vec![],
            guardrails: vec![],
            examples: vec![],
            tests: vec![],
            evals: vec![],
            permissions_required: vec![],
            allowed_cognitive_layers: vec![],
            allowed_action_modes: vec![],
            risk_ceiling: 0.3,
            provenance: "seed".to_string(),
            status: coevo_core::skills::SkillStatus::Active,
            created_at_ms: now,
            updated_at_ms: now,
        };
        let employee_skill = coevo_core::skills::AgentSkillPackage {
            skill_id: "skill-layered-index".to_string(),
            version: "2.0.0".to_string(),
            name: "Employee index skill".to_string(),
            owner_agent_id: "agent-founder-01".to_string(),
            department: "FounderOffice".to_string(),
            description: "Employee evolved version".to_string(),
            trigger_patterns: vec!["layered-index".to_string()],
            applicable_domains: vec![],
            required_tools: vec![],
            required_model_profile: None,
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            prompt_template: "employee index directive".to_string(),
            procedure_steps: vec![],
            guardrails: vec![],
            examples: vec![],
            tests: vec![],
            evals: vec![],
            permissions_required: vec![],
            allowed_cognitive_layers: vec![],
            allowed_action_modes: vec![],
            risk_ceiling: 0.3,
            provenance: "skill-evolution-proposal-index".to_string(),
            status: coevo_core::skills::SkillStatus::Active,
            created_at_ms: now,
            updated_at_ms: now,
        };
        let mut newer_company_skill = company_skill.clone();
        newer_company_skill.version = "1.1.0".to_string();
        newer_company_skill.created_at_ms = now + 10;
        newer_company_skill.updated_at_ms = now + 10;

        SkillRepo::upsert(&pool, &company_skill).await.unwrap();
        SkillRepo::upsert(&pool, &employee_skill).await.unwrap();
        SkillRepo::upsert(&pool, &newer_company_skill)
            .await
            .unwrap();

        let founder_index = SkillRuntime::load_skill_index(&pool, "agent-founder-01")
            .await
            .unwrap();
        let layered_rows = founder_index
            .iter()
            .filter(|row| row["skill_id"] == "skill-layered-index")
            .collect::<Vec<_>>();

        assert_eq!(
            layered_rows.len(),
            1,
            "expected only the effective winner for a duplicate skill_id"
        );
        assert_eq!(layered_rows[0]["version"], "2.0.0");
    }

    #[tokio::test]
    async fn model_request_includes_allowed_tool_schemas() {
        let pool = migrated_pool().await;
        let work_order_id = "wo-model-tool-schemas";
        let run_id = "run-model-tool-schemas";
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish without calling any tool.",
            "proposal": {
                "kind": "finish",
                "summary": "Tool schema manifest was available.",
                "result": {"ok": true}
            },
            "confidence": 0.88
        })]);
        let seen_requests = gateway.seen_requests.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(work_order_id, "Summarize the launch plan."),
            &test_auth(work_order_id, run_id, vec![]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let requests = seen_requests.lock().unwrap();
        assert!(
            !requests.is_empty(),
            "expected at least one model request to be observed"
        );
        let first_request = &requests[0];
        assert!(
            !first_request.tools.is_empty(),
            "expected allowed tool schemas to be passed to the model"
        );
        let file_tool = first_request
            .tools
            .iter()
            .find(|tool| tool.name == "file-readonly")
            .expect("file-readonly tool schema should be present");
        assert_eq!(
            file_tool.description.as_deref(),
            Some("File Readonly (actions: ReadFile, ListDirectory)")
        );
        assert_eq!(
            file_tool.parameters_json["type"],
            serde_json::Value::String("object".to_string())
        );
        let required = file_tool.parameters_json["required"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(required.contains(&serde_json::Value::String("action".to_string())));
        assert!(required.contains(&serde_json::Value::String("path".to_string())));
        assert_eq!(first_request.tool_choice, Some(serde_json::json!("auto")));
    }

    #[tokio::test]
    async fn stdio_mcp_servers_are_never_advertised_to_worker_from_persisted_rows() {
        std::env::set_var("COEVO_ENABLE_MCP_STDIO", "1");
        let pool = migrated_pool().await;
        let now = chrono::Utc::now().timestamp_millis().to_string();
        let tools_json = serde_json::to_string(&vec![coevo_adapters::McpToolInfo {
            name: "dangerous".to_string(),
            description: Some("legacy stdio tool".to_string()),
            input_schema: serde_json::json!({"type":"object"}),
        }])
        .unwrap();
        sqlx::query(
            "INSERT INTO mcp_servers (opc_id,id,name,transport,command,args_json,env_json,url,headers_json,enabled,status,last_error,tools_json,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind("default-opc")
        .bind("legacy-stdio")
        .bind("legacy-stdio")
        .bind("stdio")
        .bind("should-not-run")
        .bind("[]")
        .bind("{}")
        .bind(Option::<String>::None)
        .bind("{}")
        .bind(1_i64)
        .bind("connected")
        .bind(Option::<String>::None)
        .bind(tools_json)
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish without calling any tool.",
            "proposal": {
                "kind": "finish",
                "summary": "Only safe tools were advertised.",
                "result": {"ok": true}
            },
            "confidence": 0.88
        })]);
        let seen_requests = gateway.seen_requests.clone();
        let mut auth = test_auth("wo-stdio-mcp-hidden", "run-stdio-mcp-hidden", vec![]);
        auth.allowed_actions = vec!["read".to_string(), "execute".to_string()];

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-stdio-mcp-hidden", "Summarize the launch plan."),
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let requests = seen_requests.lock().unwrap();
        let tool_names = requests[0]
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            !tool_names.contains(&"urn:mcp:legacy-stdio:dangerous"),
            "stdio MCP tools from persisted legacy rows must never be advertised to worker runs: {tool_names:?}"
        );
        std::env::remove_var("COEVO_ENABLE_MCP_STDIO");
    }

    #[tokio::test]
    async fn model_backed_compaction_uses_structured_summary_when_available() {
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
                skill_directives: &[],
                system_prompt: "",
            })
            .await
            .unwrap();
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "summary": "Recent tool use was a safe file read followed by a decision to finish.",
            "provenance": ["compaction:model-summary-v1", "round:2"],
            "dropped_message_count": 3
        })]);
        let history = vec![
            ModelMessage {
                role: "assistant".to_string(),
                content: "First observation".repeat(32),
                ..Default::default()
            },
            ModelMessage {
                role: "tool".to_string(),
                content: "Second observation".repeat(32),
                tool_call_id: Some("call-1".to_string()),
                ..Default::default()
            },
        ];

        let compacted = compact_history_with_model(
            &gateway,
            &ModelProviderConfig::mock(),
            &default_model_profiles(),
            &contract,
            &auth,
            &history,
            &prompt,
            8,
        )
        .await
        .unwrap()
        .unwrap();

        assert!(compacted
            .summary
            .content
            .contains("Recent tool use was a safe file read"));
        assert_eq!(compacted.provenance[0], "compaction:model-summary-v1");
        assert_eq!(compacted.dropped_message_count, 3);
    }

    #[tokio::test]
    async fn model_backed_compaction_falls_back_when_summary_model_fails() {
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
                skill_directives: &[],
                system_prompt: "",
            })
            .await
            .unwrap();
        let history = vec![ModelMessage {
            role: "assistant".to_string(),
            content: "x".repeat(500),
            ..Default::default()
        }];

        let compacted = compact_history_with_model(
            &FailingGateway,
            &ModelProviderConfig::mock(),
            &default_model_profiles(),
            &contract,
            &auth,
            &history,
            &prompt,
            8,
        )
        .await
        .unwrap();

        assert!(compacted.is_none());
        let fallback = engine.maybe_compact(&history, 8).await.unwrap().unwrap();
        assert!(fallback
            .summary
            .content
            .contains("Compacted governed history"));
    }

    #[tokio::test]
    async fn native_tool_calls_execute_through_governance_path() {
        let pool = migrated_pool().await;
        let root =
            std::env::temp_dir().join(format!("coevo-native-tool-calls-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("evidence.txt");
        std::fs::write(&evidence_path, "native tool call evidence").unwrap();
        let work_order_id = "wo-native-tool-calls";
        let run_id = "run-native-tool-calls";
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "The file observation is enough to finish.",
            "proposal": {
                "kind": "finish",
                "summary": "Native tool call executed.",
                "result": {"ok": true}
            },
            "confidence": 0.94
        })])
        .with_streamed_tool_calls(vec![
            vec![ModelToolCall {
                index: 0,
                id: Some("call_1".to_string()),
                name: "file-readonly".to_string(),
                arguments: serde_json::json!({
                    "action": "ReadFile",
                    "path": evidence_path.to_string_lossy().to_string(),
                    "allowed_paths": [root.to_string_lossy().to_string()],
                    "max_bytes": 5000
                })
                .to_string(),
            }],
            vec![],
        ])
        .with_stream_json(vec![
            None,
            Some(serde_json::json!({
                "thought": "The file observation is enough to finish.",
                "proposal": {
                    "kind": "finish",
                    "summary": "Native tool call executed.",
                    "result": {"ok": true}
                },
                "confidence": 0.94
            })),
        ]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                work_order_id,
                "Use the file tool if needed and summarize the evidence.",
            ),
            &test_auth(work_order_id, run_id, vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
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
    async fn spawn_subagent_runs_governed_helper_and_records_completion() {
        let pool = migrated_pool().await;
        let work_order_id = "wo-spawn-subagent";
        let run_id = "run-spawn-subagent";
        // The subagent's structured() call pops this contribution from the outputs queue.
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "contribution": "Drafted three candidate mission statements with rationale.",
            "key_findings": ["clarity", "scope"]
        })])
        .with_stream_json(vec![
            // Round 1: the head delegates to a single-skill sub-agent it holds.
            Some(serde_json::json!({
                "thought": "Delegate the focused drafting to a sub-agent.",
                "proposal": {
                    "kind": "spawn_subagent",
                    "skill_id": "skill-mission-draft",
                    "task": "Draft candidate mission statements.",
                    "rationale": "Needs focused drafting help."
                },
                "confidence": 0.9
            })),
            // Round 2: the head finishes after folding in the sub-agent's contribution.
            Some(serde_json::json!({
                "thought": "The sub-agent's draft is enough to finish.",
                "proposal": {
                    "kind": "finish",
                    "summary": "Mission drafted with sub-agent help.",
                    "result": {"ok": true}
                },
                "confidence": 0.95
            })),
        ]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(work_order_id, "Draft a mission statement."),
            &test_auth(work_order_id, run_id, vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        // The sub-agent actually ran: a SubagentCompleted event carries its real contribution.
        let completed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_events WHERE run_id=? AND event_type='SubagentCompleted'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(completed, 1, "subagent should run and record completion");
        let payload: String = sqlx::query_scalar(
            "SELECT payload_json FROM worker_events WHERE run_id=? AND event_type='SubagentCompleted' LIMIT 1",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            payload.contains("candidate mission statements"),
            "completion event must carry the sub-agent's real contribution, got: {payload}"
        );
    }

    #[tokio::test]
    async fn file_evidence_mission_uses_text_response_format_before_first_attempt() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!(
            "coevo-file-evidence-text-first-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("evidence.txt");
        std::fs::write(&evidence_path, "text-first tool call evidence").unwrap();
        let work_order_id = "wo-file-evidence-text-first";
        let run_id = "run-file-evidence-text-first";
        let gateway = ScriptedGateway::new(vec![
            serde_json::json!({
                "thought": "The model should issue a native file-readonly tool call first."
            }),
            serde_json::json!({
                "thought": "The tool observation is enough to finish.",
                "proposal": {
                    "kind": "finish",
                    "summary": "File-readonly tool call executed before finish.",
                    "result": {"ok": true}
                },
                "confidence": 0.93
            }),
        ])
        .with_streamed_tool_calls(vec![
            vec![ModelToolCall {
                index: 0,
                id: Some("call_forced_file".to_string()),
                name: "file-readonly".to_string(),
                arguments: serde_json::json!({
                    "action": "ReadFile",
                    "path": evidence_path.to_string_lossy().to_string(),
                    "allowed_paths": [root.to_string_lossy().to_string()],
                    "max_bytes": 5000
                })
                .to_string(),
            }],
            vec![],
        ]);
        let seen_requests = gateway.seen_requests.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                work_order_id,
                "Read the local evidence file first, then summarize the strongest signal.",
            ),
            &test_auth(work_order_id, run_id, vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let requests = seen_requests.lock().unwrap();
        let first_request = &requests[0];
        assert_eq!(first_request.response_format, ResponseFormat::Json);
        assert_eq!(
            first_request.tool_choice,
            Some(serde_json::json!({
                "type": "function",
                "function": {
                    "name": "file-readonly"
                }
            }))
        );
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
    async fn streamed_native_tool_call_beats_finish_when_file_evidence_is_still_required() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!(
            "coevo-file-evidence-promote-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("evidence.txt");
        std::fs::write(&evidence_path, "promote me").unwrap();
        let work_order_id = "wo-file-evidence-promote";
        let run_id = "run-file-evidence-promote";
        let gateway = ScriptedGateway::new(vec![
            serde_json::json!({
                "thought": "I can finish now.",
                "proposal": {
                    "kind": "finish",
                    "summary": "Done too early.",
                    "result": {"ok": true}
                },
                "confidence": 0.9
            }),
            serde_json::json!({
                "thought": "The tool call should win here.",
                "proposal": {
                    "kind": "finish",
                    "summary": "File observation is enough.",
                    "result": {"ok": true}
                },
                "confidence": 0.91
            }),
        ])
        .with_streamed_tool_calls(vec![
            vec![ModelToolCall {
                index: 0,
                id: Some("call_promoted".to_string()),
                name: "file-readonly".to_string(),
                arguments: serde_json::json!({
                    "action": "ReadFile",
                    "path": evidence_path.to_string_lossy().to_string(),
                    "allowed_paths": [root.to_string_lossy().to_string()],
                    "max_bytes": 5000
                })
                .to_string(),
            }],
            vec![],
        ]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                work_order_id,
                "Read the local evidence file first, then summarize the strongest signal.",
            ),
            &test_auth(work_order_id, run_id, vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id=? AND tool_id='file-readonly'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_calls, 1);
    }

    #[tokio::test]
    async fn follow_up_round_preserves_reasoning_content_and_native_tool_call_history() {
        let pool = migrated_pool().await;
        let root =
            std::env::temp_dir().join(format!("coevo-follow-up-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("welcome.md");
        std::fs::write(&evidence_path, "hello from workspace").unwrap();
        let work_order_id = "wo-follow-up-history";
        let run_id = "run-follow-up-history";
        let gateway = ScriptedGateway::new(vec![])
            .with_streamed_tool_calls(vec![
                vec![ModelToolCall {
                    index: 0,
                    id: Some("call_history_1".to_string()),
                    name: "file-readonly".to_string(),
                    arguments: serde_json::json!({
                        "action": "ReadFile",
                        "path": evidence_path.to_string_lossy().to_string(),
                        "allowed_paths": [root.to_string_lossy().to_string()]
                    })
                    .to_string(),
                }],
                vec![],
            ])
            .with_stream_reasoning(vec![
                Some("I should inspect the workspace welcome file before answering.".to_string()),
                Some("The tool output is enough to finish.".to_string()),
            ])
            .with_stream_json(vec![
                None,
                Some(serde_json::json!({
                    "thought": "The tool output is enough to finish.",
                    "proposal": {
                        "kind": "finish",
                        "summary": "Completed after replaying tool context.",
                        "result": {"ok": true}
                    },
                    "confidence": 0.93
                })),
            ]);
        let seen_requests = gateway.seen_requests.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                work_order_id,
                "Read the workspace welcome file and summarize it.",
            ),
            &test_auth(work_order_id, run_id, vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let requests = seen_requests.lock().unwrap();
        assert!(requests.len() >= 2, "expected at least two model rounds");
        let second_messages = &requests[1].messages;
        let assistant_message = second_messages
            .iter()
            .find(|message| message.role == "assistant" && !message.tool_calls.is_empty())
            .expect("second round should replay assistant tool-call history");
        assert_eq!(
            assistant_message.reasoning_content.as_deref(),
            Some("I should inspect the workspace welcome file before answering.")
        );
        assert_eq!(
            assistant_message.tool_calls[0].id.as_deref(),
            Some("call_history_1")
        );
        let tool_message = second_messages
            .iter()
            .find(|message| message.role == "tool")
            .expect("second round should include the tool response");
        assert_eq!(tool_message.tool_call_id.as_deref(), Some("call_history_1"));
    }

    #[tokio::test]
    async fn follow_up_round_replays_all_executed_tool_calls_when_model_emits_many() {
        let pool = migrated_pool().await;
        let root =
            std::env::temp_dir().join(format!("coevo-multi-tool-history-{}", uuid::Uuid::new_v4()));
        let data_dir = root.join("data");
        let docs_dir = root.join("docs");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(data_dir.join("one.txt"), "alpha").unwrap();
        std::fs::write(docs_dir.join("two.txt"), "beta").unwrap();

        let gateway = ScriptedGateway::new(vec![])
            .with_streamed_tool_calls(vec![
                vec![ModelToolCall {
                    index: 0,
                    id: Some("call_root".to_string()),
                    name: "file-readonly".to_string(),
                    arguments: serde_json::json!({
                        "action": "ListDirectory",
                        "path": root.to_string_lossy().to_string(),
                        "allowed_paths": [root.to_string_lossy().to_string()]
                    })
                    .to_string(),
                }],
                vec![
                    ModelToolCall {
                        index: 0,
                        id: Some("call_data".to_string()),
                        name: "file-readonly".to_string(),
                        arguments: serde_json::json!({
                            "action": "ListDirectory",
                            "path": data_dir.to_string_lossy().to_string(),
                            "allowed_paths": [root.to_string_lossy().to_string()]
                        })
                        .to_string(),
                    },
                    ModelToolCall {
                        index: 1,
                        id: Some("call_docs".to_string()),
                        name: "file-readonly".to_string(),
                        arguments: serde_json::json!({
                            "action": "ListDirectory",
                            "path": docs_dir.to_string_lossy().to_string(),
                            "allowed_paths": [root.to_string_lossy().to_string()]
                        })
                        .to_string(),
                    },
                ],
                vec![],
            ])
            .with_stream_reasoning(vec![
                Some("Inspect the workspace root first.".to_string()),
                Some("Inspect the most relevant subdirectory next.".to_string()),
                Some("The collected tool evidence is enough to finish.".to_string()),
            ])
            .with_stream_json(vec![
                None,
                None,
                Some(serde_json::json!({
                    "thought": "The collected tool evidence is enough to finish.",
                    "proposal": {
                        "kind": "finish",
                        "summary": "Completed after replaying only executed tool calls.",
                        "result": {"ok": true}
                    },
                    "confidence": 0.94
                })),
            ]);
        let seen_requests = gateway.seen_requests.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                "wo-multi-tool-history",
                "Inspect the workspace and summarize what matters.",
            ),
            &test_auth(
                "wo-multi-tool-history",
                "run-multi-tool-history",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let requests = seen_requests.lock().unwrap();
        assert!(requests.len() >= 3, "expected at least three model rounds");
        let third_messages = &requests[2].messages;
        let replayed_assistant = third_messages
            .iter()
            .rev()
            .find(|message| message.role == "assistant" && !message.tool_calls.is_empty())
            .expect("third round should replay prior assistant tool-call history");
        assert_eq!(replayed_assistant.tool_calls.len(), 2);
        assert!(third_messages
            .iter()
            .filter(|message| message.role == "tool")
            .any(|message| message.tool_call_id.as_deref() == Some("call_data")));
        assert!(third_messages
            .iter()
            .filter(|message| message.role == "tool")
            .any(|message| message.tool_call_id.as_deref() == Some("call_docs")));
    }

    #[tokio::test]
    async fn multiple_native_tool_calls_execute_and_replay_all_calls() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!(
            "coevo-multi-native-tool-calls-{}",
            uuid::Uuid::new_v4()
        ));
        let first_path = root.join("one.txt");
        let second_path = root.join("two.txt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&first_path, "alpha").unwrap();
        std::fs::write(&second_path, "beta").unwrap();

        let gateway = ScriptedGateway::new(vec![])
            .with_streamed_tool_calls(vec![
                vec![
                    ModelToolCall {
                        index: 0,
                        id: Some("call_first".to_string()),
                        name: "file-readonly".to_string(),
                        arguments: serde_json::json!({
                            "action": "ReadFile",
                            "path": first_path.to_string_lossy().to_string(),
                            "allowed_paths": [root.to_string_lossy().to_string()],
                            "max_bytes": 5000
                        })
                        .to_string(),
                    },
                    ModelToolCall {
                        index: 1,
                        id: Some("call_second".to_string()),
                        name: "file-readonly".to_string(),
                        arguments: serde_json::json!({
                            "action": "ReadFile",
                            "path": second_path.to_string_lossy().to_string(),
                            "allowed_paths": [root.to_string_lossy().to_string()],
                            "max_bytes": 5000
                        })
                        .to_string(),
                    },
                ],
                vec![],
            ])
            .with_stream_json(vec![
                None,
                Some(serde_json::json!({
                    "thought": "The two file reads are enough to finish.",
                    "proposal": {
                        "kind": "finish",
                        "summary": "Completed after replaying both native tool calls.",
                        "result": {"ok": true}
                    },
                    "confidence": 0.95
                })),
            ]);
        let seen_requests = gateway.seen_requests.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                "wo-parallel-native-tool-calls",
                "Inspect the workspace and summarize what matters.",
            ),
            &test_auth(
                "wo-parallel-native-tool-calls",
                "run-parallel-native-tool-calls",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let requests = seen_requests.lock().unwrap();
        let second_messages = &requests[1].messages;
        let assistant_message = second_messages
            .iter()
            .find(|message| message.role == "assistant" && !message.tool_calls.is_empty())
            .expect("second round should replay both tool calls");
        assert_eq!(assistant_message.tool_calls.len(), 2);
        assert!(assistant_message
            .tool_calls
            .iter()
            .any(|call| call.id.as_deref() == Some("call_first")));
        assert!(assistant_message
            .tool_calls
            .iter()
            .any(|call| call.id.as_deref() == Some("call_second")));
        assert!(second_messages.iter().any(|message| message.role == "tool"
            && message.tool_call_id.as_deref() == Some("call_first")));
        assert!(second_messages.iter().any(|message| message.role == "tool"
            && message.tool_call_id.as_deref() == Some("call_second")));
        let tool_call_rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-parallel-native-tool-calls' AND tool_id='file-readonly'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_call_rows, 2);
    }

    #[tokio::test]
    async fn run_exits_early_when_in_process_cancellation_signal_fires_during_model_stream() {
        let pool = migrated_pool().await;
        let work_order_id = "wo-cancel-during-stream";
        let run_id = "run-cancel-during-stream";
        let auth = test_auth(work_order_id, run_id, vec![]);
        create_worker_session(&pool, &auth).await;
        WorkerRunRepo::create(
            &pool,
            "default-opc",
            run_id,
            work_order_id,
            &auth.agent_id,
            &auth.worker_id,
            &auth.session_id,
            "Running",
            "{}",
            "[]",
            "[]",
            None,
            chrono::Utc::now().timestamp_millis(),
            None,
        )
        .await
        .unwrap();
        let gateway = CancellingDuringStreamGateway::new(run_id);
        let _cancellation = crate::worker_cancel::register_run(run_id.to_string());

        let result = AgentSubHarness::execute(
            &pool,
            &AgentRunContract {
                work_order_id: work_order_id.to_string(),
                mission_intent: "Inspect the workspace and summarize what matters.".to_string(),
                required_skills: vec![],
                user_id: "default-founder".to_string(),
                opc_id: "default-opc".to_string(),
            },
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();
        assert_eq!(result.final_status, "Cancelled");
        assert_eq!(result.termination_reason, "cancelled");
        assert_eq!(gateway.seen_requests.lock().unwrap().len(), 1);
        let tool_call_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM worker_tool_calls WHERE run_id=?")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tool_call_rows, 0);
    }

    #[tokio::test]
    async fn run_exits_early_when_cancellation_state_is_written_between_rounds() {
        let pool = migrated_pool().await;
        let work_order_id = "wo-cancel-between-rounds-db";
        let run_id = "run-cancel-between-rounds-db";
        let auth = test_auth(work_order_id, run_id, vec![]);
        create_worker_session(&pool, &auth).await;
        WorkerRunRepo::create(
            &pool,
            "default-opc",
            run_id,
            work_order_id,
            &auth.agent_id,
            &auth.worker_id,
            &auth.session_id,
            "Running",
            "{}",
            "[]",
            "[]",
            None,
            chrono::Utc::now().timestamp_millis(),
            None,
        )
        .await
        .unwrap();
        let temp_root = std::env::temp_dir().join(format!(
            "coevo-cancel-between-rounds-db-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        let observed_file = temp_root.join("observed.txt");
        std::fs::write(&observed_file, "cancel me later").unwrap();
        let cancel_pool = pool.clone();
        let cancel_run_id = run_id.to_string();
        let cancel_session_id = auth.session_id.clone();
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "Read the evidence before continuing.",
            "proposal": {
                "kind": "call_tool",
                "tool_id": "file-readonly",
                "input": {
                    "action": "ReadFile",
                    "path": observed_file.to_string_lossy().to_string(),
                    "allowed_paths": [temp_root.to_string_lossy().to_string()],
                    "max_bytes": 5000
                },
                "rationale": "Inspect the allowed evidence."
            },
            "confidence": 0.9
        })])
        .with_stream_completion_hook(move || {
            let pool = cancel_pool.clone();
            let run_id = cancel_run_id.clone();
            let session_id = cancel_session_id.clone();
            tokio::spawn(async move {
                let _ = WorkerRunRepo::set_status(&pool, &run_id, "Cancelled").await;
                let _ = coevo_store::repos::worker_session_repo::WorkerSessionRepo::update_status(
                    &pool,
                    &session_id,
                    "Cancelled",
                )
                .await;
            });
            std::thread::sleep(std::time::Duration::from_millis(25));
        });

        let result = AgentSubHarness::execute(
            &pool,
            &AgentRunContract {
                work_order_id: work_order_id.to_string(),
                mission_intent: "Inspect the workspace and summarize what matters.".to_string(),
                required_skills: vec![],
                user_id: "default-founder".to_string(),
                opc_id: "default-opc".to_string(),
            },
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();
        std::fs::remove_dir_all(&temp_root).ok();

        assert_eq!(result.final_status, "Cancelled");
        assert_eq!(result.termination_reason, "cancelled");
        let tool_call_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM worker_tool_calls WHERE run_id=?")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tool_call_rows, 0);
    }

    #[tokio::test]
    async fn run_reports_distinct_termination_reason_for_round_exhaustion() {
        let pool = migrated_pool().await;
        let outputs = (0..32)
            .map(|_| {
                serde_json::json!({
                    "thought": "Keep inspecting the allowed directory without finishing.",
                    "proposal": {
                        "kind": "call_tool",
                        "tool_id": "file-readonly",
                        "input": {
                            "action": "ListDirectory",
                            "path": std::env::temp_dir().to_string_lossy().to_string(),
                            "allowed_paths": [std::env::temp_dir().to_string_lossy().to_string()]
                        },
                        "rationale": "This is a legal read-only action, but the model never finishes."
                    },
                    "confidence": 0.4
                })
            })
            .collect();
        let gateway = ScriptedGateway::new(outputs);
        let mut config = ModelProviderConfig::mock();
        config.max_tokens = 100_000;

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-max-rounds-reason", "Keep trying an unavailable tool."),
            &test_auth("wo-max-rounds-reason", "run-max-rounds-reason", vec![]),
            &default_model_profiles(),
            None,
            &gateway,
            &config,
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "TimedOut");
        assert_eq!(result.termination_reason, "max_rounds_exhausted");
    }

    #[tokio::test]
    async fn preferred_tool_ids_prioritize_matching_tools_in_prompt_order() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!(
            "coevo-preferred-tool-order-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("evidence.txt");
        std::fs::write(&evidence_path, "tool order").unwrap();
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I can finish without calling any tool.",
            "proposal": {
                "kind": "finish",
                "summary": "Tool order is visible in the request.",
                "result": {"ok": true}
            },
            "confidence": 0.88
        })]);
        let seen_requests = gateway.seen_requests.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-preferred-tool-order", "Summarize the launch plan."),
            &test_auth(
                "wo-preferred-tool-order",
                "run-preferred-tool-order",
                vec![],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &["github-readonly".to_string(), "file-readonly".to_string()],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let requests = seen_requests.lock().unwrap();
        let first_request = &requests[0];
        let tool_names = first_request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        let github_index = tool_names
            .iter()
            .position(|name| *name == "github-readonly")
            .unwrap();
        let file_index = tool_names
            .iter()
            .position(|name| *name == "file-readonly")
            .unwrap();
        assert!(github_index < file_index);
    }

    #[tokio::test]
    async fn structured_tool_proposals_without_native_tool_ids_do_not_replay_invalid_tool_messages()
    {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!(
            "coevo-structured-tool-history-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("welcome.md");
        std::fs::write(&evidence_path, "hello from structured tool history").unwrap();

        let gateway = ScriptedGateway::new(vec![])
            .with_stream_json(vec![
                Some(serde_json::json!({
                    "thought": "Read the welcome file before answering.",
                    "proposal": {
                        "kind": "call_tool",
                        "tool_id": "file-readonly",
                        "input": {
                            "action": "ReadFile",
                            "path": evidence_path.to_string_lossy().to_string(),
                            "allowed_paths": [root.to_string_lossy().to_string()]
                        },
                        "rationale": "Inspect local evidence."
                    },
                    "confidence": 0.87
                })),
                Some(serde_json::json!({
                    "thought": "The file contents are enough to finish.",
                    "proposal": {
                        "kind": "finish",
                        "summary": "Completed after replaying structured tool output safely.",
                        "result": {"ok": true}
                    },
                    "confidence": 0.92
                })),
            ])
            .with_stream_reasoning(vec![
                Some("I should inspect the local file before answering.".to_string()),
                Some("The tool result is enough to answer.".to_string()),
            ]);
        let seen_requests = gateway.seen_requests.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                "wo-structured-tool-history",
                "Read the local welcome file and summarize it.",
            ),
            &test_auth(
                "wo-structured-tool-history",
                "run-structured-tool-history",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let requests = seen_requests.lock().unwrap();
        assert!(requests.len() >= 2, "expected at least two model rounds");
        let second_messages = &requests[1].messages;
        assert!(
            second_messages
                .iter()
                .all(|message| !(message.role == "tool" && message.tool_call_id.is_none())),
            "replayed tool messages must not omit tool_call_id: {second_messages:?}"
        );
        assert!(
            second_messages.iter().any(|message| {
                message.role == "system"
                    && message
                        .content
                        .contains("Tool file-readonly completed with success=true.")
            }),
            "structured tool observations without native ids should replay as system context"
        );
    }

    #[tokio::test]
    async fn denied_native_tool_proposals_do_not_replay_assistant_tool_calls() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!(
            "coevo-denied-native-tool-history-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("welcome.md");
        std::fs::write(&evidence_path, "hello from denied tool history").unwrap();

        let gateway = ScriptedGateway::new(vec![])
            .with_streamed_tool_calls(vec![
                vec![ModelToolCall {
                    index: 0,
                    id: Some("call_denied_1".to_string()),
                    name: "missing-tool".to_string(),
                    arguments: serde_json::json!({
                        "action": "ReadFile",
                        "path": evidence_path.to_string_lossy().to_string(),
                        "allowed_paths": [root.to_string_lossy().to_string()]
                    })
                    .to_string(),
                }],
                vec![],
            ])
            .with_stream_reasoning(vec![
                Some("Inspect the workspace file first.".to_string()),
                Some("The denial note is enough to continue.".to_string()),
            ])
            .with_stream_json(vec![
                Some(serde_json::json!({
                    "thought": "Inspect the workspace file first.",
                    "proposal": {
                        "kind": "call_tool",
                        "tool_id": "missing-tool",
                        "input": {
                            "action": "ReadFile",
                            "path": evidence_path.to_string_lossy().to_string(),
                            "allowed_paths": [root.to_string_lossy().to_string()]
                        },
                        "rationale": "Need local evidence."
                    },
                    "confidence": 0.82
                })),
                Some(serde_json::json!({
                    "thought": "The denial note is enough to continue.",
                    "proposal": {
                        "kind": "finish",
                        "summary": "Finished after denial.",
                        "result": {"ok": true}
                    },
                    "confidence": 0.9
                })),
            ]);
        let seen_requests = gateway.seen_requests.clone();

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                "wo-denied-native-tool-history",
                "Read the local welcome file and summarize it.",
            ),
            &test_auth(
                "wo-denied-native-tool-history",
                "run-denied-native-tool-history",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "TimedOut");
        let requests = seen_requests.lock().unwrap();
        assert!(requests.len() >= 2, "expected a follow-up model request");
        let second_messages = &requests[1].messages;
        assert!(
            second_messages
                .iter()
                .all(|message| !(message.role == "assistant" && !message.tool_calls.is_empty())),
            "denied native tool proposals must not replay assistant tool_calls: {second_messages:?}"
        );
        assert!(
            second_messages.iter().any(|message| {
                message.role == "system"
                    && message
                        .content
                        .contains("Governance denied the previous proposal")
            }),
            "the denial should still be preserved as system context"
        );
    }

    #[tokio::test]
    async fn tool_error_in_completed_run_still_creates_self_upgrade_proposal() {
        let pool = migrated_pool().await;
        configure_active_openai_compatible(&pool).await;
        let root = std::env::temp_dir().join(format!(
            "coevo-tool-error-proposal-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let env_path = root.join(".env");
        std::fs::write(&env_path, "TOP_SECRET=demo").unwrap();

        let gateway = ScriptedGateway::new(vec![])
            .with_streamed_tool_calls(vec![
                vec![ModelToolCall {
                    index: 0,
                    id: Some("call_env".to_string()),
                    name: "file-readonly".to_string(),
                    arguments: serde_json::json!({
                        "action": "ReadFile",
                        "path": env_path.to_string_lossy().to_string()
                    })
                    .to_string(),
                }],
                vec![],
            ])
            .with_stream_reasoning(vec![
                Some("Try the guarded file first.".to_string()),
                Some("The denied read is enough evidence to conclude.".to_string()),
            ])
            .with_stream_json(vec![
                None,
                Some(serde_json::json!({
                    "thought": "The denied read is enough evidence to conclude.",
                    "proposal": {
                        "kind": "finish",
                        "summary": "The guarded file could not be read.",
                        "result": {"blocked": true}
                    },
                    "confidence": 0.96
                })),
            ]);

        let mut auth = test_auth(
            "wo-tool-error-proposal",
            "run-tool-error-proposal",
            vec!["delete".to_string()],
        );
        auth.sandbox_profile = SandboxProfile::from_track("green", Some(root.clone()));

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                "wo-tool-error-proposal",
                "Read .env from the governed workspace and quote its contents exactly.",
            ),
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        assert!(result.reflection_id.is_some());
        assert!(
            result.proposal_id.is_some(),
            "tool error should still produce proposal"
        );
        let file_tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-tool-error-proposal' AND tool_id='file-readonly' AND success=0",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(file_tool_calls, 1);
    }

    #[tokio::test]
    async fn opc_pool_receives_self_upgrade_proposal_while_global_pool_keeps_reflection() {
        let pool = migrated_pool().await;
        let opc_pool = migrated_pool().await;
        configure_active_openai_compatible(&pool).await;
        let root =
            std::env::temp_dir().join(format!("coevo-opc-pool-proposal-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let env_path = root.join(".env");
        std::fs::write(&env_path, "TOP_SECRET=demo").unwrap();

        let gateway = ScriptedGateway::new(vec![])
            .with_streamed_tool_calls(vec![
                vec![ModelToolCall {
                    index: 0,
                    id: Some("call_env".to_string()),
                    name: "file-readonly".to_string(),
                    arguments: serde_json::json!({
                        "action": "ReadFile",
                        "path": env_path.to_string_lossy().to_string()
                    })
                    .to_string(),
                }],
                vec![],
            ])
            .with_stream_reasoning(vec![
                Some("Try the guarded file first.".to_string()),
                Some("The denied read is enough evidence to conclude.".to_string()),
            ])
            .with_stream_json(vec![
                None,
                Some(serde_json::json!({
                    "thought": "The denied read is enough evidence to conclude.",
                    "proposal": {
                        "kind": "finish",
                        "summary": "The guarded file could not be read.",
                        "result": {"blocked": true}
                    },
                    "confidence": 0.96
                })),
            ]);

        let mut auth = test_auth(
            "wo-opc-pool-proposal",
            "run-opc-pool-proposal",
            vec!["delete".to_string()],
        );
        auth.sandbox_profile = SandboxProfile::from_track("green", Some(root.clone()));

        let mut contract = test_contract(
            "wo-opc-pool-proposal",
            "Read .env from the governed workspace and quote its contents exactly.",
        );
        contract.opc_id = "opc-test-scope".to_string();

        let result = AgentSubHarness::execute_with_opc_pool(
            &pool,
            &opc_pool,
            root.clone(),
            &contract,
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let proposal_id = result
            .proposal_id
            .clone()
            .expect("tool error should still produce proposal");
        let reflection_id = result
            .reflection_id
            .clone()
            .expect("reflection should be recorded");

        let global_proposal_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM skill_evolution_proposals WHERE proposal_id = ?",
        )
        .bind(&proposal_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let opc_proposal_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM skill_evolution_proposals WHERE proposal_id = ?",
        )
        .bind(&proposal_id)
        .fetch_one(&opc_pool)
        .await
        .unwrap();
        let global_reflection_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM worker_reflections WHERE reflection_id = ?")
                .bind(&reflection_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let opc_reflection_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM worker_reflections WHERE reflection_id = ?")
                .bind(&reflection_id)
                .fetch_one(&opc_pool)
                .await
                .unwrap();

        assert_eq!(global_proposal_count, 0);
        assert_eq!(opc_proposal_count, 1);
        assert_eq!(global_reflection_count, 1);
        assert_eq!(opc_reflection_count, 0);
    }

    #[tokio::test]
    async fn stream_done_event_is_persisted_in_live_stream_path() {
        let pool = migrated_pool().await;
        let work_order_id = "wo-stream-done";
        let run_id = "run-stream-done";
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "Done event should be persisted before finish.",
            "proposal": {
                "kind": "finish",
                "summary": "Done event persisted.",
                "result": {"ok": true}
            },
            "confidence": 0.9
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(work_order_id, "Confirm the stream writes a Done event."),
            &test_auth(work_order_id, run_id, vec!["delete".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let done_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_events WHERE run_id=? AND event_type='Done'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(done_events, 1);

        let assistant_delta: String = sqlx::query_scalar(
            "SELECT payload_json FROM worker_events WHERE run_id=? AND event_type='AssistantDelta' ORDER BY event_seq DESC LIMIT 1",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&assistant_delta).unwrap();
        assert_eq!(payload["done_emitted"], serde_json::json!(true));
    }

    #[test]
    fn actual_cost_uses_live_deepseek_usage_when_available() {
        let routing = ModelRoutingDecision {
            selected_provider_id: "deepseek-live".to_string(),
            selected_model_id: "deepseek-chat".to_string(),
            selected_capabilities: vec![],
            reason: "test".to_string(),
            fallback_model_ids: vec![],
            estimated_cost_usd: Some(0.0),
            estimated_latency_ms: Some(300),
            governance_notes: vec![],
            decision_id: "mrd-test".to_string(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };

        let cost = actual_or_estimated_cost_usd(&routing, 913, 81);
        assert!(cost > 0.0, "live usage should produce non-zero cost");
    }

    #[tokio::test]
    async fn denied_action_feeds_back_and_model_retries() {
        let pool = migrated_pool().await;
        let root =
            std::env::temp_dir().join(format!("coevo-denied-retry-{}", uuid::Uuid::new_v4()));
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
            &test_contract(
                work_order_id,
                "Summarize the launch plan after governance feedback.",
            ),
            &test_auth(work_order_id, run_id, vec!["file-readonly".to_string()]),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
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
    async fn string_finish_proposal_is_normalized_to_finish_action() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "The checklist can be produced from context.",
            "proposal": "finish",
            "summary": "Founder checklist ready.",
            "result": {"items": ["Review company rules"]},
            "confidence": 0.82
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-string-finish", "Summarize company rules."),
            &test_auth(
                "wo-string-finish",
                "run-string-finish",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        assert!(result.summary.contains("Completed"));
        let tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-string-finish'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_calls, 0);
    }

    #[tokio::test]
    async fn proposal_object_without_kind_defaults_to_finish_action() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "The result is ready.",
            "proposal": {
                "summary": "Founder checklist ready.",
                "result": {"items": ["Review company rules"]}
            },
            "confidence": 0.82
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-missing-kind-finish", "Summarize company rules."),
            &test_auth(
                "wo-missing-kind-finish",
                "run-missing-kind-finish",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-missing-kind-finish'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_calls, 0);
    }

    #[tokio::test]
    async fn ask_human_proposal_without_blocking_defaults_to_blocking_approval() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I should ask the founder before proceeding.",
            "proposal": {
                "question": "Should I continue?"
            },
            "confidence": 0.82
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-missing-blocking", "Ask before continuing."),
            &test_auth(
                "wo-missing-blocking",
                "run-missing-blocking",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "WaitingApproval");
    }

    #[tokio::test]
    async fn string_call_tool_without_arguments_finishes_with_error_summary() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I should use a tool, but no concrete tool payload was emitted.",
            "proposal": "call_tool",
            "confidence": 0.5
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-string-call-tool", "Summarize company rules."),
            &test_auth(
                "wo-string-call-tool",
                "run-string-call-tool",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-string-call-tool'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_calls, 0);
    }

    #[tokio::test]
    async fn call_tool_proposal_without_input_does_not_crash() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I should use a file tool, but I did not provide concrete input.",
            "proposal": {
                "kind": "call_tool",
                "tool_id": "file-readonly",
                "rationale": "Read local evidence."
            },
            "confidence": 0.5
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-missing-tool-input", "Summarize company rules."),
            &test_auth(
                "wo-missing-tool-input",
                "run-missing-tool-input",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-missing-tool-input'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_calls, 0);
    }

    #[tokio::test]
    async fn inferred_call_tool_without_input_does_not_crash() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "I should use a file tool, but I did not provide concrete input.",
            "proposal": {
                "tool_id": "file-readonly",
                "rationale": "Read local evidence."
            },
            "summary": "I need a concrete file path before using the file tool.",
            "confidence": 0.5
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-inferred-missing-tool-input", "Summarize company rules."),
            &test_auth(
                "wo-inferred-missing-tool-input",
                "run-inferred-missing-tool-input",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-inferred-missing-tool-input'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_calls, 0);
    }

    #[tokio::test]
    async fn missing_thought_and_confidence_get_safe_defaults() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "proposal": {
                "summary": "Founder checklist ready.",
                "result": {"items": ["Review company rules"]}
            }
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-missing-thought", "Summarize company rules."),
            &test_auth(
                "wo-missing-thought",
                "run-missing-thought",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
    }

    #[test]
    fn parse_reasoning_output_accepts_finish_without_result_field() {
        let parsed = parse_reasoning_output(
            serde_json::json!({
                "thought": "Enough evidence gathered.",
                "proposal": {
                    "kind": "finish",
                    "summary": "Done"
                },
                "confidence": 0.5
            }),
            &[],
            false,
        )
        .expect("finish without result should parse");

        match parsed.proposal {
            ActionProposal::Finish { summary, result } => {
                assert_eq!(summary, "Done");
                assert_eq!(result, serde_json::json!({}));
            }
            other => panic!("unexpected proposal: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_output_accepts_finish_without_summary_or_result_fields() {
        let parsed = parse_reasoning_output(
            serde_json::json!({
                "thought": "Enough evidence gathered.",
                "proposal": {
                    "kind": "finish"
                },
                "confidence": 0.5
            }),
            &[],
            false,
        )
        .expect("finish without summary/result should parse");

        match parsed.proposal {
            ActionProposal::Finish { summary, result } => {
                assert_eq!(summary, "Enough evidence gathered.");
                assert_eq!(result, serde_json::json!({}));
            }
            other => panic!("unexpected proposal: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_output_rebuilds_string_spawn_subagent() {
        let parsed = parse_reasoning_output(
            serde_json::json!({
                "thought": "I need a focused helper for this.",
                "proposal": "spawn_subagent",
                "skill_id": "skill-research",
                "task": "Summarize the competitor landscape",
                "confidence": 0.6
            }),
            &[],
            false,
        )
        .expect("string spawn_subagent with skill_id and task should parse");
        match parsed.proposal {
            ActionProposal::SpawnSubagent { skill_id, task, .. } => {
                assert_eq!(skill_id, "skill-research");
                assert_eq!(task, "Summarize the competitor landscape");
            }
            other => panic!("unexpected proposal: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_output_spawn_subagent_missing_fields_finishes_with_error() {
        let parsed = parse_reasoning_output(
            serde_json::json!({
                "thought": "spawn without a task",
                "proposal": "spawn_subagent",
                "skill_id": "skill-research",
                "confidence": 0.6
            }),
            &[],
            false,
        )
        .expect("incomplete spawn_subagent should parse to a finish-with-error, not crash");
        match parsed.proposal {
            ActionProposal::Finish { summary, .. } => {
                assert!(summary.contains("spawn_subagent"));
            }
            other => panic!("expected finish fallback, got: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_output_rebuilds_string_call_tool_when_payload_fields_exist() {
        let parsed = parse_reasoning_output(
            serde_json::json!({
                "thought": "I can use the tool directly.",
                "proposal": "call_tool",
                "tool_id": "file-readonly",
                "input": {
                    "action": "ReadFile",
                    "path": "notes.txt"
                },
                "rationale": "The file payload is already present.",
                "confidence": 0.5
            }),
            &[],
            false,
        )
        .expect("call_tool string with payload should parse");

        match parsed.proposal {
            ActionProposal::CallTool {
                tool_id,
                input,
                rationale,
            } => {
                assert_eq!(tool_id, "file-readonly");
                assert_eq!(
                    input,
                    serde_json::json!({
                        "action": "ReadFile",
                        "path": "notes.txt"
                    })
                );
                assert_eq!(rationale, "The file payload is already present.");
            }
            other => panic!("unexpected proposal: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_output_rebuilds_string_call_executor_when_payload_fields_exist() {
        let parsed = parse_reasoning_output(
            serde_json::json!({
                "thought": "I can delegate this work.",
                "proposal": "call_executor",
                "executor_id": "external-echo",
                "task": {
                    "prompt": "inspect safely"
                },
                "rationale": "The executor payload is already present.",
                "confidence": 0.5
            }),
            &[],
            false,
        )
        .expect("call_executor string with payload should parse");

        match parsed.proposal {
            ActionProposal::CallExecutor {
                executor_id,
                task,
                rationale,
            } => {
                assert_eq!(executor_id, "external-echo");
                assert_eq!(
                    task,
                    serde_json::json!({
                        "prompt": "inspect safely"
                    })
                );
                assert_eq!(rationale, "The executor payload is already present.");
            }
            other => panic!("unexpected proposal: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_output_falls_back_to_finish_when_string_call_tool_is_missing_payload() {
        let parsed = parse_reasoning_output(
            serde_json::json!({
                "thought": "I should use a tool, but the payload is incomplete.",
                "proposal": "call_tool",
                "confidence": 0.5
            }),
            &[],
            false,
        )
        .expect("incomplete string call_tool should still parse");

        match parsed.proposal {
            ActionProposal::Finish { summary, result } => {
                assert!(
                    summary.contains("call_tool") && summary.contains("tool_id"),
                    "summary should explain what is missing: {summary}"
                );
                assert!(
                    result
                        .get("error")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| value.contains("call_tool")),
                    "result should preserve a machine-readable error: {result}"
                );
                assert_eq!(
                    result.get("missing_fields"),
                    Some(&serde_json::json!(["tool_id", "input"]))
                );
            }
            other => panic!("unexpected proposal: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_output_falls_back_to_finish_when_string_call_executor_is_missing_payload() {
        let parsed = parse_reasoning_output(
            serde_json::json!({
                "thought": "I should delegate this, but the payload is incomplete.",
                "proposal": "call_executor",
                "confidence": 0.5
            }),
            &[],
            false,
        )
        .expect("incomplete string call_executor should still parse");

        match parsed.proposal {
            ActionProposal::Finish { summary, result } => {
                assert!(
                    summary.contains("call_executor") && summary.contains("executor_id"),
                    "summary should explain what is missing: {summary}"
                );
                assert!(
                    result
                        .get("error")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| value.contains("call_executor")),
                    "result should preserve a machine-readable error: {result}"
                );
                assert_eq!(
                    result.get("missing_fields"),
                    Some(&serde_json::json!(["executor_id", "task"]))
                );
            }
            other => panic!("unexpected proposal: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_output_extracts_final_fenced_json_from_live_style_follow_up_text() {
        let parsed = parse_reasoning_output(
            serde_json::json!(
                r#"I have thoroughly investigated this request.

Evidence:
{"readonly_guards":[".git",".env"],"tier":"read_only"}

Conclusion:
```json
{
  "thought": "The sandbox policy blocks access to the guarded file.",
  "proposal": {
    "kind": "finish",
    "summary": "The .env file exists but cannot be read because readonly_guards blocks it.",
    "result": {
      "blocked": true
    }
  },
  "confidence": 1.0
}
```"#
            ),
            &[],
            false,
        )
        .expect("live-style follow-up text should parse");

        match parsed.proposal {
            ActionProposal::Finish { summary, result } => {
                assert!(summary.contains("readonly_guards"));
                assert_eq!(result["blocked"], true);
            }
            other => panic!("unexpected proposal: {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_proposal_defaults_to_finish_action() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "summary": "Founder checklist ready.",
            "result": {"items": ["Review company rules"]},
            "confidence": 0.82
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-missing-proposal", "Summarize company rules."),
            &test_auth(
                "wo-missing-proposal",
                "run-missing-proposal",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-missing-proposal'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_calls, 0);
    }

    #[tokio::test]
    async fn generic_structured_response_without_action_finishes_instead_of_waiting_approval() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "Model returned a structured action.",
            "confidence": 0.95
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-generic-structured", "Summarize company rules."),
            &test_auth(
                "wo-generic-structured",
                "run-generic-structured",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let approvals: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_events WHERE run_id='run-generic-structured' AND event_type='ApprovalRequired'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(approvals, 0);
    }

    #[tokio::test]
    async fn generic_placeholder_question_finishes_instead_of_waiting_approval() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "Model returned a structured action.",
            "proposal": {
                "question": "Model returned a structured action."
            },
            "confidence": 0.95
        })]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-generic-question", "Summarize company rules."),
            &test_auth(
                "wo-generic-question",
                "run-generic-question",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let approvals: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_events WHERE run_id='run-generic-question' AND event_type='ApprovalRequired'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(approvals, 0);
    }

    #[tokio::test]
    async fn file_evidence_mission_does_not_finish_without_any_observation() {
        let pool = migrated_pool().await;
        let root =
            std::env::temp_dir().join(format!("coevo-file-evidence-gate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let evidence_path = root.join("evidence.txt");
        std::fs::write(&evidence_path, "enterprise pilot requested security review").unwrap();
        let gateway = ScriptedGateway::new(vec![
            serde_json::json!({
                "thought": "I can answer from general intuition.",
                "proposal": {
                    "kind": "finish",
                    "summary": "B2B looks strongest.",
                    "result": {"done": true}
                },
                "confidence": 0.61
            }),
            serde_json::json!({
                "thought": "I need to inspect the requested file before finishing.",
                "proposal": {
                    "kind": "call_tool",
                    "tool_id": "file-readonly",
                    "input": {
                        "action": "ReadFile",
                        "path": evidence_path.to_string_lossy().to_string(),
                        "allowed_paths": [root.to_string_lossy().to_string()]
                    },
                    "rationale": "The task explicitly requires file evidence."
                },
                "confidence": 0.74
            }),
            serde_json::json!({
                "thought": "The file observation is enough to finish.",
                "proposal": {
                    "kind": "finish",
                    "summary": "The strongest signal is enterprise demand gated by security review.",
                    "result": {"done": true}
                },
                "confidence": 0.88
            }),
        ]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                "wo-file-evidence-no-finish",
                "Read the local evidence file first, then summarize the strongest signal.",
            ),
            &test_auth(
                "wo-file-evidence-no-finish",
                "run-file-evidence-no-finish",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "Completed");
        let denied_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_events WHERE run_id='run-file-evidence-no-finish' AND event_type='WorkerBlocked' AND payload_json LIKE '%must inspect allowed file evidence before finishing%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(denied_events, 1);
        let tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-file-evidence-no-finish' AND tool_id='file-readonly'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tool_calls, 1);
    }

    #[tokio::test]
    async fn file_evidence_mission_does_not_finish_after_governance_denial_without_file_attempt() {
        let pool = migrated_pool().await;
        let gateway = ScriptedGateway::new(vec![
            serde_json::json!({
                "thought": "I can answer without reading anything.",
                "proposal": {
                    "kind": "finish",
                    "summary": "General intuition is enough.",
                    "result": {"done": true}
                },
                "confidence": 0.51
            }),
            serde_json::json!({
                "thought": "The governance note itself is enough evidence.",
                "proposal": {
                    "kind": "finish",
                    "summary": "The task was blocked before inspection.",
                    "result": {"blocked": true}
                },
                "confidence": 0.55
            }),
            serde_json::json!({
                "thought": "I still have to inspect the requested file before finishing.",
                "proposal": {
                    "kind": "call_tool",
                    "tool_id": "file-readonly",
                    "input": {
                        "action": "ListDirectory",
                        "path": ".",
                        "allowed_paths": ["."]
                    },
                    "rationale": "A real file tool attempt is still required."
                },
                "confidence": 0.7
            }),
            serde_json::json!({
                "thought": "The file tool attempt is now enough to finish.",
                "proposal": {
                    "kind": "finish",
                    "summary": "A governed file-tool attempt happened before completion.",
                    "result": {"done": true}
                },
                "confidence": 0.83
            }),
        ]);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                "wo-file-evidence-denial-loop",
                "Inspect the workspace evidence file before finishing.",
            ),
            &test_auth(
                "wo-file-evidence-denial-loop",
                "run-file-evidence-denial-loop",
                vec!["delete".to_string()],
            ),
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "Completed");
        let denied_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_events WHERE run_id='run-file-evidence-denial-loop' AND event_type='WorkerBlocked' AND payload_json LIKE '%must inspect allowed file evidence before finishing%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(denied_events, 2);
        let file_tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_tool_calls WHERE run_id='run-file-evidence-denial-loop' AND tool_id='file-readonly'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(file_tool_calls, 1);
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
    async fn governance_decisions_are_persisted_to_risk_repo() {
        let pool = migrated_pool().await;
        let root = std::env::temp_dir().join(format!("coevo-risk-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let gateway = ScriptedGateway::new(vec![serde_json::json!({
            "thought": "Inspect the allowed directory before summarizing.",
            "proposal": {
                "kind": "call_tool",
                "tool_id": "file-readonly",
                "input": {
                    "action": "ListDirectory",
                    "path": root.to_string_lossy().to_string(),
                    "allowed_paths": [root.to_string_lossy().to_string()]
                },
                "rationale": "Legal read-only evidence gathering."
            },
            "confidence": 0.8
        })]);
        let mut auth = test_auth("wo-risk-audit", "run-risk-audit", vec![]);
        auth.contract_hash = "e".repeat(64);
        let execution_contract = test_mcl_contract("wo-risk-audit");
        coevo_store::repos::contract_repo::ContractRepo::insert(
            &pool,
            &execution_contract,
            &auth.contract_hash,
        )
        .await
        .unwrap();
        auth.execution_contract = Some(execution_contract);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-risk-audit", "Inspect evidence."),
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert!(matches!(
            result.final_status.as_str(),
            "Completed" | "TimedOut"
        ));
        let risk_rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM risk_decisions WHERE contract_hash=? AND agent_id=? AND action_urn LIKE 'urn:coevo:action:file-readonly:%'",
        )
        .bind(&auth.contract_hash)
        .bind(&auth.agent_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            risk_rows > 0,
            "expected worker RiskGate decision to be persisted"
        );
    }

    #[tokio::test]
    async fn loop_uses_persisted_contract_max_hops() {
        let pool = migrated_pool().await;
        let root =
            std::env::temp_dir().join(format!("coevo-contract-max-hops-{}", uuid::Uuid::new_v4()));
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
        let mut auth = test_auth("wo-contract-max-hops", "run-contract-max-hops", vec![]);
        let mut execution_contract = test_mcl_contract("wo-contract-max-hops");
        execution_contract.termination_policy.max_hops = 2;
        auth.execution_contract = Some(execution_contract);

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-contract-max-hops", "Keep trying an unavailable tool."),
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&root).ok();

        assert_eq!(result.final_status, "TimedOut");
        let model_steps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM worker_steps WHERE run_id='run-contract-max-hops' AND step_type='ModelCall' AND output_json LIKE '%file-readonly%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(model_steps, 2);
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
        let root =
            std::env::temp_dir().join(format!("coevo-external-executor-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("seed.txt"), "sandbox seed").unwrap();
        let mut auth = test_auth(work_order_id, run_id, vec![]);
        auth.allowed_actions.push("execute".to_string());
        auth.sandbox_profile = SandboxProfile::from_track("green", Some(root.clone()));

        let result = AgentSubHarness::execute(
            &pool,
            &test_contract(
                work_order_id,
                "Ask an external executor for bounded analysis.",
            ),
            &auth,
            &default_model_profiles(),
            None,
            &gateway,
            &ModelProviderConfig::mock(),
            &[&adapter],
            &[],
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
            &[],
        )
        .await
        .unwrap();

        assert_eq!(result.final_status, "WaitingApproval");
        let messages_json: String =
            sqlx::query_scalar("SELECT messages_json FROM worker_sessions WHERE session_id=?")
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
        let messages_json: String =
            sqlx::query_scalar("SELECT messages_json FROM worker_sessions WHERE session_id=?")
                .bind(&auth.session_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let cursor: serde_json::Value = serde_json::from_str(&messages_json).unwrap();
        let digest = cursor["pending_action_digest"].as_str().unwrap();
        let mut resumed_auth = auth.clone();
        resumed_auth.run_id = "run2-resume-cursor".to_string();
        resumed_auth.approval_receipt = Some(format!("approval-receipt:{digest}"));
        let resumed = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-resume-cursor", "Pause and resume."),
            &resumed_auth,
            &default_model_profiles(),
            None,
            &resume_gateway,
            &ModelProviderConfig::mock(),
            &[],
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

    #[tokio::test]
    async fn resume_with_mismatched_pending_action_digest_fails_closed() {
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
        let auth = test_auth("wo-resume-digest", "run-resume-digest-1", vec![]);
        create_worker_session(&pool, &auth).await;
        let paused = AgentSubHarness::execute(
            &pool,
            &test_contract("wo-resume-digest", "Pause and resume."),
            &auth,
            &default_model_profiles(),
            None,
            &pause_gateway,
            &ModelProviderConfig::mock(),
            &[],
            &[],
        )
        .await
        .unwrap();
        assert_eq!(paused.final_status, "WaitingApproval");

        let mut resumed_auth = auth.clone();
        resumed_auth.run_id = "run-resume-digest-2".to_string();
        resumed_auth.approval_receipt = Some("approval-receipt:wrong-digest".to_string());
        let err = load_resume_cursor(&pool, &resumed_auth).await;
        assert!(matches!(err, Err(WorkerError::YellowApprovalRequired)));
    }
}
