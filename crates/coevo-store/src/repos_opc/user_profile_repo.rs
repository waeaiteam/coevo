use sqlx::SqlitePool;
use coevo_core::opc::UserProfile;

pub struct UserProfileRepo;
impl UserProfileRepo {
    pub async fn get(pool: &SqlitePool, user_id: &str) -> Result<Option<UserProfile>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String,String,String,String,String,String,String,String,String,String,String,String,String,String,i64,i64)>(
            "SELECT * FROM user_profiles WHERE user_id=?"
        ).bind(user_id).fetch_optional(pool).await?;
        Ok(row.map(|r| from_row(r)))
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
}
fn from_row(r:(String,String,String,String,String,String,String,String,String,String,String,String,String,String,i64,i64))->UserProfile{
    UserProfile{user_id:r.0,display_name:r.1,preferred_language:r.2,timezone:r.3,
    risk_preference:serde_json::from_str(&format!("\"{}\"",r.4)).unwrap(),
    default_mission_mode:serde_json::from_str(&format!("\"{}\"",r.5)).unwrap(),
    long_term_goals:serde_json::from_str(&r.6).unwrap_or_default(),
    business_domains:serde_json::from_str(&r.7).unwrap_or_default(),
    communication_style:r.8,
    approval_preferences:serde_json::from_str(&r.9).unwrap_or_default(),
    data_boundaries:serde_json::from_str(&r.10).unwrap_or_default(),
    budget_limits:serde_json::from_str(&r.11).unwrap_or_default(),
    favorite_tools:serde_json::from_str(&r.12).unwrap_or_default(),
    active_projects:serde_json::from_str(&r.13).unwrap_or_default(),
    created_at_ms:r.14 as u64,updated_at_ms:r.15 as u64}
}
