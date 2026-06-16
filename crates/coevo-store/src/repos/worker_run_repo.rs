use sqlx::{Row, SqlitePool};

pub struct WorkerRunRepo;
impl WorkerRunRepo {
    pub async fn create(
        pool: &SqlitePool,
        opc_id: &str,
        run_id: &str,
        wo_id: &str,
        agent_id: &str,
        worker_id: &str,
        session_id: &str,
        status: &str,
        result: &str,
        mem_ids: &str,
        errors: &str,
        audit_ref: Option<&str>,
        started: i64,
        ended: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO worker_runs (\
                run_id, opc_id, work_order_id, agent_id, worker_id, session_id, status, \
                result_json, memory_ids_json, errors_json, audit_ref, started_at_ms, ended_at_ms\
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(run_id)
        .bind(opc_id)
        .bind(wo_id)
        .bind(agent_id)
        .bind(worker_id)
        .bind(session_id)
        .bind(status)
        .bind(result)
        .bind(mem_ids)
        .bind(errors)
        .bind(audit_ref)
        .bind(started)
        .bind(ended)
        .execute(pool)
        .await?;
        Ok(())
    }
    pub async fn get(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_runs WHERE run_id=?")
            .bind(id)
            .fetch_optional(pool)
            .await
    }
    pub async fn list_by_work_order(
        pool: &SqlitePool,
        wo_id: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_runs WHERE work_order_id=? ORDER BY started_at_ms DESC")
            .bind(wo_id)
            .fetch_all(pool)
            .await
    }
    pub async fn list_by_worker(
        pool: &SqlitePool,
        worker_id: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_runs WHERE worker_id=? ORDER BY started_at_ms DESC")
            .bind(worker_id)
            .fetch_all(pool)
            .await
    }
    pub async fn set_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE worker_runs SET status=? WHERE run_id=?")
            .bind(status)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
    /// Persist queryable execution-summary columns (tokens/cost/latency) at the
    /// end of a run so the employee-growth page can aggregate without re-parsing
    /// step JSON blobs.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_summary(
        pool: &SqlitePool,
        id: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        total_tokens: i64,
        total_cost_usd: f64,
        latency_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE worker_runs SET prompt_tokens=?, completion_tokens=?, total_tokens=?, total_cost_usd=?, latency_ms=? WHERE run_id=?",
        )
        .bind(prompt_tokens)
        .bind(completion_tokens)
        .bind(total_tokens)
        .bind(total_cost_usd)
        .bind(latency_ms)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }
    pub async fn complete(
        pool: &SqlitePool,
        id: &str,
        result: &str,
        mem_ids: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE worker_runs SET status='Completed', result_json=?, memory_ids_json=?, ended_at_ms=? WHERE run_id=?")
            .bind(result).bind(mem_ids).bind(chrono::Utc::now().timestamp_millis()).bind(id).execute(pool).await?;
        Ok(())
    }
    /// Aggregate execution stats for one agent across all its runs — powers the
    /// employee growth page and the (de-stubbed) agent evaluator.
    /// Returns (total_runs, completed_runs, failed_runs, avg_latency_ms, total_tokens, total_cost_usd).
    pub async fn agent_run_stats(
        pool: &SqlitePool,
        agent_id: &str,
    ) -> Result<(i64, i64, i64, f64, i64, f64), sqlx::Error> {
        let row: (i64, i64, i64, Option<f64>, Option<i64>, Option<f64>) = sqlx::query_as(
            "SELECT \
               COUNT(*) AS total, \
               SUM(CASE WHEN status='Completed' THEN 1 ELSE 0 END) AS completed, \
               SUM(CASE WHEN status='Failed' THEN 1 ELSE 0 END) AS failed, \
               AVG(latency_ms) AS avg_latency, \
               SUM(total_tokens) AS tokens, \
               SUM(total_cost_usd) AS cost \
             FROM worker_runs WHERE agent_id = ?",
        )
        .bind(agent_id)
        .fetch_one(pool)
        .await?;
        Ok((
            row.0,
            row.1,
            row.2,
            row.3.unwrap_or(0.0),
            row.4.unwrap_or(0),
            row.5.unwrap_or(0.0),
        ))
    }
}

