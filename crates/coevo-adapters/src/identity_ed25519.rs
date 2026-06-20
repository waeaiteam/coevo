//! Real Ed25519 identity provider.
//!
//! Unlike [`MockIdentityProvider`](crate::identity::MockIdentityProvider), which accepts
//! `mock-signature:<agent_id>` by string prefix, this provider performs an actual Ed25519
//! signature verification against a registered per-agent public key, using the shared
//! [`coevo_core::crypto`] primitives (the same ones the Red-track lease double-signature
//! path already relies on).
//!
//! Proof format: `ed25519:<agent_id>:<signature_hex>`, where the signature is over the
//! UTF-8 bytes of the canonical challenge `coevo:identity:<agent_id>`. An agent proves it
//! holds the private key matching the public key registered for `agent_id`.
//!
//! Key source is a local in-memory registry (agent_id -> public key hex + roles). This is
//! the no-external-dependency path; a JWKS-backed variant would swap the registry lookup
//! for a remote key fetch without changing the verification logic.

use crate::traits::*;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Build the canonical challenge bytes an agent must sign to prove its identity.
pub fn identity_challenge(agent_id: &str) -> Vec<u8> {
    format!("coevo:identity:{agent_id}").into_bytes()
}

struct AgentKey {
    public_key_hex: String,
    roles: Vec<String>,
    tenant_id: String,
}

pub struct Ed25519IdentityProvider {
    agents: HashMap<String, AgentKey>,
}

impl Ed25519IdentityProvider {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Register an agent's Ed25519 public key (hex) and roles.
    pub fn with_agent(
        mut self,
        agent_id: impl Into<String>,
        public_key_hex: impl Into<String>,
        roles: Vec<String>,
        tenant_id: impl Into<String>,
    ) -> Self {
        self.agents.insert(
            agent_id.into(),
            AgentKey {
                public_key_hex: public_key_hex.into(),
                roles,
                tenant_id: tenant_id.into(),
            },
        );
        self
    }

    /// Number of registered agents (used by health/diagnostics).
    pub fn registered_count(&self) -> usize {
        self.agents.len()
    }

    fn parse_proof<'a>(proof: &'a str) -> Result<(&'a str, &'a str), AdapterError> {
        // ed25519:<agent_id>:<signature_hex>
        let rest = proof.strip_prefix("ed25519:").ok_or_else(|| {
            AdapterError::IdentityError(
                "identity proof must use the form ed25519:<agent_id>:<signature_hex>".into(),
            )
        })?;
        let (agent_id, signature_hex) = rest.split_once(':').ok_or_else(|| {
            AdapterError::IdentityError("identity proof missing signature segment".into())
        })?;
        if agent_id.is_empty() || signature_hex.is_empty() {
            return Err(AdapterError::IdentityError(
                "identity proof has an empty agent id or signature".into(),
            ));
        }
        Ok((agent_id, signature_hex))
    }
}

impl Default for Ed25519IdentityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IdentityProvider for Ed25519IdentityProvider {
    async fn verify_proof(
        &self,
        caller_identity_proof: &str,
    ) -> Result<IdentityClaims, AdapterError> {
        if caller_identity_proof.is_empty() {
            return Err(AdapterError::IdentityError("empty identity proof".into()));
        }
        let (agent_id, signature_hex) = Self::parse_proof(caller_identity_proof)?;
        let entry = self
            .agents
            .get(agent_id)
            .ok_or_else(|| AdapterError::IdentityError(format!("unknown agent: {agent_id}")))?;

        let challenge = identity_challenge(agent_id);
        // Real Ed25519 verification (fail-closed on any decode/length/verify failure).
        if !coevo_core::crypto::verify(&entry.public_key_hex, &challenge, signature_hex) {
            return Err(AdapterError::IdentityError(format!(
                "Ed25519 signature verification failed for agent {agent_id}"
            )));
        }

        let mut hasher = Sha256::new();
        hasher.update(agent_id.as_bytes());
        hasher.update(entry.public_key_hex.as_bytes());
        let passport_hash = hex::encode(hasher.finalize());

        Ok(IdentityClaims {
            sub: agent_id.to_string(),
            agent_id: agent_id.to_string(),
            roles: entry.roles.clone(),
            tenant_id: entry.tenant_id.clone(),
            passport_hash,
        })
    }

