use crate::error::WorkerError;
use crate::skill_runtime::SkillRuntime;
use crate::types::*;
use coevo_store::company_workspace::CompanyWorkspaceManager;
use coevo_store::repos_opc::{agent_memory_repo, memory_repo};
use sqlx::SqlitePool;
use std::path::Path;

pub struct MemoryContextBuilder;
impl MemoryContextBuilder {
    pub async fn build(
        pool: &SqlitePool,
        workspace_root: &Path,
        agent_id: &str,
        user_id: &str,
        opc_id: &str,
        _work_order_id: &str,
        contract_hash: &str,
        plan_hash: &str,
    ) -> Result<MemoryContext, WorkerError> {
        let mut company = vec![];
        let mut company_shared_files = vec![];
        let mut employee_persona_files = vec![];
        let mut agent_mem = vec![];
        let mut task = vec![];
        let mut relevant_skill_memory = vec![];
        let mut stale_ids = vec![];
        let mut excluded_revoked = 0usize;
        let mut excluded_fact_no_prov = 0usize;
        let budget = 24000usize;
        let mut used = 0usize;
        let mut user_profile = None;
        let mut company_profile = None;

        // User Profile
        if let Ok(Some(up)) =
            coevo_store::repos_opc::user_profile_repo::UserProfileRepo::get(pool, user_id).await
        {
            user_profile = Some(serde_json::to_value(up).unwrap_or_default());
        }
        // Company Profile
        if let Ok(Some(cp)) =
            coevo_store::repos_opc::opc_profile_repo::OPCProfileRepo::get(pool, opc_id).await
        {
            company_profile = Some(serde_json::to_value(cp).unwrap_or_default());
        }
        let shared_root = workspace_root.join(opc_id).join("shared");
        if shared_root.exists() {
            load_company_shared_files(
                &shared_root,
                budget.saturating_sub(used),
                &mut company_shared_files,
            )?;
            used += company_shared_files
                .iter()
                .map(|value| value.to_string().len())
                .sum::<usize>();
        }
        load_employee_persona_files(
            workspace_root,
            opc_id,
            agent_id,
            budget.saturating_sub(used),
            &mut employee_persona_files,
        )?;
        used += employee_persona_files
            .iter()
            .map(|value| value.to_string().len())
            .sum::<usize>();
        load_relevant_skill_memory(
            pool,
            workspace_root,
            opc_id,
            agent_id,
            budget.saturating_sub(used),
            &mut relevant_skill_memory,
        )
        .await?;
        used += relevant_skill_memory
            .iter()
            .map(|value| value.to_string().len())
            .sum::<usize>();

        // Company Memory
        if let Ok(all) = memory_repo::MemoryRepo::list(pool, Some("Company"), None, true).await {
            for r in all {
                if used >= budget {
                    break;
                }
                if r.status == coevo_core::opc::MemoryStatus::Revoked {
                    excluded_revoked += 1;
                    continue;
                }
                if r.cognitive_layer == coevo_core::cognitive::CognitiveLayer::Fact
                    && r.provenance.is_empty()
                {
                    excluded_fact_no_prov += 1;
                    continue;
                }
                let s = serde_json::to_string(&r).unwrap_or_default();
                used += s.len();
                if r.status == coevo_core::opc::MemoryStatus::Stale {
                    stale_ids.push(r.memory_id.clone());
                }
                company.push(serde_json::to_value(r).unwrap_or_default());
            }
        }
        // Agent Memory
        if let Ok(Some(am)) = agent_memory_repo::AgentMemoryRepo::get(pool, agent_id).await {
            let s = serde_json::to_string(&am).unwrap_or_default();
            if used + s.len() < budget {
                used += s.len();
                agent_mem.push(serde_json::to_value(am).unwrap_or_default());
            }
        }
        // Task memory — linked to this WorkOrder
        if let Ok(task_all) = memory_repo::MemoryRepo::list(pool, Some("Task"), None, false).await {
            for r in task_all {
                if used >= budget {
                    break;
                }
                if r.linked_contract_hash.as_deref() != Some(contract_hash)
                    && r.linked_plan_hash.as_deref() != Some(plan_hash)
                {
                    continue;
                }
                let s = serde_json::to_string(&r).unwrap_or_default();
                used += s.len();
                task.push(serde_json::to_value(r).unwrap_or_default());
            }
        }

        Ok(MemoryContext {
            user_profile,
            company_profile: if let Some(cp) = company_profile {
                vec![cp]
            } else {
                vec![]
            },
            company_memory: company,
            company_shared_files,
            employee_persona_files,
            agent_memory: agent_mem,
            task_memory: task,
            relevant_skill_memory,
            stale_memory_ids: stale_ids,
            excluded_revoked_count: excluded_revoked,
            context_budget_chars: used,
            fact_without_provenance: excluded_fact_no_prov,
        })
    }
}

