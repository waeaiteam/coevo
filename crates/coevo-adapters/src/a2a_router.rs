//! In-process A2A router: a real intra-company message bus for manager-to-manager
//! communication, replacing the echo-only [`MockA2aAdapter`](crate::a2a::MockA2aAdapter).
//!
//! Unlike the mock (which fabricated a "delivered" envelope and bounced the payload back),
//! this router maintains a real inbox per registered agent and performs an actual delivery:
//! a sent message is enqueued in the recipient's inbox and can be drained by the recipient.
//! It is "in-process" because intra-company managers live in the same server process — no
//! network hop is needed (cross-system A2A would layer a transport on top of this same API).
//!
//! Cognition (a manager actually *reasoning* about a message) lives in the server layer
//! where the model gateway is; this crate owns only the transport/registry, which is what
//! made the previous adapter a stub.

use crate::traits::*;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct InProcessA2aRouter {
    /// agent_id -> ordered inbox of delivered messages.
    inboxes: Mutex<HashMap<String, Vec<DeliveredMessage>>>,
}

impl InProcessA2aRouter {
    /// Create a router with a fixed set of registered agents (e.g. the company's heads).
    pub fn new(agents: Vec<String>) -> Self {
        let mut inboxes = HashMap::new();
        for agent in agents {
            inboxes.entry(agent).or_insert_with(Vec::new);
        }
        Self {
            inboxes: Mutex::new(inboxes),
        }
    }

    /// Non-destructive peek at how many messages are waiting (diagnostics/tests).
    pub fn inbox_len(&self, agent_id: &str) -> usize {
        self.inboxes
            .lock()
            .unwrap()
            .get(agent_id)
            .map(Vec::len)
            .unwrap_or(0)
    }

