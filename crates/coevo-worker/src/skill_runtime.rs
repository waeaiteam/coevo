use crate::error::WorkerError;
use crate::types::*;
use coevo_store::repos::worker_run_repo::WorkerSkillUsageRepo;
use coevo_store::repos_opc::skill_repo::SkillRepo;
use sqlx::SqlitePool;

pub struct SkillRuntime;
impl SkillRuntime {
    pub async fn load_skill_index(
        pool: &SqlitePool,
        _agent_id: &str,
    ) -> Result<Vec<serde_json::Value>, WorkerError> {
        let skills = SkillRepo::list(pool, None)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        Ok(skills
            .iter()
            .filter(|s| s.status == coevo_core::skills::SkillStatus::Active)
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
        skill_id: &str,
    ) -> Result<Option<serde_json::Value>, WorkerError> {
        let s = SkillRepo::get(pool, skill_id, None)
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        Ok(s.map(|sk| serde_json::to_value(sk).unwrap_or_default()))
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
}