async fn load_relevant_skill_memory(
    pool: &SqlitePool,
    workspace_root: &Path,
    opc_id: &str,
    agent_id: &str,
    remaining_budget: usize,
    out: &mut Vec<serde_json::Value>,
) -> Result<(), WorkerError> {
    if remaining_budget == 0 {
        return Ok(());
    }
    let index = SkillRuntime::load_skill_index(pool, agent_id).await?;
    let workspace =
        coevo_store::company_workspace::CompanyWorkspaceManager::new(workspace_root.to_path_buf());
    let mut used = 0usize;
    for skill in index {
        let Some(skill_id) = skill.get("skill_id").and_then(|value| value.as_str()) else {
            continue;
        };
        let content = SkillRuntime::load_full(pool, workspace_root, opc_id, agent_id, skill_id)
            .await?
            .and_then(|full| {
                full.get("prompt_template")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
            });
        let Some(content_md) = content else {
            continue;
        };
        let path = if workspace
            .company_employee_skill_markdown_path(opc_id, agent_id, skill_id)
            .exists()
        {
            format!("employees/{agent_id}/skills/{skill_id}/SKILL.md")
        } else {
            format!("skills/{skill_id}/SKILL.md")
        };
        let item = serde_json::json!({
            "skill_id": skill_id,
            "path": path,
            "content_md": content_md.chars().take(4000).collect::<String>(),
        });
        let cost = item.to_string().len();
        if used + cost > remaining_budget {
            break;
        }
        used += cost;
        out.push(item);
    }
    Ok(())
}

fn load_employee_persona_files(
    workspace_root: &Path,
    opc_id: &str,
    agent_id: &str,
    remaining_budget: usize,
    out: &mut Vec<serde_json::Value>,
) -> Result<(), WorkerError> {
    if remaining_budget == 0 {
        return Ok(());
    }
    const PERSONA_FILE_CHAR_LIMIT: usize = 4000;
    let workspace = CompanyWorkspaceManager::new(workspace_root.to_path_buf());
    let employee_dir = workspace.company_employee_dir(opc_id, agent_id);
    let mut used = 0usize;
    for (label, path) in [
        ("identity.md", employee_dir.join("identity.md")),
        ("soul.md", employee_dir.join("soul.md")),
        ("agents.md", employee_dir.join("agents.md")),
        ("owner.md", employee_dir.join("owner.md")),
        ("tools.md", employee_dir.join("tools.md")),
    ] {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let trimmed = content.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item = serde_json::json!({
            "path": format!("employees/{agent_id}/{label}"),
            "content_md": trimmed.chars().take(PERSONA_FILE_CHAR_LIMIT).collect::<String>(),
        });
        let cost = item.to_string().len();
        if used + cost > remaining_budget {
            break;
        }
        used += cost;
        out.push(item);
    }
    Ok(())
}

fn load_company_shared_files(
    shared_root: &Path,
    remaining_budget: usize,
    out: &mut Vec<serde_json::Value>,
) -> Result<(), WorkerError> {
    if remaining_budget == 0 {
        return Ok(());
    }
    let mut stack = vec![shared_root.to_path_buf()];
    let mut used = 0usize;
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let is_supported = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext, "md" | "txt"));
            if !is_supported {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let relative = path
                .strip_prefix(shared_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            let snippet = trimmed.chars().take(4000).collect::<String>();
            let item = serde_json::json!({
                "path": relative,
                "content_md": snippet,
            });
            let cost = item.to_string().len();
            if used + cost > remaining_budget {
                return Ok(());
            }
            used += cost;
            out.push(item);
        }
    }
    Ok(())
}