    fn delivery_id(msg: &A2aMessage) -> String {
        let mut hasher = Sha256::new();
        hasher.update(msg.from_agent.as_bytes());
        hasher.update(msg.to_agent.as_bytes());
        hasher.update(
            serde_json::to_string(&msg.payload)
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.update(msg.traceparent.as_bytes());
        hasher.update(msg.contract_hash.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[async_trait]
impl A2aProvider for InProcessA2aRouter {
    async fn send_message(&self, msg: A2aMessage) -> Result<A2aResponse, AdapterError> {
        let delivery_id = Self::delivery_id(&msg);
        let delivered = DeliveredMessage {
            delivery_id: delivery_id.clone(),
            from_agent: msg.from_agent.clone(),
            to_agent: msg.to_agent.clone(),
            payload: msg.payload.clone(),
            traceparent: msg.traceparent.clone(),
            contract_hash: msg.contract_hash.clone(),
        };
        {
            let mut guard = self.inboxes.lock().unwrap();
            let Some(inbox) = guard.get_mut(&msg.to_agent) else {
                return Err(AdapterError::A2aError(format!(
                    "target agent '{}' is not registered on this company bus",
                    msg.to_agent
                )));
            };
            inbox.push(delivered);
        }
        Ok(A2aResponse {
            from_agent: msg.to_agent.clone(),
            payload: serde_json::json!({
                "delivery_id": delivery_id,
                "from_agent": msg.from_agent,
                "to_agent": msg.to_agent,
                "status": "queued",
            }),
            success: true,
        })
    }

    async fn discover_agents(&self) -> Result<Vec<String>, AdapterError> {
        let mut agents: Vec<String> = self.inboxes.lock().unwrap().keys().cloned().collect();
        agents.sort();
        Ok(agents)
    }

    async fn health_check(&self) -> Result<bool, AdapterError> {
        Ok(true)
    }

    fn register(&self, agent_id: &str) {
        let mut guard = self.inboxes.lock().unwrap();
        guard.entry(agent_id.to_string()).or_default();
    }

    fn unregister(&self, agent_id: &str) {
        let mut guard = self.inboxes.lock().unwrap();
        guard.remove(agent_id);
    }

    fn drain_inbox(&self, agent_id: &str) -> Vec<DeliveredMessage> {
        let mut guard = self.inboxes.lock().unwrap();
        guard
            .get_mut(agent_id)
            .map(std::mem::take)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(from: &str, to: &str, body: serde_json::Value) -> A2aMessage {
        A2aMessage {
            from_agent: from.into(),
            to_agent: to.into(),
            payload: body,
            traceparent: "00-trace-span-01".into(),
            contract_hash: "contract-hash".into(),
        }
    }

    #[tokio::test]
    async fn delivers_message_into_recipient_inbox() {
        let bus = InProcessA2aRouter::new(vec!["agent-pm-01".into(), "agent-eng-01".into()]);
        let resp = bus
            .send_message(msg(
                "agent-pm-01",
                "agent-eng-01",
                serde_json::json!({"ask": "feasible?"}),
            ))
            .await
            .unwrap();
        assert!(resp.success);
        assert_eq!(resp.payload["status"], "queued");
        assert_eq!(bus.inbox_len("agent-eng-01"), 1);

        let drained = bus.drain_inbox("agent-eng-01");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].from_agent, "agent-pm-01");
        assert_eq!(drained[0].payload["ask"], "feasible?");
        // Draining clears the inbox.
        assert_eq!(bus.inbox_len("agent-eng-01"), 0);
    }

    #[tokio::test]
    async fn rejects_unregistered_recipient() {
        let bus = InProcessA2aRouter::new(vec!["agent-pm-01".into()]);
        let err = bus
            .send_message(msg("agent-pm-01", "agent-ghost", serde_json::json!({})))
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn register_adds_a_new_recipient() {
        let bus = InProcessA2aRouter::new(vec![]);
        bus.register("agent-new-01");
        bus.send_message(msg(
            "agent-pm-01",
            "agent-new-01",
            serde_json::json!({"x": 1}),
        ))
        .await
        .unwrap();
        assert_eq!(bus.inbox_len("agent-new-01"), 1);
    }

    #[tokio::test]
    async fn unregister_removes_agent_and_queued_messages() {
        let bus = InProcessA2aRouter::new(vec!["agent-a".into(), "agent-b".into()]);
        bus.send_message(msg(
            "agent-b",
            "agent-a",
            serde_json::json!({"text": "stale meeting turn"}),
        ))
        .await
        .unwrap();
        assert_eq!(bus.inbox_len("agent-a"), 1);

        bus.unregister("agent-a");

        assert!(!bus
            .discover_agents()
            .await
            .unwrap()
            .contains(&"agent-a".to_string()));
        assert!(bus.drain_inbox("agent-a").is_empty());
        assert!(bus
            .send_message(msg("agent-b", "agent-a", serde_json::json!({})))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn discover_returns_sorted_registry() {
        let bus = InProcessA2aRouter::new(vec!["agent-b".into(), "agent-a".into()]);
        assert_eq!(
            bus.discover_agents().await.unwrap(),
            vec!["agent-a", "agent-b"]
        );
    }

    #[tokio::test]
    async fn dyn_provider_register_send_and_drain_roundtrip() {
        // Exercise the same trait-object surface the server's meeting loop uses:
        // register through &dyn, deliver, then drain the recipient's real inbox.
        let bus: std::sync::Arc<dyn A2aProvider> =
            std::sync::Arc::new(InProcessA2aRouter::new(vec![]));
        bus.register("agent-pm-01");
        bus.register("agent-eng-01");
        bus.send_message(msg(
            "agent-pm-01",
            "agent-eng-01",
            serde_json::json!({"text": "is this feasible?"}),
        ))
        .await
        .unwrap();
        let drained = bus.drain_inbox("agent-eng-01");
        assert_eq!(drained.len(), 1, "message must flow through the bus");
        assert_eq!(drained[0].from_agent, "agent-pm-01");
        assert_eq!(drained[0].payload["text"], "is this feasible?");
        // Draining is destructive: a second drain is empty.
        assert!(bus.drain_inbox("agent-eng-01").is_empty());
    }
}
