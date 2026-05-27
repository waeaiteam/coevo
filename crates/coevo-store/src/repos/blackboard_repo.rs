use crate::models::BlackboardEntryRow;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct BlackboardRepo;

impl BlackboardRepo {
    pub async fn insert(
        pool: &SqlitePool,
        key: &str,
        value_json: &str,
        layer: &str,
        source_agent_id: &str,
        contract_hash: &str,
        ttl_ms: Option<i64>,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = ttl_ms.map(|t| now + t);
        // Get current max version for this key
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT MAX(version) FROM blackboard_entries WHERE entry_key = ?"
        )
        .bind(key)
        .fetch_optional(pool)
        .await?;
        let version = row.map(|(v,)| v + 1).unwrap_or(1);
        sqlx::query(
            "INSERT INTO blackboard_entries (id, entry_key, version, value_json, cognitive_layer, source_agent_id, contract_hash, is_valid, created_at_ms, expires_at_ms) VALUES (?,?,?,?,?,?,?,1,?,?)"
        )
        .bind(&id)
        .bind(key)
        .bind(version)
        .bind(value_json)
        .bind(layer)
        .bind(source_agent_id)
        .bind(contract_hash)
        .bind(now)
        .bind(expires_at)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn find_latest(pool: &SqlitePool, key: &str) -> Result<Option<BlackboardEntryRow>, sqlx::Error> {
        sqlx::query_as::<_, BlackboardEntryRow>(
            "SELECT * FROM blackboard_entries WHERE entry_key = ? AND is_valid = 1 ORDER BY version DESC LIMIT 1"
        )
        .bind(key)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: &str) -> Result<Option<BlackboardEntryRow>, sqlx::Error> {
        sqlx::query_as::<_, BlackboardEntryRow>("SELECT * FROM blackboard_entries WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn invalidate(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE blackboard_entries SET is_valid = 0 WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn update_layer(pool: &SqlitePool, id: &str, layer: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE blackboard_entries SET cognitive_layer = ? WHERE id = ?")
            .bind(layer)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn expire_stale_facts(pool: &SqlitePool) -> Result<Vec<BlackboardEntryRow>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let rows = sqlx::query_as::<_, BlackboardEntryRow>(
            "SELECT * FROM blackboard_entries WHERE cognitive_layer = 'Fact' AND is_valid = 1 AND expires_at_ms IS NOT NULL AND expires_at_ms < ?"
        )
        .bind(now)
        .fetch_all(pool)
        .await?;
        for row in &rows {
            sqlx::query("UPDATE blackboard_entries SET cognitive_layer = 'StaleFact' WHERE id = ?")
                .bind(&row.id)
                .execute(pool)
                .await?;
        }
        Ok(rows)
    }
}
