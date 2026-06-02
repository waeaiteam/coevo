use crate::agent_harness::{AgentRunContract, AgentSubHarness, RunAuthorization};
use crate::error::WorkerError;
use crate::queue::WorkerQueueService;
use crate::r#loop::{SandboxProfile, SandboxTier};
use coevo_core::opc::{AutonomyCeiling, ModelPreference};
use coevo_models::gateway::select_gateway;
use coevo_models::router::{default_model_profiles, ModelCapability, ModelProfile, PrivacyLevel};
use coevo_models::types::{ModelProviderConfig, ModelProviderKind};
use coevo_store::repos::worker_run_repo::{WorkerEventRepo, WorkerRunRepo, WorkerStepRepo};
use coevo_store::repos::{agent_worker_repo::AgentWorkerRepo, model_config_repo::ModelConfigRepo};
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
    pub summary: String,
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
            cost_per_1k_input_usd: 0.0,
            cost_per_1k_output_usd: 0.0,
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
        let now = || chrono::Utc::now().timestamp_millis();
        let wo = work_order_repo::WorkOrderRepo::get(pool, work_order_id)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?
            .ok_or(WorkerError::WorkOrderNotFound(work_order_id.into()))?;
        let agent_id = wo.selected_agents.first().cloned().unwrap_or_default();
        if agent_id.is_empty() {
            return Err(WorkerError::WorkerNotFound("No agent selected".into()));
        }

        // Authoritative governance gate stays in Product Harness.
        if wo.track == "red" {
            return Err(WorkerError::RedTrackBlocked(
                "RED_TRACK_BLOCKED_UNTIL_PRODUCTION_VERIFIER: Alpha does not support Red Track execution."
                    .into(),
            ));
        }
        if wo.track == "yellow" && options.approval_receipt.is_none() {
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
        .bind(&session_id)
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

        WorkerQueueService::acquire(pool, &session_id, &run_id, 120_000).await?;
        AgentWorkerRepo::upsert(
            pool,
            &worker_id,
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
                SandboxProfile::from_track(&wo.track, std::env::current_dir().ok()).tier
            });
        let model_preference = wo
            .governance_proposal
            .as_ref()
            .map(|proposal| model_preference_to_role(proposal.model_preference).to_string());
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
                std::env::current_dir().ok(),
            ),
            model_preference,
        };
        let sub_result = AgentSubHarness::execute(
            pool,
            &run_contract,
            &authorization,
            &model_profiles,
            options.max_runtime_ms,
            gateway.as_ref(),
            &provider_config,
            &[],
        )
        .await;
        let sub_result = match sub_result {
            Ok(result) => result,
            Err(err) => {
                Self::finalize_run_failure(pool, &worker_id, &session_id, &run_id, &err).await;
                return Err(err);
            }
        };

        WorkerRunRepo::set_status(pool, &run_id, &sub_result.final_status)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
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
        summary: String,
    ) -> Result<WorkerHarnessResult, WorkerError> {
        let w_steps = WorkerStepRepo::list_by_run(pool, run_id)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let w_events = WorkerEventRepo::list_by_run(pool, run_id)
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
            worker_runs: vec![serde_json::json!({"run_id":run_id,"status":status})],
            worker_steps: to_json(w_steps),
            worker_events: to_json(w_events),
            skill_usage: to_json(w_skills),
            tool_calls: to_json(w_tools),
            memory_ids: mem_ids,
            reflection_id,
            proposal_id,
            status: status.into(),
            summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
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

    #[tokio::test]
    async fn red_track_blocks_before_model_provider_resolution() {
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
        .expect_err("red should be blocked before provider resolution");

        assert!(err
            .to_string()
            .contains("RED_TRACK_BLOCKED_UNTIL_PRODUCTION_VERIFIER"));
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
