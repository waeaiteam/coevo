//! Yellow Track Runner — async execution with approval window.
//! BR ≤ 1, IR ≤ 1. NEGATIVE_CONSENT or EXPLICIT_APPROVAL.
//! Per coevo whitepaper Section 11.2.

use coevo_core::cognitive::{CognitiveLayer, ProvenanceEnvelope};
use coevo_core::contract::{ApprovalMode, ContractState};
use coevo_core::decision::{ActionProposalSpec, GateDecision};
use coevo_core::metadata::CommonMetadataHeader;
use coevo_customs::propose::CognitiveCustoms;
use coevo_mcl::compiler::MCLCompiler;
use coevo_mcl::state_machine::{MCLStateMachine, TransitionEvent};
use coevo_policy::mock::MockPolicyEngine;
use coevo_policy::traits::PolicyEngine;
use coevo_risk::decision_tree::RiskGate;
use coevo_router::pcdt::PcdtRouter;
use coevo_store::repos::approval_repo::ApprovalRepo;
use coevo_store::repos::contract_repo::ContractRepo;
use coevo_store::repos::plan_repo::PlanRepo;
use sqlx::SqlitePool;

/// Yellow Track result.
#[derive(Debug, Clone)]
pub struct YellowTrackResult {
    pub contract_hash: String,
    pub plan_hash: String,
    pub traceparent: String,
    pub approval_id: Option<String>,
    pub approval_mode: String,
    pub decision: String,
    pub entries_created: Vec<String>,
}

/// Yellow Track runner.
pub struct YellowTrackRunner;

