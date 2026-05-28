use sqlx::SqlitePool;
use coevo_core::opc::MemoryRecord;
use coevo_core::cognitive::CognitiveLayer;

pub struct MemoryRepo;
impl MemoryRepo {
    pub async fn list(pool: &SqlitePool, scope: Option<&str>, owner_id: Option<&str>, include_revoked: bool) -> Result<Vec<MemoryRecord>, sqlx::Error> {
        let mut q = "SELECT * FROM memory_records WHERE 1=1".to_string();
        if let Some(s) = scope { q.push_str(&format!(" AND scope='{}'", s)); }
        if let Some(o) = owner_id { q.push_str(&format!(" AND owner_id='{}'", o)); }
        if !include_revoked { q.push_str(" AND status != 'Revoked'"); }
        q.push_str(" ORDER BY created_at_ms DESC LIMIT 100");
        let rows: Vec<(String,String,String,String,String,String,String,String,String,f64,i64,i64,i64,String,String,String,String,String,String)> = sqlx::query_as(&q).fetch_all(pool).await?;
        Ok(rows.into_iter().map(|r| MemoryRecord{
            memory_id:r.0,scope:serde_json::from_str(&format!("\"{}\"",r.1)).unwrap(),owner_id:r.2,title:r.3,content:r.4,
            tags:serde_json::from_str(&r.5).unwrap_or_default(),source:r.6,provenance:r.7,confidence:r.8,
            ttl_seconds:r.9,created_at_ms:r.10 as u64,updated_at_ms:r.11 as u64,access_policy:r.12,
            status:serde_json::from_str(&format!("\"{}\"",r.13)).unwrap(),
            cognitive_layer:serde_json::from_str(&format!("\"{}\"",r.14)).unwrap(),
            linked_contract_hash:r.15,linked_plan_hash:r.16,linked_adr_id:r.17,
        }).collect())
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
        let rows: Vec<(String,String,String,String,String,String,String,String,String,f64,i64,i64,i64,String,String,String,String,String,String)> = sqlx::query_as(&q).bind(&pattern).bind(&pattern).fetch_all(pool).await?;
        Ok(rows.into_iter().map(|r| MemoryRecord{
            memory_id:r.0,scope:serde_json::from_str(&format!("\"{}\"",r.1)).unwrap(),owner_id:r.2,title:r.3,content:r.4,
            tags:serde_json::from_str(&r.5).unwrap_or_default(),source:r.6,provenance:r.7,confidence:r.8,
            ttl_seconds:r.9,created_at_ms:r.10 as u64,updated_at_ms:r.11 as u64,access_policy:r.12,
            status:serde_json::from_str(&format!("\"{}\"",r.13)).unwrap(),
            cognitive_layer:serde_json::from_str(&format!("\"{}\"",r.14)).unwrap(),
            linked_contract_hash:r.15,linked_plan_hash:r.16,linked_adr_id:r.17,
        }).collect())
    }
}