    async fn verify_mfa(&self, token: &str, user_id: &str) -> Result<bool, AdapterError> {
        // MFA secrets are not part of the local key registry; a real TOTP/WebAuthn check
        // requires a per-user secret store, which this provider does not own. Treat MFA as
        // an explicit Ed25519-signed assertion over `mfa:<user_id>` from the same key, so
        // it is still a real signature check rather than a string compare.
        let signature_hex = token.strip_prefix("ed25519:").ok_or_else(|| {
            AdapterError::IdentityError("MFA token must be an ed25519:<signature_hex>".into())
        })?;
        let entry = self
            .agents
            .get(user_id)
            .ok_or_else(|| AdapterError::IdentityError(format!("unknown user: {user_id}")))?;
        let challenge = format!("coevo:mfa:{user_id}").into_bytes();
        Ok(coevo_core::crypto::verify(
            &entry.public_key_hex,
            &challenge,
            signature_hex,
        ))
    }

    async fn issue_passport(
        &self,
        agent_id: &str,
        roles: Vec<String>,
    ) -> Result<String, AdapterError> {
        let mut hasher = Sha256::new();
        hasher.update(agent_id.as_bytes());
        hasher.update(serde_json::to_string(&roles).unwrap_or_default().as_bytes());
        Ok(format!("passport:{}", hex::encode(hasher.finalize())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_core::crypto;

    // Sign the identity challenge with the platform key and register the platform's own
    // public key for the agent — exercises the full real verification path end to end.
    fn signed_proof(agent_id: &str) -> (String, String) {
        let public_key = crypto::platform_public_key_hex();
        let signature = crypto::sign(&identity_challenge(agent_id));
        (public_key, format!("ed25519:{agent_id}:{signature}"))
    }

    #[tokio::test]
    async fn accepts_a_valid_ed25519_proof() {
        let (pk, proof) = signed_proof("agent-real-01");
        let provider = Ed25519IdentityProvider::new().with_agent(
            "agent-real-01",
            pk,
            vec!["Proposer".into()],
            "tenant-a",
        );
        let claims = provider.verify_proof(&proof).await.unwrap();
        assert_eq!(claims.agent_id, "agent-real-01");
        assert_eq!(claims.roles, vec!["Proposer".to_string()]);
        assert_eq!(claims.tenant_id, "tenant-a");
    }

    #[tokio::test]
    async fn rejects_a_tampered_signature() {
        let (pk, proof) = signed_proof("agent-real-01");
        let provider = Ed25519IdentityProvider::new().with_agent(
            "agent-real-01",
            pk,
            vec!["Proposer".into()],
            "tenant-a",
        );
        // Flip the last hex char of the signature.
        let mut chars: Vec<char> = proof.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == '0' { '1' } else { '0' };
        let tampered: String = chars.into_iter().collect();
        assert!(provider.verify_proof(&tampered).await.is_err());
    }

    #[tokio::test]
    async fn rejects_unknown_agent() {
        let (_pk, proof) = signed_proof("agent-real-01");
        let provider = Ed25519IdentityProvider::new(); // empty registry
        assert!(provider.verify_proof(&proof).await.is_err());
    }

    #[tokio::test]
    async fn rejects_empty_and_malformed_proofs() {
        let provider = Ed25519IdentityProvider::new();
        assert!(provider.verify_proof("").await.is_err());
        assert!(provider.verify_proof("not-a-proof").await.is_err());
        assert!(provider.verify_proof("ed25519:agent-only").await.is_err());
        assert!(provider.verify_proof("ed25519::sig").await.is_err());
    }

    #[tokio::test]
    async fn rejects_proof_signed_for_a_different_agent() {
        // Signature is over the challenge for agent-A, but presented as agent-B.
        let public_key = crypto::platform_public_key_hex();
        let signature = crypto::sign(&identity_challenge("agent-A"));
        let provider = Ed25519IdentityProvider::new().with_agent(
            "agent-B",
            public_key,
            vec!["Critic".into()],
            "tenant-a",
        );
        let proof = format!("ed25519:agent-B:{signature}");
        assert!(provider.verify_proof(&proof).await.is_err());
    }
}