impl YellowTrackRunner {
    /// Run the Yellow Track end-to-end.
    pub async fn run(
        pool: &SqlitePool,
        user_intent: &str,
        agent_ids: Vec<String>,
        tenant_id: &str,
        environment: &str,
    ) -> Result<YellowTrackResult, YellowTrackError> {
        let zero = "0000000000000000000000000000000000000000000000000000000000000000";
        let policy = Box::new(MockPolicyEngine::new());
        let compiler = MCLCompiler::new();

        // Compile
        let meta = CommonMetadataHeader::new(
            zero.to_string(),
            zero.to_string(),
            tenant_id.to_string(),
            zero.to_string(),
            "Proposer".to_string(),
        );
        let result = compiler
            .compile(user_intent, "ACTIVE", None, &meta)
            .await
            .map_err(|e| YellowTrackError::CompilationFailed(e.to_string()))?;

        let mut contract = result.contract;
        let contract_hash = result.contract_hash;

        ContractRepo::insert(pool, &contract, &contract_hash)
            .await
            .map_err(|e| YellowTrackError::StorageError(e.to_string()))?;

        // Activate
        let t1 = MCLStateMachine::transition(
            ContractState::DraftContract,
            TransitionEvent::PolicyValidationPass,
        )?;
        ContractRepo::update_state(pool, &contract_hash, &format!("{:?}", t1.new_state)).await?;

        let t2 = MCLStateMachine::transition(
            t1.new_state,
            TransitionEvent::ContractActivation,
        )?;
        ContractRepo::update_state(pool, &contract_hash, &format!("{:?}", t2.new_state)).await?;

        // Route
        let route_result = PcdtRouter::compute(&contract, agent_ids.clone(), None)
            .map_err(|e| YellowTrackError::RoutingFailed(e.to_string()))?;
        PlanRepo::insert(pool, &route_result.plan, &contract_hash).await?;

        // Risk Gate evaluation
        let risk_gate = RiskGate::new(policy);
        let action = ActionProposalSpec {
            action_urn: "urn:coevo:action:write:internal-notification".to_string(),
            target_environment: environment.to_string(),
            parameters: serde_json::json!({"message": user_intent}),
            emergency_mode: false,
            blast_radius: 1,
            irreversibility: 1,
            environment_sensitivity: if environment == "staging" { 1 } else { 0 },
            reversibility: 1,
        };

        let gating = risk_gate
            .evaluate(
                &action,
                &contract,
                &[0.7], // support reputation
                &[0.8], // support evidence weight
                &[],     // no opposition
                &[],     // no opposition evidence
                0.3,     // service impact
                0.2,     // time criticality
                0.1,     // failure propagation
                false,   // no veto
            )
            .await;

        let (approval_id, decision_str, final_decision) = match gating.decision {
            GateDecision::Allow => (None, "ALLOW".to_string(), GateDecision::Allow),
            GateDecision::RequireHumanApproval => {
                // Create approval request
                let approval_mode = match contract.human_approval_policy.approval_mode {
                    ApprovalMode::NegativeConsent => "NEGATIVE_CONSENT",
                    ApprovalMode::ExplicitApproval => "EXPLICIT_APPROVAL",
                };
                let timeout_ms = match contract.human_approval_policy.approval_mode {
                    ApprovalMode::NegativeConsent => {
                        contract.human_approval_policy.negative_consent_timeout_secs as i64 * 1000
                    }
                    ApprovalMode::ExplicitApproval => 300_000, // 5 min
                };
                let aid = ApprovalRepo::create(
                    pool,
                    &contract_hash,
                    &action.action_urn,
                    approval_mode,
                    "YellowTrackRunner",
                    timeout_ms,
                )
                .await?;

                (
                    Some(aid),
                    "REQUIRE_HUMAN_APPROVAL".to_string(),
                    gating.decision,
                )
            }
            _ => (None, format!("{:?}", gating.decision), gating.decision),
        };

        // Write Hypothesis to blackboard
        let traceparent = format!(
            "00-{}-{}-01",
            hex::encode(uuid::Uuid::new_v4().as_bytes()),
            hex::encode(&rand::random::<[u8; 8]>())
        );
        let meta = CommonMetadataHeader::new(
            contract_hash.clone(),
            contract.institution_policy_hash.clone(),
            tenant_id.to_string(),
            route_result.plan_hash.clone(),
            "Proposer".to_string(),
        );

        let provenance = ProvenanceEnvelope {
            source_agent_id: agent_ids.first().cloned().unwrap_or_default(),
            verification_tool_urn: "urn:mcp:tool:unit-test-runner".to_string(),
            environmental_scope: coevo_core::cognitive::EnvironmentalScope {
                environment: coevo_core::cognitive::Environment::Staging,
                tenant_id: tenant_id.to_string(),
            },
            ttl_seconds: 7200,
            cryptographic_signature: "yellow-track-signature".to_string(),
            verification_report: Some(serde_json::json!({"passed": true})),
            created_at: chrono::Utc::now(),
        };

        let receipt = CognitiveCustoms::propose(
            pool,
            &format!("yellow-result-{}", uuid::Uuid::new_v4()),
            0,
            &serde_json::json!({"status": "pending_approval", "decision": decision_str}),
            CognitiveLayer::Hypothesis,
            &provenance,
            &meta,
            &contract.evidence_requirement,
            &[],
        )
        .await
        .map_err(|e| YellowTrackError::CustomsRejected(e.to_string()))?;

        Ok(YellowTrackResult {
            contract_hash,
            plan_hash: route_result.plan_hash,
            traceparent,
            approval_id,
            approval_mode: match contract.human_approval_policy.approval_mode {
                ApprovalMode::NegativeConsent => "NEGATIVE_CONSENT".to_string(),
                ApprovalMode::ExplicitApproval => "EXPLICIT_APPROVAL".to_string(),
            },
            decision: decision_str,
            entries_created: vec![format!("{}@v{}", receipt.key, receipt.new_version)],
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum YellowTrackError {
    #[error("compilation failed: {0}")]
    CompilationFailed(String),
    #[error("state machine error: {0}")]
    StateMachineError(#[from] coevo_mcl::state_machine::StateMachineError),
    #[error("routing failed: {0}")]
    RoutingFailed(String),
    #[error("cognitive customs rejected: {0}")]
    CustomsRejected(String),
    #[error("storage error: {0}")]
    StorageError(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
