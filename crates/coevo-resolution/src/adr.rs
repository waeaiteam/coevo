//! ADR-A Builder — generates complete Agent Decision Records.
//! Per coevo whitepaper Section 10.

use coevo_core::decision::*;
use uuid::Uuid;

/// Builder for constructing ADR-A records.
pub struct AdrBuilder {
    mcl_reference: String,
    proposer_agent: String,
    critic_objections: Vec<CriticObjection>,
    conflict_status: ConflictStatus,
    selected_option: String,
    rejected_alternatives: Vec<RejectedAlternative>,
    risk_accepted: AcceptedRisk,
    human_override_reason: Option<String>,
    responsibility_anchor: ResponsibilityAnchor,
    follow_up_monitoring_plan: Option<String>,
}

impl AdrBuilder {
    pub fn new(issue: &str, mcl_reference: &str, proposer_agent: &str) -> Self {
        Self {
            issue: issue.to_string(),
            mcl_reference: mcl_reference.to_string(),
            proposer_agent: proposer_agent.to_string(),
            critic_objections: vec![],
            conflict_status: ConflictStatus::Consensus,
            selected_option: "consensus_path".to_string(),
            rejected_alternatives: vec![],
            risk_accepted: AcceptedRisk {
                risk_description: "Standard operational risk".to_string(),
                risk_score: 0.1,
                mitigation_notes: None,
            },
            human_override_reason: None,
            responsibility_anchor: ResponsibilityAnchor {
                human_role: "CISO".to_string(),
                mfa_signature_fingerprint: format!(
                    "mfa-fp-{}",
                    hex::encode(&rand::random::<[u8; 16]>())
                ),
            },
            follow_up_monitoring_plan: Some(
                "Monitor for 24h; auto-rollback if error rate exceeds 5%".to_string(),
            ),
        }
    }

    pub fn with_consensus(mut self, status: ConflictStatus, ratio: f64) -> Self {
        self.conflict_status = status;
        self.risk_accepted.risk_score = 1.0 - ratio;
        self
    }

    pub fn with_veto_blockers(mut self, blockers: Vec<String>) -> Self {
        self.critic_objections = blockers
            .into_iter()
            .map(|id| CriticObjection {
                critic_agent_id: id,
                objection_reason: "Veto power exercised".to_string(),
                evidence_chain_urns: vec![],
            })
            .collect();
        self.conflict_status = ConflictStatus::Divergence;
        self
    }

    pub fn with_rejected_alternatives(mut self, alternatives: Vec<RejectedAlternative>) -> Self {
        self.rejected_alternatives = alternatives;
        self
    }

    pub fn with_human_override(mut self, reason: &str) -> Self {
        self.human_override_reason = Some(reason.to_string());
        self
    }

    pub fn build(self) -> AdrA {
        AdrA {
            decision_id: format!("adr-{}", Uuid::new_v4()),
            mcl_reference: self.mcl_reference,
            proposer_agent: self.proposer_agent,
            critic_objections: self.critic_objections,
            blocker_conflict_status: self.conflict_status,
            selected_option: self.selected_option,
            rejected_alternatives: self.rejected_alternatives,
            risk_accepted: self.risk_accepted,
            human_override_reason: self.human_override_reason,
            responsibility_anchor: self.responsibility_anchor,
            follow_up_monitoring_plan: self.follow_up_monitoring_plan,
            post_execution_feedback: None,
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}
