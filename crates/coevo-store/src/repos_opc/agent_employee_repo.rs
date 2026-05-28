use sqlx::SqlitePool;
use coevo_core::opc::*;
use coevo_store::seed::seed_employees;

pub struct AgentEmployeeRepo;
impl AgentEmployeeRepo {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<AgentEmployee>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,String,String,String,String,String,String,String,String,String,String,String,String,f64,String,Option<String>,String,i64,i64)>(
            "SELECT agent_id,display_name,department,role,passport_json,model_profile_json,tool_scopes_json,memory_scope,permission_boundary_json,allowed_cognitive_layers_json,allowed_action_modes_json,risk_ceiling,reputation_vector_json,supervisor_agent_id,lifecycle_status,created_at_ms,updated_at_ms FROM agent_employees WHERE lifecycle_status != 'Retired'"
        ).fetch_all(pool).await?;
        Ok(rows.into_iter().map(|r| AgentEmployee{
            agent_id:r.0,display_name:r.1,department:serde_json::from_str(&format!("\"{}\"",r.2)).unwrap(),role:r.3,
            passport:serde_json::from_str(&r.4).unwrap_or_default(),model_profile:serde_json::from_str(&r.5).unwrap_or_default(),
            tool_scopes:serde_json::from_str(&r.6).unwrap_or_default(),memory_scope:serde_json::from_str(&format!("\"{}\"",r.7)).unwrap(),
            permission_boundary:serde_json::from_str(&r.8).unwrap_or_default(),
            allowed_cognitive_layers:serde_json::from_str(&r.9).unwrap_or_default(),
            allowed_action_modes:serde_json::from_str(&r.10).unwrap_or_default(),
            risk_ceiling:r.11,reputation_vector:serde_json::from_str(&r.12).unwrap_or_default(),
            supervisor_agent_id:r.13,lifecycle_status:serde_json::from_str(&format!("\"{}\"",r.14)).unwrap(),
            created_at_ms:r.15 as u64,updated_at_ms:r.16 as u64
        }).collect())
    }
    pub async fn upsert(pool: &SqlitePool, a: &AgentEmployee) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR REPLACE INTO agent_employees VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&a.agent_id).bind(&a.display_name).bind(serde_json::to_string(&a.department).unwrap().trim_matches('"'))
            .bind(&a.role).bind(serde_json::to_string(&a.passport).unwrap()).bind(serde_json::to_string(&a.model_profile).unwrap())
            .bind(serde_json::to_string(&a.tool_scopes).unwrap()).bind(serde_json::to_string(&a.memory_scope).unwrap().trim_matches('"'))
            .bind(serde_json::to_string(&a.permission_boundary).unwrap()).bind(serde_json::to_string(&a.allowed_cognitive_layers).unwrap())
            .bind(serde_json::to_string(&a.allowed_action_modes).unwrap()).bind(a.risk_ceiling)
            .bind(serde_json::to_string(&a.reputation_vector).unwrap()).bind(&a.supervisor_agent_id)
            .bind(serde_json::to_string(&a.lifecycle_status).unwrap().trim_matches('"'))
            .bind(a.created_at_ms as i64).bind(a.updated_at_ms as i64)
            .execute(pool).await?; Ok(())
    }
    pub async fn seed(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        for e in seed_employees() { Self::upsert(pool, &e).await?; } Ok(())
    }
}
