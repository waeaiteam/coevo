use sqlx::SqlitePool;

pub struct AgentWorkerRepo;
impl AgentWorkerRepo {
    pub async fn upsert(
        pool: &SqlitePool,
        worker_id: &str,
        opc_id: &str,
        agent_id: &str,
        department: &str,
        status: &str,
        work_order_id: Option<&str>,
        session_id: Option<&str>,
        loaded_skills: &str,
        memory_scope: &str,
        tool_scope: &str,
        created: i64,
        updated: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO agent_workers (
                worker_id,
                agent_id,
                department,
                status,
                current_work_order_id,
                current_session_id,
                loaded_skills_json,
                memory_scope,
                tool_scope_json,
                created_at_ms,
                updated_at_ms,
                opc_id
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(worker_id)
        .bind(agent_id)
        .bind(department)
        .bind(status)
        .bind(work_order_id)
        .bind(session_id)
        .bind(loaded_skills)
        .bind(memory_scope)
        .bind(tool_scope)
        .bind(created)
        .bind(updated)
        .bind(opc_id)
        .execute(pool)
        .await?;
        Ok(())
    }
    pub async fn get(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM agent_workers WHERE worker_id=?")
            .bind(id)
            .fetch_optional(pool)
            .await
    }
    pub async fn get_by_agent(
        pool: &SqlitePool,
        agent_id: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM agent_workers WHERE agent_id=? LIMIT 1")
            .bind(agent_id)
            .fetch_optional(pool)
            .await
    }
    pub async fn list(pool: &SqlitePool) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM agent_workers ORDER BY updated_at_ms DESC LIMIT 50")
            .fetch_all(pool)
            .await
    }
    pub async fn set_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE agent_workers SET status=?, updated_at_ms=? WHERE worker_id=?")
            .bind(status)
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
