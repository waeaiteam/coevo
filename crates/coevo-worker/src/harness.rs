use crate::agent_harness::{AgentRunContract, AgentSubHarness, RunAuthorization};
use crate::error::WorkerError;
use crate::queue::WorkerQueueService;
use crate::r#loop::{SandboxProfile, SandboxTier};
use coevo_core::contract::MCLSpec;
use coevo_core::opc::{AutonomyCeiling, ExecutorStatus, ExternalExecutorPassport, ModelPreference};
use coevo_models::gateway::select_gateway;
use coevo_models::router::{default_model_profiles, ModelCapability, ModelProfile, PrivacyLevel};
use coevo_models::types::{ModelProviderConfig, ModelProviderKind};
use coevo_store::repos::worker_run_repo::{WorkerEventRepo, WorkerRunRepo, WorkerStepRepo};
use coevo_store::repos::{
    agent_worker_repo::AgentWorkerRepo, contract_repo::ContractRepo,
    model_config_repo::ModelConfigRepo,
};
use coevo_store::repos_opc::work_order_repo;
use sqlx::SqlitePool;
use sqlx::{Column, Row};

pub struct WorkerHarnessOptions {
    pub approval_receipt: Option<String>,
    pub max_runtime_ms: Option<i64>,
    pub deterministic_mode: bool,
    pub preferred_tool_ids: Vec<String>,
    pub allow_mock_model_routing: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkerHarnessResult {
    pub work_order_id: String,
    pub worker_runs: Vec<serde_json::Value>,
    pub worker_steps: Vec<serde_json::Value>,
    pub worker_events: Vec<serde_json::Value>,
    pub skill_usage: Vec<serde_json::Value>,
    pub tool_calls: Vec<serde_json::Value>,
    pub memory_ids: Vec<String>,
    pub reflection_id: Option<String>,
    pub proposal_id: Option<String>,
    pub status: String,
    pub termination_reason: String,
    pub summary: String,
}

fn workspace_root_from_env_or_cwd() -> Option<std::path::PathBuf> {
    std::env::var("COEVO_WORKSPACE_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
}

async fn load_execution_contract(
    pool: &SqlitePool,
    opc_pool: &SqlitePool,
    contract_hash: &str,
) -> Result<Option<MCLSpec>, WorkerError> {
    if contract_hash.trim().is_empty() {
        return Ok(None);
    }
    match ContractRepo::find_spec_by_hash(opc_pool, contract_hash).await {
        Ok(Some(contract)) => return Ok(Some(contract)),
        Ok(None) => {}
        Err(err) => return Err(WorkerError::Internal(err.to_string())),
    }
    if std::ptr::eq(pool, opc_pool) {
        return Ok(None);
    }
    ContractRepo::find_spec_by_hash(pool, contract_hash)
        .await
        .map_err(|err| WorkerError::Internal(err.to_string()))
}

fn select_selected_executor_passports(
    work_order: &coevo_core::opc::WorkOrder,
    executor_passports: Vec<ExternalExecutorPassport>,
) -> Result<Vec<ExternalExecutorPassport>, WorkerError> {
    if work_order.selected_executors.is_empty() {
        return Ok(Vec::new());
    }

    let mut passports_by_id = std::collections::HashMap::new();
    for passport in executor_passports {
        passports_by_id.insert(passport.executor_id.clone(), passport);
    }

    let mut selected = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for executor_id in &work_order.selected_executors {
        if !seen.insert(executor_id) {
            continue;
        }
        let Some(passport) = passports_by_id.get(executor_id) else {
            return Err(WorkerError::ToolUnavailable(format!(
                "Selected executor {executor_id} is missing"
            )));
        };
        if passport.status == ExecutorStatus::Disabled {
            return Err(WorkerError::ToolUnavailable(format!(
                "Selected executor {executor_id} is disabled"
            )));
        }
        selected.push(passport.clone());
    }

    Ok(selected)
}

async fn model_profiles_for_execution(
    pool: &SqlitePool,
    allow_mock_model_routing: bool,
) -> Result<(Vec<ModelProfile>, ModelProviderConfig), WorkerError> {
    let active = ModelConfigRepo::get_active_config(pool)
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;
    match active {
        Some(config) if config.kind != ModelProviderKind::Mock => {
            Ok((model_profiles_from_config(&config), config))
        }
        Some(config) if allow_mock_model_routing => Ok((default_model_profiles(), config)),
        Some(_) => Err(WorkerError::Internal(
            "MODEL_PROVIDER_NOT_CONFIGURED: active provider is Mock; configure a real model provider before WorkOrder execution".into(),
        )),
        None if allow_mock_model_routing => Ok((default_model_profiles(), ModelProviderConfig::mock())),
        None => Err(WorkerError::Internal(
            "MODEL_PROVIDER_NOT_CONFIGURED: configure a real model provider before WorkOrder execution".into(),
        )),
    }
}

fn model_profiles_from_config(config: &ModelProviderConfig) -> Vec<ModelProfile> {
    let provider_id = config.provider_id.clone();
    let privacy_level = match config.kind {
        ModelProviderKind::Local | ModelProviderKind::Ollama => PrivacyLevel::LocalOnly,
        _ => PrivacyLevel::PublicApi,
    };
    let provider_name = format!("{:?}", config.kind);
    let max_context_tokens = config.max_tokens.max(4096);
    let base_caps = vec![
        ModelCapability::FastText,
        ModelCapability::Summarization,
        ModelCapability::StructuredJSON,
    ];
    let (default_input_cost, default_output_cost) = provider_pricing(config);
    let profiles = [
        (
            config.fast_model.as_str(),
            format!("{} Fast", provider_name),
            {
                let mut caps = base_caps.clone();
                caps.push(ModelCapability::ToolPlanning);
                caps
            },
            true,
            true,
            300,
        ),
        (
            config.default_model.as_str(),
            format!("{} Default", provider_name),
            vec![
                ModelCapability::FastText,
                ModelCapability::Summarization,
                ModelCapability::StructuredJSON,
                ModelCapability::DeepReasoning,
                ModelCapability::ToolPlanning,
                ModelCapability::CodeReview,
                ModelCapability::LongContext,
                ModelCapability::VisionUnderstanding,
                ModelCapability::ImageGeneration,
                ModelCapability::SlideGeneration,
                ModelCapability::ThreeDGeneration,
            ],
            true,
            true,
            800,
        ),
        (
            config.reasoning_model.as_str(),
            format!("{} Reasoning", provider_name),
            vec![
                ModelCapability::DeepReasoning,
                ModelCapability::ToolPlanning,
                ModelCapability::RiskCritique,
                ModelCapability::StructuredJSON,
                ModelCapability::CodeGeneration,
                ModelCapability::CodeReview,
                ModelCapability::LongContext,
                ModelCapability::SkillGeneration,
            ],
            true,
            true,
            1000,
        ),
        (
            config.structured_output_model.as_str(),
            format!("{} Structured", provider_name),
            vec![
                ModelCapability::StructuredJSON,
                ModelCapability::FastText,
                ModelCapability::Summarization,
                ModelCapability::SkillGeneration,
            ],
            true,
            false,
            600,
        ),
    ];
    let mut out: Vec<ModelProfile> = vec![];
    for (model_id, display_name, capabilities, supports_json, supports_tools, latency) in profiles {
        if model_id.trim().is_empty() {
            continue;
        }
        if let Some(existing) = out.iter_mut().find(|p| p.model_id == model_id) {
            for capability in capabilities {
                if !existing.capabilities.contains(&capability) {
                    existing.capabilities.push(capability);
                }
            }
            existing.supports_json = existing.supports_json || supports_json;
            existing.supports_tools = existing.supports_tools || supports_tools;
            existing.avg_latency_ms = existing.avg_latency_ms.min(latency);
            continue;
        }
        out.push(ModelProfile {
            provider_id: provider_id.clone(),
            model_id: model_id.to_string(),
            display_name,
            capabilities,
            max_context_tokens,
            cost_per_1k_input_usd: default_input_cost,
            cost_per_1k_output_usd: default_output_cost,
            avg_latency_ms: latency,
            supports_json,
            supports_tools,
            privacy_level,
            enabled: true,
        });
    }
    for profile in &mut out {
        profile
            .capabilities
            .sort_by_key(|capability| format!("{capability:?}"));
        profile.capabilities.dedup();
    }
    out
}

fn provider_pricing(config: &ModelProviderConfig) -> (f64, f64) {
    let model_ids = [
        config.default_model.as_str(),
        config.fast_model.as_str(),
        config.reasoning_model.as_str(),
        config.structured_output_model.as_str(),
    ];
    let lower = model_ids
        .iter()
        .find(|id| !id.trim().is_empty())
        .copied()
        .unwrap_or_default()
        .to_ascii_lowercase();

    match config.kind {
        ModelProviderKind::DeepSeek => {
            if lower.contains("deepseek-v4-flash") || lower.contains("deepseek-chat") {
                (0.00014, 0.00028)
            } else if lower.contains("deepseek-reasoner") {
                (0.00055, 0.00219)
            } else {
                (0.00014, 0.00028)
            }
        }
        ModelProviderKind::OpenAICompatible => {
            if lower.contains("deepseek-v4-flash") || lower.contains("deepseek-chat") {
                (0.00014, 0.00028)
            } else if lower.contains("deepseek-reasoner") {
                (0.00055, 0.00219)
            } else {
                (0.0, 0.0)
            }
        }
        _ => (0.0, 0.0),
    }
}

fn sandbox_tier_from_ceiling(ceiling: AutonomyCeiling) -> SandboxTier {
    match ceiling {
        AutonomyCeiling::ReadOnly => SandboxTier::ReadOnly,
        AutonomyCeiling::WorkspaceWrite => SandboxTier::WorkspaceWrite,
        AutonomyCeiling::FullAccess => SandboxTier::FullAccess,
    }
}

fn model_preference_to_role(preference: ModelPreference) -> &'static str {
    match preference {
        ModelPreference::Fast => "fast",
        ModelPreference::Standard => "standard",
        ModelPreference::Reasoning => "reasoning",
    }
}

pub struct WorkerHarness;
impl WorkerHarness {
    async fn finalize_run_failure(
        pool: &SqlitePool,
        worker_id: &str,
        session_id: &str,
        run_id: &str,
        err: &WorkerError,
    ) {
        let now = chrono::Utc::now().timestamp_millis();
        let safe_error = err.to_string();
        let payload = serde_json::json!({
            "status": "Failed",
            "error": safe_error,
        });
        sqlx::query(
            "UPDATE worker_runs SET status='Failed', errors_json=?, ended_at_ms=? WHERE run_id=?",
        )
        .bind(
            serde_json::to_string(&serde_json::json!([safe_error]))
                .unwrap_or_else(|_| "[]".to_string()),
        )
        .bind(now)
        .bind(run_id)
        .execute(pool)
        .await
        .ok();
        WorkerEventRepo::append(
            pool,
            run_id,
            "LifecycleError",
            &serde_json::to_string(&payload)
                .unwrap_or_else(|_| "{\"status\":\"Failed\"}".to_string()),
        )
        .await
        .ok();
        WorkerEventRepo::append(
            pool,
            run_id,
            "LifecycleEnd",
            &serde_json::to_string(&serde_json::json!({"status":"Failed"}))
                .unwrap_or_else(|_| "{\"status\":\"Failed\"}".to_string()),
        )
        .await
        .ok();
        sqlx::query(
            "UPDATE worker_sessions SET status='Failed',updated_at_ms=? WHERE session_id=?",
        )
        .bind(now)
        .bind(session_id)
        .execute(pool)
        .await
        .ok();
        AgentWorkerRepo::set_status(pool, worker_id, "Failed")
            .await
            .ok();
        WorkerQueueService::release(pool, session_id, run_id)
            .await
            .ok();
    }

