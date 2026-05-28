use sqlx::SqlitePool;
use crate::types::*;
use crate::error::WorkerError;
use crate::queue::WorkerQueueService;
use crate::tool_policy::ToolPolicyEngine;
use crate::tool_registry::ToolRegistry;
use crate::skill_runtime::SkillRuntime;
use coevo_store::repos::agent_worker_repo::AgentWorkerRepo;
use coevo_store::repos::worker_run_repo::{WorkerRunRepo, WorkerStepRepo, WorkerEventRepo, WorkerSkillUsageRepo, WorkerToolCallRepo, WorkerReflectionRepo, WorkerQueueRepo};
use coevo_store::repos_opc::{work_order_repo, memory_repo};
use sqlx::Row;
use coevo_core::opc::*;
use coevo_core::cognitive::CognitiveLayer;

pub struct WorkerHarnessOptions { pub approval_receipt: Option<String>, pub max_runtime_ms: Option<i64>, pub deterministic_mode: bool, pub preferred_tool_ids: Vec<String> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkerHarnessResult { pub work_order_id: String, pub worker_runs: Vec<serde_json::Value>, pub worker_steps: Vec<serde_json::Value>, pub worker_events: Vec<serde_json::Value>, pub skill_usage: Vec<serde_json::Value>, pub tool_calls: Vec<serde_json::Value>, pub memory_ids: Vec<String>, pub reflection_id: Option<String>, pub proposal_id: Option<String>, pub status: String, pub summary: String }

pub struct WorkerHarness;
impl WorkerHarness {
    pub async fn run_work_order(pool: &SqlitePool, work_order_id: &str, options: WorkerHarnessOptions) -> Result<WorkerHarnessResult, WorkerError> {
        let now = || chrono::Utc::now().timestamp_millis();

        // 1. Load WorkOrder
        let wo = work_order_repo::WorkOrderRepo::get(pool, work_order_id).await.map_err(|e| WorkerError::Internal(e.to_string()))?.ok_or(WorkerError::WorkOrderNotFound(work_order_id.into()))?;
        let agent_id = wo.selected_agents.first().cloned().unwrap_or_default();
        if agent_id.is_empty() { return Err(WorkerError::WorkerNotFound("No agent selected".into())); }

        // 2. Get or create AgentWorker
        let worker_id = format!("worker-{}", agent_id);
        match AgentWorkerRepo::get(pool, &worker_id).await.map_err(|e| WorkerError::Internal(e.to_string()))? {
            Some(_row) => { AgentWorkerRepo::set_status(pool, &worker_id, "Assigned").await.map_err(|e| WorkerError::Internal(e.to_string()))?; }
            None => { AgentWorkerRepo::upsert(pool, &worker_id, &agent_id, "Default", "Assigned", Some(&wo.work_order_id), None, "[]", "Task", "[]", now(), now()).await.map_err(|e| WorkerError::Internal(e.to_string()))?; }
        };

        // 3. Session + Queue
        let session_id = format!("session-{}-{}", &wo.work_order_id, uuid::Uuid::new_v4().to_string().chars().take(8).collect::<String>());
        WorkerQueueService::acquire(pool, &session_id, &worker_id, 120_000).await?;

        // 4. WorkerRun
        let run_id = format!("run-{}", uuid::Uuid::new_v4());
        WorkerRunRepo::create(pool, &run_id, &wo.work_order_id, &agent_id, &worker_id, &session_id, "Running", "{}", "[]", "[]", None, now(), None).await.map_err(|e| WorkerError::Internal(e.to_string()))?;

        // 5. Lifecycle start
        WorkerEventRepo::append(pool, &run_id, "LifecycleStart", &serde_json::to_string(&serde_json::json!({"status":"Running"})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;

        // Helper: create step
        let mut steps_created: Vec<serde_json::Value> = vec![];
        let add_step = |steps: &mut Vec<serde_json::Value>, pool: &SqlitePool, run_id: &str, step_type: &str, input: serde_json::Value| -> String {
            let idx = steps.len() as i64;
            let sid = format!("step-{}-{}", run_id, idx);
            let _ = sqlx::query("INSERT INTO worker_steps VALUES (?,?,?,?,?,?,?,?,?,?)")
                .bind(&sid).bind(run_id).bind(idx).bind(step_type).bind(&serde_json::to_string(&input).unwrap()).bind(Option::<String>::None).bind("Completed").bind(now()).bind(Option::<i64>::None).bind(Option::<String>::None).execute(pool);
            steps.push(serde_json::json!({"step_id":sid,"run_id":run_id,"step_index":idx,"step_type":step_type}));
            sid
        };

        // 7. Steps: BuildContext, LoadMemory, LoadSkillIndex
        add_step(&mut steps_created, pool, &run_id, "BuildContext", serde_json::json!({"intent":wo.mission_intent}));
        add_step(&mut steps_created, pool, &run_id, "LoadMemory", serde_json::json!({"agent_id":agent_id}));

        let index = SkillRuntime::load_skill_index(pool, &agent_id).await?;
        let selected = SkillRuntime::select_relevant(&wo.mission_intent, &wo.required_skills, &index);
        add_step(&mut steps_created, pool, &run_id, "LoadSkillIndex", serde_json::json!({"skills_found":index.len(),"selected":selected}));

        for sid in &selected {
            if let Some(_full) = SkillRuntime::load_full(pool, sid).await? {
                add_step(&mut steps_created, pool, &run_id, "LoadSkillFull", serde_json::json!({"loaded_skill":sid}));
                WorkerSkillUsageRepo::create(pool, &format!("su-{}",uuid::Uuid::new_v4()), &run_id, sid, "1.0.0", "execution", true, 0.9, "", now()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            }
        }

        // 8. Track check
        if wo.track == "red" {
            WorkerEventRepo::append(pool, &run_id, "WorkerBlocked", &serde_json::to_string(&serde_json::json!({"reason":"Red Track blocked"})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            WorkerRunRepo::set_status(pool, &run_id, "Blocked").await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            WorkerQueueService::release(pool, &session_id, &worker_id).await?;
            return Ok(WorkerHarnessResult{work_order_id:wo.work_order_id,worker_runs:vec![],worker_steps:steps_created,worker_events:vec![],skill_usage:vec![],tool_calls:vec![],memory_ids:vec![],reflection_id:None,proposal_id:None,status:"Blocked".into(),summary:"Red Track blocked by default.".into()});
        }
        if wo.track == "yellow" && options.approval_receipt.is_none() {
            WorkerEventRepo::append(pool, &run_id, "ApprovalRequired", &serde_json::to_string(&serde_json::json!({"reason":"Yellow requires approval"})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            WorkerRunRepo::set_status(pool, &run_id, "WaitingApproval").await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            WorkerQueueService::release(pool, &session_id, &worker_id).await?;
            return Ok(WorkerHarnessResult{work_order_id:wo.work_order_id,worker_runs:vec![],worker_steps:steps_created,worker_events:vec![],skill_usage:vec![],tool_calls:vec![],memory_ids:vec![],reflection_id:None,proposal_id:None,status:"WaitingApproval".into(),summary:"Yellow Track: WaitingApproval.".into()});
        }

        // 9. ToolPolicy + Tool execution
        let registry = ToolRegistry::default_registry();
        let filtered = ToolPolicyEngine::filter(registry.list(), &wo.track, &wo.allowed_actions, &wo.restricted_actions);
        add_step(&mut steps_created, pool, &run_id, "SelectTool", serde_json::json!({"filtered_tools":filtered.len()}));

        let lower = wo.mission_intent.to_lowercase();
        let tool_id = if lower.contains("github.com") || lower.contains("github") { "github-readonly" }
        else if lower.contains("read") || lower.contains("file") || lower.contains("local") { "file-readonly" }
        else { "" };

        let mut mem_ids: Vec<String> = vec![];
        if !tool_id.is_empty() {
            WorkerEventRepo::append(pool, &run_id, "ToolStart", &serde_json::to_string(&serde_json::json!({"tool_id":tool_id})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            add_step(&mut steps_created, pool, &run_id, "CallTool", serde_json::json!({"tool_id":tool_id}));

            let input = if tool_id == "github-readonly" { serde_json::json!({"repo_url":"https://github.com/waeaiteam/coevo","action":"ReadReadme","max_bytes":5000}) }
            else { serde_json::json!({"action":"ReadFile","path":"README.md"}) };
            let tool_result = registry.execute(tool_id, input).await.unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
            let output_str = serde_json::to_string(&tool_result).unwrap_or_default();

            WorkerToolCallRepo::create(pool, &format!("tc-{}",uuid::Uuid::new_v4()), &run_id, tool_id, tool_id, &format!("{} execution",tool_id), &output_str.chars().take(500).collect::<String>(), tool_result.get("error").is_none(), 0.5, None, now(), Some(now())).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
            WorkerEventRepo::append(pool, &run_id, "ToolEnd", &serde_json::to_string(&serde_json::json!({"tool_id":tool_id})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        }

        // 10. Write Task Memory
        let mem_id = format!("task-mem-{}", uuid::Uuid::new_v4());
        let mem = MemoryRecord{memory_id:mem_id.clone(),scope:MemoryScope::Task,owner_id:wo.work_order_id.clone(),title:format!("WorkerRun {}", &run_id),content:format!("Harness: {}", wo.mission_intent),tags:vec![],source:"worker-harness".into(),provenance:format!("worker-run-{}",run_id),confidence:0.9,ttl_seconds:86400,created_at_ms:now() as u64,updated_at_ms:now() as u64,access_policy:String::new(),status:MemoryStatus::Active,cognitive_layer:CognitiveLayer::Hypothesis,linked_contract_hash:Some(wo.contract_hash.clone()),linked_plan_hash:Some(wo.plan_hash.clone()),linked_adr_id:None};
        memory_repo::MemoryRepo::create(pool, &mem).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        mem_ids.push(mem_id.clone());
        add_step(&mut steps_created, pool, &run_id, "WriteMemory", serde_json::json!({"memory_id":mem_id}));
        WorkerEventRepo::append(pool, &run_id, "MemoryWrite", &serde_json::to_string(&serde_json::json!({"memory_id":mem_id})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;

        // 11. Reflection
        add_step(&mut steps_created, pool, &run_id, "Reflect", serde_json::json!({"type":"post-execution"}));
        let ref_id = format!("ref-{}", uuid::Uuid::new_v4());
        WorkerReflectionRepo::create(pool, &ref_id, &wo.work_order_id, &run_id, &agent_id, &worker_id, "[]", "[]", "[]", "[]", "[]", false, now()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;

        // 12. Complete
        WorkerRunRepo::set_status(pool, &run_id, "Completed").await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerEventRepo::append(pool, &run_id, "LifecycleEnd", &serde_json::to_string(&serde_json::json!({"status":"Completed"})).unwrap()).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        WorkerQueueService::release(pool, &session_id, &worker_id).await?;

        // Load events for response
        let events = WorkerEventRepo::list_by_run(pool, &run_id).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        let ev_list: Vec<serde_json::Value> = events.iter().map(|r| serde_json::json!({"event_id":r.get::<String,_>("event_id"),"event_type":r.get::<String,_>("event_type"),"run_id":r.get::<String,_>("run_id")})).collect();

        Ok(WorkerHarnessResult{work_order_id:wo.work_order_id,worker_runs:vec![serde_json::json!({"run_id":run_id,"status":"Completed"})],worker_steps:steps_created,worker_events:ev_list,skill_usage:vec![],tool_calls:vec![],memory_ids:mem_ids,reflection_id:Some(ref_id),proposal_id:None,status:"Completed".into(),summary:"WorkerHarness execution completed.".into()})
    }
}
