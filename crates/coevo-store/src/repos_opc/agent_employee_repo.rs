use crate::enum_db::{
    department_from_db, department_to_db, lifecycle_status_from_db, lifecycle_status_to_db,
    memory_scope_from_db, memory_scope_to_db,
};
use crate::seed::seed_employees;
use coevo_core::opc::*;
use sqlx::{Row, SqlitePool};

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
            let e = AgentEmployee {
                agent_id: row.get("agent_id"),
                display_name: row.get("display_name"),
                department: department_from_db(&dept),
                role: row.get("role"),
                passport: serde_json::from_str::<AgentPassport>(row.get("passport_json"))
                    .unwrap_or_else(|_| AgentPassport {
                        passport_id: String::new(),
                        issued_by: String::new(),
                        roles: vec![],
                        capabilities: vec![],
                        restrictions: vec![],
                        expires_at_ms: None,
                    }),
                model_profile: serde_json::from_str::<ModelProviderProfile>(
                    row.get("model_profile_json"),
                )
                .unwrap_or_else(|_| ModelProviderProfile {
                    provider: String::new(),
                    base_url: String::new(),
                    api_key_ref: String::new(),
                    default_model: String::new(),
                    fast_model: String::new(),
                    reasoning_model: String::new(),
                    structured_output_model: String::new(),
                    timeout_ms: 30000,
                    max_tokens: 4096,
                    max_cost_per_task_usd: 1.0,
                }),
                tool_scopes: serde_json::from_str(row.get("tool_scopes_json")).unwrap_or_default(),
                memory_scope: memory_scope_from_db(&scope),
                permission_boundary: serde_json::from_str::<PermissionBoundary>(
                    row.get("permission_boundary_json"),
                )
                .unwrap_or_else(|_| PermissionBoundary {
                    max_risk_score: 0.3,
                    can_write_fact: false,
                    can_write_decision: false,
                    can_access_network: false,
                    can_access_filesystem: false,
                    can_call_external_executor: false,
                    can_propose_skill: false,
                }),
                allowed_cognitive_layers: serde_json::from_str(
                    row.get("allowed_cognitive_layers_json"),
                )
                .unwrap_or_default(),
                allowed_action_modes: serde_json::from_str(row.get("allowed_action_modes_json"))
                    .unwrap_or_default(),
                risk_ceiling: row.get("risk_ceiling"),
                reputation_vector: serde_json::from_str(row.get("reputation_vector_json"))
                    .unwrap_or_else(|_| {
                        coevo_core::reputation::ReputationVector::new(String::new())
                    }),
                supervisor_agent_id: row.get("supervisor_agent_id"),
                lifecycle_status: lifecycle_status_from_db(&status),
                created_at_ms: row.get::<i64, _>("created_at_ms") as u64,
                updated_at_ms: row.get::<i64, _>("updated_at_ms") as u64,
            };
            result.push(e);
        }
        Ok(result)
    }
    pub async fn upsert(pool: &SqlitePool, a: &AgentEmployee) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO agent_employees VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&a.agent_id)
        .bind(&a.display_name)
        .bind(department_to_db(a.department))
        .bind(&a.role)
        .bind(serde_json::to_string(&a.passport).unwrap())
        .bind(serde_json::to_string(&a.model_profile).unwrap())
        .bind(serde_json::to_string(&a.tool_scopes).unwrap())
        .bind(memory_scope_to_db(a.memory_scope))
        .bind(serde_json::to_string(&a.permission_boundary).unwrap())
        .bind(serde_json::to_string(&a.allowed_cognitive_layers).unwrap())
        .bind(serde_json::to_string(&a.allowed_action_modes).unwrap())
        .bind(a.risk_ceiling)
        .bind(serde_json::to_string(&a.reputation_vector).unwrap())
        .bind(&a.supervisor_agent_id)
        .bind(lifecycle_status_to_db(a.lifecycle_status))
        .bind(a.created_at_ms as i64)
        .bind(a.updated_at_ms as i64)
        .execute(pool)
        .await?;
        Ok(())
    }
    pub async fn seed(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        for e in seed_employees() {
            Self::upsert(pool, &e).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AgentEmployeeRepo;
    use crate::repos_opc::{executor_repo::ExecutorRepo, skill_repo::SkillRepo};
    use crate::{migrate::run_migrations, pool::create_pool, pool::create_test_pool};
    use coevo_core::opc::{
        ExecutorSourceType, ExecutorStatus, ExternalExecutorPassport, MemoryScope,
        PermissionBoundary, SandboxLevel,
    };
    use sqlx::migrate::{Migration, MigrationType, Migrator};
    use std::borrow::Cow;

    #[tokio::test]
    async fn seed_employees_accepts_all_builtin_departments() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        AgentEmployeeRepo::seed(&pool).await.unwrap();
        let employees = AgentEmployeeRepo::list(&pool).await.unwrap();

        assert!(employees.iter().any(|e| e.agent_id == "agent-critic-01"));
        assert!(employees.iter().any(|e| e.agent_id == "agent-risk-01"));
        assert!(employees
            .iter()
            .any(|e| matches!(e.department, coevo_core::opc::Department::Governance)));
        assert!(employees.len() >= 10);
    }

    #[tokio::test]
    async fn first_run_quick_start_seed_chain_accepts_builtin_values() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        AgentEmployeeRepo::seed(&pool).await.unwrap();
        SkillRepo::seed_default(&pool).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let executor = ExternalExecutorPassport {
            executor_id: "mock-openclaw".to_string(),
            display_name: "Mock OpenClaw".to_string(),
            source_type: ExecutorSourceType::OpenClaw,
            runtime_endpoint: String::new(),
            capabilities: vec![],
            required_credentials: vec![],
            permission_boundary: PermissionBoundary {
                max_risk_score: 0.5,
                can_write_fact: false,
                can_write_decision: false,
                can_access_network: false,
                can_access_filesystem: false,
                can_call_external_executor: false,
                can_propose_skill: false,
            },
            file_scope: vec![],
            network_scope: vec![],
            memory_scope: MemoryScope::Executor,
            risk_ceiling: 0.5,
            supported_actions: vec!["read".to_string()],
            sandbox_level: SandboxLevel::None,
            health_check_url: String::new(),
            audit_callback_url: String::new(),
            status: ExecutorStatus::Registered,
            created_at_ms: now,
            updated_at_ms: now,
        };
        ExecutorRepo::register(&pool, &executor).await.unwrap();

        assert!(AgentEmployeeRepo::list(&pool).await.unwrap().len() >= 10);
        assert!(SkillRepo::list(&pool, None).await.unwrap().len() >= 5);
        assert_eq!(
            ExecutorRepo::get(&pool, "mock-openclaw")
                .await
                .unwrap()
                .unwrap()
                .status,
            ExecutorStatus::Registered
        );
    }

    #[tokio::test]
    async fn existing_databases_upgrade_employee_department_constraint() {
        let db_path = std::env::temp_dir().join(format!(
            "coevo-upgrade-{}.db",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let db_url = db_path.to_string_lossy().to_string();
        let pool = create_pool(&db_url).await.unwrap();

        let legacy_migrator = Migrator {
            migrations: Cow::Owned(vec![Migration::new(
                16,
                Cow::Borrowed("agent employees"),
                MigrationType::Simple,
                Cow::Borrowed(include_str!("../../migrations/016_agent_employees.sql")),
                false,
            )]),
            ..Migrator::DEFAULT
        };
        legacy_migrator.run(&pool).await.unwrap();

        let err = AgentEmployeeRepo::seed(&pool).await.unwrap_err();
        assert!(err.to_string().contains("CHECK constraint failed"));
        drop(pool);

        let pool = create_pool(&db_url).await.unwrap();
        run_migrations(&pool).await.unwrap();
        AgentEmployeeRepo::seed(&pool).await.unwrap();
        assert!(AgentEmployeeRepo::list(&pool)
            .await
            .unwrap()
            .iter()
            .any(|e| matches!(e.department, coevo_core::opc::Department::Governance)));

        drop(pool);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }
}
