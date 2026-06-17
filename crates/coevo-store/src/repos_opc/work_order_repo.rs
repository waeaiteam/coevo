use coevo_core::opc::*;
use sqlx::{Row, SqlitePool};

pub struct WorkOrderRepo;
impl WorkOrderRepo {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> WorkOrder {
        let s: String = row.get("status");
        let governance_proposal = row
            .try_get::<Option<String>, _>("governance_proposal_json")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok());
        let governance_verdict = row
            .try_get::<Option<String>, _>("governance_verdict_json")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok());
        WorkOrder {
            work_order_id: row.get("work_order_id"),
            conversation_id: row.get("conversation_id"),
            contract_hash: row.get("contract_hash"),
            plan_hash: row.get("plan_hash"),
            user_id: row.get("user_id"),
            opc_id: row.get("opc_id"),
            mission_intent: row.get("mission_intent"),
            selected_agents: serde_json::from_str(row.get("selected_agents_json"))
                .unwrap_or_default(),
            selected_executors: serde_json::from_str(row.get("selected_executors_json"))
                .unwrap_or_default(),
            required_skills: serde_json::from_str(row.get("required_skills_json"))
                .unwrap_or_default(),
            track: row.get("track"),
            status: serde_json::from_str(&format!("\"{}\"", s)).unwrap_or(WorkOrderStatus::Draft),
            allowed_actions: serde_json::from_str(row.get("allowed_actions_json"))
                .unwrap_or_default(),
            restricted_actions: serde_json::from_str(row.get("restricted_actions_json"))
                .unwrap_or_default(),
            risk_summary: row.get("risk_summary"),
            governance_proposal,
            governance_verdict,
            created_at_ms: row.get::<i64, _>("created_at_ms") as u64,
            updated_at_ms: row.get::<i64, _>("updated_at_ms") as u64,
        }
    }
    pub async fn list(pool: &SqlitePool) -> Result<Vec<WorkOrder>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM work_orders ORDER BY created_at_ms DESC LIMIT 100")
            .fetch_all(pool)
            .await?;
        Ok(rows.iter().map(|r| Self::from_row(r)).collect())
    }
    pub async fn list_by_opc(
        pool: &SqlitePool,
        opc_id: &str,
    ) -> Result<Vec<WorkOrder>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT * FROM work_orders WHERE opc_id=? ORDER BY created_at_ms DESC LIMIT 100",
        )
        .bind(opc_id)
        .fetch_all(pool)
        .await?;
        Ok(rows.iter().map(|r| Self::from_row(r)).collect())
    }
    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<WorkOrder>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM work_orders WHERE work_order_id=?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        Ok(row.as_ref().map(|r| Self::from_row(r)))
    }
    pub async fn create(pool: &SqlitePool, w: &WorkOrder) -> Result<(), sqlx::Error> {
        if w.contract_hash.is_empty() || w.plan_hash.is_empty() {
            return Err(sqlx::Error::Protocol(
                "contract_hash/plan_hash required".into(),
            ));
        }
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO work_orders (
                work_order_id,
                conversation_id,
                contract_hash,
                plan_hash,
                user_id,
                opc_id,
                mission_intent,
                selected_agents_json,
                selected_executors_json,
                required_skills_json,
                track,
                status,
                allowed_actions_json,
                restricted_actions_json,
                risk_summary,
                governance_proposal_json,
                governance_verdict_json,
                created_at_ms,
                updated_at_ms
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&w.work_order_id)
        .bind(&w.conversation_id)
        .bind(&w.contract_hash)
        .bind(&w.plan_hash)
        .bind(&w.user_id)
        .bind(&w.opc_id)
        .bind(&w.mission_intent)
        .bind(serde_json::to_string(&w.selected_agents).unwrap())
        .bind(serde_json::to_string(&w.selected_executors).unwrap())
        .bind(serde_json::to_string(&w.required_skills).unwrap())
        .bind(&w.track)
        .bind(serde_json::to_string(&w.status).unwrap().trim_matches('"'))
        .bind(serde_json::to_string(&w.allowed_actions).unwrap())
        .bind(serde_json::to_string(&w.restricted_actions).unwrap())
        .bind(&w.risk_summary)
        .bind(
            w.governance_proposal
                .as_ref()
                .map(|value| serde_json::to_string(value).unwrap()),
        )
        .bind(
            w.governance_verdict
                .as_ref()
                .map(|value| serde_json::to_string(value).unwrap()),
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(())
    }
    pub async fn update_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tx = pool.begin().await?;
        let result = match status {
            "Draft" => {
                sqlx::query(
                    "UPDATE work_orders
                 SET status=?,updated_at_ms=?
                 WHERE work_order_id=? AND status IN (?)",
                )
                .bind(status)
                .bind(now)
                .bind(id)
                .bind("Draft")
                .execute(&mut *tx)
                .await?
            }
            "Planned" => {
                sqlx::query(
                    "UPDATE work_orders
                 SET status=?,updated_at_ms=?
                 WHERE work_order_id=? AND status IN (?,?)",
                )
                .bind(status)
                .bind(now)
                .bind(id)
                .bind("Draft")
                .bind("Planned")
                .execute(&mut *tx)
                .await?
            }
            "Running" => {
                sqlx::query(
                    "UPDATE work_orders
                 SET status=?,updated_at_ms=?
                 WHERE work_order_id=? AND status IN (?,?,?)",
                )
                .bind(status)
                .bind(now)
                .bind(id)
                .bind("Planned")
                .bind("WaitingApproval")
                .bind("Running")
                .execute(&mut *tx)
                .await?
            }
            "WaitingApproval" => {
                sqlx::query(
                    "UPDATE work_orders
                 SET status=?,updated_at_ms=?
                 WHERE work_order_id=? AND status IN (?,?,?)",
                )
                .bind(status)
                .bind(now)
                .bind(id)
                .bind("Planned")
                .bind("Running")
                .bind("WaitingApproval")
                .execute(&mut *tx)
                .await?
            }
            "Completed" | "Failed" | "Cancelled" | "Blocked" => {
                sqlx::query(
                    "UPDATE work_orders
                 SET status=?,updated_at_ms=?
                 WHERE work_order_id=? AND status IN (?,?,?,?)",
                )
                .bind(status)
                .bind(now)
                .bind(id)
                .bind("Planned")
                .bind("WaitingApproval")
                .bind("Running")
                .bind(status)
                .execute(&mut *tx)
                .await?
            }
            _ => {
                sqlx::query(
                    "UPDATE work_orders
                 SET status=?,updated_at_ms=?
                 WHERE work_order_id=? AND status=?",
                )
                .bind(status)
                .bind(now)
                .bind(id)
                .bind(status)
                .execute(&mut *tx)
                .await?
            }
        };

        if result.rows_affected() > 0 {
            tx.commit().await?;
            return Ok(());
        }

        let current =
            sqlx::query_scalar::<_, String>("SELECT status FROM work_orders WHERE work_order_id=?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.rollback().await?;

        let Some(current) = current else {
            return Err(sqlx::Error::RowNotFound);
        };
        validate_status_transition(&current, status)
    }
}

