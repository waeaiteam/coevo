use sqlx::SqlitePool;
use crate::error::WorkerError;
use crate::queue::WorkerQueueService;
use crate::tool_policy::ToolPolicyEngine;
use crate::tool_registry::ToolRegistry;
use crate::skill_runtime::SkillRuntime;
use crate::memory_context::MemoryContextBuilder;
use crate::reflection::ReflectionEngine;
use crate::self_upgrade::SelfUpgradeLoop;
use coevo_store::repos::agent_worker_repo::AgentWorkerRepo;
use coevo_store::repos::worker_run_repo::{WorkerRunRepo, WorkerStepRepo, WorkerEventRepo, WorkerSkillUsageRepo, WorkerToolCallRepo};
use coevo_store::repos_opc::{work_order_repo, memory_repo};
use sqlx::{Row, Column};
use crate::types::WorkerRun;
use coevo_models::router::{ModelRouter, default_model_profiles, required_capabilities_for_step, ModelRoutingRequest, PrivacyLevel};
use coevo_core::opc::*;
use coevo_core::cognitive::CognitiveLayer;

pub struct WorkerHarnessOptions { pub approval_receipt: Option<String>, pub max_runtime_ms: Option<i64>, pub deterministic_mode: bool, pub preferred_tool_ids: Vec<String> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkerHarnessResult { pub work_order_id: String, pub worker_runs: Vec<serde_json::Value>, pub worker_steps: Vec<serde_json::Value>, pub worker_events: Vec<serde_json::Value>, pub skill_usage: Vec<serde_json::Value>, pub tool_calls: Vec<serde_json::Value>, pub memory_ids: Vec<String>, pub reflection_id: Option<String>, pub proposal_id: Option<String>, pub status: String, pub summary: String }

fn find_github_url(text: &str) -> Option<String> {
    for prefix in &["https://github.com/","http://github.com/","github.com/"] {
        if let Some(idx) = text.find(prefix) {
            let rest = &text[idx+prefix.len()..];
            let end = rest.find(|c:char| c.is_whitespace()||c==')'||c==']').unwrap_or(rest.len());
            let path = rest[..end].trim_end_matches('/');
            if path.split('/').count() >= 2 { return Some(format!("https://github.com/{}",path)); }
        }
    }
    None
}

async fn step_create(pool: &SqlitePool, steps: &mut Vec<serde_json::Value>, run_id: &str, step_type: &str, input: &serde_json::Value, output: Option<&serde_json::Value>) -> Result<String, WorkerError> {
    let idx = steps.len() as i64;
    let sid = format!("s-{}-{}", &run_id[..8.min(run_id.len())], idx);
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO worker_steps VALUES (?,?,?,?,?,?,?,?,?,?)")
        .bind(&sid).bind(run_id).bind(idx).bind(step_type).bind(serde_json::to_string(input).unwrap())
        .bind(output.map(|o| serde_json::to_string(o).unwrap())).bind("Completed").bind(now).bind(Some(now)).bind(Option::<String>::None)
        .execute(pool).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
    steps.push(serde_json::json!({"step_id":sid,"run_id":run_id,"step_index":idx,"step_type":step_type,"status":"Completed"}));
    Ok(sid)
}

pub struct WorkerHarness;
impl WorkerHarness {
    pub async fn run_work_order(pool: &SqlitePool, work_order_id: &str, options: WorkerHarnessOptions) -> Result<WorkerHarnessResult, WorkerError> {
        let now = || chrono::Utc::now().timestamp_millis();
        let mut steps: Vec<serde_json::Value> = vec![];
        let mut mem_ids: Vec<String> = vec![];

        let wo = work_order_repo::WorkOrderRepo::get(pool, work_order_id).await.map_err(|e| WorkerError::Internal(e.to_string()))?.ok_or(WorkerError::WorkOrderNotFound(work_order_id.into()))?;
        let agent_id = wo.selected_agents.first().cloned().unwrap_or_default();
        if agent_id.is_empty() { return Err(WorkerError::WorkerNotFound("No agent selected".into())); }

        let worker_id = format!("worker-{}", agent_id);
        match AgentWorkerRepo::get(pool, &worker_id).await.map_err(|e| WorkerError::Internal(e.to_string()))? {
            Some(_) => { AgentWorkerRepo::set_status(pool, &worker_id, "Assigned").await.map_err(|e| WorkerError::Internal(e.to_string()))?; }
            None => { AgentWorkerRepo::upsert(pool, &worker_id, &agent_id, "Default", "Assigned", Some(work_order_id), None, "[]", "Task", "[]", now(), now()).await.map_err(|e| WorkerError::Internal(e.to_string()))?; }
        }

        // Stable session_id
        let session_id = format!("session-{}", work_order_id);
        sqlx::query("INSERT OR IGNORE INTO worker_sessions VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&session_id).bind(&worker_id).bind(work_order_id).bind(&agent_id).bind("MissionChat")
            .bind("[]").bind("[]").bind("[]").bind("[]").bind("Running").bind(now()).bind(now())
            .execute(pool).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        sqlx::query("UPDATE worker_sessions SET status='Running',updated_at_ms=? WHERE session_id=?").bind(now()).bind(&session_id).execute(pool).await.map_err(|e| WorkerError::Internal(e.to_string()))?;

        let run_id = format!("run-{}", uuid::Uuid::new_v4());
        WorkerRunRepo::create(pool, &run_id, work_order_id, &agent_id, &worker_id, &session_id, "Running", "{}", "[]", "[]", None, now(), None).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        // Acquire queue with run_id and update AgentWorker
        WorkerQueueService::acquire(pool, &session_id, &run_id, 120_000).await?;
        AgentWorkerRepo::upsert(pool, &worker_id, &agent_id, "Default", "Executing", Some(work_order_id), Some(&session_id), "[]", "Task", "[]", now(), now()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerEventRepo::append(pool, &run_id, "LifecycleStart", &serde_json::to_string(&serde_json::json!({"status":"Running"})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;

        // MemoryContext with real data
        let mem_ctx = MemoryContextBuilder::build(pool, &agent_id, &wo).await?;
        step_create(pool, &mut steps, &run_id, "BuildContext", &serde_json::json!({"intent":wo.mission_intent}), None).await?;
        step_create(pool, &mut steps, &run_id, "LoadMemory", &serde_json::json!({
            "user_profile_loaded":mem_ctx.user_profile.is_some(),"company_profile_loaded":!mem_ctx.company_profile.is_empty(),
            "company_memory_count":mem_ctx.company_memory.len(),"agent_memory_count":mem_ctx.agent_memory.len(),
            "task_memory_count":mem_ctx.task_memory.len(),"stale_memory_ids":mem_ctx.stale_memory_ids.len(),
            "excluded_revoked_count":mem_ctx.excluded_revoked_count,
            "excluded_fact_without_provenance":mem_ctx.fact_without_provenance
        }), None).await?;

        // SkillRuntime
        let index = SkillRuntime::load_skill_index(pool, &agent_id).await?;
        let selected = SkillRuntime::select_relevant(&wo.mission_intent, &wo.required_skills, &index);
        step_create(pool, &mut steps, &run_id, "LoadSkillIndex", &serde_json::json!({"skills_found":index.len(),"selected":selected}), None).await?;
        for sid in &selected {
            if let Some(_full) = SkillRuntime::load_full(pool, sid).await? {
                step_create(pool, &mut steps, &run_id, "LoadSkillFull", &serde_json::json!({"loaded_skill":sid}), None).await?;
                WorkerSkillUsageRepo::create(pool, &format!("su-{}",uuid::Uuid::new_v4()), &run_id, sid, "1.0.0", "execution", true, 0.9, "", now()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            }
        }

        // Track check
        if wo.track == "red" {
            return Self::finish(pool, work_order_id, &session_id, &worker_id, &run_id, &agent_id, &wo, steps, mem_ids, "Blocked", "Red Track blocked by default.").await;
        }
        if wo.track == "yellow" && options.approval_receipt.is_none() {
            WorkerEventRepo::append(pool, &run_id, "ApprovalRequired", &serde_json::to_string(&serde_json::json!({"reason":"Yellow requires approval"})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            WorkerRunRepo::set_status(pool, &run_id, "WaitingApproval").await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            sqlx::query("UPDATE worker_sessions SET status='WaitingApproval',updated_at_ms=? WHERE session_id=?").bind(now()).bind(&session_id).execute(pool).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            WorkerQueueService::release(pool, &session_id, &run_id).await?;
            return Self::build_result(pool, work_order_id, &run_id, steps, vec![], None, None, "WaitingApproval", "Yellow Track: WaitingApproval.".into()).await;
        }

        // ModelRouter: record routing decision for Think step (cognition only, not authorization)
        let route_req = ModelRoutingRequest{
            work_order_id: work_order_id.into(), agent_id: agent_id.clone(),
            worker_step_type: "Think".into(), intent: wo.mission_intent.clone(),
            required_capabilities: required_capabilities_for_step("Think", &wo.mission_intent),
            track: wo.track.clone(), risk_score: if wo.track=="red"{0.9}else if wo.track=="yellow"{0.6}else{0.3},
            max_latency_ms: options.max_runtime_ms.map(|m| m as u64),
            max_cost_usd: None, privacy_boundary: PrivacyLevel::PublicApi,
            preferred_model_id: None,
        };
        let route_decision = ModelRouter::route(&route_req, &default_model_profiles(), None).unwrap_or_else(|_|
            coevo_models::router::ModelRoutingDecision{
                selected_provider_id:"mock".into(),selected_model_id:"mock-fast".into(),
                selected_capabilities:vec![],reason:"NoModelAvailable — using mock".into(),
                fallback_model_ids:vec![],estimated_cost_usd:None,estimated_latency_ms:None,
                governance_notes:vec!["ModelRouter failed, using mock fallback".into()],
                decision_id:format!("mrd-{}",uuid::Uuid::new_v4()),created_at_ms:now(),
            });
        step_create(pool, &mut steps, &run_id, "ModelCall",
            &serde_json::json!({"intent":wo.mission_intent}),
            Some(&serde_json::to_value(&route_decision).unwrap())).await?;

        // ToolPolicy + Tool execution
        let registry = ToolRegistry::default_registry();
        let allowed = ToolPolicyEngine::filter(registry.list(), &wo.track, &wo.allowed_actions, &wo.restricted_actions);
        step_create(pool, &mut steps, &run_id, "SelectTool", &serde_json::json!({"allowed_tools":allowed.len()}), None).await?;

        let lower = wo.mission_intent.to_lowercase();
        let gh_url = find_github_url(&lower);
        let tool_id = if gh_url.is_some() && allowed.iter().any(|t| t.tool_id == "github-readonly") { "github-readonly" }
        else if allowed.iter().any(|t| t.tool_id == "file-readonly") { "file-readonly" }
        else { "" };

        let mut tool_failed = false;
        if !tool_id.is_empty() {
            WorkerEventRepo::append(pool, &run_id, "ToolStart", &serde_json::to_string(&serde_json::json!({"tool_id":tool_id})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            step_create(pool, &mut steps, &run_id, "CallTool", &serde_json::json!({"tool_id":tool_id}), None).await?;

            let input = if tool_id == "github-readonly" {
                if let Some(url) = &gh_url { serde_json::json!({"repo_url":url,"action":"ReadReadme","max_bytes":5000}) }
                else { serde_json::json!({"error":"No valid GitHub URL found"}) }
            } else { serde_json::json!({"action":"ReadFile","path":"README.md"}) };

            let tool_result = registry.execute(tool_id, input).await.unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
            let success = tool_result.get("error").is_none();
            tool_failed = !success;
            let output_str = serde_json::to_string(&tool_result).unwrap_or_default();
            WorkerToolCallRepo::create(pool, &format!("tc-{}",uuid::Uuid::new_v4()), &run_id, tool_id, tool_id, &format!("{} execution",tool_id), &output_str.chars().take(500).collect::<String>(), success, 0.5, None, now(), Some(now())).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            WorkerEventRepo::append(pool, &run_id, "ToolEnd", &serde_json::to_string(&serde_json::json!({"tool_id":tool_id,"success":success})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        }

        // Task Memory
        let mem_id = format!("tm-{}", uuid::Uuid::new_v4());
        let mem = MemoryRecord{memory_id:mem_id.clone(),scope:MemoryScope::Task,owner_id:wo.work_order_id.clone(),title:format!("WorkerRun {}", &run_id),content:format!("Harness: {}", wo.mission_intent),tags:vec![],source:"worker-harness".into(),provenance:format!("worker-run-{}",run_id),confidence:0.9,ttl_seconds:86400,created_at_ms:now() as u64,updated_at_ms:now() as u64,access_policy:String::new(),status:MemoryStatus::Active,cognitive_layer:CognitiveLayer::Hypothesis,linked_contract_hash:Some(wo.contract_hash.clone()),linked_plan_hash:Some(wo.plan_hash.clone()),linked_adr_id:None};
        memory_repo::MemoryRepo::create(pool, &mem).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        mem_ids.push(mem_id.clone());
        step_create(pool, &mut steps, &run_id, "WriteMemory", &serde_json::json!({"memory_id":mem_id}), None).await?;
        WorkerEventRepo::append(pool, &run_id, "MemoryWrite", &serde_json::to_string(&serde_json::json!({"memory_id":mem_id})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;

        // Reflect ModelRouter decision
        let reflect_route = ModelRouter::route(&ModelRoutingRequest{
            work_order_id: work_order_id.into(), agent_id: agent_id.clone(),
            worker_step_type: "Reflect".into(), intent: wo.mission_intent.clone(),
            required_capabilities: required_capabilities_for_step("Reflect", &wo.mission_intent),
            track: wo.track.clone(), risk_score: if wo.track=="red"{0.9}else if wo.track=="yellow"{0.6}else{0.3},
            max_latency_ms: None, max_cost_usd: None, privacy_boundary: PrivacyLevel::PublicApi, preferred_model_id: None,
        }, &default_model_profiles(), None);
        if let Ok(ref d) = reflect_route {
            step_create(pool, &mut steps, &run_id, "ModelCall",
                &serde_json::json!({"purpose":"Reflect"}),
                Some(&serde_json::to_value(d).unwrap())).await?;
        }

        // Reflection with real fields
        step_create(pool, &mut steps, &run_id, "Reflect", &serde_json::json!({"type":"post-execution"}), None).await?;
        let reflection = ReflectionEngine::reflect(pool, &run_id, work_order_id, &agent_id, &worker_id, &steps, &[], &[]).await?;
        let ref_id = Some(reflection.reflection_id.clone());

        // ProposeSkillUpdate ModelRouter decision
        let skill_route = ModelRouter::route(&ModelRoutingRequest{
            work_order_id: work_order_id.into(), agent_id: agent_id.clone(),
            worker_step_type: "ProposeSkillUpdate".into(), intent: wo.mission_intent.clone(),
            required_capabilities: vec![coevo_models::router::ModelCapability::SkillGeneration, coevo_models::router::ModelCapability::StructuredJSON],
            track: wo.track.clone(), risk_score: if wo.track=="red"{0.9}else if wo.track=="yellow"{0.6}else{0.3},
            max_latency_ms: None, max_cost_usd: None, privacy_boundary: PrivacyLevel::PublicApi, preferred_model_id: None,
        }, &default_model_profiles(), None);
        if let Ok(ref d) = skill_route {
            step_create(pool, &mut steps, &run_id, "ModelCall",
                &serde_json::json!({"purpose":"ProposeSkillUpdate"}),
                Some(&serde_json::to_value(d).unwrap())).await?;
        }

        // SelfUpgrade — generate proposal if tool failed or skill update needed
        let mut proposal_id = None;
        if tool_failed || reflection.needs_human_review || !reflection.skill_to_update_json.as_array().map(|a|a.is_empty()).unwrap_or(true) {
            let run = WorkerRun{run_id:run_id.clone(),work_order_id:work_order_id.into(),agent_id:agent_id.clone(),worker_id:worker_id.clone(),session_id:session_id.clone(),status:if tool_failed{crate::types::WorkerRunStatus::Failed}else{crate::types::WorkerRunStatus::Completed},result_json:serde_json::json!({}),memory_ids_json:serde_json::json!([]),errors_json:serde_json::json!([]),audit_ref:None,started_at_ms:now(),ended_at_ms:Some(now())};
            proposal_id = SelfUpgradeLoop::run(pool, &run, &reflection, None).await?;
        }

        let final_status = if tool_failed { "Failed" } else { "Completed" };
        WorkerRunRepo::set_status(pool, &run_id, final_status).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerEventRepo::append(pool, &run_id, "LifecycleEnd", &serde_json::to_string(&serde_json::json!({"status":final_status})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        sqlx::query("UPDATE worker_sessions SET status=?,updated_at_ms=? WHERE session_id=?").bind(final_status).bind(now()).bind(&session_id).execute(pool).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerQueueService::release(pool, &session_id, &run_id).await?;

        Self::build_result(pool, work_order_id, &run_id, steps, mem_ids, ref_id, proposal_id, final_status, format!("WorkerHarness {} execution.", final_status)).await
    }

    async fn finish(pool: &SqlitePool, wo_id: &str, session_id: &str, worker_id: &str, run_id: &str, agent_id: &str, wo: &coevo_core::opc::WorkOrder, steps: Vec<serde_json::Value>, mem_ids: Vec<String>, status: &str, summary: &str) -> Result<WorkerHarnessResult, WorkerError> {
        let now = chrono::Utc::now().timestamp_millis();
        WorkerEventRepo::append(pool, run_id, "WorkerBlocked", &serde_json::to_string(&serde_json::json!({"reason":"Red Track blocked"})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerRunRepo::set_status(pool, run_id, status).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        sqlx::query("UPDATE worker_sessions SET status=?,updated_at_ms=? WHERE session_id=?").bind(status).bind(now).bind(session_id).execute(pool).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerQueueService::release(pool, session_id, run_id).await?;

        let run = WorkerRun{run_id:run_id.into(),work_order_id:wo_id.into(),agent_id:agent_id.into(),worker_id:worker_id.into(),session_id:session_id.into(),status:crate::types::WorkerRunStatus::Blocked,result_json:serde_json::json!({}),memory_ids_json:serde_json::json!([]),errors_json:serde_json::json!([]),audit_ref:None,started_at_ms:now,ended_at_ms:Some(now)};
        let reflection = ReflectionEngine::reflect(pool, run_id, wo_id, agent_id, worker_id, &steps, &[], &[]).await?;
        let proposal_id = SelfUpgradeLoop::run(pool, &run, &reflection, None).await?;
        Self::build_result(pool, wo_id, run_id, steps, mem_ids, Some(reflection.reflection_id), proposal_id, status, summary.into()).await
    }

    async fn build_result(pool: &SqlitePool, wo_id: &str, run_id: &str, _steps: Vec<serde_json::Value>, mem_ids: Vec<String>, reflection_id: Option<String>, proposal_id: Option<String>, status: &str, summary: String) -> Result<WorkerHarnessResult, WorkerError> {
        let w_steps = WorkerStepRepo::list_by_run(pool, run_id).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        let w_events = WorkerEventRepo::list_by_run(pool, run_id).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        let w_skills = sqlx::query("SELECT * FROM worker_skill_usage WHERE run_id=?").bind(run_id).fetch_all(pool).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        let w_tools = sqlx::query("SELECT * FROM worker_tool_calls WHERE run_id=?").bind(run_id).fetch_all(pool).await.map_err(|e| WorkerError::Internal(e.to_string()))?;

        fn to_json(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<serde_json::Value> {
            rows.iter().map(|r| {
                let mut m = serde_json::Map::new();
                for (i, col) in r.columns().iter().enumerate() {
                    let name = col.name().to_string();
                    if let Ok(v) = r.try_get::<String, _>(i) { m.insert(name, serde_json::Value::String(v)); continue; }
                    if let Ok(v) = r.try_get::<i64, _>(i) { m.insert(name, serde_json::Value::Number(v.into())); continue; }
                    if let Ok(v) = r.try_get::<f64, _>(i) { m.insert(name, serde_json::json!(v)); continue; }
                }
                serde_json::Value::Object(m)
            }).collect()
        }

        Ok(WorkerHarnessResult{work_order_id:wo_id.into(),worker_runs:vec![serde_json::json!({"run_id":run_id,"status":status})],worker_steps:to_json(w_steps),worker_events:to_json(w_events),skill_usage:to_json(w_skills),tool_calls:to_json(w_tools),memory_ids:mem_ids,reflection_id,proposal_id,status:status.into(),summary})
    }
}
