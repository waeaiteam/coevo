use sqlx::SqlitePool;

pub struct WorkerRunStepRepo;
impl WorkerRunStepRepo {
    pub async fn append(pool: &SqlitePool, step_id: &str, session_id: &str, step_type: &str, input: &str, output: Option<&str>, status: &str, created: i64) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_run_steps VALUES (?,?,?,?,?,?,?)").bind(step_id).bind(session_id).bind(step_type).bind(input).bind(output).bind(status).bind(created).execute(pool).await?; Ok(())
    }
    pub async fn list_by_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_run_steps WHERE session_id=? ORDER BY created_at_ms").bind(session_id).fetch_all(pool).await
    }
}

pub struct WorkerEventRepo;
impl WorkerEventRepo {
    pub async fn append(pool: &SqlitePool, event_id: &str, session_id: &str, event_type: &str, payload: &str, created: i64) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_events VALUES (?,?,?,?,?)").bind(event_id).bind(session_id).bind(event_type).bind(payload).bind(created).execute(pool).await?; Ok(())
    }
    pub async fn list_by_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_events WHERE session_id=? ORDER BY created_at_ms").bind(session_id).fetch_all(pool).await
    }
}
