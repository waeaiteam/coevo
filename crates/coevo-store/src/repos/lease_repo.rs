use crate::models::LeaseRow;
use coevo_core::lease::EmergencyLease;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct LeaseRepo;

impl LeaseRepo {
    pub async fn insert(pool: &SqlitePool, lease: &EmergencyLease) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO leases (id, lease_id, contract_hash, agent_id, lease_scope_json, lease_budget, operations_used, granted_at_ms, expires_at_ms, ttl_ms, monitoring_signature, diagnostic_signature, is_active, was_revoked) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,1,0)"
        )
        .bind(&id)
        .bind(&lease.lease_id)
        .bind(&lease.contract_hash)
        .bind(&lease.agent_id)
        .bind(serde_json::to_string(&lease.lease_scope).unwrap())
        .bind(lease.lease_budget as i32)
        .bind(lease.operations_used as i32)
        .bind(lease.granted_at_ms as i64)
        .bind(lease.expires_at_ms as i64)
        .bind(lease.ttl_ms as i64)
        .bind(&lease.monitoring_signature)
        .bind(&lease.diagnostic_signature)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_active(
        pool: &SqlitePool,
        lease_id: &str,
    ) -> Result<Option<LeaseRow>, sqlx::Error> {
        sqlx::query_as::<_, LeaseRow>(
            "SELECT * FROM leases WHERE lease_id = ? AND is_active = 1 AND was_revoked = 0",
        )
        .bind(lease_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn consume_operation(pool: &SqlitePool, lease_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE leases SET operations_used = operations_used + 1 WHERE lease_id = ? AND is_active = 1 AND was_revoked = 0 AND operations_used < lease_budget",
        )
        .bind(lease_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn revoke(pool: &SqlitePool, lease_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE leases SET is_active = 0, was_revoked = 1 WHERE lease_id = ?")
            .bind(lease_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn expire_all(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            "UPDATE leases SET is_active = 0 WHERE is_active = 1 AND expires_at_ms < ?",
        )
        .bind(now)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
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

    async fn pool_with_contract(hash: &str) -> SqlitePool {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
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
            allowed_action_modes: vec![ActionMode::CommitReady],
            human_approval_policy: HumanApprovalPolicy {
                approval_mode: ApprovalMode::ExplicitApproval,
                authorized_roles: vec!["Admin".to_string()],
                negative_consent_timeout_secs: 0,
                mfa_auth_url: None,
            },
            evidence_requirement: EvidenceRequirement {
                minimum_level: "unit_tests_passing".to_string(),
                require_json_report: true,
            },
            risk_tolerance_profile: RiskToleranceProfile {
                max_risk_score: 0.8,
                allow_emergency_lease: true,
            },
            termination_policy: TerminationPolicy {
                max_token_budget: 100000,
                max_hops: 6,
                max_latency_ms: 300000,
                max_stance_rounds: 3,
            },
            responsibility_anchor_policy: ResponsibilityAnchorPolicy {
                required_human_roles: vec![],
                agent_forbidden_actions: vec![],
            },
        };
        ContractRepo::insert(&pool, &contract, hash).await.unwrap();
        pool
    }

    fn lease_row(lease_id: &str, budget: u32, used: u32) -> coevo_core::lease::EmergencyLease {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        coevo_core::lease::EmergencyLease {
            lease_id: lease_id.to_string(),
            contract_hash: "lease-contract".to_string(),
            agent_id: "agent-x".to_string(),
            lease_scope: vec!["urn:coevo:action:test".to_string()],
            lease_budget: budget,
            operations_used: used,
            granted_at_ms: now,
            expires_at_ms: now + 60_000,
            ttl_ms: 60_000,
            monitoring_signature: "monitoring".to_string(),
            diagnostic_signature: "diagnostic".to_string(),
            is_active: true,
            was_revoked: false,
        }
    }

    #[tokio::test]
    async fn consume_operation_is_noop_when_budget_is_exhausted() {
        let pool = pool_with_contract("lease-contract").await;
        let lease = lease_row("lease-budget-exhausted", 1, 1);
        LeaseRepo::insert(&pool, &lease).await.unwrap();

        let rows = LeaseRepo::consume_operation(&pool, &lease.lease_id)
            .await
            .unwrap();
        assert_eq!(rows, 0);

        let stored = LeaseRepo::find_active(&pool, &lease.lease_id)
            .await
            .unwrap()
            .expect("lease should still be present");
        assert_eq!(stored.operations_used, 1);
    }
}
