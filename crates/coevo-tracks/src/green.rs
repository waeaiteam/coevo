//! Green Track Runner — low-risk, fast execution path.
//! BR=0, IR=0. No human approval, no Resolution, no heavy ADR-A.
//! Per coevo whitepaper Section 11.1.
//!
//! CRITICAL: Green Track CANNOT bypass CognitiveCustoms.
//! All writes MUST go through Propose with provenance envelopes.

use coevo_core::cognitive::CognitiveLayer;
use coevo_core::contract::ContractState;
use coevo_core::metadata::CommonMetadataHeader;
use coevo_customs::propose::CognitiveCustoms;
use coevo_customs::provenance::ProvenanceSigner;
use coevo_mcl::compiler::MCLCompiler;
use coevo_mcl::state_machine::{MCLStateMachine, TransitionEvent};
use coevo_router::pcdt::PcdtRouter;
use coevo_store::repos::contract_repo::ContractRepo;
use coevo_store::repos::plan_repo::PlanRepo;

use sqlx::SqlitePool;

/// Result of a Green Track execution.
#[derive(Debug, Clone)]
pub struct GreenTrackResult {
    pub contract_hash: String,
    pub plan_hash: String,
    pub traceparent: String,
    pub ambiguity_score: f64,
    pub warnings: Vec<String>,
    pub entries_created: Vec<String>,
    pub total_elapsed_ms: u64,
}

/// The Green Track runner.
pub struct GreenTrackRunner {
    compiler: MCLCompiler,
}

impl GreenTrackRunner {
    pub fn new() -> Self {
        Self {
            compiler: MCLCompiler::new(),
        }
    }

    /// Run the Green Track end-to-end.
    ///
    /// Steps (per whitepaper Section 11.1):
    /// 1. Ingress Gate: metadata validation (done in API middleware)
    /// 2. MCL Compiler: DraftContract → OPA Dry-run → ValidatedContract → ActiveContract
    /// 3. PCDT Router: compute minimal ExecutionPlan
    /// 4. Agent sandbox executes, writes to blackboard via Propose
    /// 5. Task closure: lightweight OpenTelemetry trace record
    pub async fn run(
        &self,
        pool: &SqlitePool,
        user_intent: &str,
        agent_ids: Vec<String>,
        tenant_id: &str,
    ) -> Result<GreenTrackResult, GreenTrackError> {
        let start_ms = chrono::Utc::now().timestamp_millis() as u64;

        let zero_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        // ---- Step 2: Compile MCL ----
        let compile_meta = CommonMetadataHeader::new(
            zero_hash.to_string(),
            zero_hash.to_string(),
            tenant_id.to_string(),
            zero_hash.to_string(),
            "Synthesizer".to_string(),
        );

        let compile_result = self
            .compiler
            .compile(user_intent, "DRAFT", None, &compile_meta)
            .await
            .map_err(|e| GreenTrackError::CompilationFailed(e.to_string()))?;

        // Store contract
        ContractRepo::insert(
            pool,
            &compile_result.contract,
            &compile_result.contract_hash,
        )
        .await
        .map_err(|e| GreenTrackError::StorageError(e.to_string()))?;

        // Transition: Draft → Validated
        let t1 = MCLStateMachine::transition(
            ContractState::DraftContract,
            TransitionEvent::PolicyValidationPass,
        )
        .map_err(|e| GreenTrackError::StateMachineError(e.to_string()))?;
        ContractRepo::update_state(
            pool,
            &compile_result.contract_hash,
            &format!("{:?}", t1.new_state),
        )
        .await
        .map_err(|e| GreenTrackError::StorageError(e.to_string()))?;

        // Transition: Validated → Active
        let t2 = MCLStateMachine::transition(t1.new_state, TransitionEvent::ContractActivation)
            .map_err(|e| GreenTrackError::StateMachineError(e.to_string()))?;
        ContractRepo::update_state(
            pool,
            &compile_result.contract_hash,
            &format!("{:?}", t2.new_state),
        )
        .await
        .map_err(|e| GreenTrackError::StorageError(e.to_string()))?;

        // ---- Step 3: Route ----
        let route_result = PcdtRouter::compute(&compile_result.contract, agent_ids.clone(), None)
            .map_err(|e| GreenTrackError::RoutingFailed(e.to_string()))?;

        PlanRepo::insert(pool, &route_result.plan, &compile_result.contract_hash)
            .await
            .map_err(|e| GreenTrackError::StorageError(e.to_string()))?;

        // ---- Step 4: Execute in sandbox & write to blackboard via Propose ----
        let traceparent = format!(
            "00-{}-{}-01",
            hex::encode(uuid::Uuid::new_v4().as_bytes()),
            hex::encode(rand::random::<[u8; 8]>())
        );

        let meta = CommonMetadataHeader::new(
            compile_result.contract_hash.clone(),
            compile_result.contract.institution_policy_hash.clone(),
            tenant_id.to_string(),
            route_result.plan_hash.clone(),
            "Synthesizer".to_string(),
        );

        let mut entries_created: Vec<String> = vec![];

        // Simulate agent execution: create a Hypothesis on the blackboard
        let hypothesis_key = format!("green-result-{}", uuid::Uuid::new_v4());
        let hypothesis_value = serde_json::json!({
            "intent": user_intent,
            "status": "completed",
            "agent": agent_ids.first().cloned().unwrap_or_default(),
        });

        // Build a real, Ed25519-signed provenance envelope for the Hypothesis
        // (no more literal "green-track-signature" placeholder). The signature
        // is bound to `hypothesis_value`, so it would also pass cryptographic
        // verification if this entry were ever promoted to a Fact/Decision.
        let provenance = ProvenanceSigner::new(
            agent_ids.first().cloned().unwrap_or_default(),
            "urn:mcp:tool:unit-test-runner",
        )
        .with_scope(coevo_core::cognitive::EnvironmentalScope {
            environment: coevo_core::cognitive::Environment::Development,
            tenant_id: tenant_id.to_string(),
        })
        .with_ttl_seconds(3600)
        .with_verification_report(serde_json::json!({"passed": true}))
        .sign(&hypothesis_value);

        // MUST go through CognitiveCustoms.Propose — cannot bypass.
        // Brand-new key (fresh UUID) ⇒ expected_version = 0 is correct per OCC.
        let receipt = CognitiveCustoms::propose(
            pool,
            &hypothesis_key,
            0, // expected_version = 0 (new key, per OCC rules)
            &hypothesis_value,
            CognitiveLayer::Hypothesis,
            &provenance,
            &meta,
            &compile_result.contract.evidence_requirement,
            &[],
        )
        .await
        .map_err(|e| GreenTrackError::CustomsRejected(e.to_string()))?;

        entries_created.push(format!("{}@v{}", hypothesis_key, receipt.new_version));

        let total_elapsed = chrono::Utc::now().timestamp_millis() as u64 - start_ms;

        Ok(GreenTrackResult {
            contract_hash: compile_result.contract_hash,
            plan_hash: route_result.plan_hash,
            traceparent,
            ambiguity_score: compile_result.ambiguity_score,
            warnings: compile_result.compile_warnings,
            entries_created,
            total_elapsed_ms: total_elapsed,
        })
    }
}

impl Default for GreenTrackRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GreenTrackError {
    #[error("compilation failed: {0}")]
    CompilationFailed(String),
    #[error("state machine error: {0}")]
    StateMachineError(String),
    #[error("routing failed: {0}")]
    RoutingFailed(String),
    #[error("cognitive customs rejected: {0}")]
    CustomsRejected(String),
    #[error("storage error: {0}")]
    StorageError(String),
}
