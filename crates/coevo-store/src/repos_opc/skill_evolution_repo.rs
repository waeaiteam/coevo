use coevo_core::skills::*;
use sqlx::{Row, SqlitePool};

pub struct SkillEvolutionRepo;
impl SkillEvolutionRepo {
    fn proposal_from_row(row: &sqlx::sqlite::SqliteRow) -> SkillEvolutionProposal {
        let st: String = row.get("source_type");
        let pt: String = row.get("proposal_type");
        let s: String = row.get("status");
        SkillEvolutionProposal {
            proposal_id: row.get("proposal_id"),
            source_type: serde_json::from_str(&format!("\"{}\"", st))
                .unwrap_or(EvolutionSourceType::Failure),
            source_refs: serde_json::from_str(row.get("source_refs_json")).unwrap_or_default(),
            target_skill_id: row.get("target_skill_id"),
            proposal_type: serde_json::from_str(&format!("\"{}\"", pt))
                .unwrap_or(EvolutionProposalType::PatchSkill),
            diagnosis: row.get("diagnosis"),
            proposed_changes: row.get("proposed_changes"),
            expected_benefit: row.get("expected_benefit"),
            risk_assessment: row.get("risk_assessment"),
            generated_tests: serde_json::from_str(row.get("generated_tests_json"))
                .unwrap_or_default(),
            status: serde_json::from_str(&format!("\"{}\"", s))
                .unwrap_or(EvolutionProposalStatus::Draft),
            created_by_agent: row.get("created_by_agent"),
            created_at_ms: row.get::<i64, _>("created_at_ms") as u64,
        }
    }
    pub async fn create_proposal(
        pool: &SqlitePool,
        p: &SkillEvolutionProposal,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO skill_evolution_proposals VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&p.proposal_id)
            .bind(
                serde_json::to_string(&p.source_type)
                    .unwrap()
                    .trim_matches('"'),
            )
            .bind(serde_json::to_string(&p.source_refs).unwrap())
            .bind(&p.target_skill_id)
            .bind(
                serde_json::to_string(&p.proposal_type)
                    .unwrap()
                    .trim_matches('"'),
            )
            .bind(&p.diagnosis)
            .bind(&p.proposed_changes)
            .bind(&p.expected_benefit)
            .bind(&p.risk_assessment)
            .bind(serde_json::to_string(&p.generated_tests).unwrap())
            .bind(serde_json::to_string(&p.status).unwrap().trim_matches('"'))
            .bind(&p.created_by_agent)
            .bind(p.created_at_ms as i64)
            .execute(pool)
            .await?;
        Ok(())
    }
    pub async fn list(
        pool: &SqlitePool,
        status: Option<&str>,
    ) -> Result<Vec<SkillEvolutionProposal>, sqlx::Error> {
        let rows = if let Some(s) = status {
            sqlx::query("SELECT * FROM skill_evolution_proposals WHERE status=? ORDER BY created_at_ms DESC").bind(s).fetch_all(pool).await?
        } else {
            sqlx::query(
                "SELECT * FROM skill_evolution_proposals ORDER BY created_at_ms DESC LIMIT 50",
            )
            .fetch_all(pool)
            .await?
        };
        Ok(rows.iter().map(|r| Self::proposal_from_row(r)).collect())
    }
    pub async fn update_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE skill_evolution_proposals SET status=? WHERE proposal_id=?")
            .bind(status)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
    pub async fn append_eval(pool: &SqlitePool, r: &SkillEvalResult) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO skill_eval_results VALUES (?,?,?,?,?,?,?,?,?,?)")
            .bind(&r.eval_id)
            .bind(&r.skill_id)
            .bind(&r.version)
            .bind(&r.run_id)
            .bind(r.passed as i32)
            .bind(r.score)
            .bind(serde_json::to_string(&r.failures).unwrap())
            .bind(r.regression_detected as i32)
            .bind(&r.verifier_notes)
            .bind(r.created_at_ms as i64)
            .execute(pool)
            .await?;
        Ok(())
    }
    pub async fn record_version(
        pool: &SqlitePool,
        r: &SkillVersionRecord,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR REPLACE INTO skill_version_records VALUES (?,?,?,?,?,?,?,?,?)")
            .bind(&r.skill_id)
            .bind(&r.version)
            .bind(&r.parent_version)
            .bind(&r.diff_summary)
            .bind(&r.change_reason)
            .bind(
                r.verifier_result
                    .as_ref()
                    .map(|v| serde_json::to_string(v).unwrap()),
            )
            .bind(&r.approved_by)
            .bind(r.rollback_available as i32)
            .bind(r.created_at_ms as i64)
            .execute(pool)
            .await?;
        Ok(())
    }
}
