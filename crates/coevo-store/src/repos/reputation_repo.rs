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

/// A single reputation snapshot, used to draw an employee's growth curve.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReputationSnapshotRow {
    pub snapshot_id: String,
    pub agent_id: String,
    pub run_id: Option<String>,
    pub domain_competence: f64,
    pub uncertainty_honesty: f64,
    pub policy_compliance: f64,
    pub resource_efficiency: f64,
    pub task_count: i64,
    pub overall_score: f64,
    pub created_at_ms: i64,
}

pub struct ReputationHistoryRepo;

impl ReputationHistoryRepo {
    #[allow(clippy::too_many_arguments)]
    pub async fn snapshot(
        pool: &SqlitePool,
        agent_id: &str,
        run_id: Option<&str>,
        domain_competence: f64,
        uncertainty_honesty: f64,
        policy_compliance: f64,
        resource_efficiency: f64,
        task_count: i64,
    ) -> Result<(), sqlx::Error> {
        let overall =
            (domain_competence + uncertainty_honesty + policy_compliance + resource_efficiency)
                / 4.0;
        sqlx::query(
            "INSERT INTO reputation_history (snapshot_id, agent_id, run_id, domain_competence, uncertainty_honesty, policy_compliance, resource_efficiency, task_count, overall_score, created_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(agent_id)
        .bind(run_id)
        .bind(domain_competence)
        .bind(uncertainty_honesty)
        .bind(policy_compliance)
        .bind(resource_efficiency)
        .bind(task_count)
        .bind(overall)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_by_agent(
        pool: &SqlitePool,
        agent_id: &str,
        limit: i64,
    ) -> Result<Vec<ReputationSnapshotRow>, sqlx::Error> {
        sqlx::query_as::<_, ReputationSnapshotRow>(
            "SELECT * FROM reputation_history WHERE agent_id = ? ORDER BY created_at_ms ASC LIMIT ?",
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}
