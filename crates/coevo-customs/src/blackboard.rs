//! Blackboard — shared state read/write with layer enforcement.
//! Per coevo whitepaper Section 8.

use coevo_core::cognitive::{BlackboardEntry, CognitiveLayer, ProvenanceEnvelope};
use coevo_store::repos::blackboard_repo::BlackboardRepo;
use sqlx::SqlitePool;

/// Blackboard operations.
pub struct Blackboard;

impl Blackboard {
    /// Read the latest valid entry for a key.
    pub async fn read(
        pool: &SqlitePool,
        key: &str,
    ) -> Result<Option<BlackboardEntry>, BlackboardError> {
        let row = BlackboardRepo::find_latest(pool, key).await?;
        match row {
            Some(r) => {
                let value: serde_json::Value =
                    serde_json::from_str(&r.value_json).unwrap_or(serde_json::Value::Null);
                let layer: CognitiveLayer =
                    serde_json::from_str(&format!("\"{}\"", r.cognitive_layer))
                        .unwrap_or(CognitiveLayer::Hypothesis);
                Ok(Some(BlackboardEntry {
                    key: r.entry_key,
                    version: r.version as u64,
                    value,
                    layer,
                    provenance: ProvenanceEnvelope {
                        source_agent_id: r.source_agent_id,
                        verification_tool_urn: String::new(),
                        environmental_scope: coevo_core::cognitive::EnvironmentalScope {
                            environment: coevo_core::cognitive::Environment::Development,
                            tenant_id: String::new(),
                        },
                        ttl_seconds: 3600,
                        cryptographic_signature: String::new(),
                        verification_report: None,
                        created_at: chrono::Utc::now(),
                    },
                    is_valid: r.is_valid != 0,
                    created_at_ms: r.created_at_ms as u64,
                    expires_at_ms: r.expires_at_ms.map(|t| t as u64),
                }))
            }
            None => Ok(None),
        }
    }

    /// Write to the blackboard. Returns the new entry ID and version.
    pub async fn write(
        pool: &SqlitePool,
        key: &str,
        value: &serde_json::Value,
        layer: CognitiveLayer,
        source_agent_id: &str,
        contract_hash: &str,
        ttl_ms: Option<i64>,
    ) -> Result<(String, u64), BlackboardError> {
        let entry_id = BlackboardRepo::insert(
            pool,
            key,
            &serde_json::to_string(value).unwrap(),
            serde_json::to_string(&layer).unwrap().trim_matches('"'),
            source_agent_id,
            contract_hash,
            ttl_ms,
        )
        .await?;

        let row = BlackboardRepo::find_by_id(pool, &entry_id).await?;
        let version = row.map(|r| r.version as u64).unwrap_or(1);

        Ok((entry_id, version))
    }

    /// Invalidate a blackboard entry (mark as no longer valid).
    pub async fn invalidate(pool: &SqlitePool, entry_id: &str) -> Result<(), BlackboardError> {
        BlackboardRepo::invalidate(pool, entry_id).await?;
        Ok(())
    }

    /// Update the cognitive layer of an entry (e.g., Hypothesis→Fact promotion).
    pub async fn update_layer(
        pool: &SqlitePool,
        entry_id: &str,
        layer: CognitiveLayer,
    ) -> Result<(), BlackboardError> {
        let layer_str = serde_json::to_string(&layer).unwrap();
        BlackboardRepo::update_layer(pool, entry_id, layer_str.trim_matches('"')).await?;
        Ok(())
    }

    /// Expire facts that have exceeded their TTL.
    pub async fn expire_stale_facts(pool: &SqlitePool) -> Result<Vec<String>, BlackboardError> {
        let rows = BlackboardRepo::expire_stale_facts(pool).await?;
        Ok(rows.into_iter().map(|r| r.id).collect())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlackboardError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
