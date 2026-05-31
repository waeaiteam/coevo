use sqlx::SqlitePool;
use crate::types::*;
use crate::error::WorkerError;
use coevo_store::repos::worker_run_repo::WorkerReflectionRepo;

pub struct ReflectionEngine;
impl ReflectionEngine {
    #[allow(clippy::too_many_arguments)]
    pub async fn reflect(pool: &SqlitePool, run_id: &str, wo_id: &str, agent_id: &str, worker_id: &str, steps: &[serde_json::Value], tool_calls: &[serde_json::Value], skill_usage: &[serde_json::Value]) -> Result<ReflectionRecord, WorkerError> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut what_worked: Vec<String> = vec![];
        let mut what_failed: Vec<String> = vec![];
        let mut memory_to_add: Vec<String> = vec![];
        let skill_to_update: Vec<String> = if skill_usage.is_empty() {
            vec![]
        } else {
            vec![format!("Observed {} skill usage records", skill_usage.len())]
        };

        for s in steps {
            let st = s["step_type"].as_str().unwrap_or("");
            if let Some(err) = s["error"].as_str() { what_failed.push(format!("Step {}: {}", st, err)); }
            else { what_worked.push(format!("Step {} completed", st)); }
        }
        for tc in tool_calls {
            if tc["success"].as_bool().unwrap_or(false) { what_worked.push(format!("Tool {} succeeded", tc["tool_id"].as_str().unwrap_or(""))); }
            else { what_failed.push(format!("Tool {} failed", tc["tool_id"].as_str().unwrap_or(""))); }
        }
        if !what_worked.is_empty() { memory_to_add.push("Successful worker harness execution pattern".into()); }
        if !what_failed.is_empty() { memory_to_add.push("Worker execution failure pattern".into()); }

        let reflection_id = format!("ref-{}", uuid::Uuid::new_v4());
        let record = ReflectionRecord{
            reflection_id: reflection_id.clone(),
            work_order_id: wo_id.into(), run_id: run_id.into(), agent_id: agent_id.into(), worker_id: worker_id.into(),
            what_worked_json: serde_json::to_value(&what_worked).unwrap_or_default(),
            what_failed_json: serde_json::to_value(&what_failed).unwrap_or_default(),
            memory_to_add_json: serde_json::to_value(&memory_to_add).unwrap_or_default(),
            skill_to_update_json: serde_json::to_value(&skill_to_update).unwrap_or_default(),
            user_preference_observed_json: serde_json::json!([]),
            needs_human_review: !what_failed.is_empty(),
            created_at_ms: now,
        };
        WorkerReflectionRepo::create(pool, &reflection_id, wo_id, run_id, agent_id, worker_id, &serde_json::to_string(&what_worked).unwrap(), &serde_json::to_string(&what_failed).unwrap(), &serde_json::to_string(&memory_to_add).unwrap(), &serde_json::to_string(&skill_to_update).unwrap(), "[]", !what_failed.is_empty(), now).await.map_err(|e| WorkerError::Internal(e.to_string()))?;
        Ok(record)
    }
}
