//! A2A adapter backed by an in-memory agent registry.

use crate::traits::*;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

pub struct MockA2aAdapter {
    registered_agents: Mutex<Vec<String>>,
}

impl MockA2aAdapter {
    pub fn new() -> Self {
        Self {
            registered_agents: Mutex::new(vec![
                "agent-synthesizer-01".to_string(),
                "agent-critic-01".to_string(),
                "agent-proposer-01".to_string(),
                "agent-diagnostic-01".to_string(),
            ]),
        }
    }

    pub fn with_agents(self, agents: Vec<String>) -> Self {
        *self.registered_agents.lock().unwrap() = agents;
        self
    }
}

impl Default for MockA2aAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl A2aProvider for MockA2aAdapter {
    async fn send_message(&self, msg: A2aMessage) -> Result<A2aResponse, AdapterError> {
        let A2aMessage {
            from_agent,
            to_agent,
            payload,
            traceparent,
            contract_hash,
        } = msg;

        if !self.registered_agents.lock().unwrap().contains(&to_agent) {
            return Err(AdapterError::A2aError(format!(
                "target agent '{}' not found",
                to_agent
            )));
        }

        let mut hasher = Sha256::new();
        hasher.update(from_agent.as_bytes());
        hasher.update(to_agent.as_bytes());
        hasher.update(serde_json::to_string(&payload).unwrap().as_bytes());
        hasher.update(traceparent.as_bytes());
        hasher.update(contract_hash.as_bytes());
        let delivery_id = hex::encode(hasher.finalize());

        Ok(A2aResponse {
            from_agent: to_agent.clone(),
            payload: serde_json::json!({
                "delivery_id": delivery_id,
                "from_agent": from_agent,
                "to_agent": to_agent,
                "contract_hash": contract_hash,
                "traceparent": traceparent,
                "status": "delivered",
                "payload": payload
            }),
            success: true,
        })
    }

    async fn discover_agents(&self) -> Result<Vec<String>, AdapterError> {
        Ok(self.registered_agents.lock().unwrap().clone())
    }

    async fn health_check(&self) -> Result<bool, AdapterError> {
        Ok(true)
    }

    fn register(&self, agent_id: &str) {
        let mut guard = self.registered_agents.lock().unwrap();
        if !guard.iter().any(|a| a == agent_id) {
            guard.push(agent_id.to_string());
        }
    }

    fn unregister(&self, agent_id: &str) {
        let mut guard = self.registered_agents.lock().unwrap();
        guard.retain(|registered| registered != agent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn discover_agents_reflects_configured_registry() {
        let adapter =
            MockA2aAdapter::new().with_agents(vec!["agent-alpha".into(), "agent-beta".into()]);

        assert_eq!(
            adapter.discover_agents().await.unwrap(),
            vec!["agent-alpha", "agent-beta"]
        );
    }

    #[tokio::test]
    async fn send_message_embeds_delivery_metadata() {
        let adapter = MockA2aAdapter::new();
        let response = adapter
            .send_message(A2aMessage {
                from_agent: "agent-source".into(),
                to_agent: "agent-synthesizer-01".into(),
                payload: serde_json::json!({"hello": "world"}),
                traceparent: "00-abc-def-01".into(),
                contract_hash: "contract".into(),
            })
            .await
            .unwrap();

        assert!(response.success);
        assert_eq!(response.payload["status"], "delivered");
        assert_eq!(response.payload["payload"]["hello"], "world");
        assert!(!response.payload["delivery_id"].as_str().unwrap().is_empty());
    }
}
