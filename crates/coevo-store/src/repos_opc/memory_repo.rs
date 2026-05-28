use sqlx::{SqlitePool, Row};
use coevo_core::opc::*;
use coevo_core::cognitive::CognitiveLayer;

pub struct MemoryRepo;
impl MemoryRepo {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> MemoryRecord {
        let scope_s: String = row.get("scope");
        let status_s: String = row.get("status");
        let layer_s: String = row.get("cognitive_layer");
        MemoryRecord{
            memory_id: row.get("memory_id"),
            scope: serde_json::from_str(&format!("\"{}\"",scope_s)).unwrap_or(MemoryScope::Task),
            owner_id: row.get("owner_id"),
            title: row.get("title"),
            content: row.get("content"),
            tags: serde_json::from_str(row.get("tags_json")).unwrap_or_default(),
            source: row.get("source"),
            provenance: row.get("provenance"),
            confidence: row.get("confidence"),
            ttl_seconds: row.get("ttl_seconds"),
            created_at_ms: row.get::<i64,_>("created_at_ms") as u64,
            updated_at_ms: row.get::<i64,_>("updated_at_ms") as u64,
            access_policy: row.get("access_policy"),
            status: serde_json::from_str(&format!("\"{}\"",status_s)).unwrap_or(MemoryStatus::Active),
            cognitive_layer: serde_json::from_str(&format!("\"{}\"",layer_s)).unwrap_or(CognitiveLayer::Hypothesis),
            linked_contract_hash: row.get("linked_contract_hash"),
            linked_plan_hash: row.get("linked_plan_hash"),
            linked_adr_id: row.get("linked_adr_id"),
        }
    }

    pub async fn list(pool: &SqlitePool, scope: Option<&str>, owner_id: Option<&str>, include_revoked: bool) -> Result<Vec<MemoryRecord>, sqlx::Error> {
        let mut q = "SELECT * FROM memory_records WHERE 1=1".to_string();
        if let Some(s) = scope { q.push_str(&format!(" AND scope='{}'", s)); }
        if let Some(o) = owner_id { q.push_str(&format!(" AND owner_id='{}'", o)); }
        if !include_revoked { q.push_str(" AND status != 'Revoked'"); }
        q.push_str(" ORDER BY created_at_ms DESC LIMIT 100");
        let rows = sqlx::query(&q).fetch_all(pool).await?;
        Ok(rows.iter().map(|r| Self::from_row(r)).collect())
    }

    pub async fn create(pool: &SqlitePool, m: &MemoryRecord) -> Result<(), sqlx::Error> {
        if m.cognitive_layer == CognitiveLayer::Fact && m.provenance.is_empty() {
            return Err(sqlx::Error::Protocol("Fact memory requires provenance".to_string()));
        }
        sqlx::query("INSERT INTO memory_records VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&m.memory_id).bind(serde_json::to_string(&m.scope).unwrap().trim_matches('"')).bind(&m.owner_id)
            .bind(&m.title).bind(&m.content).bind(serde_json::to_string(&m.tags).unwrap())
            .bind(&m.source).bind(&m.provenance).bind(m.confidence).bind(m.ttl_seconds)
            .bind(m.created_at_ms as i64).bind(m.updated_at_ms as i64).bind(&m.access_policy)
            .bind(serde_json::to_string(&m.status).unwrap().trim_matches('"'))
            .bind(serde_json::to_string(&m.cognitive_layer).unwrap().trim_matches('"'))
            .bind(&m.linked_contract_hash).bind(&m.linked_plan_hash).bind(&m.linked_adr_id)
            .execute(pool).await?;
        Ok(())
    }

    pub async fn mark_stale(pool: &SqlitePool, mid: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE memory_records SET status='Stale',updated_at_ms=? WHERE memory_id=?").bind(chrono::Utc::now().timestamp_millis()).bind(mid).execute(pool).await?; Ok(())
    }
    pub async fn revoke(pool: &SqlitePool, mid: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE memory_records SET status='Revoked',updated_at_ms=? WHERE memory_id=?").bind(chrono::Utc::now().timestamp_millis()).bind(mid).execute(pool).await?; Ok(())
    }
    pub async fn search(pool: &SqlitePool, query: &str, scope: Option<&str>, owner_id: Option<&str>) -> Result<Vec<MemoryRecord>, sqlx::Error> {
        let mut q = "SELECT * FROM memory_records WHERE (title LIKE ? OR content LIKE ?) AND status != 'Revoked'".to_string();
        if let Some(s) = scope { q.push_str(&format!(" AND scope='{}'", s)); }
        if let Some(o) = owner_id { q.push_str(&format!(" AND owner_id='{}'", o)); }
        q.push_str(" ORDER BY created_at_ms DESC LIMIT 50");
        let pattern = format!("%{}%", query);
        let rows = sqlx::query(&q).bind(&pattern).bind(&pattern).fetch_all(pool).await?;
        Ok(rows.iter().map(|r| Self::from_row(r)).collect())
    }
}
