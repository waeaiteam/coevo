//! Emergency lease types.
//! Per coevo whitepaper Section 9.3.

use serde::{Deserialize, Serialize};

/// An emergency self-healing lease granted after dual-sign verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyLease {
    /// Unique lease identifier.
    pub lease_id: String,
    /// The contract this lease is bound to.
    pub contract_hash: String,
    /// The agent granted the lease.
    pub agent_id: String,
    /// URN scope of permitted writes.
    pub lease_scope: Vec<String>,
    /// Maximum number of operations allowed.
    pub lease_budget: u32,
    /// Operations consumed so far.
    pub operations_used: u32,
    /// When the lease was granted (Unix ms).
    pub granted_at_ms: u64,
    /// When the lease expires (Unix ms).
    pub expires_at_ms: u64,
    /// TTL in milliseconds.
    pub ttl_ms: u64,
    /// Signature from the monitoring system (Prometheus/Azure Monitor).
    pub monitoring_signature: String,
    /// Signature from the diagnostic agent (top 10% reputation).
    pub diagnostic_signature: String,
    /// Whether the lease is still active.
    pub is_active: bool,
    /// Whether the lease was revoked before expiry.
    pub was_revoked: bool,
}

impl EmergencyLease {
    /// Check if the lease is currently valid (not expired, not revoked, budget remaining).
    pub fn is_valid(&self) -> bool {
        if !self.is_active || self.was_revoked {
            return false;
        }
        let now = chrono::Utc::now().timestamp_millis() as u64;
        if now >= self.expires_at_ms {
            return false;
        }
        if self.operations_used >= self.lease_budget {
            return false;
        }
        true
    }

    /// Check if a given URN is within the lease scope.
    pub fn is_in_scope(&self, urn: &str) -> bool {
        self.lease_scope
            .iter()
            .any(|scope| urn.starts_with(scope.as_str()))
    }

    /// Consume one operation from the lease budget.
    pub fn consume_operation(&mut self) -> Result<(), LeaseError> {
        if !self.is_valid() {
            return Err(LeaseError::LeaseExpiredOrRevoked);
        }
        if self.operations_used >= self.lease_budget {
            return Err(LeaseError::BudgetExhausted {
                used: self.operations_used,
                budget: self.lease_budget,
            });
        }
        self.operations_used += 1;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    #[error("lease has expired or been revoked")]
    LeaseExpiredOrRevoked,
    #[error("lease budget exhausted ({used}/{budget})")]
    BudgetExhausted { used: u32, budget: u32 },
    #[error("URN '{urn}' is not within lease scope")]
    OutOfScope { urn: String },
}
