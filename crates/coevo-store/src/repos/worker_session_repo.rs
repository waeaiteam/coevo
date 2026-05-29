use sqlx::SqlitePool;

pub struct WorkerSessionRepo;
impl WorkerSessionRepo {
    pub async fn create(
        pool: &SqlitePool,
        session_id: &str,
        work_order_id: &str,
        agent_id: &str,
        worker_id: &str,
        status: &str,
        skills: &str,
        tools: &str,
        mem: &str,
        started: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_sessions (session_id, work_order_id, agent_id, worker_id, loaded_skill_ids_json, tool_call_ids_json, context_memory_ids_json, status, created_at_ms, updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?)")
            .bind(session_id).bind(work_order_id).bind(agent_id).bind(worker_id).bind(skills).bind(tools).bind(mem).bind(status).bind(started).bind(started).execute(pool).await?;
        Ok(())
    }
    pub async fn get(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_sessions WHERE session_id=?")
            .bind(session_id)
            .fetch_optional(pool)
            .await
    }
    pub async fn list_all(pool: &SqlitePool) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_sessions ORDER BY created_at_ms DESC LIMIT 50")
            .fetch_all(pool)
            .await
    }
    pub async fn list_by_work_order(
        pool: &SqlitePool,
        wo_id: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_sessions WHERE work_order_id=? ORDER BY created_at_ms")
            .bind(wo_id)
            .fetch_all(pool)
            .await
    }
    pub async fn update_status(
        pool: &SqlitePool,
        session_id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE worker_sessions SET status=?,updated_at_ms=? WHERE session_id=?")
            .bind(status)
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(session_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::WorkerSessionRepo;
    use crate::{migrate::run_migrations, pool::create_test_pool};
    use sqlx::Row;

    #[tokio::test]
    async fn create_list_and_update_use_current_worker_sessions_schema() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();

        WorkerSessionRepo::create(
            &pool,
            "session-store-test",
            "wo-store-test",
            "agent-founder-01",
            "worker-agent-founder-01",
            "Running",
            r#"["skill-mission-draft"]"#,
            r#"["file-readonly"]"#,
            r#"["mem-1"]"#,
            now,
        )
        .await
        .unwrap();

        WorkerSessionRepo::update_status(&pool, "session-store-test", "Completed")
            .await
            .unwrap();

        let sessions = WorkerSessionRepo::list_by_work_order(&pool, "wo-store-test")
            .await
            .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].get::<String, _>("session_id"),
            "session-store-test"
        );
        assert_eq!(sessions[0].get::<String, _>("status"), "Completed");
    }
}