pub struct WorkerStepRepo;
impl WorkerStepRepo {
    pub async fn create(
        pool: &SqlitePool,
        step_id: &str,
        run_id: &str,
        step_index: i64,
        step_type: &str,
        input_json: &str,
        output_json: Option<&str>,
        status: &str,
        started: i64,
        ended: Option<i64>,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO worker_steps (\
                step_id, run_id, step_index, step_type, input_json, output_json, \
                status, started_at_ms, ended_at_ms, error\
            ) VALUES (?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(step_id)
        .bind(run_id)
        .bind(step_index)
        .bind(step_type)
        .bind(input_json)
        .bind(output_json)
        .bind(status)
        .bind(started)
        .bind(ended)
        .bind(error)
        .execute(pool)
        .await?;
        Ok(())
    }
    pub async fn list_by_run(
        pool: &SqlitePool,
        run_id: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_steps WHERE run_id=? ORDER BY step_index")
            .bind(run_id)
            .fetch_all(pool)
            .await
    }
    /// Compute the next free step index for a run.
    ///
    /// WARNING: reading the index and inserting in two separate statements is
    /// racy under concurrency (UNIQUE(run_id, step_index)). Prefer
    /// [`WorkerStepRepo::create_auto_index`], which assigns the index and
    /// inserts atomically in a single statement.
    pub async fn next_index(pool: &SqlitePool, run_id: &str) -> Result<i64, sqlx::Error> {
        let r = sqlx::query(
            "SELECT COALESCE(MAX(step_index),-1)+1 as nxt FROM worker_steps WHERE run_id=?",
        )
        .bind(run_id)
        .fetch_one(pool)
        .await?;
        Ok(r.get::<i64, _>("nxt"))
    }
    /// Insert a step with the next free step_index for the run, atomically.
    ///
    /// The index is computed inside the INSERT statement itself, so concurrent
    /// writers cannot race the read-then-insert window. As belt-and-braces a
    /// bounded retry handles UNIQUE(run_id, step_index) violations that could
    /// still surface from other writers using explicit indexes.
    /// Returns the step_index that was assigned.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_auto_index(
        pool: &SqlitePool,
        step_id: &str,
        run_id: &str,
        step_type: &str,
        input_json: &str,
        output_json: Option<&str>,
        status: &str,
        started: i64,
        ended: Option<i64>,
        error: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        const MAX_ATTEMPTS: u32 = 5;
        let mut attempt = 0;
        loop {
            attempt += 1;
            let result = sqlx::query(
                "INSERT INTO worker_steps (\
                    step_id, run_id, step_index, step_type, input_json, output_json, \
                    status, started_at_ms, ended_at_ms, error\
                ) SELECT ?, ?, COALESCE(MAX(step_index)+1, 0), ?, ?, ?, ?, ?, ?, ? \
                FROM worker_steps WHERE run_id=? \
                RETURNING step_index",
            )
            .bind(step_id)
            .bind(run_id)
            .bind(step_type)
            .bind(input_json)
            .bind(output_json)
            .bind(status)
            .bind(started)
            .bind(ended)
            .bind(error)
            .bind(run_id)
            .fetch_one(pool)
            .await;
            match result {
                Ok(row) => return Ok(row.get::<i64, _>("step_index")),
                Err(err) => {
                    let unique_violation = err
                        .as_database_error()
                        .map(|db| db.is_unique_violation())
                        .unwrap_or(false);
                    if unique_violation && attempt < MAX_ATTEMPTS {
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }
}

pub struct WorkerEventRepo;
impl WorkerEventRepo {
    pub async fn append(
        pool: &SqlitePool,
        run_id: &str,
        event_type: &str,
        payload_json: &str,
    ) -> Result<sqlx::sqlite::SqliteRow, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = pool.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        let operation = async {
            let seq: i64 = sqlx::query(
                "SELECT COALESCE(MAX(event_seq),-1)+1 as nxt FROM worker_events WHERE run_id=?",
            )
            .bind(run_id)
            .fetch_one(&mut *conn)
            .await?
            .get("nxt");
            let eid = format!("{}-event-{}", run_id, seq);
            sqlx::query(
                "INSERT INTO worker_events (
                    event_id,
                    run_id,
                    event_seq,
                    event_type,
                    payload_json,
                    created_at_ms
                ) VALUES (?,?,?,?,?,?)",
            )
            .bind(&eid)
            .bind(run_id)
            .bind(seq)
            .bind(event_type)
            .bind(payload_json)
            .bind(now)
            .execute(&mut *conn)
            .await?;
            sqlx::query("SELECT * FROM worker_events WHERE event_id=?")
                .bind(&eid)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(sqlx::Error::RowNotFound)
        }
        .await;
        match operation {
            Ok(row) => {
                sqlx::query("COMMIT").execute(&mut *conn).await?;
                Ok(row)
            }
            Err(err) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                Err(err)
            }
        }
    }
    pub async fn list_by_run(
        pool: &SqlitePool,
        run_id: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_events WHERE run_id=? ORDER BY event_seq")
            .bind(run_id)
            .fetch_all(pool)
            .await
    }
}

pub struct WorkerSkillUsageRepo;
impl WorkerSkillUsageRepo {
    pub async fn create(
        pool: &SqlitePool,
        usage_id: &str,
        run_id: &str,
        skill_id: &str,
        version: &str,
        used_for: &str,
        success: bool,
        score: f64,
        notes: &str,
        created: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO worker_skill_usage (\
                usage_id, run_id, skill_id, version, used_for, success, score, notes, created_at_ms\
            ) VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(usage_id)
        .bind(run_id)
        .bind(skill_id)
        .bind(version)
        .bind(used_for)
        .bind(success as i32)
        .bind(score)
        .bind(notes)
        .bind(created)
        .execute(pool)
        .await?;
        Ok(())
    }
}

pub struct WorkerToolCallRepo;
impl WorkerToolCallRepo {
    pub async fn create(
        pool: &SqlitePool,
        tc_id: &str,
        run_id: &str,
        tool_id: &str,
        tool_type: &str,
        input_summary: &str,
        output_summary: &str,
        success: bool,
        risk: f64,
        mem_id: Option<&str>,
        started: i64,
        ended: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO worker_tool_calls (\
                tool_call_id, run_id, tool_id, tool_type, input_summary, output_summary, \
                success, risk_ceiling, memory_id, started_at_ms, ended_at_ms\
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(tc_id)
        .bind(run_id)
        .bind(tool_id)
        .bind(tool_type)
        .bind(input_summary)
        .bind(output_summary)
        .bind(success as i32)
        .bind(risk)
        .bind(mem_id)
        .bind(started)
        .bind(ended)
        .execute(pool)
        .await?;
        Ok(())
    }
}

pub struct WorkerReflectionRepo;
impl WorkerReflectionRepo {
    pub async fn create(
        pool: &SqlitePool,
        reflection_id: &str,
        wo_id: &str,
        run_id: &str,
        agent_id: &str,
        worker_id: &str,
        what_worked: &str,
        what_failed: &str,
        mem_add: &str,
        skill_update: &str,
        user_pref: &str,
        needs_human: bool,
        created: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO worker_reflections (\
                reflection_id, work_order_id, run_id, agent_id, worker_id, \
                what_worked_json, what_failed_json, memory_to_add_json, \
                skill_to_update_json, user_preference_observed_json, \
                needs_human_review, created_at_ms\
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(reflection_id)
        .bind(wo_id)
        .bind(run_id)
        .bind(agent_id)
        .bind(worker_id)
        .bind(what_worked)
        .bind(what_failed)
        .bind(mem_add)
        .bind(skill_update)
        .bind(user_pref)
        .bind(needs_human as i32)
        .bind(created)
        .execute(pool)
        .await?;
        Ok(())
    }
    pub async fn get_by_run(
        pool: &SqlitePool,
        run_id: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM worker_reflections WHERE run_id=? LIMIT 1")
            .bind(run_id)
            .fetch_optional(pool)
            .await
    }
}

pub struct WorkerQueueRepo;
impl WorkerQueueRepo {
    pub async fn acquire(
        pool: &SqlitePool,
        session_id: &str,
        run_id: &str,
        ttl_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let lid = format!("lane-{}", session_id);
        sqlx::query(
            "INSERT INTO worker_queue_lanes (\
                lane_id, session_id, active_run_id, status, locked_until_ms, created_at_ms, updated_at_ms\
            ) VALUES (?,?,?,?,?,?,?) \
            ON CONFLICT(session_id) DO UPDATE SET active_run_id=?,status='Active',locked_until_ms=?,updated_at_ms=?",
        )
            .bind(&lid).bind(session_id).bind(run_id).bind("Active").bind(now+ttl_ms).bind(now).bind(now).bind(run_id).bind(now+ttl_ms).bind(now).execute(pool).await?;
        Ok(())
    }
    pub async fn release(
        pool: &SqlitePool,
        session_id: &str,
        run_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE worker_queue_lanes SET active_run_id=NULL,status='Idle',locked_until_ms=NULL,updated_at_ms=? WHERE session_id=? AND active_run_id=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(session_id).bind(run_id).execute(pool).await?;
        Ok(())
    }
    pub async fn get_lane(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT active_run_id, status, locked_until_ms FROM worker_queue_lanes WHERE session_id=?").bind(session_id).fetch_optional(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkerEventRepo, WorkerStepRepo};
    use crate::{
        migrate::run_migrations,
        pool::{create_pool, create_test_pool},
    };
    use sqlx::Row;

    #[tokio::test]
    async fn worker_event_append_uses_named_columns_after_session_id_migration() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        let event = WorkerEventRepo::append(
            &pool,
            "run-store-event-test",
            "LifecycleStart",
            r#"{"status":"Running"}"#,
        )
        .await
        .unwrap();

        assert_eq!(event.get::<String, _>("run_id"), "run-store-event-test");
        assert_eq!(event.get::<i64, _>("event_seq"), 0);
        assert_eq!(event.get::<String, _>("event_type"), "LifecycleStart");
    }

