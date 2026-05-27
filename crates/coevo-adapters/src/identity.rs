//! Mock Identity adapter — simulates OIDC/mTLS authentication.

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use crate::traits::*;

pub struct MockIdentityProvider {
    known_agents: Vec<(String, Vec<String>)>, // (agent_id, roles)
}

impl MockIdentityProvider {
    pub fn new() -> Self {
        Self {
            known_agents: vec![
                ("agent-synthesizer-01".to_string(), vec!["Synthesizer".to_string()]),
                ("agent-critic-01".to_string(), vec!["Critic".to_string()]),
                ("agent-proposer-01".to_string(), vec!["Proposer".to_string()]),
                ("agent-diagnostic-01".to_string(), vec!["Diagnostic".to_string()]),
                ("agent-admin-01".to_string(), vec!["Admin".to_string(), "HumanApprover".to_string()]),
            ],
        }
    }

    pub fn with_agent(mut self, agent_id: &str, roles: Vec<String>) -> Self {
        self.known_agents.push((agent_id.to_string(), roles));
        self
    }
}

impl Default for MockIdentityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IdentityProvider for MockIdentityProvider {
    async fn verify_proof(&self, caller_identity_proof: &str) -> Result<IdentityClaims, AdapterError> {
        // Mock: parse proof as "agent:<agent_id>:<tenant_id>"
        // In real impl, this would verify Ed25519 signature against JWKS
        if caller_identity_proof.is_empty() {
            return Err(AdapterError::IdentityError("empty identity proof".to_string()));
        }

        // For mock, we accept "mock-signature:<agent_id>" format
        let agent_id = if let Some(stripped) = caller_identity_proof.strip_prefix("mock-signature:") {
            stripped.to_string()
        } else if caller_identity_proof.starts_with("agent:") {
            caller_identity_proof
                .split(':')
                .nth(1)
                .unwrap_or("unknown")
                .to_string()
        } else {
            // Default: use the proof itself as identity
            caller_identity_proof.to_string()
        };

        let entry = self
            .known_agents
            .iter()
            .find(|(id, _)| id == &agent_id)
            .ok_or_else(|| AdapterError::IdentityError(format!("unknown agent: {}", agent_id)))?;

        let mut hasher = Sha256::new();
        hasher.update(agent_id.as_bytes());
        let passport_hash = hex::encode(hasher.finalize());

        Ok(IdentityClaims {
            sub: agent_id.clone(),
            agent_id: agent_id.clone(),
            roles: entry.1.clone(),
            tenant_id: "coevo-default-tenant".to_string(),
            passport_hash,
        })
    }

    async fn verify_mfa(&self, token: &str, _user_id: &str) -> Result<bool, AdapterError> {
        // Mock MFA: accept "mfa-valid" or any non-empty token
        Ok(token == "mfa-valid" || token.len() > 5)
    }

    async fn issue_passport(&self, agent_id: &str, roles: Vec<String>) -> Result<String, AdapterError> {
        let mut hasher = Sha256::new();
        hasher.update(agent_id.as_bytes());
        hasher.update(serde_json::to_string(&roles).unwrap().as_bytes());
        Ok(format!("passport:{}", hex::encode(hasher.finalize())))
    }
}
