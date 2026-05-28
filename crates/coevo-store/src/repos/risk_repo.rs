use crate::models::RiskDecisionRow;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct RiskRepo;

impl RiskRepo {
    pub async fn insert(
        pool: &SqlitePool,
        decision_id: &str,
        contract_hash: &str,
        agent_id: &str,
        action_urn: &str,
        decision: &str,
        required_confidence: f64,
        available_confidence: f64,
        action_risk: f64,
        inaction_risk: f64,
        reason: &str,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO risk_decisions (id, decision_id, contract_hash, agent_id, action_urn, decision, required_confidence, available_confidence, action_risk, inaction_risk, reason, decided_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)"
        )
        .bind(&id)
        .bind(decision_id)
        .bind(contract_hash)
        .bind(agent_id)
        .bind(action_urn)
        .bind(decision)
        .bind(required_confidence)
        .bind(available_confidence)
        .bind(action_risk)
        .bind(inaction_risk)
        .bind(reason)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_by_contract(
        pool: &SqlitePool,
        contract_hash: &str,
    ) -> Result<Vec<RiskDecisionRow>, sqlx::Error> {
        sqlx::query_as::<_, RiskDecisionRow>(
            "SELECT * FROM risk_decisions WHERE contract_hash = ? ORDER BY decided_at_ms DESC",
        )
        .bind(contract_hash)
        .fetch_all(pool)
        .await
    }
}
