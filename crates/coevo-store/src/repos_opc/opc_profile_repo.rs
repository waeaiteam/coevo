use coevo_core::opc::OPCProfile;
use sqlx::{Row, SqlitePool};

pub struct OPCProfileRepo;
impl OPCProfileRepo {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> OPCProfile {
        OPCProfile {
            opc_id: row.get("opc_id"),
            founder_user_id: row.get("founder_user_id"),
            name: row.get("name"),
            mission: row.get("mission"),
            current_strategy: row.get("current_strategy"),
            operating_principles: serde_json::from_str(row.get("operating_principles_json"))
                .unwrap_or_default(),
            active_projects: serde_json::from_str(row.get("active_projects_json"))
                .unwrap_or_default(),
            asset_indexes: serde_json::from_str(row.get("asset_indexes_json")).unwrap_or_default(),
            policy_profile: row.get("policy_profile"),
            memory_policy: serde_json::from_str(row.get("memory_policy_json")).unwrap_or_else(
                |_| coevo_core::opc::MemoryPolicy {
                    fact_ttl_default_seconds: 3600,
                    require_provenance_for_fact: true,
                    auto_stale_days: 30,
                },
            ),
            default_departments: serde_json::from_str(row.get("default_departments_json"))
                .unwrap_or_default(),
            created_at_ms: row.get::<i64, _>("created_at_ms") as u64,
            updated_at_ms: row.get::<i64, _>("updated_at_ms") as u64,
        }
    }
    pub async fn get(pool: &SqlitePool, opc_id: &str) -> Result<Option<OPCProfile>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM opc_profiles WHERE opc_id=?")
            .bind(opc_id)
            .fetch_optional(pool)
            .await?;
        Ok(row.as_ref().map(|r| Self::from_row(r)))
    }
    pub async fn upsert(pool: &SqlitePool, p: &OPCProfile) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR REPLACE INTO opc_profiles VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&p.opc_id)
            .bind(&p.founder_user_id)
            .bind(&p.name)
            .bind(&p.mission)
            .bind(&p.current_strategy)
            .bind(serde_json::to_string(&p.operating_principles).unwrap())
            .bind(serde_json::to_string(&p.active_projects).unwrap())
            .bind(serde_json::to_string(&p.asset_indexes).unwrap())
            .bind(&p.policy_profile)
            .bind(serde_json::to_string(&p.memory_policy).unwrap())
            .bind(serde_json::to_string(&p.default_departments).unwrap())
            .bind(p.created_at_ms as i64)
            .bind(p.updated_at_ms as i64)
            .execute(pool)
            .await?;
        Ok(())
    }
}
