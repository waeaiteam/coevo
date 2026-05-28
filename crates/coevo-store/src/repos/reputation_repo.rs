use crate::models::ReputationVectorRow;
use coevo_core::reputation::ReputationVector;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ReputationRepo;

impl ReputationRepo {
    pub async fn upsert(pool: &SqlitePool, rv: &ReputationVector) -> Result<(), sqlx::Error> {
        let existing = sqlx::query_as::<_, ReputationVectorRow>(
            "SELECT * FROM reputation_vectors WHERE agent_id = ?",
        )
        .bind(&rv.agent_id)
        .fetch_optional(pool)
        .await?;

        let now = chrono::Utc::now().timestamp_millis();
        if existing.is_some() {
            sqlx::query(
                "UPDATE reputation_vectors SET task_domain_competence=?, uncertainty_honesty=?, policy_compliance=?, resource_efficiency=?, task_count=?, high_difficulty_avoidance_count=?, last_updated_ms=? WHERE agent_id=?"
            )
            .bind(rv.task_domain_competence)
            .bind(rv.uncertainty_honesty)
            .bind(rv.policy_compliance)
            .bind(rv.resource_efficiency)
            .bind(rv.task_count as i64)
            .bind(rv.high_difficulty_avoidance_count as i32)
            .bind(now)
            .bind(&rv.agent_id)
            .execute(pool)
            .await?;
        } else {
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO reputation_vectors (id, agent_id, task_domain_competence, uncertainty_honesty, policy_compliance, resource_efficiency, task_count, high_difficulty_avoidance_count, last_updated_ms) VALUES (?,?,?,?,?,?,?,?,?)"
            )
            .bind(&id)
            .bind(&rv.agent_id)
            .bind(rv.task_domain_competence)
            .bind(rv.uncertainty_honesty)
            .bind(rv.policy_compliance)
            .bind(rv.resource_efficiency)
            .bind(rv.task_count as i64)
            .bind(rv.high_difficulty_avoidance_count as i32)
            .bind(now)
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    pub async fn find_by_agent(
        pool: &SqlitePool,
        agent_id: &str,
    ) -> Result<Option<ReputationVectorRow>, sqlx::Error> {
        sqlx::query_as::<_, ReputationVectorRow>(
            "SELECT * FROM reputation_vectors WHERE agent_id = ?",
        )
        .bind(agent_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_top_n(
        pool: &SqlitePool,
        n: i64,
    ) -> Result<Vec<ReputationVectorRow>, sqlx::Error> {
        sqlx::query_as::<_, ReputationVectorRow>(
            "SELECT * FROM reputation_vectors ORDER BY (task_domain_competence + uncertainty_honesty + policy_compliance + resource_efficiency) / 4.0 DESC LIMIT ?"
        )
        .bind(n)
        .fetch_all(pool)
        .await
    }
}