fn validate_status_transition(current: &str, next: &str) -> Result<(), sqlx::Error> {
    if current == next {
        return Ok(());
    }
    let allowed = match current {
        "Draft" => matches!(next, "Planned"),
        "Planned" => matches!(
            next,
            "Running" | "WaitingApproval" | "Completed" | "Failed" | "Cancelled" | "Blocked"
        ),
        "WaitingApproval" => {
            matches!(
                next,
                "Running" | "Completed" | "Failed" | "Cancelled" | "Blocked"
            )
        }
        "Running" => matches!(
            next,
            "WaitingApproval" | "Completed" | "Failed" | "Cancelled" | "Blocked"
        ),
        "Completed" | "Failed" | "Cancelled" | "Blocked" => false,
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(sqlx::Error::Protocol(
            format!("illegal work order status transition: {current} -> {next}").into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::WorkOrderRepo;
    use crate::{migrate::run_migrations, pool::create_pool, pool::create_test_pool};
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn create_persists_work_order_against_current_schema() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-store-create-test".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "verify work order persistence".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["FileReadonly".to_string()],
            restricted_actions: vec![],
            risk_summary: "green".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        WorkOrderRepo::create(&pool, &work_order).await.unwrap();
        let saved = WorkOrderRepo::get(&pool, "wo-store-create-test")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(saved.work_order_id, work_order.work_order_id);
        assert_eq!(saved.status, WorkOrderStatus::Planned);
        assert_eq!(saved.selected_agents, work_order.selected_agents);
    }

    #[tokio::test]
    async fn create_persists_conversation_binding_for_task_workspace() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-conversation-binding".to_string(),
            conversation_id: Some("conv-product-feedback".to_string()),
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "persist this task in its originating conversation".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec!["delete".to_string()],
            risk_summary: "green".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        WorkOrderRepo::create(&pool, &work_order).await.unwrap();
        let saved = WorkOrderRepo::get(&pool, "wo-conversation-binding")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            saved.conversation_id.as_deref(),
            Some("conv-product-feedback")
        );
    }

    #[tokio::test]
    async fn update_status_allows_blocked_work_orders() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-status-blocked".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "persist blocked work order status".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec!["delete".to_string()],
            risk_summary: "green".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        WorkOrderRepo::create(&pool, &work_order).await.unwrap();
        WorkOrderRepo::update_status(&pool, &work_order.work_order_id, "Blocked")
            .await
            .unwrap();

        let saved = WorkOrderRepo::get(&pool, &work_order.work_order_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkOrderStatus::Blocked);
    }

    #[tokio::test]
    async fn update_status_rejects_terminal_state_rollbacks() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-terminal-rollback".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "guard terminal rollback".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: "green".to_string(),
            status: WorkOrderStatus::Completed,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "green".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        WorkOrderRepo::create(&pool, &work_order).await.unwrap();
        let err = WorkOrderRepo::update_status(&pool, &work_order.work_order_id, "Planned")
            .await
            .expect_err("terminal work orders should not roll back");
        assert!(matches!(err, sqlx::Error::Protocol(_)));

        let saved = WorkOrderRepo::get(&pool, &work_order.work_order_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkOrderStatus::Completed);
    }

    #[tokio::test]
    async fn update_status_rejects_non_terminal_rollbacks() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-forward-jump".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "guard forward jumps".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "green".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        WorkOrderRepo::create(&pool, &work_order).await.unwrap();
        let err = WorkOrderRepo::update_status(&pool, &work_order.work_order_id, "Draft")
            .await
            .expect_err("planned work orders should not roll back to draft");
        assert!(matches!(err, sqlx::Error::Protocol(_)));

        let saved = WorkOrderRepo::get(&pool, &work_order.work_order_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkOrderStatus::Planned);
    }

    #[tokio::test]
    async fn update_status_reports_the_live_current_status_on_stale_writes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("wo-stale-write-{unique}.db"));
        let db_url = db_path.to_string_lossy().to_string();
        let pool = create_pool(&db_url).await.unwrap();
        let competing_pool = create_pool(&db_url).await.unwrap();
        run_migrations(&pool).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let work_order = WorkOrder {
            work_order_id: "wo-stale-write".to_string(),
            conversation_id: None,
            contract_hash: "a".repeat(64),
            plan_hash: "b".repeat(64),
            user_id: "default-founder".to_string(),
            opc_id: "default-opc".to_string(),
            mission_intent: "surface a stale status mismatch".to_string(),
            selected_agents: vec!["agent-founder-01".to_string()],
            selected_executors: vec![],
            required_skills: vec!["skill-mission-draft".to_string()],
            track: "green".to_string(),
            status: WorkOrderStatus::Planned,
            allowed_actions: vec!["read".to_string()],
            restricted_actions: vec![],
            risk_summary: "green".to_string(),
            governance_proposal: None,
            governance_verdict: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        WorkOrderRepo::create(&pool, &work_order).await.unwrap();

        let mut competing_tx = competing_pool.begin().await.unwrap();
        sqlx::query(
            "UPDATE work_orders SET status='Completed', updated_at_ms=? WHERE work_order_id=?",
        )
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(&work_order.work_order_id)
        .execute(&mut *competing_tx)
        .await
        .unwrap();

        let update = tokio::spawn({
            let pool = pool.clone();
            let work_order_id = work_order.work_order_id.clone();
            async move { WorkOrderRepo::update_status(&pool, &work_order_id, "Running").await }
        });

        tokio::task::yield_now().await;
        competing_tx.commit().await.unwrap();

        let err = update
            .await
            .unwrap()
            .expect_err("stale write must be rejected");
        let message = err.to_string();
        assert!(message.contains("illegal work order status transition: Completed -> Running"));

        let saved = WorkOrderRepo::get(&pool, &work_order.work_order_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkOrderStatus::Completed);

        let _ = std::fs::remove_file(db_path);
    }
}
