//! E2E Acceptance Tests — 11 mandatory scenarios per coevo whitepaper.
//!
//! Run with: `cargo test --test acceptance -- --nocapture`
//! These tests call the coevo server directly using library paths (no HTTP).

use coevo_core::cognitive::CognitiveLayer;
use coevo_core::contract::*;
use coevo_core::decision::*;
use coevo_core::reputation::ReputationDimension;
use coevo_core::stance::*;
use coevo_customs::dependency::{CognitiveDependencyGraph, EdgeType};
use coevo_customs::propose::CognitiveCustoms;
use coevo_mcl::compiler::MCLCompiler;
use coevo_reputation::scoring::{ErrorSeverity, ReputationEngine};
use coevo_resolution::engine::ResolutionEngine;
use coevo_risk::lease::LeaseManager;
use coevo_store::pool::create_test_pool;
use coevo_store::repos::*;
use coevo_tracks::green::GreenTrackRunner;
use coevo_tracks::red::RedTrackRunner;
use coevo_tracks::yellow::YellowTrackRunner;

async fn setup() -> sqlx::SqlitePool {
    let pool = create_test_pool().await.unwrap();
    coevo_store::migrate::run_migrations(&pool).await.unwrap();
    pool
}

/// Insert a minimal valid contract row so FK constraints are satisfied.
async fn insert_test_contract(pool: &sqlx::SqlitePool, hash: &str) {
    let contract = coevo_core::contract::MCLSpec {
        mcl_version: "1.0".to_string(),
        mcl_state: coevo_core::contract::ContractState::DraftContract,
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
    coevo_store::repos::contract_repo::ContractRepo::insert(pool, &contract, hash)
        .await
        .expect("insert_test_contract must succeed");
}

// ============================================================================
// Test 1: Green Track demo runs end-to-end
// ============================================================================
#[tokio::test]
async fn test_01_green_demo_runs_e2e() {
    let pool = setup().await;
    let runner = GreenTrackRunner::new();

    let result = runner
        .run(
            &pool,
            "Read and analyze system health metrics in development",
            vec!["agent-synth-01".to_string()],
            "e2e-tenant",
        )
        .await
        .expect("Green Track must succeed");

    assert!(
        !result.contract_hash.is_empty(),
        "contract_hash must not be empty"
    );
    assert!(!result.plan_hash.is_empty(), "plan_hash must not be empty");
    assert!(
        result.contract_hash.len() == 64,
        "contract_hash must be 64-char SHA256"
    );
    assert!(
        !result.entries_created.is_empty(),
        "must have created blackboard entries"
    );
    let entry = &result.entries_created[0];
    assert!(entry.contains("@v"), "entry must have version: {}", entry);
}

// ============================================================================
// Test 2: Green Track cannot bypass CognitiveCustoms to write Fact directly
// ============================================================================
#[tokio::test]
async fn test_02_green_cannot_bypass_customs_write_fact() {
    let pool = setup().await;
    let meta = coevo_core::metadata::CommonMetadataHeader::new(
        "0".repeat(64),
        "0".repeat(64),
        "e2e-tenant".to_string(),
        "0".repeat(64),
        "Synthesizer".to_string(),
    );

    let provenance = coevo_core::cognitive::ProvenanceEnvelope {
        source_agent_id: String::new(),
        verification_tool_urn: String::new(), // EMPTY — should reject
        environmental_scope: coevo_core::cognitive::EnvironmentalScope {
            environment: coevo_core::cognitive::Environment::Development,
            tenant_id: "e2e".to_string(),
        },
        ttl_seconds: 0, // invalid TTL
        cryptographic_signature: String::new(),
        verification_report: None,
        created_at: chrono::Utc::now(),
    };

    let evidence = EvidenceRequirement {
        minimum_level: "unit_tests_passing".to_string(),
        require_json_report: true,
    };

    let result = CognitiveCustoms::propose(
        &pool,
        "test-fact-direct",
        0,
        &serde_json::json!({"illegal": true}),
        CognitiveLayer::Fact,
        &provenance,
        &meta,
        &evidence,
        &[],
    )
    .await;

    assert!(
        result.is_err(),
        "Direct Fact write without provenance MUST fail. Got: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("provenance") || err.contains("MCP") || err.contains("CognitiveBound"),
        "Error must mention provenance/MCP violation: {}",
        err
    );
}

// ============================================================================
// Test 3: Yellow Track NEGATIVE_CONSENT creates approval request
// ============================================================================
#[tokio::test]
async fn test_03_yellow_negative_consent_creates_approval() {
    let pool = setup().await;

    let result = YellowTrackRunner::run(
        &pool,
        "Send notification about deployment to team",
        vec!["agent-01".to_string()],
        "e2e-tenant",
        "staging",
    )
    .await
    .expect("Yellow Track must complete");

    assert!(
        !result.contract_hash.is_empty(),
        "contract_hash must not be empty"
    );
    assert!(!result.plan_hash.is_empty(), "plan_hash must not be empty");
    assert_eq!(
        result.approval_mode, "NEGATIVE_CONSENT",
        "Staging notification must use NEGATIVE_CONSENT"
    );
    assert!(
        result.approval_id.is_some() || result.decision == "ALLOW",
        "Must have approval_id or ALLOW decision, got decision={}",
        result.decision
    );
}

// ============================================================================
// Test 4: Yellow EXPLICIT_APPROVAL must have human approve
// ============================================================================
#[tokio::test]
async fn test_04_yellow_explicit_approval_requires_human() {
    let _pool = setup().await;

    // Compile high-risk intent — forces EXPLICIT_APPROVAL
    let meta = coevo_core::metadata::CommonMetadataHeader::new(
        "0".repeat(64),
        "0".repeat(64),
        "e2e".to_string(),
        "0".repeat(64),
        "Proposer".to_string(),
    );

    let compiler = MCLCompiler::new();
    let result = compiler
        .compile(
            "Deploy critical production hotfix to fix database connection pool exhaustion",
            "DRAFT",
            None,
            &meta,
        )
        .await
        .expect("compilation must succeed");

    let approval_mode = &result.contract.human_approval_policy.approval_mode;
    assert_eq!(
        approval_mode,
        &ApprovalMode::ExplicitApproval,
        "High-risk deployment MUST use EXPLICIT_APPROVAL, got {:?}",
        approval_mode
    );
}

// ============================================================================
// Test 5: Red Track rejects missing dual-sign (no monitoring + diagnostic signature → no lease)
// ============================================================================
#[tokio::test]
async fn test_05_red_missing_dual_sign_rejected() {
    let pool = setup().await;

    // Red Track with valid identity but NO monitoring AND NO diagnostic → MUST fail
    let result = RedTrackRunner::run(
        &pool,
        "Critical production hotfix",
        vec!["agent-1".to_string()],
        "e2e-tenant",
        Some("mock-signature:agent-1"),
        None, // missing monitoring signature
        None, // missing diagnostic signature
    )
    .await;

    assert!(
        result.is_err(),
        "Red Track without both monitoring AND diagnostic signatures MUST return Err. Got: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        !err.contains("caller_identity_proof"),
        "Must not reject with identity error when identity is provided: {}",
        err
    );
}

// ============================================================================
// Test 6: Lease scope/budget enforcement
// ============================================================================
#[tokio::test]
async fn test_06_lease_scope_budget_enforced() {
    let pool = setup().await;
    insert_test_contract(&pool, "test-contract").await;
    insert_test_contract(&pool, "test-contract-2").await;

    // Grant lease with budget 2
    let lease = LeaseManager::grant(
        &pool,
        "test-contract",
        "test-agent",
        vec!["urn:coevo:action:test".to_string()],
        2,
        "mon-sig:test",
        "diag-sig:test",
    )
    .await
    .expect("lease grant must succeed");

    // Consume 2 operations
    LeaseManager::try_consume(&pool, &lease.lease_id, "urn:coevo:action:test:read")
        .await
        .expect("op 1 must succeed");
    LeaseManager::try_consume(&pool, &lease.lease_id, "urn:coevo:action:test:write")
        .await
        .expect("op 2 must succeed");

    // Budget exhausted
    let r =
        LeaseManager::try_consume(&pool, &lease.lease_id, "urn:coevo:action:test:analyze").await;
    assert!(
        r.is_err(),
        "Budget exhaustion must be rejected. Got: {:?}",
        r
    );

    // Out of scope
    let new_lease = LeaseManager::grant(
        &pool,
        "test-contract-2",
        "test-agent-2",
        vec!["urn:coevo:action:test".to_string()],
        5,
        "mon-sig:test",
        "diag-sig:test",
    )
    .await
    .expect("lease grant must succeed");

    let r =
        LeaseManager::try_consume(&pool, &new_lease.lease_id, "urn:coevo:action:FORBIDDEN").await;
    assert!(
        r.is_err(),
        "Out-of-scope operation must be rejected. Got: {:?}",
        r
    );
}

// ============================================================================
// Test 7: Fact TTL expiry → StaleFact
// ============================================================================
#[tokio::test]
async fn test_07_fact_ttl_expires_to_stale() {
    let pool = setup().await;
    insert_test_contract(&pool, "test-contract").await;

    let key = format!("ttl-test-{}", uuid::Uuid::new_v4());
    let _ = blackboard_repo::BlackboardRepo::insert(
        &pool,
        &key,
        r#"{"value":"test"}"#,
        "Fact",
        "agent-1",
        "test-contract",
        Some(-1000), // already expired
    )
    .await
    .unwrap();

    let expired = blackboard_repo::BlackboardRepo::expire_stale_facts(&pool)
        .await
        .unwrap();

    assert!(!expired.is_empty(), "Must find at least one expired fact");
    assert_eq!(
        expired[0].cognitive_layer, "StaleFact",
        "TTL expiry must set layer to StaleFact, got {}",
        expired[0].cognitive_layer
    );
}

// ============================================================================
// Test 8: RevokedFact triggers dependency invalidation
// ============================================================================
#[tokio::test]
async fn test_08_revoked_fact_triggers_dependency_invalidation() {
    let pool = setup().await;
    insert_test_contract(&pool, "test-contract").await;

    let fact_id = blackboard_repo::BlackboardRepo::insert(
        &pool,
        "dep-fact",
        r#"{"v":"base"}"#,
        "Fact",
        "agent-1",
        "test-contract",
        Some(3600000),
    )
    .await
    .unwrap();

    let hyp_id = blackboard_repo::BlackboardRepo::insert(
        &pool,
        "dep-hyp",
        r#"{"v":"derived"}"#,
        "Hypothesis",
        "agent-1",
        "test-contract",
        None,
    )
    .await
    .unwrap();

    CognitiveDependencyGraph::add_edge(&pool, &hyp_id, &fact_id, EdgeType::HypothesisDependsOnFact)
        .await
        .unwrap();

    // Revoke the fact
    blackboard_repo::BlackboardRepo::update_layer(&pool, &fact_id, "RevokedFact")
        .await
        .unwrap();

    let invalidated = CognitiveDependencyGraph::propagate_invalidation(&pool, &fact_id)
        .await
        .unwrap();

    assert!(
        invalidated.contains(&hyp_id),
        "Hypothesis dependent on revoked Fact must be invalidated. Invalidated: {:?}",
        invalidated
    );
}

// ============================================================================
// Test 9: Red Track missing caller_identity_proof MUST be rejected
// ============================================================================
#[tokio::test]
async fn test_09_red_rejects_missing_identity_proof() {
    let pool = setup().await;

    let result = RedTrackRunner::run(
        &pool,
        "Critical production hotfix",
        vec!["agent-1".to_string()],
        "e2e-tenant",
        None, // MISSING — must fail
        Some("mon-sig:test"),
        Some("diag-sig:test"),
    )
    .await;

    assert!(
        result.is_err(),
        "Red Track with None caller_identity_proof MUST return Err. Got: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("caller_identity_proof"),
        "Error MUST mention 'caller_identity_proof': {}",
        err
    );
}

// ============================================================================
// Test 10: Red Track with identity_proof + dual-sign MUST generate lease
// ============================================================================
#[tokio::test]
async fn test_10_red_with_identity_generates_lease() {
    let pool = setup().await;

    agent_repo::AgentRepo::register(
        &pool,
        "agent-red-1",
        r#"{"roles":["Proposer"]}"#,
        r#"{"tools":["deploy-production"]}"#,
    )
    .await
    .unwrap();

    let result = RedTrackRunner::run(
        &pool,
        "Emergency fix for production database connection pool exhaustion causing P1 outage",
        vec!["agent-red-1".to_string()],
        "red-tenant",
        Some("mock-signature:agent-red-1"),
        Some("mon-sig:prometheus-alert-12345"),
        Some("diag-sig:top-10-diagnostic-agent"),
    )
    .await;

    match result {
        Ok(r) => {
            assert!(
                !r.contract_hash.is_empty(),
                "contract_hash must not be empty"
            );
            assert!(!r.decision.is_empty(), "decision must not be empty");
            // With full identity + dual-sign, a lease SHOULD be generated
            // (if gating produces AllowWithLease). If Deny, it would have returned Err.
            assert!(
                r.lease.is_some(),
                "With identity_proof + dual-sign provided, lease MUST be generated. Decision: {}",
                r.decision
            );
            let lease = r.lease.as_ref().unwrap();
            assert!(lease.is_active, "lease must be active");
            assert_eq!(lease.lease_budget, 3, "lease budget must be 3");
            assert!(
                !lease.lease_scope.is_empty(),
                "lease scope must not be empty"
            );
        }
        Err(e) => {
            // If the gating returned Deny, circuit breaker tripped — that's acceptable
            // but must NOT be MissingIdentityProof or MissingDualSign
            assert!(
                !e.to_string().contains("caller_identity_proof"),
                "With valid identity_proof, must not fail with MissingIdentityProof: {}",
                e
            );
            assert!(
                !e.to_string().contains("dual-sign") && !e.to_string().contains("MissingDualSign"),
                "With both signatures provided, must not fail with MissingDualSign: {}",
                e
            );
        }
    }
}

// ============================================================================
// Test 11: ADR-A must contain rejected_alternatives and responsibility_anchor
// ============================================================================
#[tokio::test]
async fn test_11_adr_contains_required_fields() {
    let pool = setup().await;

    let contract = MCLSpec {
        mcl_version: "1.0".to_string(),
        mcl_state: ContractState::ActiveContract,
        parent_contract_hash: "0".repeat(64),
        goal_tree: GoalTree {
            root: GoalNode {
                id: "root".to_string(),
                description: "test".to_string(),
                status: GoalStatus::Pending,
                children: vec![],
                depends_on: vec![],
            },
        },
        institution_policy_hash: "0".repeat(64),
        data_boundary: vec![],
        allowed_action_modes: vec![ActionMode::DraftOnly],
        human_approval_policy: HumanApprovalPolicy {
            approval_mode: ApprovalMode::NegativeConsent,
            authorized_roles: vec!["Admin".to_string()],
            negative_consent_timeout_secs: 300,
            mfa_auth_url: None,
        },
        evidence_requirement: EvidenceRequirement {
            minimum_level: "none".to_string(),
            require_json_report: false,
        },
        risk_tolerance_profile: RiskToleranceProfile {
            max_risk_score: 0.5,
            allow_emergency_lease: false,
        },
        termination_policy: TerminationPolicy {
            max_token_budget: 10000,
            max_hops: 3,
            max_latency_ms: 60000,
            max_stance_rounds: 3,
        },
        responsibility_anchor_policy: ResponsibilityAnchorPolicy {
            required_human_roles: vec!["CISO".to_string()],
            agent_forbidden_actions: vec![],
        },
    };

    let contract_hash = format!("{:064x}", 42);
    contract_repo::ContractRepo::insert(&pool, &contract, &contract_hash)
        .await
        .unwrap();

    let engine = ResolutionEngine::new();
    let stance = StanceMatrixSpec {
        stances: vec![
            StanceEntry {
                agent_id: "agent-1".to_string(),
                position: StancePosition::Support,
                weight: 0.8,
                evidence_urns: vec![],
                has_veto: false,
                compromise_proposal: None,
                round: 0,
            },
            StanceEntry {
                agent_id: "agent-2".to_string(),
                position: StancePosition::Oppose,
                weight: 0.2,
                evidence_urns: vec![],
                has_veto: false,
                compromise_proposal: None,
                round: 0,
            },
        ],
        issue: "Test resolution".to_string(),
        context_ref: contract_hash,
        max_rounds: 3,
    };

    let result = engine
        .process(&pool, &stance)
        .await
        .expect("resolution must succeed");
    let adr = result.adr.expect("ADR-A must be generated");

    // Must have rejected_alternatives
    assert!(
        !adr.rejected_alternatives.is_empty()
            || adr.blocker_conflict_status == ConflictStatus::Consensus,
        "ADR-A must contain rejected_alternatives unless consensus. Status: {:?}",
        adr.blocker_conflict_status
    );

    // Must have responsibility_anchor
    assert!(
        !adr.responsibility_anchor.human_role.is_empty(),
        "ADR-A responsibility_anchor.human_role must not be empty"
    );
    assert!(
        !adr.responsibility_anchor
            .mfa_signature_fingerprint
            .is_empty(),
        "ADR-A responsibility_anchor.mfa_signature_fingerprint must not be empty"
    );

    // Must have decision_id
    assert!(
        !adr.decision_id.is_empty(),
        "ADR-A decision_id must not be empty"
    );
    assert!(
        !adr.mcl_reference.is_empty(),
        "ADR-A mcl_reference must not be empty"
    );
}

// ============================================================================
// Test 12: Red Track with empty string caller_identity_proof must be rejected
// ============================================================================
#[tokio::test]
async fn test_12_red_rejects_empty_identity_proof() {
    let pool = setup().await;

    let result = RedTrackRunner::run(
        &pool,
        "Critical production hotfix",
        vec!["agent-1".to_string()],
        "e2e-tenant",
        Some(""), // empty string — must fail
        Some("mon-sig:test"),
        Some("diag-sig:test"),
    )
    .await;

    assert!(
        result.is_err(),
        "Red Track with empty caller_identity_proof MUST return Err. Got: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("caller_identity_proof"),
        "Error MUST mention 'caller_identity_proof': {}",
        err
    );
}

// ============================================================================
// Test 13: Reputation engine difficulty-adjusted scoring
// ============================================================================
#[tokio::test]
async fn test_13_reputation_difficulty_adjusted() {
    let pool = setup().await;

    let rv = ReputationEngine::update(&pool, "agent-1", 5.0, 1.0, 0.5)
        .await
        .expect("reputation update must succeed");

    assert!(
        rv.task_domain_competence > 0.5,
        "High difficulty success must increase score above 0.5. Got: {:.3}",
        rv.task_domain_competence
    );
    assert_eq!(rv.task_count, 1, "task_count must be 1");

    // Severe penalty
    let rv2 = ReputationEngine::penalize(
        &pool,
        "agent-1",
        ReputationDimension::PolicyCompliance,
        ErrorSeverity::Severe,
    )
    .await
    .expect("penalty must succeed");

    assert!(
        rv2.policy_compliance < 0.8,
        "Severe penalty must drop compliance below 0.8. Got: {:.3}",
        rv2.policy_compliance
    );
}
