use crate::models::ExecutionPlanRow;
use coevo_core::plan::ExecutionPlanSpec;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct PlanRepo;

impl PlanRepo {
    pub async fn insert(
        pool: &SqlitePool,
        plan: &ExecutionPlanSpec,
        contract_hash: &str,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO execution_plans (id, plan_hash, contract_hash, execution_plan_version, parent_plan_hash, primary_path_dag_json, agent_configs_json, failback_rules_json, hard_resource_ceilings_json, exploration_budget_quota, created_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?)"
        )
        .bind(&id)
        .bind(&plan.plan_hash)
        .bind(contract_hash)
        .bind(&plan.execution_plan_version)
        .bind(&plan.parent_plan_hash)
        .bind(serde_json::to_string(&plan.primary_path_dag).unwrap())
        .bind(serde_json::to_string(&plan.agent_configs).unwrap())
        .bind(serde_json::to_string(&plan.failback_routing_rules).unwrap())
        .bind(serde_json::to_string(&plan.hard_resource_ceilings).unwrap())
        .bind(plan.exploration_budget_quota)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_by_hash(
        pool: &SqlitePool,
        plan_hash: &str,
    ) -> Result<Option<ExecutionPlanRow>, sqlx::Error> {
        sqlx::query_as::<_, ExecutionPlanRow>("SELECT * FROM execution_plans WHERE plan_hash = ?")
            .bind(plan_hash)
            .fetch_optional(pool)
            .await
    }

    pub async fn find_by_contract(
        pool: &SqlitePool,
        contract_hash: &str,
    ) -> Result<Vec<ExecutionPlanRow>, sqlx::Error> {
        sqlx::query_as::<_, ExecutionPlanRow>(
            "SELECT * FROM execution_plans WHERE contract_hash = ? ORDER BY created_at_ms DESC",
        )
        .bind(contract_hash)
        .fetch_all(pool)
        .await
    }
}
