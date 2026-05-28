use sqlx::{SqlitePool, Row};

pub struct WorkerRunRepo;
impl WorkerRunRepo {
    pub async fn create(pool: &SqlitePool, run_id: &str, wo_id: &str, agent_id: &str, worker_id: &str, session_id: &str, status: &str, result: &str, mem_ids: &str, errors: &str, audit_ref: Option<&str>, started: i64, ended: Option<i64>) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_runs VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(run_id).bind(wo_id).bind(agent_id).bind(worker_id).bind(session_id)
            .bind(status).bind(result).bind(mem_ids).bind(errors).bind(audit_ref)
            .bind(started).bind(ended).execute(pool).await?; Ok(())
    }
    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_runs WHERE run_id=?").bind(id).fetch_optional(pool).await
    }
    pub async fn list_by_work_order(pool: &SqlitePool, wo_id: &str) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_runs WHERE work_order_id=? ORDER BY started_at_ms DESC").bind(wo_id).fetch_all(pool).await
    }
    pub async fn set_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE worker_runs SET status=? WHERE run_id=?").bind(status).bind(id).execute(pool).await?; Ok(())
    }
    pub async fn complete(pool: &SqlitePool, id: &str, result: &str, mem_ids: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE worker_runs SET status='Completed', result_json=?, memory_ids_json=?, ended_at_ms=? WHERE run_id=?")
            .bind(result).bind(mem_ids).bind(chrono::Utc::now().timestamp_millis()).bind(id).execute(pool).await?; Ok(())
    }
}

pub struct WorkerStepRepo;
impl WorkerStepRepo {
    pub async fn create(pool: &SqlitePool, step_id: &str, run_id: &str, step_index: i64, step_type: &str, input_json: &str, output_json: Option<&str>, status: &str, started: i64, ended: Option<i64>, error: Option<&str>) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_steps VALUES (?,?,?,?,?,?,?,?,?,?)")
            .bind(step_id).bind(run_id).bind(step_index).bind(step_type)
            .bind(input_json).bind(output_json).bind(status).bind(started).bind(ended).bind(error).execute(pool).await?; Ok(())
    }
    pub async fn list_by_run(pool: &SqlitePool, run_id: &str) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_steps WHERE run_id=? ORDER BY step_index").bind(run_id).fetch_all(pool).await
    }
    pub async fn next_index(pool: &SqlitePool, run_id: &str) -> Result<i64, sqlx::Error> {
        let r = sqlx::query("SELECT COALESCE(MAX(step_index),-1)+1 as nxt FROM worker_steps WHERE run_id=?").bind(run_id).fetch_one(pool).await?;
        Ok(r.get::<i64,_>("nxt"))
    }
}

pub struct WorkerEventRepo;
impl WorkerEventRepo {
    pub async fn append(pool: &SqlitePool, run_id: &str, event_type: &str, payload_json: &str) -> Result<sqlx::sqlite::SqliteRow, sqlx::Error> {
        let seq: i64 = sqlx::query("SELECT COALESCE(MAX(event_seq),-1)+1 as nxt FROM worker_events WHERE run_id=?").bind(run_id).fetch_one(pool).await?.get("nxt");
        let now = chrono::Utc::now().timestamp_millis();
        let eid = format!("{}-event-{}", run_id, seq);
        sqlx::query("INSERT INTO worker_events VALUES (?,?,?,?,?,?)").bind(&eid).bind(run_id).bind(seq).bind(event_type).bind(payload_json).bind(now).execute(pool).await?;
        sqlx::query("SELECT * FROM worker_events WHERE event_id=?").bind(&eid).fetch_optional(pool).await?.ok_or(sqlx::Error::RowNotFound)
    }
    pub async fn list_by_run(pool: &SqlitePool, run_id: &str) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_events WHERE run_id=? ORDER BY event_seq").bind(run_id).fetch_all(pool).await
    }
}

pub struct WorkerSkillUsageRepo;
impl WorkerSkillUsageRepo {
    pub async fn create(pool: &SqlitePool, usage_id: &str, run_id: &str, skill_id: &str, version: &str, used_for: &str, success: bool, score: f64, notes: &str, created: i64) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_skill_usage VALUES (?,?,?,?,?,?,?,?,?)")
            .bind(usage_id).bind(run_id).bind(skill_id).bind(version).bind(used_for).bind(success as i32).bind(score).bind(notes).bind(created).execute(pool).await?; Ok(())
    }
}

pub struct WorkerToolCallRepo;
impl WorkerToolCallRepo {
    pub async fn create(pool: &SqlitePool, tc_id: &str, run_id: &str, tool_id: &str, tool_type: &str, input_summary: &str, output_summary: &str, success: bool, risk: f64, mem_id: Option<&str>, started: i64, ended: Option<i64>) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_tool_calls VALUES (?,?,?,?,?,?,?,?,?,?,?)")
            .bind(tc_id).bind(run_id).bind(tool_id).bind(tool_type).bind(input_summary).bind(output_summary).bind(success as i32).bind(risk).bind(mem_id).bind(started).bind(ended).execute(pool).await?; Ok(())
    }
}

pub struct WorkerReflectionRepo;
impl WorkerReflectionRepo {
    pub async fn create(pool: &SqlitePool, reflection_id: &str, wo_id: &str, run_id: &str, agent_id: &str, worker_id: &str, what_worked: &str, what_failed: &str, mem_add: &str, skill_update: &str, user_pref: &str, needs_human: bool, created: i64) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO worker_reflections VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(reflection_id).bind(wo_id).bind(run_id).bind(agent_id).bind(worker_id)
            .bind(what_worked).bind(what_failed).bind(mem_add).bind(skill_update).bind(user_pref)
            .bind(needs_human as i32).bind(created).execute(pool).await?; Ok(())
    }
    pub async fn get_by_run(pool: &SqlitePool, run_id: &str) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_reflections WHERE run_id=? LIMIT 1").bind(run_id).fetch_optional(pool).await
    }
}

pub struct WorkerQueueRepo;
impl WorkerQueueRepo {
    pub async fn acquire(pool: &SqlitePool, session_id: &str, run_id: &str, ttl_ms: i64) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let lid = format!("lane-{}", session_id);
        sqlx::query("INSERT INTO worker_queue_lanes VALUES (?,?,?,?,?,?,?) ON CONFLICT(session_id) DO UPDATE SET active_run_id=?,status='Active',locked_until_ms=?,updated_at_ms=?")
            .bind(&lid).bind(session_id).bind(run_id).bind("Active").bind(now+ttl_ms).bind(now).bind(now).bind(run_id).bind(now+ttl_ms).bind(now).execute(pool).await?; Ok(())
    }
    pub async fn release(pool: &SqlitePool, session_id: &str, run_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE worker_queue_lanes SET active_run_id=NULL,status='Idle',locked_until_ms=NULL,updated_at_ms=? WHERE session_id=? AND active_run_id=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(session_id).bind(run_id).execute(pool).await?; Ok(())
    }
    pub async fn get_lane(pool: &SqlitePool, session_id: &str) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT active_run_id, status, locked_until_ms FROM worker_queue_lanes WHERE session_id=?").bind(session_id).fetch_optional(pool).await
    }
}
