//! Emergency Lease Manager — dual-sign verification and lease lifecycle.
//! Per coevo whitepaper Section 9.3.
//!
//! # Dual-sign payload format
//!
//! Granting an emergency lease requires two real Ed25519 signatures (RFC 8032,
//! lowercase hex over the platform key by default): one from the monitoring
//! system and one from the diagnostic agent. Both signatures cover the same
//! deterministic attestation bytes produced by [`lease_signing_payload`]:
//!
//! ```json
//! {
//!   "agent_id":    "<agent granted the lease>",
//!   "lease_scope": ["<urn>", ...],            // exactly as passed to `grant`
//!   "purpose":     "coevo:emergency-lease:v1",
//!   "role":        "monitoring" | "diagnostic"
//! }
//! ```
//!
//! The bytes are canonicalized with [`coevo_core::crypto::canonical_bytes`]
//! (object keys sorted at every level).
//!
//! The payload deliberately binds only what an **external** signer knows at
//! signing time — the agent being authorized, the URN scope of the emergency
//! operation, and that signer's `role`. It does **not** include the lease id
//! (generated inside [`LeaseManager::grant`]) nor the internal `contract_hash`
//! (computed by the Red track during compilation, after the monitoring system
//! and diagnostic agent have already vouched). The per-signer `role` ensures the
//! monitoring and diagnostic vouches cannot be swapped or replayed for the other
//! role.
//!
//! Callers (e.g. the coevo-worker Red track / the monitoring + diagnostic
//! parties) produce the two signatures with [`LeaseManager::sign_attestation`]
//! using the platform signing key, then pass the resulting hex strings to
//! [`LeaseManager::grant`].

use coevo_core::crypto;
use coevo_core::lease::{EmergencyLease, LeaseError};
use coevo_store::repos::lease_repo::LeaseRepo;

use sqlx::SqlitePool;

/// Role of a lease attestation signer. Bound into the signed payload so a
/// monitoring vouch cannot be replayed as a diagnostic vouch (or vice versa).
pub const LEASE_ROLE_MONITORING: &str = "monitoring";
/// Diagnostic-agent attestation role (top-10% reputation agent).
pub const LEASE_ROLE_DIAGNOSTIC: &str = "diagnostic";

/// Build the canonical attestation bytes that both lease signatures cover.
///
/// See the module-level docs for the exact JSON shape. `role` must be either
/// [`LEASE_ROLE_MONITORING`] or [`LEASE_ROLE_DIAGNOSTIC`].
pub fn lease_signing_payload(agent_id: &str, lease_scope: &[String], role: &str) -> Vec<u8> {
    let payload = serde_json::json!({
        "purpose": "coevo:emergency-lease:v1",
        "agent_id": agent_id,
        "lease_scope": lease_scope,
        "role": role,
    });
    crypto::canonical_bytes(&payload)
}

/// Lease Manager for emergency self-healing.
pub struct LeaseManager;

impl LeaseManager {
    /// Grant a new emergency lease after dual-sign verification.
    /// Requires both monitoring system signature and diagnostic agent signature.
    ///
    /// Both `monitoring_signature` and `diagnostic_signature` are real Ed25519
    /// signatures (lowercase hex) over the canonical attestation bytes from
    /// [`lease_signing_payload`] — the monitoring signature over the
    /// `"monitoring"` role payload, the diagnostic signature over the
    /// `"diagnostic"` role payload — verified against the platform public key.
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

        // Verify both signatures are genuine Ed25519 vouches over the
        // role-bound attestation payload (real crypto, not a string prefix).
        let public_key = crypto::platform_public_key_hex();

        let monitoring_bytes = lease_signing_payload(agent_id, &lease_scope, LEASE_ROLE_MONITORING);
        if !crypto::verify(&public_key, &monitoring_bytes, monitoring_signature) {
            return Err(LeaseError::OutOfScope {
                urn: "invalid monitoring signature".to_string(),
            });
        }

