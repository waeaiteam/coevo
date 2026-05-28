use sqlx::{SqlitePool, Row};
use coevo_core::opc::*;

pub struct UserProfileRepo;
impl UserProfileRepo {
    pub async fn get(pool: &SqlitePool, user_id: &str) -> Result<Option<UserProfile>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM user_profiles WHERE user_id=?")
            .bind(user_id).fetch_optional(pool).await?;
        Ok(row.map(|r| Self::from_row(&r)))
    }
    pub async fn upsert(pool: &SqlitePool, p: &UserProfile) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR REPLACE INTO user_profiles VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&p.user_id).bind(&p.display_name).bind(&p.preferred_language).bind(&p.timezone)
            .bind(serde_json::to_string(&p.risk_preference).unwrap().trim_matches('"'))
            .bind(serde_json::to_string(&p.default_mission_mode).unwrap().trim_matches('"'))
            .bind(serde_json::to_string(&p.long_term_goals).unwrap())
            .bind(serde_json::to_string(&p.business_domains).unwrap())
            .bind(&p.communication_style)
            .bind(serde_json::to_string(&p.approval_preferences).unwrap())
            .bind(serde_json::to_string(&p.data_boundaries).unwrap())
            .bind(serde_json::to_string(&p.budget_limits).unwrap())
            .bind(serde_json::to_string(&p.favorite_tools).unwrap())
            .bind(serde_json::to_string(&p.active_projects).unwrap())
            .bind(p.created_at_ms as i64).bind(p.updated_at_ms as i64)
            .execute(pool).await?;
        Ok(())
    }
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> UserProfile {
        let risk_s: String = row.get("risk_preference");
        let mode_s: String = row.get("default_mission_mode");
        UserProfile{
            user_id: row.get("user_id"), display_name: row.get("display_name"),
            preferred_language: row.get("preferred_language"), timezone: row.get("timezone"),
            risk_preference: serde_json::from_str(&format!("\"{}\"",risk_s)).unwrap_or(RiskPreference::Balanced),
            default_mission_mode: serde_json::from_str(&format!("\"{}\"",mode_s)).unwrap_or(MissionMode::Auto),
            long_term_goals: serde_json::from_str(row.get("long_term_goals_json")).unwrap_or_default(),
            business_domains: serde_json::from_str(row.get("business_domains_json")).unwrap_or_default(),
            communication_style: row.get("communication_style"),
            approval_preferences: serde_json::from_str(row.get("approval_preferences_json")).unwrap_or(ApprovalPreferences{auto_approve_below_risk:0.3,require_explicit_for_yellow:true,require_mfa_for_red:true,negative_consent_timeout_secs:300}),
            data_boundaries: serde_json::from_str(row.get("data_boundaries_json")).unwrap_or_default(),
            budget_limits: serde_json::from_str(row.get("budget_limits_json")).unwrap_or(BudgetLimits{max_cost_per_task_usd:50.0,max_cost_per_day_usd:500.0,max_agents_per_task:5}),
            favorite_tools: serde_json::from_str(row.get("favorite_tools_json")).unwrap_or_default(),
            active_projects: serde_json::from_str(row.get("active_projects_json")).unwrap_or_default(),
            created_at_ms: row.get::<i64,_>("created_at_ms") as u64,
            updated_at_ms: row.get::<i64,_>("updated_at_ms") as u64,
        }
    }
}