    pub async fn run_work_order(
        pool: &SqlitePool,
        work_order_id: &str,
        options: WorkerHarnessOptions,
    ) -> Result<WorkerHarnessResult, WorkerError> {
        Self::run_work_order_with_pools(pool, pool, work_order_id, options).await
    }

    pub async fn run_work_order_with_pools(
        pool: &SqlitePool,
        opc_pool: &SqlitePool,
        work_order_id: &str,
        options: WorkerHarnessOptions,
    ) -> Result<WorkerHarnessResult, WorkerError> {
        let now = || chrono::Utc::now().timestamp_millis();
        let wo = match work_order_repo::WorkOrderRepo::get(pool, work_order_id)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?
        {
            Some(wo) => wo,
            None if !std::ptr::eq(pool, opc_pool) => {
                work_order_repo::WorkOrderRepo::get(opc_pool, work_order_id)
                    .await
                    .map_err(|e| WorkerError::Internal(e.to_string()))?
                    .ok_or(WorkerError::WorkOrderNotFound(work_order_id.into()))?
            }
            None => return Err(WorkerError::WorkOrderNotFound(work_order_id.into())),
        };
        let agent_id = wo.selected_agents.first().cloned().unwrap_or_default();
        if agent_id.is_empty() {
            return Err(WorkerError::WorkerNotFound("No agent selected".into()));
        }

        // Authoritative governance gate stays in the Product Harness. Yellow and Red
        // both require a human-approved receipt before execution; Green runs
        // autonomously. High-risk (Red) work proceeds once it carries explicit
        // approval — there is no alpha hard-block.
        if (wo.track == "yellow" || wo.track == "red") && options.approval_receipt.is_none() {
            return Err(WorkerError::YellowApprovalRequired);
        }
        let (model_profiles, provider_config) =
            model_profiles_for_execution(pool, options.allow_mock_model_routing).await?;
        let gateway = select_gateway(provider_config.kind);

        let worker_id = format!("worker-{}", agent_id);
        match AgentWorkerRepo::get(pool, &worker_id)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?
        {
            Some(_) => {
                AgentWorkerRepo::set_status(pool, &worker_id, "Assigned")
                    .await
                    .map_err(|e| WorkerError::Internal(e.to_string()))?;
            }
            None => {
                AgentWorkerRepo::upsert(
                    pool,
                    &worker_id,
                    if wo.opc_id.trim().is_empty() {
                        "default-opc"
                    } else {
                        &wo.opc_id
                    },
                    &agent_id,
                    "Default",
                    "Assigned",
                    Some(work_order_id),
                    None,
                    "[]",
                    "Task",
                    "[]",
                    now(),
                    now(),
                )
                .await
                .map_err(|e| WorkerError::Internal(e.to_string()))?;
            }
        }

        let session_id = format!("session-{}", work_order_id);
        sqlx::query(
            "INSERT OR IGNORE INTO worker_sessions (
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
        .bind(&session_id)
        .bind(if wo.opc_id.trim().is_empty() {
            "default-opc"
        } else {
            &wo.opc_id
        })
        .bind(&worker_id)
        .bind(work_order_id)
        .bind(&agent_id)
        .bind("MissionChat")
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("[]")
        .bind("Running")
        .bind(now())
        .bind(now())
        .execute(pool)
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;
        sqlx::query(
            "UPDATE worker_sessions SET status='Running',updated_at_ms=? WHERE session_id=?",
        )
        .bind(now())
        .bind(&session_id)
        .execute(pool)
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;

        let run_id = format!("run-{}", uuid::Uuid::new_v4());
        WorkerRunRepo::create(
            pool,
            if wo.opc_id.trim().is_empty() {
                "default-opc"
            } else {
                &wo.opc_id
            },
            &run_id,
            work_order_id,
            &agent_id,
            &worker_id,
            &session_id,
            "Running",
            "{}",
            "[]",
            "[]",
            None,
            now(),
            None,
        )
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;

        let _cancellation = crate::worker_cancel::register_run(run_id.clone());

        WorkerQueueService::acquire(pool, &session_id, &run_id, 120_000).await?;
        AgentWorkerRepo::upsert(
            pool,
            &worker_id,
            if wo.opc_id.trim().is_empty() {
                "default-opc"
            } else {
                &wo.opc_id
            },
            &agent_id,
            "Default",
            "Executing",
            Some(work_order_id),
            Some(&session_id),
            "[]",
            "Task",
            "[]",
            now(),
            now(),
        )
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerEventRepo::append(
            pool,
            &run_id,
            "LifecycleStart",
            &serde_json::to_string(&serde_json::json!({"status":"Running"})).unwrap(),
        )
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;

        let run_contract = AgentRunContract {
            work_order_id: work_order_id.to_string(),
            mission_intent: wo.mission_intent.clone(),
            required_skills: wo.required_skills.clone(),
            user_id: wo.user_id.clone(),
            opc_id: wo.opc_id.clone(),
        };
        let effective_tier = wo
            .governance_verdict
            .as_ref()
            .map(|verdict| sandbox_tier_from_ceiling(verdict.effective_tier))
            .unwrap_or_else(|| {
                SandboxProfile::from_track(&wo.track, workspace_root_from_env_or_cwd()).tier
            });
        let model_preference = wo
            .governance_proposal
            .as_ref()
            .map(|proposal| model_preference_to_role(proposal.model_preference).to_string());
        let execution_contract = load_execution_contract(pool, opc_pool, &wo.contract_hash).await?;
        let authorization = RunAuthorization {
            work_order_id: work_order_id.to_string(),
            agent_id: agent_id.clone(),
            worker_id: worker_id.clone(),
            session_id: session_id.clone(),
            run_id: run_id.clone(),
            track: wo.track.clone(),
            allowed_actions: wo.allowed_actions.clone(),
            restricted_actions: wo.restricted_actions.clone(),
            approval_receipt: options.approval_receipt.clone(),
            contract_hash: wo.contract_hash.clone(),
            plan_hash: wo.plan_hash.clone(),
            sandbox_profile: SandboxProfile::from_tier(
                effective_tier,
                workspace_root_from_env_or_cwd(),
            ),
            model_preference,
            execution_contract,
        };
        // Bind real adapters for every registered external executor so the agent
        // loop can dispatch CallExecutor proposals to live runtimes (Docker, local
        // process, HTTP runtimes) under governance — no more "no adapter bound".
        let executor_passports = {
            let mut list = coevo_store::repos_opc::executor_repo::ExecutorRepo::list(opc_pool)
                .await
                .unwrap_or_default();
            if !std::ptr::eq(pool, opc_pool) {
                if let Ok(extra) =
                    coevo_store::repos_opc::executor_repo::ExecutorRepo::list(pool).await
                {
                    for passport in extra {
                        if !list.iter().any(|e| e.executor_id == passport.executor_id) {
                            list.push(passport);
                        }
                    }
                }
            }
            list
        };
        let selected_executor_passports =
            select_selected_executor_passports(&wo, executor_passports)?;
        let bound_executors: Vec<crate::r#loop::BoundExecutorAdapter> = selected_executor_passports
            .into_iter()
            .map(|p| crate::r#loop::BoundExecutorAdapter::new(p, wo.clone()))
            .collect();
        let external_agent_refs: Vec<&dyn crate::r#loop::ExternalAgentAdapter> = bound_executors
            .iter()
            .map(|b| b as &dyn crate::r#loop::ExternalAgentAdapter)
            .collect();

        let sub_result = AgentSubHarness::execute_with_opc_pool(
            pool,
            opc_pool,
            std::env::var("COEVO_HOME")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(std::path::PathBuf::from)
                .or_else(workspace_root_from_env_or_cwd)
                .unwrap_or_else(std::env::temp_dir),
            &run_contract,
            &authorization,
            &model_profiles,
            options.max_runtime_ms,
            gateway.as_ref(),
            &provider_config,
            &external_agent_refs,
            &options.preferred_tool_ids,
        )
        .await;
        let sub_result = match sub_result {
            Ok(result) => result,
            Err(err) => {
                Self::finalize_run_failure(pool, &worker_id, &session_id, &run_id, &err).await;
                return Err(err);
            }
        };

        sqlx::query("UPDATE worker_runs SET result_json=? WHERE run_id=?")
            .bind(
                serde_json::to_string(&serde_json::json!({
                    "status": &sub_result.final_status,
                    "termination_reason": &sub_result.termination_reason,
                    "summary": &sub_result.summary,
                    "memory_ids": &sub_result.memory_ids,
                }))
                .unwrap_or_else(|_| "{}".to_string()),
            )
            .bind(&run_id)
            .execute(pool)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerRunRepo::set_status(pool, &run_id, &sub_result.final_status)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;

        // Close the self-optimization loop's "evaluate" arm: turn the run outcome
        // into a reputation update and a growth-history snapshot for this employee.
        {
            let outcome = match sub_result.final_status.as_str() {
                "Completed" => 1.0,
                "WaitingApproval" => 0.6,
                "TimedOut" | "Blocked" => 0.2,
                _ => 0.0, // Failed / Cancelled
            };
            // Difficulty proxy: more reasoning rounds (tokens) => harder task.
            let difficulty = if sub_result.total_tokens > 4000 {
                4.0
            } else if sub_result.total_tokens > 1500 {
                3.0
            } else {
                2.0
            };
            // Expected outcome = the success expectation at decision time, proxied by
            // the track's autonomy: Green read-only work is expected to succeed more
            // often than approval-gated Yellow/Red work. Beating expectation rewards
            // reputation; missing it penalizes — a flat 0.5 made every track identical.
            let expected_outcome = match wo.track.as_str() {
                "green" => 0.7,
                "yellow" => 0.55,
                "red" => 0.5,
                _ => 0.6,
            };
            if let Ok(rv) = coevo_reputation::scoring::ReputationEngine::update(
                pool,
                &sub_result.agent_id,
                difficulty,
                outcome,
                expected_outcome,
            )
            .await
            {
                let _ = coevo_store::repos::reputation_repo::ReputationHistoryRepo::snapshot(
                    pool,
                    &sub_result.agent_id,
                    Some(&run_id),
                    rv.task_domain_competence,
                    rv.uncertainty_honesty,
                    rv.policy_compliance,
                    rv.resource_efficiency,
                    rv.task_count as i64,
                )
                .await;
            }
        }
        WorkerEventRepo::append(
            pool,
            &run_id,
            "LifecycleEnd",
            &serde_json::to_string(&serde_json::json!({"status":sub_result.final_status})).unwrap(),
        )
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))?;
        sqlx::query("UPDATE worker_sessions SET status=?,updated_at_ms=? WHERE session_id=?")
            .bind(&sub_result.final_status)
            .bind(now())
            .bind(&session_id)
            .execute(pool)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerQueueService::release(pool, &session_id, &run_id).await?;

        Self::build_result(
            pool,
            work_order_id,
            &run_id,
            sub_result.memory_ids,
            sub_result.reflection_id,
            sub_result.proposal_id,
            &sub_result.final_status,
            &sub_result.termination_reason,
            sub_result.summary,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn build_result(
        pool: &SqlitePool,
        wo_id: &str,
        run_id: &str,
        mem_ids: Vec<String>,
        reflection_id: Option<String>,
        proposal_id: Option<String>,
        status: &str,
        termination_reason: &str,
        summary: String,
    ) -> Result<WorkerHarnessResult, WorkerError> {
        let w_steps = WorkerStepRepo::list_by_run(pool, run_id)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let w_events = WorkerEventRepo::list_by_run(pool, run_id)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let w_runs = WorkerRunRepo::list_by_work_order(pool, wo_id)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let w_skills = sqlx::query("SELECT * FROM worker_skill_usage WHERE run_id=?")
            .bind(run_id)
            .fetch_all(pool)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let w_tools = sqlx::query("SELECT * FROM worker_tool_calls WHERE run_id=?")
            .bind(run_id)
            .fetch_all(pool)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;

        fn to_json(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<serde_json::Value> {
            rows.iter()
                .map(|r| {
                    let mut m = serde_json::Map::new();
                    for (i, col) in r.columns().iter().enumerate() {
                        let name = col.name().to_string();
                        if let Ok(v) = r.try_get::<String, _>(i) {
                            m.insert(name, serde_json::Value::String(v));
                            continue;
                        }
                        if let Ok(v) = r.try_get::<i64, _>(i) {
                            m.insert(name, serde_json::Value::Number(v.into()));
                            continue;
                        }
                        if let Ok(v) = r.try_get::<f64, _>(i) {
                            m.insert(name, serde_json::json!(v));
                            continue;
                        }
                    }
                    serde_json::Value::Object(m)
                })
                .collect()
        }

        Ok(WorkerHarnessResult {
            work_order_id: wo_id.into(),
            worker_runs: to_json(w_runs),
            worker_steps: to_json(w_steps),
            worker_events: to_json(w_events),
            skill_usage: to_json(w_skills),
            tool_calls: to_json(w_tools),
            memory_ids: mem_ids,
            reflection_id,
            proposal_id,
            status: status.into(),
            termination_reason: termination_reason.into(),
            summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_core::opc::{
        ExecutorSourceType, ExecutorStatus, ExternalExecutorPassport, MemoryScope,
        PermissionBoundary, SandboxLevel, WorkOrder, WorkOrderStatus,
    };
    use coevo_models::router::{ModelCapability, ModelRouter, ModelRoutingRequest, PrivacyLevel};
    use coevo_store::migrate::run_migrations;
    use coevo_store::pool::create_test_pool;
    use coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo;
    use coevo_store::repos_opc::skill_repo::SkillRepo;
    use coevo_store::repos_opc::work_order_repo::WorkOrderRepo;
    use sqlx::Row;

    fn test_work_order(work_order_id: &str, track: &str) -> WorkOrder {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        WorkOrder {
            work_order_id: work_order_id.to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "Analyze README".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: track.to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec!["delete".to_string()],
            risk_summary: "test".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        }
    }

    fn test_mcl_spec(work_order_id: &str, max_hops: u32) -> MCLSpec {
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
                max_hops,
                max_latency_ms: 60_000,
                max_stance_rounds: 16,
            },
            responsibility_anchor_policy: coevo_core::contract::ResponsibilityAnchorPolicy {
                required_human_roles: vec!["founder".to_string()],
                agent_forbidden_actions: vec![],
            },
        }
    }

    fn test_executor(executor_id: &str, status: ExecutorStatus) -> ExternalExecutorPassport {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        ExternalExecutorPassport {
            executor_id: executor_id.to_string(),
            display_name: executor_id.to_string(),
            source_type: ExecutorSourceType::LocalProcess,
            runtime_endpoint: "http://localhost".to_string(),
            capabilities: vec!["read".to_string()],
            required_credentials: vec![],
            permission_boundary: PermissionBoundary {
                max_risk_score: 0.5,
                can_write_fact: false,
                can_write_decision: false,
                can_access_network: false,
                can_access_filesystem: false,
                can_call_external_executor: false,
                can_propose_skill: false,
            },
            file_scope: vec![],
            network_scope: vec![],
            memory_scope: MemoryScope::Executor,
            risk_ceiling: 0.5,
            supported_actions: vec!["read".to_string()],
            sandbox_level: SandboxLevel::Process,
            health_check_url: String::new(),
            audit_callback_url: String::new(),
            status,
            created_at_ms: now,
            updated_at_ms: now,
        }
    }

    #[tokio::test]
    async fn load_execution_contract_prefers_company_scope_and_falls_back_to_global() {
        let global_pool = create_test_pool().await.unwrap();
        let company_pool = create_test_pool().await.unwrap();
        run_migrations(&global_pool).await.unwrap();
        run_migrations(&company_pool).await.unwrap();
        let shared_hash = "c".repeat(64);
        ContractRepo::insert(&global_pool, &test_mcl_spec("wo-global", 16), &shared_hash)
            .await
            .unwrap();
        ContractRepo::insert(&company_pool, &test_mcl_spec("wo-company", 2), &shared_hash)
            .await
            .unwrap();

        let scoped = load_execution_contract(&global_pool, &company_pool, &shared_hash)
            .await
            .unwrap()
            .expect("company contract should load");
        assert_eq!(scoped.termination_policy.max_hops, 2);

        let global_only_hash = "d".repeat(64);
        ContractRepo::insert(
            &global_pool,
            &test_mcl_spec("wo-global-only", 5),
            &global_only_hash,
        )
        .await
        .unwrap();
        let fallback = load_execution_contract(&global_pool, &company_pool, &global_only_hash)
            .await
            .unwrap()
            .expect("global fallback contract should load");
        assert_eq!(fallback.termination_policy.max_hops, 5);
    }

    #[test]
    fn workspace_root_prefers_env_override() {
        let root = std::env::temp_dir().join(format!("coevo-workspace-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("COEVO_WORKSPACE_DIR", &root);

        let selected = workspace_root_from_env_or_cwd().expect("workspace root should resolve");

        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(selected, root);
    }

    #[tokio::test]
    async fn red_without_approval_waits_before_model_provider_resolution() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        AgentEmployeeRepo::seed(&pool).await.unwrap();
        SkillRepo::seed_default(&pool).await.unwrap();
        let wo = test_work_order("wo-red-governance-first", "red");
        WorkOrderRepo::create(&pool, &wo).await.unwrap();

        let err = WorkerHarness::run_work_order(
            &pool,
            &wo.work_order_id,
            WorkerHarnessOptions {
                approval_receipt: None,
                max_runtime_ms: None,
                deterministic_mode: true,
                preferred_tool_ids: vec![],
                allow_mock_model_routing: false,
            },
        )
        .await
        .expect_err("red should require approval before provider resolution");

        assert!(err.to_string().contains("Yellow approval required"));
        let sessions =
            sqlx::query("SELECT COUNT(*) as count FROM worker_sessions WHERE work_order_id=?")
                .bind(&wo.work_order_id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .get::<i64, _>("count");
        let runs = sqlx::query("SELECT COUNT(*) as count FROM worker_runs WHERE work_order_id=?")
            .bind(&wo.work_order_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<i64, _>("count");
        assert_eq!(sessions, 0);
        assert_eq!(runs, 0);
    }

    #[tokio::test]
    async fn yellow_without_approval_waits_before_model_provider_resolution() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        AgentEmployeeRepo::seed(&pool).await.unwrap();
        SkillRepo::seed_default(&pool).await.unwrap();
        let wo = test_work_order("wo-yellow-governance-first", "yellow");
        WorkOrderRepo::create(&pool, &wo).await.unwrap();

        let err = WorkerHarness::run_work_order(
            &pool,
            &wo.work_order_id,
            WorkerHarnessOptions {
                approval_receipt: None,
                max_runtime_ms: None,
                deterministic_mode: true,
                preferred_tool_ids: vec![],
                allow_mock_model_routing: false,
            },
        )
        .await
        .expect_err("yellow should require approval before provider resolution");

        assert!(err.to_string().contains("Yellow approval required"));
        let sessions =
            sqlx::query("SELECT COUNT(*) as count FROM worker_sessions WHERE work_order_id=?")
                .bind(&wo.work_order_id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .get::<i64, _>("count");
        let runs = sqlx::query("SELECT COUNT(*) as count FROM worker_runs WHERE work_order_id=?")
            .bind(&wo.work_order_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<i64, _>("count");
        assert_eq!(sessions, 0);
        assert_eq!(runs, 0);
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

    #[test]
    fn merged_profiles_keep_reasoning_when_model_ids_are_identical() {
        let config = ModelProviderConfig {
            provider_id: "desktop".to_string(),
            kind: ModelProviderKind::DeepSeek,
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            default_model: "deepseek-v4-flash".to_string(),
            fast_model: "deepseek-v4-flash".to_string(),
            reasoning_model: "deepseek-v4-flash".to_string(),
            structured_output_model: "deepseek-v4-flash".to_string(),
            max_tokens: 8192,
            temperature: 0.2,
            timeout_ms: 30000,
            max_cost_per_task_usd: 5.0,
        };
        let profiles = model_profiles_from_config(&config);
        assert_eq!(profiles.len(), 1);
        let selected = &profiles[0];
        assert_eq!(selected.model_id, "deepseek-v4-flash");
        assert!(selected
            .capabilities
            .contains(&ModelCapability::DeepReasoning));
        assert!(selected
            .capabilities
            .contains(&ModelCapability::StructuredJSON));
        assert!(selected
            .capabilities
            .contains(&ModelCapability::ToolPlanning));

        let request = ModelRoutingRequest {
            work_order_id: "wo-1".to_string(),
            agent_id: "agent-1".to_string(),
            worker_step_type: "ModelCall".to_string(),
            intent: "analyze risk and plan tools".to_string(),
            required_capabilities: vec![
                ModelCapability::DeepReasoning,
                ModelCapability::StructuredJSON,
                ModelCapability::ToolPlanning,
            ],
            track: "green".to_string(),
            risk_score: 0.3,
            max_latency_ms: Some(10_000),
            max_cost_usd: None,
            privacy_boundary: PrivacyLevel::PublicApi,
            preferred_model_id: Some("deepseek-v4-flash".to_string()),
        };
        let routed = ModelRouter::route(&request, &profiles, None).expect("route should resolve");
        assert_ne!(routed.selected_model_id, "unavailable");
        assert_eq!(routed.selected_model_id, "deepseek-v4-flash");
    }

    #[test]
    fn deepseek_profiles_keep_non_zero_pricing_for_live_cost_rollups() {
        let config = ModelProviderConfig {
            provider_id: "desktop".to_string(),
            kind: ModelProviderKind::DeepSeek,
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            default_model: "deepseek-v4-flash".to_string(),
            fast_model: "deepseek-v4-flash".to_string(),
            reasoning_model: "deepseek-v4-flash".to_string(),
            structured_output_model: "deepseek-v4-flash".to_string(),
            max_tokens: 8192,
            temperature: 0.2,
            timeout_ms: 30000,
            max_cost_per_task_usd: 5.0,
        };

        let profiles = model_profiles_from_config(&config);
        assert_eq!(profiles.len(), 1);
        let profile = &profiles[0];
        assert!(
            profile.cost_per_1k_input_usd > 0.0,
            "live provider input pricing should not remain zero"
        );
        assert!(
            profile.cost_per_1k_output_usd > 0.0,
            "live provider output pricing should not remain zero"
        );
    }

    async fn configure_broken_openai_compatible(pool: &sqlx::SqlitePool) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("desktop-broken")
            .bind("OpenAICompatible")
            .bind("http://127.0.0.1:9/v1")
            .bind("sk-broken")
            .bind("sk-b****oken")
            .bind("deepseek-chat")
            .bind("deepseek-chat")
            .bind("deepseek-chat")
            .bind("deepseek-chat")
            .bind(8192)
            .bind(0.2)
            .bind(1000)
            .bind(5.0)
            .bind(1)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
    }

    #[test]
    fn selected_executor_selection_is_closed_over_missing_and_disabled_entries() {
        let mut work_order = test_work_order("wo-selected-executors", "green");
        work_order.selected_executors = vec!["executor-beta".to_string()];

        let selected = select_selected_executor_passports(
            &work_order,
            vec![
                test_executor("executor-alpha", ExecutorStatus::Registered),
                test_executor("executor-beta", ExecutorStatus::Registered),
            ],
        )
        .expect("selected executor should be retained");

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].executor_id, "executor-beta");

        let missing = select_selected_executor_passports(
            &work_order,
            vec![test_executor("executor-alpha", ExecutorStatus::Registered)],
        )
        .expect_err("missing selected executor must fail closed");
        assert!(
            matches!(missing, WorkerError::ToolUnavailable(message) if message.contains("executor-beta"))
        );

        let disabled = select_selected_executor_passports(
            &work_order,
            vec![test_executor("executor-beta", ExecutorStatus::Disabled)],
        )
        .expect_err("disabled selected executor must fail closed");
        assert!(
            matches!(disabled, WorkerError::ToolUnavailable(message) if message.contains("disabled"))
        );
    }

    #[tokio::test]
    async fn green_work_order_creates_run_and_key_steps() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        AgentEmployeeRepo::seed(&pool).await.unwrap();
        SkillRepo::seed_default(&pool).await.unwrap();
        let wo = test_work_order("wo-green-key-steps", "green");
        WorkOrderRepo::create(&pool, &wo).await.unwrap();

        let result = WorkerHarness::run_work_order(
            &pool,
            &wo.work_order_id,
            WorkerHarnessOptions {
                approval_receipt: None,
                max_runtime_ms: None,
                deterministic_mode: true,
                preferred_tool_ids: vec![],
                allow_mock_model_routing: false,
            },
        )
        .await
        .unwrap();

        assert!(matches!(result.status.as_str(), "Completed" | "Failed"));
        let runs = sqlx::query("SELECT COUNT(*) as count FROM worker_runs WHERE work_order_id=?")
            .bind(&wo.work_order_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<i64, _>("count");
        assert_eq!(runs, 1);
        let steps = sqlx::query("SELECT step_type FROM worker_steps WHERE run_id IN (SELECT run_id FROM worker_runs WHERE work_order_id=?)")
            .bind(&wo.work_order_id)
            .fetch_all(&pool)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<String, _>("step_type"))
            .collect::<Vec<_>>();
        assert!(steps.iter().any(|s| s == "BuildContext"));
        assert!(steps.iter().any(|s| s == "LoadMemory"));
        assert!(steps.iter().any(|s| s == "SelectTool"));
        assert!(steps.iter().any(|s| s == "WriteMemory"));
    }

    #[tokio::test]
    async fn agent_sub_harness_uses_authorization_actions_to_block_read_tool() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        AgentEmployeeRepo::seed(&pool).await.unwrap();
        SkillRepo::seed_default(&pool).await.unwrap();
        let mut wo = test_work_order("wo-green-restricted-read", "green");
        wo.mission_intent = "Analyze README.md".to_string();
        wo.allowed_actions = vec!["read".to_string()];
        wo.restricted_actions = vec!["read".to_string(), "ReadFile".to_string()];
        WorkOrderRepo::create(&pool, &wo).await.unwrap();

        let _ = WorkerHarness::run_work_order(
            &pool,
            &wo.work_order_id,
            WorkerHarnessOptions {
                approval_receipt: None,
                max_runtime_ms: None,
                deterministic_mode: true,
                preferred_tool_ids: vec![],
                allow_mock_model_routing: false,
            },
        )
        .await
        .unwrap();

        let file_tool_calls = sqlx::query("SELECT COUNT(*) as count FROM worker_tool_calls WHERE tool_id='file-readonly' AND run_id IN (SELECT run_id FROM worker_runs WHERE work_order_id=?)")
            .bind(&wo.work_order_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<i64, _>("count");
        assert_eq!(file_tool_calls, 0);
    }

    #[tokio::test]
    async fn failed_sub_harness_marks_run_session_failed_and_releases_queue() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_broken_openai_compatible(&pool).await;
        AgentEmployeeRepo::seed(&pool).await.unwrap();
        SkillRepo::seed_default(&pool).await.unwrap();
        let wo = test_work_order("wo-green-failure-cleanup", "green");
        WorkOrderRepo::create(&pool, &wo).await.unwrap();

        let err = WorkerHarness::run_work_order(
            &pool,
            &wo.work_order_id,
            WorkerHarnessOptions {
                approval_receipt: None,
                max_runtime_ms: None,
                deterministic_mode: true,
                preferred_tool_ids: vec![],
                allow_mock_model_routing: false,
            },
        )
        .await
        .expect_err("broken provider should fail");
        assert!(!err.to_string().is_empty());

        let run_status =
            sqlx::query("SELECT status FROM worker_runs WHERE work_order_id=? LIMIT 1")
                .bind(&wo.work_order_id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .get::<String, _>("status");
        assert_eq!(run_status, "Failed");

        let session_status = sqlx::query("SELECT status FROM worker_sessions WHERE work_order_id=? ORDER BY created_at_ms DESC LIMIT 1")
            .bind(&wo.work_order_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<String, _>("status");
        assert_eq!(session_status, "Failed");

        let lifecycle_error_count = sqlx::query("SELECT COUNT(*) as count FROM worker_events WHERE run_id IN (SELECT run_id FROM worker_runs WHERE work_order_id=?) AND event_type='LifecycleError'")
            .bind(&wo.work_order_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<i64, _>("count");
        assert!(lifecycle_error_count >= 1);

        let lane =
            sqlx::query("SELECT status, active_run_id FROM worker_queue_lanes WHERE session_id=?")
                .bind(format!("session-{}", &wo.work_order_id))
                .fetch_one(&pool)
                .await
                .unwrap();
        let lane_status = lane.get::<String, _>("status");
        let active_run: Option<String> = lane.get("active_run_id");
        assert_eq!(lane_status, "Idle");
        assert!(active_run.is_none());
    }
}
