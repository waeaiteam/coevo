use coevo_core::opc::*;
use sqlx::{Row, SqlitePool};

pub struct AgentMemoryRepo;
impl AgentMemoryRepo {
    pub async fn get(
        pool: &SqlitePool,
        agent_id: &str,
    ) -> Result<Option<AgentMemory>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM agent_memories WHERE agent_id=?")
            .bind(agent_id)
            .fetch_optional(pool)
            .await?;
        Ok(row.map(|r| AgentMemory {
            agent_id: r.get("agent_id"),
            memory_records: serde_json::from_str(r.get("memory_records_json")).unwrap_or_default(),
            working_preferences: r.get("working_preferences"),
            learned_constraints: serde_json::from_str(r.get("learned_constraints_json"))
                .unwrap_or_default(),
            recurring_failures: serde_json::from_str(r.get("recurring_failures_json"))
                .unwrap_or_default(),
            successful_patterns: serde_json::from_str(r.get("successful_patterns_json"))
                .unwrap_or_default(),
            recent_tasks: serde_json::from_str(r.get("recent_tasks_json")).unwrap_or_default(),
            performance_notes: r.get("performance_notes"),
            skill_usage_stats: r.get("skill_usage_stats"),
        }))
    }
    pub async fn upsert(pool: &SqlitePool, m: &AgentMemory) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO agent_memories (\
                agent_id, memory_records_json, working_preferences, learned_constraints_json, \
                recurring_failures_json, successful_patterns_json, recent_tasks_json, \
                performance_notes, skill_usage_stats\
            ) VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(&m.agent_id)
        .bind(serde_json::to_string(&m.memory_records).unwrap())
        .bind(&m.working_preferences)
        .bind(serde_json::to_string(&m.learned_constraints).unwrap())
        .bind(serde_json::to_string(&m.recurring_failures).unwrap())
        .bind(serde_json::to_string(&m.successful_patterns).unwrap())
        .bind(serde_json::to_string(&m.recent_tasks).unwrap())
        .bind(&m.performance_notes)
        .bind(&m.skill_usage_stats)
        .execute(pool)
        .await?;
        Ok(())
    }
}
