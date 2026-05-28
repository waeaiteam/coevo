//! Emergency Lease Manager — dual-sign verification and lease lifecycle.
//! Per coevo whitepaper Section 9.3.

use coevo_core::lease::{EmergencyLease, LeaseError};
use coevo_store::repos::lease_repo::LeaseRepo;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Lease Manager for emergency self-healing.
pub struct LeaseManager;

impl LeaseManager {
    /// Grant a new emergency lease after dual-sign verification.
    /// Requires both monitoring system signature and diagnostic agent signature.
    pub async fn grant(
        pool: &SqlitePool,
        contract_hash: &str,
        agent_id: &str,
        lease_scope: Vec<String>,
        lease_budget: u32,
        monitoring_signature: &str,
        diagnostic_signature: &str,
    ) -> Result<EmergencyLease, LeaseError> {
        // ---- Dual-sign validation ----
        if monitoring_signature.is_empty() {
            return Err(LeaseError::OutOfScope {
                urn: "missing monitoring signature".to_string(),
            });
        }
        if diagnostic_signature.is_empty() {
            return Err(LeaseError::OutOfScope {
                urn: "missing diagnostic signature".to_string(),
            });
        }

        // Verify monitoring signature format (mock: must start with "mon-sig:")
        if !monitoring_signature.starts_with("mon-sig:") {
            return Err(LeaseError::OutOfScope {
                urn: "invalid monitoring signature format".to_string(),
            });
        }

        // Verify diagnostic signature format (mock: must start with "diag-sig:")
        if !diagnostic_signature.starts_with("diag-sig:") {
            return Err(LeaseError::OutOfScope {
                urn: "invalid diagnostic signature format".to_string(),
            });
        }

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let ttl_ms = 15 * 60 * 1000; // 15 minutes
        let lease_id = format!("lease-{}", uuid::Uuid::new_v4());

        let lease = EmergencyLease {
            lease_id: lease_id.clone(),
            contract_hash: contract_hash.to_string(),
            agent_id: agent_id.to_string(),
            lease_scope,
            lease_budget,
            operations_used: 0,
            granted_at_ms: now,
            expires_at_ms: now + ttl_ms,
            ttl_ms,
            monitoring_signature: monitoring_signature.to_string(),
            diagnostic_signature: diagnostic_signature.to_string(),
            is_active: true,
            was_revoked: false,
        };

        LeaseRepo::insert(pool, &lease)
            .await
            .map_err(|_| LeaseError::OutOfScope {
                urn: "failed to persist lease".to_string(),
            })?;

        Ok(lease)
    }

    /// Check if a lease is valid and consume one operation.
    pub async fn try_consume(
        pool: &SqlitePool,
        lease_id: &str,
        action_urn: &str,
    ) -> Result<(), LeaseError> {
        let row = LeaseRepo::find_active(pool, lease_id)
            .await
            .map_err(|_| LeaseError::LeaseExpiredOrRevoked)?;

        let row = row.ok_or(LeaseError::LeaseExpiredOrRevoked)?;

        // Check TTL
        let now = chrono::Utc::now().timestamp_millis() as u64;
        if now >= row.expires_at_ms as u64 {
            LeaseRepo::revoke(pool, lease_id).await.ok();
            return Err(LeaseError::LeaseExpiredOrRevoked);
        }

        // Check scope
        let scope: Vec<String> = serde_json::from_str(&row.lease_scope_json).unwrap_or_default();
        let in_scope = scope.iter().any(|s| action_urn.starts_with(s.as_str()));
        if !in_scope {
            return Err(LeaseError::OutOfScope {
                urn: action_urn.to_string(),
            });
        }

        // Check budget
        if row.operations_used >= row.lease_budget {
            return Err(LeaseError::BudgetExhausted {
                used: row.operations_used as u32,
                budget: row.lease_budget as u32,
            });
        }

        // Consume operation
        LeaseRepo::consume_operation(pool, lease_id)
            .await
            .map_err(|_| LeaseError::LeaseExpiredOrRevoked)?;

        Ok(())
    }

    /// Revoke a lease immediately.
    pub async fn revoke(pool: &SqlitePool, lease_id: &str) -> Result<(), LeaseError> {
        LeaseRepo::revoke(pool, lease_id)
            .await
            .map_err(|_| LeaseError::LeaseExpiredOrRevoked)?;
        Ok(())
    }
}
