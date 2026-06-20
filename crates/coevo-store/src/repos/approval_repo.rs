use crate::models::ApprovalRequestRow;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ApprovalRepo;

impl ApprovalRepo {
    pub async fn create(
        pool: &SqlitePool,
        opc_id: &str,
        contract_hash: &str,
        action_urn: &str,
        approval_mode: &str,
        requested_by: &str,
        timeout_ms: i64,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + timeout_ms;
        sqlx::query(
            "INSERT INTO approval_requests (id, opc_id, contract_hash, action_urn, approval_mode, status, requested_by, requested_at_ms, expires_at_ms) VALUES (?,?,?,?,?,?,?,?,?)"
        )
        .bind(&id)
        .bind(opc_id)
        .bind(contract_hash)
        .bind(action_urn)
        .bind(approval_mode)
        .bind("pending")
        .bind(requested_by)
        .bind(now)
        .bind(expires_at)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_by_id(
        pool: &SqlitePool,
        opc_id: &str,
        id: &str,
    ) -> Result<Option<ApprovalRequestRow>, sqlx::Error> {
        sqlx::query_as::<_, ApprovalRequestRow>(
            "SELECT * FROM approval_requests WHERE id = ? AND opc_id = ?",
        )
        .bind(id)
        .bind(opc_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn approve(
        pool: &SqlitePool,
        opc_id: &str,
        id: &str,
        approved_by: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query("UPDATE approval_requests SET status = 'approved', approved_by = ?, decided_at_ms = ? WHERE id = ? AND opc_id = ? AND status = 'pending' AND expires_at_ms >= ?")
            .bind(approved_by)
            .bind(now)
            .bind(id)
            .bind(opc_id)
            .bind(now)
            .execute(pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    pub async fn deny(
        pool: &SqlitePool,
        opc_id: &str,
        id: &str,
        denied_by: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query("UPDATE approval_requests SET status = 'denied', approved_by = ?, decided_at_ms = ? WHERE id = ? AND opc_id = ? AND status = 'pending' AND expires_at_ms >= ?")
            .bind(denied_by)
            .bind(now)
            .bind(id)
            .bind(opc_id)
            .bind(now)
            .execute(pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    pub async fn consume_approved(
        pool: &SqlitePool,
        opc_id: &str,
        id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            "UPDATE approval_requests SET status = 'consumed' WHERE id = ? AND opc_id = ? AND status = 'approved' AND expires_at_ms >= ?",
        )
        .bind(id)
        .bind(opc_id)
        .bind(now)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    pub async fn expire_pending(pool: &SqlitePool) -> Result<Vec<ApprovalRequestRow>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let rows = sqlx::query_as::<_, ApprovalRequestRow>(
            "SELECT * FROM approval_requests WHERE status = 'pending' AND expires_at_ms < ?",
        )
        .bind(now)
        .fetch_all(pool)
        .await?;
        let mut expired = Vec::new();
        for row in &rows {
            let result = sqlx::query(
                "UPDATE approval_requests SET status = 'expired' WHERE id = ? AND status = 'pending' AND expires_at_ms < ?",
            )
                .bind(&row.id)
                .bind(now)
                .execute(pool)
                .await?;
            if result.rows_affected() == 1 {
                expired.push(row.clone());
            }
        }
        Ok(expired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate::run_migrations;
    use crate::pool::create_test_pool;
    use crate::repos::contract_repo::ContractRepo;
    use coevo_core::contract::{
        ActionMode, ApprovalMode, ContractState, EvidenceRequirement, GoalNode, GoalStatus,
        GoalTree, HumanApprovalPolicy, MCLSpec, ResponsibilityAnchorPolicy, RiskToleranceProfile,
        TerminationPolicy,
    };

    async fn insert_contract(pool: &SqlitePool, hash: &str) {
        let contract = MCLSpec {
            mcl_version: "1.0".to_string(),
            mcl_state: ContractState::ActiveContract,
            parent_contract_hash: "0".repeat(64),
            goal_tree: GoalTree {
                root: GoalNode {
                    id: "root".to_string(),
                    description: "test".to_string(),
                    status: GoalStatus::Pending,
                    children: vec![],
                    depends_on: vec![],
                },
            },
            institution_policy_hash: "0".repeat(64),
            data_boundary: vec![],
            allowed_action_modes: vec![ActionMode::DraftOnly],
            human_approval_policy: HumanApprovalPolicy {
                approval_mode: ApprovalMode::NegativeConsent,
                authorized_roles: vec!["Admin".to_string()],
                negative_consent_timeout_secs: 300,
                mfa_auth_url: None,
            },
            evidence_requirement: EvidenceRequirement {
                minimum_level: "none".to_string(),
                require_json_report: false,
            },
            risk_tolerance_profile: RiskToleranceProfile {
                max_risk_score: 0.6,
                allow_emergency_lease: false,
            },
            termination_policy: TerminationPolicy {
                max_token_budget: 10000,
                max_hops: 3,
                max_latency_ms: 60000,
                max_stance_rounds: 3,
            },
            responsibility_anchor_policy: ResponsibilityAnchorPolicy {
                required_human_roles: vec!["Admin".to_string()],
                agent_forbidden_actions: vec![],
            },
        };
        ContractRepo::insert(pool, &contract, hash).await.unwrap();
    }

    #[tokio::test]
    async fn approve_requires_pending_and_unexpired_request() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let contract_hash = "a".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let approval_id = ApprovalRepo::create(
            &pool,
            "opc-authz",
            &contract_hash,
            "urn:coevo:work-order:wo-expired:execute",
            "NEGATIVE_CONSENT",
            "default-founder",
            -1,
        )
        .await
        .unwrap();

        let err = ApprovalRepo::approve(&pool, "opc-authz", &approval_id, "approver-1")
            .await
            .expect_err("expired approval should not transition to approved");
        assert!(matches!(err, sqlx::Error::RowNotFound));
    }

    #[tokio::test]
    async fn terminal_approval_rows_cannot_be_overwritten() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let contract_hash = "b".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let approval_id = ApprovalRepo::create(
            &pool,
            "opc-authz",
            &contract_hash,
            "urn:coevo:work-order:wo-terminal:execute",
            "NEGATIVE_CONSENT",
            "default-founder",
            300_000,
        )
        .await
        .unwrap();

        ApprovalRepo::approve(&pool, "opc-authz", &approval_id, "approver-1")
            .await
            .unwrap();
        let err = ApprovalRepo::deny(&pool, "opc-authz", &approval_id, "approver-2")
            .await
            .expect_err("approved approval should not transition to denied");
        assert!(matches!(err, sqlx::Error::RowNotFound));

        let row = ApprovalRepo::find_by_id(&pool, "opc-authz", &approval_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "approved");
        assert_eq!(row.approved_by.as_deref(), Some("approver-1"));
    }

    #[tokio::test]
    async fn approved_receipts_are_consumed_once() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let contract_hash = "e".repeat(64);
        insert_contract(&pool, &contract_hash).await;

        let approval_id = ApprovalRepo::create(
            &pool,
            "opc-authz",
            &contract_hash,
            "urn:coevo:work-order:wo-consume:execute",
            "EXPLICIT_APPROVAL",
            "default-founder",
            300_000,
        )
        .await
        .unwrap();

        ApprovalRepo::approve(&pool, "opc-authz", &approval_id, "approver-1")
            .await
            .unwrap();
        ApprovalRepo::consume_approved(&pool, "opc-authz", &approval_id)
            .await
            .unwrap();

        let err = ApprovalRepo::consume_approved(&pool, "opc-authz", &approval_id)
            .await
            .expect_err("consumed approval receipt should not be reusable");
        assert!(matches!(err, sqlx::Error::RowNotFound));

        let row = ApprovalRepo::find_by_id(&pool, "opc-authz", &approval_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "consumed");
        assert_eq!(row.approved_by.as_deref(), Some("approver-1"));
    }
}