        let diagnostic_bytes = lease_signing_payload(agent_id, &lease_scope, LEASE_ROLE_DIAGNOSTIC);
        if !crypto::verify(&public_key, &diagnostic_bytes, diagnostic_signature) {
            return Err(LeaseError::OutOfScope {
                urn: "invalid diagnostic signature".to_string(),
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

    /// Produce a dual-sign attestation signature with the platform signing key.
    ///
    /// Convenience for callers (e.g. the Red track, or the monitoring /
    /// diagnostic parties) that hold the platform key and need to mint a valid
    /// monitoring or diagnostic vouch for [`LeaseManager::grant`]. `role` must
    /// be [`LEASE_ROLE_MONITORING`] or [`LEASE_ROLE_DIAGNOSTIC`]; pass the
    /// *same* `agent_id` and `lease_scope` you will pass to `grant`.
    pub fn sign_attestation(agent_id: &str, lease_scope: &[String], role: &str) -> String {
        crypto::sign(&lease_signing_payload(agent_id, lease_scope, role))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_store::pool::create_test_pool;

    async fn pool_with_contract(hash: &str) -> SqlitePool {
        let pool = create_test_pool().await.unwrap();
        coevo_store::migrate::run_migrations(&pool).await.unwrap();
        let contract = coevo_core::contract::MCLSpec {
            mcl_version: "1.0".to_string(),
            mcl_state: coevo_core::contract::ContractState::ActiveContract,
            parent_contract_hash: "0".repeat(64),
            goal_tree: coevo_core::contract::GoalTree {
                root: coevo_core::contract::GoalNode {
                    id: "root".to_string(),
                    description: "test".to_string(),
                    status: coevo_core::contract::GoalStatus::Pending,
                    children: vec![],
                    depends_on: vec![],
                },
            },
            institution_policy_hash: "0".repeat(64),
            data_boundary: vec![],
            allowed_action_modes: vec![coevo_core::contract::ActionMode::CommitReady],
            human_approval_policy: coevo_core::contract::HumanApprovalPolicy {
                approval_mode: coevo_core::contract::ApprovalMode::ExplicitApproval,
                authorized_roles: vec!["Admin".to_string()],
                negative_consent_timeout_secs: 0,
                mfa_auth_url: None,
            },
            evidence_requirement: coevo_core::contract::EvidenceRequirement {
                minimum_level: "unit_tests_passing".to_string(),
                require_json_report: true,
            },
            risk_tolerance_profile: coevo_core::contract::RiskToleranceProfile {
                max_risk_score: 0.8,
                allow_emergency_lease: true,
            },
            termination_policy: coevo_core::contract::TerminationPolicy {
                max_token_budget: 100000,
                max_hops: 6,
                max_latency_ms: 300000,
                max_stance_rounds: 3,
            },
            responsibility_anchor_policy: coevo_core::contract::ResponsibilityAnchorPolicy {
                required_human_roles: vec![],
                agent_forbidden_actions: vec![],
            },
        };
        coevo_store::repos::contract_repo::ContractRepo::insert(&pool, &contract, hash)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn grant_succeeds_with_real_dual_sign() {
        let pool = pool_with_contract("lease-contract-ok").await;
        let scope = vec!["urn:coevo:action:test".to_string()];
        let mon = LeaseManager::sign_attestation("agent-x", &scope, LEASE_ROLE_MONITORING);
        let diag = LeaseManager::sign_attestation("agent-x", &scope, LEASE_ROLE_DIAGNOSTIC);
        let lease =
            LeaseManager::grant(&pool, "lease-contract-ok", "agent-x", scope, 2, &mon, &diag)
                .await
                .expect("real dual-sign must be accepted");
        assert!(lease.is_active);
        assert_eq!(lease.lease_budget, 2);
    }

    #[tokio::test]
    async fn grant_rejects_garbage_signatures() {
        let pool = pool_with_contract("lease-contract-bad").await;
        let scope = vec!["urn:coevo:action:test".to_string()];
        let err = LeaseManager::grant(
            &pool,
            "lease-contract-bad",
            "agent-x",
            scope,
            2,
            "mon-sig:not-real",
            "diag-sig:not-real",
        )
        .await
        .expect_err("garbage signatures must be rejected");
        assert!(matches!(err, LeaseError::OutOfScope { .. }));
    }

    #[tokio::test]
    async fn grant_rejects_role_swapped_signatures() {
        let pool = pool_with_contract("lease-contract-swap").await;
        let scope = vec!["urn:coevo:action:test".to_string()];
        // Sign with the diagnostic role, then present it in the monitoring slot.
        let diag = LeaseManager::sign_attestation("agent-x", &scope, LEASE_ROLE_DIAGNOSTIC);
        let err = LeaseManager::grant(
            &pool,
            "lease-contract-swap",
            "agent-x",
            scope,
            2,
            &diag, // wrong role for the monitoring slot
            &diag,
        )
        .await
        .expect_err("role-swapped signature must be rejected");
        assert!(matches!(err, LeaseError::OutOfScope { .. }));
    }
}
