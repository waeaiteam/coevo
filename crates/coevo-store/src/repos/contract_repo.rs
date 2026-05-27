use crate::models::ContractRow;
use coevo_core::contract::MCLSpec;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ContractRepo;

impl ContractRepo {
    pub async fn insert(pool: &SqlitePool, contract: &MCLSpec, contract_hash: &str) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO contracts (id, contract_hash, mcl_version, mcl_state, parent_contract_hash, goal_tree_json, institution_policy_hash, data_boundary_json, allowed_action_modes_json, human_approval_policy_json, evidence_requirement_json, risk_tolerance_profile_json, termination_policy_json, responsibility_anchor_policy_json, created_at_ms, updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        )
        .bind(&id)
        .bind(contract_hash)
        .bind(&contract.mcl_version)
        .bind(serde_json::to_string(&contract.mcl_state).unwrap().trim_matches('"'))
        .bind(&contract.parent_contract_hash)
        .bind(serde_json::to_string(&contract.goal_tree).unwrap())
        .bind(&contract.institution_policy_hash)
        .bind(serde_json::to_string(&contract.data_boundary).unwrap())
        .bind(serde_json::to_string(&contract.allowed_action_modes).unwrap())
        .bind(serde_json::to_string(&contract.human_approval_policy).unwrap())
        .bind(serde_json::to_string(&contract.evidence_requirement).unwrap())
        .bind(serde_json::to_string(&contract.risk_tolerance_profile).unwrap())
        .bind(serde_json::to_string(&contract.termination_policy).unwrap())
        .bind(serde_json::to_string(&contract.responsibility_anchor_policy).unwrap())
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_by_hash(pool: &SqlitePool, hash: &str) -> Result<Option<ContractRow>, sqlx::Error> {
        sqlx::query_as::<_, ContractRow>("SELECT * FROM contracts WHERE contract_hash = ?")
            .bind(hash)
            .fetch_optional(pool)
            .await
    }

    pub async fn update_state(
        pool: &SqlitePool,
        hash: &str,
        new_state: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE contracts SET mcl_state = ?, updated_at_ms = ? WHERE contract_hash = ?")
            .bind(new_state)
            .bind(now)
            .bind(hash)
            .execute(pool)
            .await?;
        Ok(())
    }
}