    #[tokio::test]
    async fn worker_event_append_assigns_unique_sequential_event_seq_under_concurrency() {
        let db_path = std::env::temp_dir().join(format!(
            "coevo-worker-events-concurrency-{}.db",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&db_path);
        let pool = create_pool(&db_path.to_string_lossy()).await.unwrap();
        run_migrations(&pool).await.unwrap();

        let run_id = "run-store-concurrency";
        let append_count = 24usize;
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(append_count));
        let mut tasks = Vec::with_capacity(append_count);
        for idx in 0..append_count {
            let pool = pool.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                WorkerEventRepo::append(
                    &pool,
                    run_id,
                    "ContentDelta",
                    &format!(r#"{{"delta":"chunk-{idx}"}}"#),
                )
                .await
            }));
        }

        let mut rows = Vec::with_capacity(append_count);
        for task in tasks {
            rows.push(task.await.unwrap().unwrap());
        }

        let mut seqs = rows
            .iter()
            .map(|row| row.get::<i64, _>("event_seq"))
            .collect::<Vec<_>>();
        seqs.sort_unstable();
        assert_eq!(seqs, (0..append_count as i64).collect::<Vec<_>>());

        let persisted: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM worker_events WHERE run_id=?")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(persisted, append_count as i64);

        pool.close().await;
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn worker_step_create_auto_index_assigns_unique_sequential_indexes_under_concurrency() {
        let db_path = std::env::temp_dir().join(format!(
            "coevo-worker-steps-concurrency-{}.db",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&db_path);
        let pool = create_pool(&db_path.to_string_lossy()).await.unwrap();
        run_migrations(&pool).await.unwrap();

        let run_id = "run-step-concurrency";
        let step_count = 24usize;
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(step_count));
        let mut tasks = Vec::with_capacity(step_count);
        for idx in 0..step_count {
            let pool = pool.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                let now = chrono::Utc::now().timestamp_millis();
                WorkerStepRepo::create_auto_index(
                    &pool,
                    &format!("step-{idx}"),
                    run_id,
                    "ModelCall",
                    "{}",
                    None,
                    "Completed",
                    now,
                    Some(now),
                    None,
                )
                .await
            }));
        }

