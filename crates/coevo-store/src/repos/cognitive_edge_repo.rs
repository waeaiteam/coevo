use crate::models::CognitiveEdgeRow;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct CognitiveEdgeRepo;

impl CognitiveEdgeRepo {
    pub async fn insert(
        pool: &SqlitePool,
        source_entry_id: &str,
        target_entry_id: &str,
        edge_type: &str,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO cognitive_edges (id, source_entry_id, target_entry_id, edge_type, created_at_ms) VALUES (?,?,?,?,?)"
        )
        .bind(&id)
        .bind(source_entry_id)
        .bind(target_entry_id)
        .bind(edge_type)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(id)
    }

    /// Get all downstream entries that depend on the given entry (target_entry_id is the dependency).
    pub async fn find_dependents(
        pool: &SqlitePool,
        entry_id: &str,
    ) -> Result<Vec<CognitiveEdgeRow>, sqlx::Error> {
        sqlx::query_as::<_, CognitiveEdgeRow>(
            "SELECT * FROM cognitive_edges WHERE target_entry_id = ?",
        )
        .bind(entry_id)
        .fetch_all(pool)
        .await
    }

    /// Get all upstream entries that the given entry depends on.
    pub async fn find_dependencies(
        pool: &SqlitePool,
        entry_id: &str,
    ) -> Result<Vec<CognitiveEdgeRow>, sqlx::Error> {
        sqlx::query_as::<_, CognitiveEdgeRow>(
            "SELECT * FROM cognitive_edges WHERE source_entry_id = ?",
        )
        .bind(entry_id)
        .fetch_all(pool)
        .await
    }
}
