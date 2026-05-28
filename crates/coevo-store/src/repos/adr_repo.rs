use crate::models::AdrRow;
use coevo_core::decision::AdrA;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct AdrRepo;

impl AdrRepo {
    pub async fn insert(pool: &SqlitePool, adr: &AdrA) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO adr_records (id, decision_id, mcl_reference, proposer_agent, critic_objections_json, blocker_conflict_status, selected_option, rejected_alternatives_json, risk_accepted_json, human_override_reason, responsibility_anchor_json, follow_up_monitoring_plan, post_execution_feedback_json, created_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        )
        .bind(&id)
        .bind(&adr.decision_id)
        .bind(&adr.mcl_reference)
        .bind(&adr.proposer_agent)
        .bind(serde_json::to_string(&adr.critic_objections).unwrap())
        .bind(serde_json::to_string(&adr.blocker_conflict_status).unwrap().trim_matches('"'))
        .bind(&adr.selected_option)
        .bind(serde_json::to_string(&adr.rejected_alternatives).unwrap())
        .bind(serde_json::to_string(&adr.risk_accepted).unwrap())
        .bind(&adr.human_override_reason)
        .bind(serde_json::to_string(&adr.responsibility_anchor).unwrap())
        .bind(&adr.follow_up_monitoring_plan)
        .bind(adr.post_execution_feedback.as_ref().map(|f| serde_json::to_string(f).unwrap()))
        .bind(adr.created_at_ms as i64)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_by_decision(
        pool: &SqlitePool,
        decision_id: &str,
    ) -> Result<Option<AdrRow>, sqlx::Error> {
        sqlx::query_as::<_, AdrRow>("SELECT * FROM adr_records WHERE decision_id = ?")
            .bind(decision_id)
            .fetch_optional(pool)
            .await
    }
}
