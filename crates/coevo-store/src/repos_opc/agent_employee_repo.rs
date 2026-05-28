use sqlx::{SqlitePool, Row};
use coevo_core::opc::*;
use crate::seed::seed_employees;

pub struct AgentEmployeeRepo;
impl AgentEmployeeRepo {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<AgentEmployee>, sqlx::Error> {
        let rows = sqlx::query("SELECT agent_id,display_name,department,role,passport_json,model_profile_json,tool_scopes_json,memory_scope,permission_boundary_json,allowed_cognitive_layers_json,allowed_action_modes_json,risk_ceiling,reputation_vector_json,supervisor_agent_id,lifecycle_status,created_at_ms,updated_at_ms FROM agent_employees WHERE lifecycle_status != 'Retired'")
            .fetch_all(pool).await?;
        let mut result = vec![];
        for row in rows {
            let dept: String = row.get("department");
            let scope: String = row.get("memory_scope");
            let status: String = row.get("lifecycle_status");
            let e = AgentEmployee{
                agent_id: row.get("agent_id"), display_name: row.get("display_name"),
                department: serde_json::from_str(&format!("\"{}\"",dept)).unwrap_or(Department::Custom),
                role: row.get("role"),
                passport: serde_json::from_str::<AgentPassport>(row.get("passport_json")).unwrap_or_else(|_| AgentPassport{passport_id:String::new(),issued_by:String::new(),roles:vec![],capabilities:vec![],restrictions:vec![],expires_at_ms:None}),
                model_profile: serde_json::from_str::<ModelProviderProfile>(row.get("model_profile_json")).unwrap_or_else(|_| ModelProviderProfile{provider:String::new(),base_url:String::new(),api_key_ref:String::new(),default_model:String::new(),fast_model:String::new(),reasoning_model:String::new(),structured_output_model:String::new(),timeout_ms:30000,max_tokens:4096,max_cost_per_task_usd:1.0}),
                tool_scopes: serde_json::from_str(row.get("tool_scopes_json")).unwrap_or_default(),
                memory_scope: serde_json::from_str(&format!("\"{}\"",scope)).unwrap_or(MemoryScope::Agent),
                permission_boundary: serde_json::from_str::<PermissionBoundary>(row.get("permission_boundary_json")).unwrap_or_else(|_| PermissionBoundary{max_risk_score:0.3,can_write_fact:false,can_write_decision:false,can_access_network:false,can_access_filesystem:false,can_call_external_executor:false,can_propose_skill:false}),
                allowed_cognitive_layers: serde_json::from_str(row.get("allowed_cognitive_layers_json")).unwrap_or_default(),
                allowed_action_modes: serde_json::from_str(row.get("allowed_action_modes_json")).unwrap_or_default(),
                risk_ceiling: row.get("risk_ceiling"),
                reputation_vector: serde_json::from_str(row.get("reputation_vector_json")).unwrap_or_else(|_| coevo_core::reputation::ReputationVector::new(String::new())),
                supervisor_agent_id: row.get("supervisor_agent_id"),
                lifecycle_status: serde_json::from_str(&format!("\"{}\"",status)).unwrap_or(LifecycleStatus::Draft),
                created_at_ms: row.get::<i64,_>("created_at_ms") as u64,
                updated_at_ms: row.get::<i64,_>("updated_at_ms") as u64,
            };
            result.push(e);
        }
        Ok(result)
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