        let mut indexes = Vec::with_capacity(step_count);
        for task in tasks {
            indexes.push(task.await.unwrap().unwrap());
        }
        indexes.sort_unstable();
        assert_eq!(indexes, (0..step_count as i64).collect::<Vec<_>>());

        let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM worker_steps WHERE run_id=?")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(persisted, step_count as i64);

        pool.close().await;
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn worker_step_create_auto_index_starts_at_zero_and_interoperates_with_explicit_create() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();

        let first = WorkerStepRepo::create_auto_index(
            &pool,
            "step-auto-0",
            "run-auto",
            "BuildContext",
            "{}",
            None,
            "Completed",
            now,
            Some(now),
            None,
        )
        .await
        .unwrap();
        assert_eq!(first, 0);

        WorkerStepRepo::create(
            &pool,
            "step-explicit-1",
            "run-auto",
            1,
            "ModelCall",
            "{}",
            None,
            "Completed",
            now,
            Some(now),
            None,
        )
        .await
        .unwrap();

        let third = WorkerStepRepo::create_auto_index(
            &pool,
            "step-auto-2",
            "run-auto",
            "Reflect",
            "{}",
            None,
            "Completed",
            now,
            Some(now),
            None,
        )
        .await
        .unwrap();
        assert_eq!(third, 2);

        let next = WorkerStepRepo::next_index(&pool, "run-auto").await.unwrap();
        assert_eq!(next, 3);
    }
}
