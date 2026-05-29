use crate::enum_db::{skill_status_from_db, skill_status_to_db};
use coevo_core::skills::*;
use sqlx::{Row, SqlitePool};

pub struct SkillRepo;
impl SkillRepo {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> AgentSkillPackage {
        let s: String = row.get("status");
        AgentSkillPackage {
            skill_id: row.get("skill_id"),
            version: row.get("version"),
            name: row.get("name"),
            owner_agent_id: row.get("owner_agent_id"),
            department: row.get("department"),
            description: row.get("description"),
            trigger_patterns: serde_json::from_str(row.get("trigger_patterns_json"))
                .unwrap_or_default(),
            applicable_domains: serde_json::from_str(row.get("applicable_domains_json"))
                .unwrap_or_default(),
            required_tools: serde_json::from_str(row.get("required_tools_json"))
                .unwrap_or_default(),
            required_model_profile: row
                .get::<Option<String>, _>("required_model_profile_json")
                .and_then(|s| serde_json::from_str(&s).ok()),
            input_schema: serde_json::from_str(row.get("input_schema_json"))
                .unwrap_or(serde_json::json!({})),
            output_schema: serde_json::from_str(row.get("output_schema_json"))
                .unwrap_or(serde_json::json!({})),
            prompt_template: row.get("prompt_template"),
            procedure_steps: serde_json::from_str(row.get("procedure_steps_json"))
                .unwrap_or_default(),
            guardrails: serde_json::from_str(row.get("guardrails_json")).unwrap_or_default(),
            examples: serde_json::from_str(row.get("examples_json")).unwrap_or_default(),
            tests: serde_json::from_str(row.get("tests_json")).unwrap_or_default(),
            evals: serde_json::from_str(row.get("evals_json")).unwrap_or_default(),
            permissions_required: serde_json::from_str(row.get("permissions_required_json"))
                .unwrap_or_default(),
            allowed_cognitive_layers: serde_json::from_str(
                row.get("allowed_cognitive_layers_json"),
            )
            .unwrap_or_default(),
            allowed_action_modes: serde_json::from_str(row.get("allowed_action_modes_json"))
                .unwrap_or_default(),
            risk_ceiling: row.get("risk_ceiling"),
            provenance: row.get("provenance"),
            status: skill_status_from_db(&s),
            created_at_ms: row.get::<i64, _>("created_at_ms") as u64,
            updated_at_ms: row.get::<i64, _>("updated_at_ms") as u64,
        }
    }
    pub async fn list(
        pool: &SqlitePool,
        agent_id: Option<&str>,
    ) -> Result<Vec<AgentSkillPackage>, sqlx::Error> {
        let rows = if let Some(aid) = agent_id {
            sqlx::query(
                "SELECT * FROM agent_skills WHERE owner_agent_id=? ORDER BY created_at_ms DESC",
            )
            .bind(aid)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query("SELECT * FROM agent_skills ORDER BY created_at_ms DESC LIMIT 100")
                .fetch_all(pool)
                .await?
        };
        Ok(rows.iter().map(|r| Self::from_row(r)).collect())
    }
    pub async fn get(
        pool: &SqlitePool,
        skill_id: &str,
        version: Option<&str>,
    ) -> Result<Option<AgentSkillPackage>, sqlx::Error> {
        let row = if let Some(v) = version {
            sqlx::query("SELECT * FROM agent_skills WHERE skill_id=? AND version=?")
                .bind(skill_id)
                .bind(v)
                .fetch_optional(pool)
                .await?
        } else {
            sqlx::query(
                "SELECT * FROM agent_skills WHERE skill_id=? ORDER BY created_at_ms DESC LIMIT 1",
            )
            .bind(skill_id)
            .fetch_optional(pool)
            .await?
        };
        Ok(row.as_ref().map(|r| Self::from_row(r)))
    }
    pub async fn upsert(pool: &SqlitePool, s: &AgentSkillPackage) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR REPLACE INTO agent_skills VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&s.skill_id).bind(&s.version).bind(&s.name).bind(&s.owner_agent_id).bind(&s.department).bind(&s.description)
            .bind(serde_json::to_string(&s.trigger_patterns).unwrap()).bind(serde_json::to_string(&s.applicable_domains).unwrap())
            .bind(serde_json::to_string(&s.required_tools).unwrap())
            .bind(s.required_model_profile.as_ref().map(|m|serde_json::to_string(m).unwrap()))
            .bind(serde_json::to_string(&s.input_schema).unwrap()).bind(serde_json::to_string(&s.output_schema).unwrap())
            .bind(&s.prompt_template).bind(serde_json::to_string(&s.procedure_steps).unwrap())
            .bind(serde_json::to_string(&s.guardrails).unwrap()).bind(serde_json::to_string(&s.examples).unwrap())
            .bind(serde_json::to_string(&s.tests).unwrap()).bind(serde_json::to_string(&s.evals).unwrap())
            .bind(serde_json::to_string(&s.permissions_required).unwrap())
            .bind(serde_json::to_string(&s.allowed_cognitive_layers).unwrap())
            .bind(serde_json::to_string(&s.allowed_action_modes).unwrap()).bind(s.risk_ceiling).bind(&s.provenance)
            .bind(skill_status_to_db(s.status))
            .bind(s.created_at_ms as i64).bind(s.updated_at_ms as i64).execute(pool).await?;
        Ok(())
    }
    pub async fn activate(
        pool: &SqlitePool,
        skill_id: &str,
        version: &str,
    ) -> Result<(), sqlx::Error> {
        let skill = Self::get(pool, skill_id, Some(version))
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        if skill.risk_ceiling >= 0.8 {
            return Err(sqlx::Error::Protocol(
                "Red skill requires human approval".into(),
            ));
        }
        sqlx::query("UPDATE agent_skills SET status='Active',updated_at_ms=? WHERE skill_id=? AND version=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(skill_id).bind(version).execute(pool).await?;
        // Deprecate other active versions
        sqlx::query("UPDATE agent_skills SET status='Deprecated',updated_at_ms=? WHERE skill_id=? AND status='Active' AND version!=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(skill_id).bind(version).execute(pool).await?;
        Ok(())
    }
    pub async fn rollback(
        pool: &SqlitePool,
        skill_id: &str,
        target_version: &str,
    ) -> Result<(), sqlx::Error> {
        let _target = Self::get(pool, skill_id, Some(target_version))
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        sqlx::query("UPDATE agent_skills SET status='Deprecated',updated_at_ms=? WHERE skill_id=? AND status='Active'")
            .bind(chrono::Utc::now().timestamp_millis()).bind(skill_id).execute(pool).await?;
        sqlx::query("UPDATE agent_skills SET status='Active',updated_at_ms=? WHERE skill_id=? AND version=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(skill_id).bind(target_version).execute(pool).await?;
        Ok(())
    }
    pub async fn seed_default(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let skills = vec![
            (
                "skill-mission-draft",
                "1.0.0",
                "Mission Drafting",
                "agent-founder-01",
                "FounderOffice",
                0.3,
            ),
            (
                "skill-risk-review",
                "1.0.0",
                "Risk Review",
                "agent-risk-01",
                "Governance",
                0.5,
            ),
            (
                "skill-code-review",
                "1.0.0",
                "Code Review",
                "agent-engineer-01",
                "Engineering",
                0.4,
            ),
            (
                "skill-report-gen",
                "1.0.0",
                "Report Generation",
                "agent-synth-01",
                "FounderOffice",
                0.3,
            ),
            (
                "skill-fact-check",
                "1.0.0",
                "Fact Check",
                "agent-critic-01",
                "Governance",
                0.4,
            ),
        ];
        for (id, ver, name, owner, dept, risk) in skills {
            let sk = AgentSkillPackage {
                skill_id: id.into(),
                version: ver.into(),
                name: name.into(),
                owner_agent_id: owner.into(),
                department: dept.into(),
                description: String::new(),
                trigger_patterns: vec![],
                applicable_domains: vec![],
                required_tools: vec![],
                required_model_profile: None,
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                prompt_template: String::new(),
                procedure_steps: vec![],
                guardrails: vec!["no escalation".into()],
                examples: vec![],
                tests: vec![SkillTestCase {
                    test_id: "test-1".into(),
                    description: "default".into(),
                    input: serde_json::json!({}),
                    expected_output_schema: serde_json::json!({}),
                    forbidden_behaviors: vec![],
                    required_evidence: vec![],
                    pass_criteria: vec![],
                }],
                evals: vec![],
                permissions_required: vec![],
                allowed_cognitive_layers: vec!["Hypothesis".into()],
                allowed_action_modes: vec!["DRAFT_ONLY".into()],
                risk_ceiling: risk,
                provenance: "seed".into(),
                status: SkillStatus::Active,
                created_at_ms: now,
                updated_at_ms: now,
            };
            Self::upsert(pool, &sk).await?;
        }
        Ok(())
    }
}
