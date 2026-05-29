use sqlx::{SqlitePool, Row};

pub struct WorkerSessionRepo;
impl WorkerSessionRepo {
    pub async fn create(pool: &SqlitePool, session_id: &str, work_order_id: &str, agent_id: &str, worker_id: &str, status: &str, skills: &str, tools: &str, mem: &str, started: i64) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_sessions VALUES (?,?,?,?,?,?,?,?,?,?)").bind(session_id).bind(work_order_id).bind(agent_id).bind(worker_id).bind(skills).bind(tools).bind(mem).bind(status).bind(started).bind(Option::<i64>::None).execute(pool).await?; Ok(())
    }
    pub async fn get(pool: &SqlitePool, session_id: &str) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_sessions WHERE session_id=?").bind(session_id).fetch_optional(pool).await
    }
    pub async fn list_by_work_order(pool: &SqlitePool, wo_id: &str) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_sessions WHERE work_order_id=? ORDER BY started_at_ms").bind(wo_id).fetch_all(pool).await
    }
    pub async fn update_status(pool: &SqlitePool, session_id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE worker_sessions SET status=?,ended_at_ms=? WHERE session_id=?").bind(status).bind(chrono::Utc::now().timestamp_millis()).bind(session_id).execute(pool).await?; Ok(())
    }
}
