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
        let mut excluded_fact_no_prov = 0usize;
        let budget = 24000usize;
        let mut used = 0usize;
        let mut user_profile = None;
        let mut company_profile = None;

        // User Profile
        if let Ok(Some(up)) = coevo_store::repos_opc::user_profile_repo::UserProfileRepo::get(pool, "default-founder").await {
            user_profile = Some(serde_json::to_value(up).unwrap_or_default());
        }
        // Company Profile
        if let Ok(Some(cp)) = coevo_store::repos_opc::opc_profile_repo::OPCProfileRepo::get(pool, "default-opc").await {
            company_profile = Some(serde_json::to_value(cp).unwrap_or_default());
        }

        // Company Memory
        if let Ok(all) = memory_repo::MemoryRepo::list(pool, Some("Company"), None, true).await {
            for r in all {
                if used >= budget { break; }
                if r.status == coevo_core::opc::MemoryStatus::Revoked { excluded_revoked += 1; continue; }
                if r.cognitive_layer == coevo_core::cognitive::CognitiveLayer::Fact && r.provenance.is_empty() { excluded_fact_no_prov += 1; continue; }
                let s = serde_json::to_string(&r).unwrap_or_default();
                used += s.len();
                if r.status == coevo_core::opc::MemoryStatus::Stale { stale_ids.push(r.memory_id.clone()); }
                company.push(serde_json::to_value(r).unwrap_or_default());
            }
        }
        // Agent Memory
        if let Ok(Some(am)) = agent_memory_repo::AgentMemoryRepo::get(pool, agent_id).await {
            let s = serde_json::to_string(&am).unwrap_or_default();
            if used + s.len() < budget { used += s.len(); agent_mem.push(serde_json::to_value(am).unwrap_or_default()); }
        }
        // Task memory — linked to this WorkOrder
        if let Ok(task_all) = memory_repo::MemoryRepo::list(pool, Some("Task"), None, false).await {
            for r in task_all {
                if used >= budget { break; }
                if r.linked_contract_hash.as_ref() != Some(&wo.contract_hash) && r.linked_plan_hash.as_ref() != Some(&wo.plan_hash) { continue; }
                let s = serde_json::to_string(&r).unwrap_or_default();
                used += s.len();
                task.push(serde_json::to_value(r).unwrap_or_default());
            }
        }

        Ok(MemoryContext{
            user_profile,
            company_profile: if let Some(cp) = company_profile { vec![cp] } else { vec![] },
            company_memory: company,
            agent_memory: agent_mem,
            task_memory: task,
            relevant_skill_memory: vec![],
            stale_memory_ids: stale_ids,
            excluded_revoked_count: excluded_revoked,
            context_budget_chars: used,
            fact_without_provenance: excluded_fact_no_prov,
        })
    }
}
