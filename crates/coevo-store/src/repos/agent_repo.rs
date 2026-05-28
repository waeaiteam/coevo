use crate::models::AgentRow;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct AgentRepo;

impl AgentRepo {
    pub async fn register(
        pool: &SqlitePool,
        agent_id: &str,
        passport_json: &str,
        capabilities_json: &str,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO agent_registry (id, agent_id, passport_json, capabilities_json, status, registered_at_ms) VALUES (?,?,?,?,?,?)"
        )
        .bind(&id)
        .bind(agent_id)
        .bind(passport_json)
        .bind(capabilities_json)
        .bind("active")
        .bind(now)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn list_active(pool: &SqlitePool) -> Result<Vec<AgentRow>, sqlx::Error> {
        sqlx::query_as::<_, AgentRow>("SELECT * FROM agent_registry WHERE status = 'active'")
            .fetch_all(pool)
            .await
    }

    pub async fn find_by_id(
        pool: &SqlitePool,
        agent_id: &str,
    ) -> Result<Option<AgentRow>, sqlx::Error> {
        sqlx::query_as::<_, AgentRow>("SELECT * FROM agent_registry WHERE agent_id = ?")
            .bind(agent_id)
            .fetch_optional(pool)
            .await
    }

    pub async fn update_status(
        pool: &SqlitePool,
        agent_id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE agent_registry SET status = ? WHERE agent_id = ?")
            .bind(status)
            .bind(agent_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
