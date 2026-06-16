//! Blackboard — shared state read/write with layer enforcement.
//! Per coevo whitepaper Section 8.
//!
//! Provenance envelopes are persisted to and loaded from the
//! `provenance_envelopes` table (migration 005). The blackboard no longer
//! fabricates empty-signature / now()-timestamp envelopes on read; it returns
//! the real envelope that was committed alongside the entry.

use std::str::FromStr;

use coevo_core::cognitive::{
    BlackboardEntry, CognitiveLayer, Environment, EnvironmentalScope, ProvenanceEnvelope,
};
use coevo_store::repos::blackboard_repo::BlackboardRepo;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Blackboard operations.
pub struct Blackboard;

impl Blackboard {
    /// Read the latest valid entry for a key, including its persisted
    /// provenance envelope.
    pub async fn read(
        pool: &SqlitePool,
        key: &str,
    ) -> Result<Option<BlackboardEntry>, BlackboardError> {
        let row = BlackboardRepo::find_latest(pool, key).await?;
        match row {
            Some(r) => {
                let value: serde_json::Value =
                    serde_json::from_str(&r.value_json).unwrap_or(serde_json::Value::Null);
                // Real FromStr parse — unknown layers are an error, never a
                // silent Hypothesis fallback.
                let layer = CognitiveLayer::from_str(&r.cognitive_layer)
                    .map_err(|e| BlackboardError::UnknownLayer(e.0))?;

                // Load the real provenance envelope for this entry id.
                let provenance = load_provenance(pool, &r.id)
                    .await?
                    .unwrap_or_else(|| fallback_provenance(&r.source_agent_id));

                Ok(Some(BlackboardEntry {
                    key: r.entry_key,
                    version: r.version as u64,
                    value,
                    layer,
                    provenance,
                    is_valid: r.is_valid != 0,
                    created_at_ms: r.created_at_ms as u64,
                    expires_at_ms: r.expires_at_ms.map(|t| t as u64),
                }))
            }
            None => Ok(None),
        }
    }

    /// Write to the blackboard, persisting the provenance envelope alongside the
    /// entry. Returns the new entry ID and version.
    pub async fn write(
        pool: &SqlitePool,
        key: &str,
        value: &serde_json::Value,
        layer: CognitiveLayer,
        provenance: &ProvenanceEnvelope,
        contract_hash: &str,
        ttl_ms: Option<i64>,
    ) -> Result<(String, u64), BlackboardError> {
        let entry_id = BlackboardRepo::insert(
            pool,
            key,
            &serde_json::to_string(value).unwrap(),
            layer.as_db_str(),
            &provenance.source_agent_id,
            contract_hash,
            ttl_ms,
        )
        .await?;

        // Persist the provenance envelope keyed to this entry.
        insert_provenance(pool, &entry_id, provenance).await?;

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
        BlackboardRepo::update_layer(pool, entry_id, layer.as_db_str()).await?;
        Ok(())
    }

    /// Expire facts that have exceeded their TTL.
    pub async fn expire_stale_facts(pool: &SqlitePool) -> Result<Vec<String>, BlackboardError> {
        let rows = BlackboardRepo::expire_stale_facts(pool).await?;
        Ok(rows.into_iter().map(|r| r.id).collect())
    }
}

/// Insert a provenance envelope row for `entry_id`.
///
/// Implemented with raw `sqlx` against the `provenance_envelopes` table
/// (migration 005). coevo-customs owns this read/write path; coevo-store does
/// not yet expose a provenance repo (see final report).
async fn insert_provenance(
    pool: &SqlitePool,
    entry_id: &str,
    provenance: &ProvenanceEnvelope,
) -> Result<(), BlackboardError> {
    let id = Uuid::new_v4().to_string();
    let scope_json =
        serde_json::to_string(&provenance.environmental_scope).map_err(BlackboardError::Serde)?;
    let report_json = match &provenance.verification_report {
        Some(v) => Some(serde_json::to_string(v).map_err(BlackboardError::Serde)?),
        None => None,
    };
    sqlx::query(
        "INSERT INTO provenance_envelopes (id, entry_id, source_agent_id, verification_tool_urn, environmental_scope_json, ttl_seconds, cryptographic_signature, verification_report_json, created_at) VALUES (?,?,?,?,?,?,?,?,?)"
    )
    .bind(&id)
    .bind(entry_id)
    .bind(&provenance.source_agent_id)
    .bind(&provenance.verification_tool_urn)
    .bind(&scope_json)
    .bind(provenance.ttl_seconds)
    .bind(&provenance.cryptographic_signature)
    .bind(&report_json)
    .bind(provenance.created_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Load the most recent provenance envelope persisted for `entry_id`.
async fn load_provenance(
    pool: &SqlitePool,
    entry_id: &str,
) -> Result<Option<ProvenanceEnvelope>, BlackboardError> {
    let row: Option<coevo_store::models::ProvenanceEnvelopeRow> =
        sqlx::query_as::<_, coevo_store::models::ProvenanceEnvelopeRow>(
            "SELECT * FROM provenance_envelopes WHERE entry_id = ? ORDER BY rowid DESC LIMIT 1",
        )
        .bind(entry_id)
        .fetch_optional(pool)
        .await?;

    match row {
        None => Ok(None),
        Some(r) => {
            let environmental_scope: EnvironmentalScope =
                serde_json::from_str(&r.environmental_scope_json)
                    .map_err(BlackboardError::Serde)?;
            let verification_report = match r.verification_report_json {
                Some(ref s) => Some(serde_json::from_str(s).map_err(BlackboardError::Serde)?),
                None => None,
            };
            let created_at = chrono::DateTime::parse_from_rfc3339(&r.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            Ok(Some(ProvenanceEnvelope {
                source_agent_id: r.source_agent_id,
                verification_tool_urn: r.verification_tool_urn,
                environmental_scope,
                ttl_seconds: r.ttl_seconds,
                cryptographic_signature: r.cryptographic_signature,
                verification_report,
                created_at,
            }))
        }
    }
}

/// Minimal placeholder envelope for legacy entries that predate provenance
/// persistence (no row in `provenance_envelopes`). Distinguished from a real
/// envelope by an explicit "unverified-legacy" signature marker rather than an
/// empty string, so callers can tell it is not cryptographically backed.
fn fallback_provenance(source_agent_id: &str) -> ProvenanceEnvelope {
    ProvenanceEnvelope {
        source_agent_id: source_agent_id.to_string(),
        verification_tool_urn: String::new(),
        environmental_scope: EnvironmentalScope {
            environment: Environment::Development,
            tenant_id: String::new(),
        },
        ttl_seconds: 0,
        cryptographic_signature: "unverified-legacy".to_string(),
        verification_report: None,
        created_at: chrono::Utc::now(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlackboardError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("unknown cognitive layer stored in blackboard: {0:?}")]
    UnknownLayer(String),
    #[error("serialization error: {0}")]
    Serde(#[source] serde_json::Error),
}
