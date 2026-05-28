use sqlx::SqlitePool;
use crate::types::*;
use crate::error::WorkerError;
use coevo_core::opc::WorkOrder;
use coevo_store::repos_opc::{memory_repo, agent_memory_repo};

pub struct MemoryContextBuilder;
impl MemoryContextBuilder {
    pub async fn build(pool: &SqlitePool, agent_id: &str, wo: &WorkOrder) -> Result<MemoryContext, WorkerError> {
        let mut company = vec![];
        let mut agent_mem = vec![];
        let mut task = vec![];
        let mut stale_ids = vec![];
        let mut excluded_revoked = 0usize;
        let budget = 24000usize;
        let mut used = 0usize;

        // Company Memory (no revoked)
        if let Ok(all) = memory_repo::MemoryRepo::list(pool, Some("Company"), None, false).await {
            for r in all {
                if used >= budget { break; }
                let s = serde_json::to_string(&r).unwrap_or_default();
                used += s.len();
                if r.status == coevo_core::opc::MemoryStatus::Stale { stale_ids.push(r.memory_id.clone()); }
                company.push(serde_json::to_value(r).unwrap_or_default());
            }
        }
        // Agent Memory
        if let Ok(am) = agent_memory_repo::AgentMemoryRepo::get(pool, agent_id).await {
            if let Some(am) = am {
                let s = serde_json::to_string(&am).unwrap_or_default();
                if used + s.len() < budget { used += s.len(); agent_mem.push(serde_json::to_value(am).unwrap_or_default()); }
            }
        }
        // Task memory
        if let Ok(task_all) = memory_repo::MemoryRepo::list(pool, Some("Task"), None, false).await {
            for r in task_all {
                if used >= budget { break; }
                // Only memory linked to this work order
                if r.linked_contract_hash.as_ref() != Some(&wo.contract_hash) && r.linked_plan_hash.as_ref() != Some(&wo.plan_hash) { continue; }
                let s = serde_json::to_string(&r).unwrap_or_default();
                used += s.len();
                task.push(serde_json::to_value(r).unwrap_or_default());
            }
        }
        // Count excluded revoked
        if let Ok(all) = memory_repo::MemoryRepo::list(pool, Some("Company"), None, true).await {
            excluded_revoked = all.iter().filter(|r| r.status == coevo_core::opc::MemoryStatus::Revoked).count();
        }

        Ok(MemoryContext{
            user_profile: None,
            company_memory: company,
            agent_memory: agent_mem,
            task_memory: task,
            relevant_skill_memory: vec![],
            stale_memory_ids: stale_ids,
            excluded_revoked_count: excluded_revoked,
            context_budget_chars: used,
        })
    }
}
