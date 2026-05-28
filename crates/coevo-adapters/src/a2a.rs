//! Mock A2A adapter — simulates agent-to-agent messaging.

use crate::traits::*;
use async_trait::async_trait;

pub struct MockA2aAdapter {
    registered_agents: Vec<String>,
}

impl MockA2aAdapter {
    pub fn new() -> Self {
        Self {
            registered_agents: vec![
                "agent-synthesizer-01".to_string(),
                "agent-critic-01".to_string(),
                "agent-proposer-01".to_string(),
                "agent-diagnostic-01".to_string(),
            ],
        }
    }

    pub fn with_agents(mut self, agents: Vec<String>) -> Self {
        self.registered_agents = agents;
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
        if !self.registered_agents.contains(&msg.to_agent) {
            return Err(AdapterError::A2aError(format!(
                "target agent '{}' not found",
                msg.to_agent
            )));
        }
        // Simulate successful delivery with echo
        Ok(A2aResponse {
            from_agent: msg.to_agent,
            payload: serde_json::json!({
                "echo": msg.payload,
                "traceparent": msg.traceparent,
                "status": "delivered"
            }),
            success: true,
        })
    }

    async fn discover_agents(&self) -> Result<Vec<String>, AdapterError> {
        Ok(self.registered_agents.clone())
    }

    async fn health_check(&self) -> Result<bool, AdapterError> {
        Ok(true)
    }
}
