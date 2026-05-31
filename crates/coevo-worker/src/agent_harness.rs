use crate::error::WorkerError;
use crate::memory_context::MemoryContextBuilder;
use crate::reflection::ReflectionEngine;
use crate::self_upgrade::SelfUpgradeLoop;
use crate::skill_runtime::SkillRuntime;
use crate::tool_policy::ToolPolicyEngine;
use crate::tool_registry::ToolRegistry;
use crate::types::WorkerRun;
use coevo_core::cognitive::CognitiveLayer;
use coevo_core::opc::{MemoryRecord, MemoryScope, MemoryStatus};
use coevo_models::router::{
    required_capabilities_for_step, ModelProfile, ModelRouter, ModelRoutingDecision,
    ModelRoutingRequest, PrivacyLevel,
};
use coevo_store::repos::worker_run_repo::{WorkerEventRepo, WorkerSkillUsageRepo, WorkerToolCallRepo};
use coevo_store::repos_opc::memory_repo;
use sqlx::SqlitePool;
use std::path::PathBuf;

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
    pub async fn execute(
        pool: &SqlitePool,
        run_contract: &AgentRunContract,
        authorization: &RunAuthorization,
        model_profiles: &[ModelProfile],
        max_runtime_ms: Option<i64>,
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
            &serde_json::json!({"intent":run_contract.mission_intent}),
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

        let think_route = route_for_step(
            run_contract,
            authorization,
            "Think",
            required_capabilities_for_step("Think", &run_contract.mission_intent),
            model_profiles,
            max_runtime_ms.map(|m| m as u64),
        );
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "ModelCall",
            &serde_json::json!({"intent":run_contract.mission_intent}),
            Some(&serde_json::to_value(&think_route).unwrap()),
        )
        .await?;

        let registry = ToolRegistry::default_registry();
        let allowed = ToolPolicyEngine::filter(
            registry.list(),
            &authorization.track,
            &authorization.allowed_actions,
            &authorization.restricted_actions,
        );
        step_create(
            pool,
            &mut steps,
            &authorization.run_id,
            "SelectTool",
            &serde_json::json!({"allowed_tools":allowed.len()}),
            None,
        )
        .await?;

        let lower = run_contract.mission_intent.to_lowercase();
        let gh_url = find_github_url(&lower);
        let file_roots = workspace_roots();
        let file_target = find_readonly_file_target(&run_contract.mission_intent, &file_roots);
        let tool_id = if gh_url.is_some() && allowed.iter().any(|t| t.tool_id == "github-readonly")
        {
            "github-readonly"
        } else if file_target.is_some() && allowed.iter().any(|t| t.tool_id == "file-readonly") {
            "file-readonly"
        } else {
            ""
        };

        let mut tool_failed = false;
        let mut last_tool_summary = String::new();
        if !tool_id.is_empty() {
            WorkerEventRepo::append(
                pool,
                &authorization.run_id,
                "ToolStart",
                &serde_json::to_string(&serde_json::json!({"tool_id":tool_id})).unwrap(),
            )
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
            step_create(
                pool,
                &mut steps,
                &authorization.run_id,
                "CallTool",
                &serde_json::json!({"tool_id":tool_id}),
                None,
            )
            .await?;

            let input = if tool_id == "github-readonly" {
                if let Some(url) = &gh_url {
                    serde_json::json!({"repo_url":url,"action":"ReadReadme","max_bytes":5000})
                } else {
                    serde_json::json!({"error":"No valid GitHub URL found"})
                }
            } else {
                serde_json::json!({
                    "action":"ReadFile",
                    "path":file_target.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                    "allowed_paths":file_roots.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
                    "max_bytes":5000
                })
            };
            let tool_result = registry
                .execute(tool_id, input)
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
            let success = tool_result.get("error").is_none();
            tool_failed = !success;
            let output_str = serde_json::to_string(&tool_result).unwrap_or_default();
            last_tool_summary = output_str.chars().take(1000).collect::<String>();
            let tool_type = match tool_id {
                "github-readonly" => "GitHubReadonly",
                "file-readonly" => "FileReadonly",
                _ => tool_id,
            };
            WorkerToolCallRepo::create(
                pool,
                &format!("tc-{}", uuid::Uuid::new_v4()),
                &authorization.run_id,
                tool_id,
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
                &serde_json::to_string(&serde_json::json!({"tool_id":tool_id,"success":success}))
                    .unwrap(),
            )
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
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

        let final_status = if tool_failed { "Failed" } else { "Completed" }.to_string();
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

fn find_github_url(text: &str) -> Option<String> {
    for prefix in &["https://github.com/", "http://github.com/", "github.com/"] {
        if let Some(idx) = text.find(prefix) {
            let rest = &text[idx + prefix.len()..];
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ')' || c == ']')
                .unwrap_or(rest.len());
            let path = rest[..end].trim_end_matches('/');
            if path.split('/').count() >= 2 {
                return Some(format!("https://github.com/{}", path));
            }
        }
    }
    None
}

fn workspace_roots() -> Vec<PathBuf> {
    if let Ok(root) = std::env::var("COEVO_WORKSPACE_DIR") {
        return vec![PathBuf::from(root)];
    }
    if let Ok(home) = std::env::var("COEVO_HOME") {
        return vec![PathBuf::from(home).join("workspace")];
    }
    std::env::current_dir().map(|p| vec![p]).unwrap_or_default()
}

fn clean_path_token(token: &str) -> &str {
    token.trim_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '`' | ',' | ';' | ':' | ')' | '(' | '[' | ']' | '{' | '}' | '<' | '>'
            )
    })
}

fn looks_like_file_reference(token: &str) -> bool {
    token.contains('/')
        || token.contains('\\')
        || token.contains(".md")
        || token.contains(".txt")
        || token.contains(".json")
        || token.contains(".toml")
        || token.contains(".yaml")
        || token.contains(".yml")
        || token.contains(".rs")
        || token.contains(".ts")
        || token.contains(".tsx")
}

fn find_readonly_file_target(intent: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    for token in intent
        .split_whitespace()
        .map(clean_path_token)
        .filter(|t| looks_like_file_reference(t))
    {
        let candidate = PathBuf::from(token);
        if candidate.is_absolute() && candidate.is_file() {
            return Some(candidate);
        }
        for root in roots {
            let joined = root.join(token);
            if joined.is_file() {
                return Some(joined);
            }
        }
    }

    for fallback in [
        "README.md",
        "README.zh-CN.md",
        "mission-notes.md",
        "welcome.md",
    ] {
        for root in roots {
            let joined = root.join(fallback);
            if joined.is_file() {
                return Some(joined);
            }
        }
    }
    None
}
