use crate::error::WorkerError;
use crate::types::*;
use coevo_store::company_workspace::CompanyWorkspaceManager;
use coevo_store::repos::worker_run_repo::WorkerSkillUsageRepo;
use coevo_store::repos_opc::skill_repo::SkillRepo;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

pub struct SkillRuntime;
impl SkillRuntime {
    fn is_employee_scoped_skill(
        skill: &coevo_core::skills::AgentSkillPackage,
        agent_id: &str,
    ) -> bool {
        skill.provenance.starts_with("skill-evolution-")
            && !skill.owner_agent_id.trim().is_empty()
            && skill.owner_agent_id == agent_id
    }

    pub async fn load_skill_index(
        pool: &SqlitePool,
        agent_id: &str,
    ) -> Result<Vec<serde_json::Value>, WorkerError> {
        let skills = SkillRepo::list(pool, None)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let mut winners = BTreeMap::<String, coevo_core::skills::AgentSkillPackage>::new();
        for skill in skills.into_iter().filter(|s| {
            s.status == coevo_core::skills::SkillStatus::Active
                && (!s.provenance.starts_with("skill-evolution-") || s.owner_agent_id == agent_id)
        }) {
            let key = skill.skill_id.clone();
            let should_replace = winners
                .get(&key)
                .map(|current| {
                    (
                        Self::is_employee_scoped_skill(&skill, agent_id),
                        skill.created_at_ms,
                        skill.updated_at_ms,
                    ) > (
                        Self::is_employee_scoped_skill(current, agent_id),
                        current.created_at_ms,
                        current.updated_at_ms,
                    )
                })
                .unwrap_or(true);
            if should_replace {
                winners.insert(key, skill);
            }
        }
        Ok(winners
            .into_values()
            .map(|s| {
                serde_json::json!({
                    "skill_id": s.skill_id, "name": s.name, "version": s.version,
                    "trigger_patterns": s.trigger_patterns, "risk_ceiling": s.risk_ceiling,
                    "owner_agent_id": s.owner_agent_id, "status": "Active"
                })
            })
            .collect())
    }

    pub fn select_relevant(
        intent: &str,
        required_skills: &[String],
        index: &[serde_json::Value],
    ) -> Vec<String> {
        let lower = intent.to_lowercase();
        let mut scores: Vec<(&str, i32)> = vec![];
        for sk in index {
            let sid = sk["skill_id"].as_str().unwrap_or("");
            let mut score = 0;
            if required_skills.iter().any(|r| r == sid) {
                score += 100;
            }
            if let Some(patterns) = sk["trigger_patterns"].as_array() {
                for p in patterns {
                    if lower.contains(p.as_str().unwrap_or("").to_lowercase().as_str()) {
                        score += 10;
                    }
                }
            }
            if score > 0 {
                scores.push((sid, score));
            }
        }
        scores.sort_by_key(|(_, s)| -*s);
        scores
            .iter()
            .take(3)
            .map(|(id, _)| id.to_string())
            .collect()
    }

    pub async fn load_full(
        pool: &SqlitePool,
        workspace_root: &std::path::Path,
        opc_id: &str,
        agent_id: &str,
        skill_id: &str,
    ) -> Result<Option<serde_json::Value>, WorkerError> {
        let skills = SkillRepo::list(pool, None)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let selected = skills
            .into_iter()
            .filter(|skill| {
                skill.skill_id == skill_id
                    && skill.status == coevo_core::skills::SkillStatus::Active
            })
            .max_by(|left, right| {
                let left_key = (
                    Self::is_employee_scoped_skill(left, agent_id),
                    left.created_at_ms,
                    left.updated_at_ms,
                );
                let right_key = (
                    Self::is_employee_scoped_skill(right, agent_id),
                    right.created_at_ms,
                    right.updated_at_ms,
                );
                left_key.cmp(&right_key)
            });
        Ok(selected.map(|sk| {
            Self::ensure_skill_markdown_materialized(workspace_root, opc_id, agent_id, &sk);
            let mut value = serde_json::to_value(sk).unwrap_or_default();
            if let Some(prompt_template) =
                Self::resolve_prompt_template(workspace_root, opc_id, agent_id, skill_id, &value)
            {
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "prompt_template".to_string(),
                        serde_json::Value::String(prompt_template),
                    );
                }
            }
            value
        }))
    }

    pub async fn record_usage(pool: &SqlitePool, r: &SkillUsageRecord) -> Result<(), WorkerError> {
        WorkerSkillUsageRepo::create(
            pool,
            &r.usage_id,
            &r.run_id,
            &r.skill_id,
            &r.version,
            &r.used_for,
            r.success,
            r.score,
            &r.notes,
            r.created_at_ms,
        )
        .await
        .map_err(|e| WorkerError::Internal(e.to_string()))
    }

    fn resolve_prompt_template(
        workspace_root: &std::path::Path,
        opc_id: &str,
        agent_id: &str,
        skill_id: &str,
        skill_value: &serde_json::Value,
    ) -> Option<String> {
        let workspace = CompanyWorkspaceManager::new(workspace_root.to_path_buf());
        for path in [
            workspace.company_employee_skill_markdown_path(opc_id, agent_id, skill_id),
            workspace.company_skill_markdown_path(opc_id, skill_id),
        ] {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        skill_value
            .get("prompt_template")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    fn ensure_skill_markdown_materialized(
        workspace_root: &std::path::Path,
        opc_id: &str,
        agent_id: &str,
        skill: &coevo_core::skills::AgentSkillPackage,
    ) {
        let workspace = CompanyWorkspaceManager::new(workspace_root.to_path_buf());
        let is_employee_skill = Self::is_employee_scoped_skill(skill, agent_id);
        let path = if is_employee_skill {
            workspace.company_employee_skill_markdown_path(opc_id, agent_id, &skill.skill_id)
        } else {
            workspace.company_skill_markdown_path(opc_id, &skill.skill_id)
        };
        if path.exists() {
            return;
        }
        let write_agent = if is_employee_skill {
            Some(agent_id)
        } else {
            None
        };
        let _ = workspace.write_company_skill_markdown(opc_id, skill, write_agent);
    }
}
