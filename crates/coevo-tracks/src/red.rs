//! Red Track Runner — physical circuit breaker, emergency lease, full ADR-A.
//! IR=3. Requires caller_identity_proof, dual-sign lease, MFA approval.
//! Per coevo whitepaper Section 11.3.

use coevo_core::cognitive::CognitiveLayer;
use coevo_core::contract::{ApprovalMode, ContractState};
use coevo_core::decision::{
    AcceptedRisk, AdrA, ConflictStatus, RejectedAlternative, ResponsibilityAnchor,
};
use coevo_core::decision::{ActionProposalSpec, GateDecision, GateDecisionSpec};
use coevo_core::lease::EmergencyLease;
use coevo_core::metadata::CommonMetadataHeader;
use coevo_customs::propose::CognitiveCustoms;
use coevo_customs::provenance::ProvenanceSigner;
use coevo_mcl::compiler::MCLCompiler;
use coevo_mcl::state_machine::{MCLStateMachine, TransitionEvent};
use coevo_risk::decision_tree::RiskGate;
use coevo_risk::lease::LeaseManager;
use coevo_router::pcdt::PcdtRouter;
use coevo_store::repos::adr_repo::AdrRepo;
use coevo_store::repos::contract_repo::ContractRepo;
use coevo_store::repos::plan_repo::PlanRepo;
use coevo_store::repos::risk_repo::RiskRepo;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Red Track result.
#[derive(Debug, Clone)]
pub struct RedTrackResult {
    pub contract_hash: String,
    pub plan_hash: String,
    pub traceparent: String,
    pub lease: Option<EmergencyLease>,
    pub decision: String,
    pub requires_mfa: bool,
    pub entries_created: Vec<String>,
}

/// Red Track runner.
pub struct RedTrackRunner;

fn canonical_gate_decision(decision: GateDecision) -> &'static str {
    match decision {
        GateDecision::Allow => "ALLOW",
        GateDecision::Deny => "DENY",
        GateDecision::RequireHumanApproval => "REQUIRE_HUMAN_APPROVAL",
        GateDecision::DeferForMoreEvidence => "DEFER_FOR_MORE_EVIDENCE",
        GateDecision::AllowWithLease => "ALLOW_WITH_LEASE",
        GateDecision::EscalateToResolution => "ESCALATE_TO_RESOLUTION",
    }
}

fn finalize_red_gate(
    gating: &GateDecisionSpec,
    lease: Option<EmergencyLease>,
) -> Result<Option<EmergencyLease>, RedTrackError> {
    match gating.decision {
        GateDecision::AllowWithLease => lease.map(Some).ok_or_else(|| {
            RedTrackError::LeaseError(format!(
                "ALLOW_WITH_LEASE requires a persisted lease: {}",
                gating.reason
            ))
        }),
        GateDecision::RequireHumanApproval => Err(RedTrackError::HumanApprovalRequired {
            reason: gating.reason.clone(),
        }),
        GateDecision::Deny => Err(RedTrackError::CircuitBreakerTripped {
            reason: gating.reason.clone(),
        }),
        _ => Ok(lease),
    }
}

fn red_track_adr(
    decision_id: &str,
    contract: &coevo_core::contract::MCLSpec,
    contract_hash: &str,
    proposer_agent: &str,
    identity_proof: &str,
    monitoring_signature: &str,
    diagnostic_signature: &str,
    gating: &coevo_core::decision::GateDecisionSpec,
) -> AdrA {
    let human_role = contract
        .responsibility_anchor_policy
        .required_human_roles
        .first()
        .cloned()
        .unwrap_or_else(|| "CISO".to_string());
    let mut fingerprint = Sha256::new();
    fingerprint.update(identity_proof.as_bytes());
    fingerprint.update(monitoring_signature.as_bytes());
    fingerprint.update(diagnostic_signature.as_bytes());

    let selected_option = canonical_gate_decision(gating.decision).to_string();
    let rejected_alternatives = vec![
        RejectedAlternative {
            option_id: "require_human_approval".to_string(),
            description: "Pause execution until a human approval path is completed.".to_string(),
            rejection_reason: if gating.decision == GateDecision::RequireHumanApproval {
                "Selected option already requires human approval.".to_string()
            } else {
                "Explicit approval was not selected because the emergency-lease path was chosen."
                    .to_string()
            },
            evidence_chain: vec![contract_hash.to_string()],
        },
        RejectedAlternative {
            option_id: "deny".to_string(),
            description: "Fail closed and do not execute the production action.".to_string(),
            rejection_reason:
                "Residual operational risk was accepted only under the governed red-track path."
                    .to_string(),
            evidence_chain: vec![contract_hash.to_string()],
        },
    ];

    AdrA {
        decision_id: decision_id.to_string(),
        mcl_reference: contract_hash.to_string(),
        proposer_agent: proposer_agent.to_string(),
        critic_objections: vec![],
        blocker_conflict_status: ConflictStatus::TradeOff,
        selected_option,
        rejected_alternatives,
        risk_accepted: AcceptedRisk {
            risk_description: gating.reason.clone(),
            risk_score: gating.action_risk.max(gating.inaction_risk),
            mitigation_notes: Some(
                "Red Track dual-sign controls and post-execution monitoring remain mandatory."
                    .to_string(),
            ),
        },
        human_override_reason: None,
        responsibility_anchor: ResponsibilityAnchor {
            human_role,
            mfa_signature_fingerprint: hex::encode(fingerprint.finalize()),
        },
        follow_up_monitoring_plan: Some(
            "Monitor production outcome for 24h and revoke the lease immediately on anomaly."
                .to_string(),
        ),
        post_execution_feedback: None,
        created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
    }
}

impl RedTrackRunner {
    /// Run the Red Track end-to-end (12 steps per whitepaper 11.3).
    pub async fn run(
        pool: &SqlitePool,
        user_intent: &str,
        agent_ids: Vec<String>,
        tenant_id: &str,
        caller_identity_proof: Option<&str>,
        monitoring_signature: Option<&str>,
        diagnostic_signature: Option<&str>,
    ) -> Result<RedTrackResult, RedTrackError> {
        // ---- Step 1: Verify caller_identity_proof is present ----
        let identity_proof = caller_identity_proof.ok_or(RedTrackError::MissingIdentityProof)?;

        if identity_proof.is_empty() {
            return Err(RedTrackError::MissingIdentityProof);
        }

        // ---- Step 1b: Verify dual-sign (monitoring + diagnostic) is present ----
        // Red Track ALWAYS requires dual-sign for production operations
        let mon_sig = monitoring_signature.ok_or(RedTrackError::MissingDualSign(
            "monitoring_signature".to_string(),
        ))?;
        let diag_sig = diagnostic_signature.ok_or(RedTrackError::MissingDualSign(
            "diagnostic_signature".to_string(),
        ))?;
        if mon_sig.is_empty() {
            return Err(RedTrackError::MissingDualSign(
                "monitoring_signature is empty".to_string(),
            ));
        }
        if diag_sig.is_empty() {
            return Err(RedTrackError::MissingDualSign(
                "diagnostic_signature is empty".to_string(),
            ));
        }

        let zero = "0000000000000000000000000000000000000000000000000000000000000000";
        // Fail-closed by default: DenyAll unless tests / COEVO_ENABLE_MOCK_POLICY_ENGINE=1.
        let policy = crate::policy_select::select_policy_engine();
        let compiler = MCLCompiler::new();

        // ---- Step 2: Compile with OPA injection ----
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
            .map_err(|e| RedTrackError::CompilationFailed(e.to_string()))?;

        let mut contract = result.contract;
        // Override to high-risk production
        contract.human_approval_policy.approval_mode = ApprovalMode::ExplicitApproval;
        contract.risk_tolerance_profile.allow_emergency_lease = true;
        let contract_hash = result.contract_hash;

        ContractRepo::insert(pool, &contract, &contract_hash)
            .await
            .map_err(|e| RedTrackError::StorageError(e.to_string()))?;

        // Activate
        let t1 = MCLStateMachine::transition(
            ContractState::DraftContract,
            TransitionEvent::PolicyValidationPass,
        )?;
        ContractRepo::update_state(pool, &contract_hash, &format!("{:?}", t1.new_state)).await?;
        let t2 = MCLStateMachine::transition(t1.new_state, TransitionEvent::ContractActivation)?;
        ContractRepo::update_state(pool, &contract_hash, &format!("{:?}", t2.new_state)).await?;

        // ---- Step 3: Route with strict limits ----
        let route_result = PcdtRouter::compute(&contract, agent_ids.clone(), None)
            .map_err(|e| RedTrackError::RoutingFailed(e.to_string()))?;
        PlanRepo::insert(pool, &route_result.plan, &contract_hash).await?;

        // ---- Step 4-5: Risk Gate evaluation ----
        let risk_gate = RiskGate::new(policy);
        let action = ActionProposalSpec {
            action_urn: "urn:coevo:action:production:write".to_string(),
            target_environment: "production".to_string(),
            parameters: serde_json::json!({"payload": user_intent}),
            emergency_mode: true,
            blast_radius: 3,
            irreversibility: 3,
            environment_sensitivity: 3,
            reversibility: 3,
        };

        let gating = risk_gate
            .evaluate(
                &action,
                &contract,
                &[0.6], // support
                &[0.7],
                &[0.5], // opposition
                &[0.4],
                0.8, // high service impact
                0.9, // high time criticality
                0.7, // failure propagation
                false,
            )
            .await;

        let decided_by = agent_ids
            .first()
            .map(String::as_str)
            .unwrap_or("RedTrackRunner");
        let decision_id = uuid::Uuid::new_v4().to_string();
        RiskRepo::insert(
            pool,
            &decision_id,
            &contract_hash,
            decided_by,
            &action.action_urn,
            canonical_gate_decision(gating.decision),
            gating.required_confidence,
            gating.available_confidence,
            gating.action_risk,
            gating.inaction_risk,
            &gating.reason,
        )
        .await
        .map_err(|e| RedTrackError::StorageError(e.to_string()))?;
        let adr = red_track_adr(
            &decision_id,
            &contract,
            &contract_hash,
            decided_by,
            identity_proof,
            mon_sig,
            diag_sig,
            &gating,
        );
        AdrRepo::insert(pool, &adr)
            .await
            .map_err(|e| RedTrackError::StorageError(e.to_string()))?;

        // ---- Step 6-8: Emergency Lease if needed ----
        // Red Track: dual-sign already validated in Step 1b.
        // Generate lease whenever identity + dual-sign are present.
        let lease = match gating.decision {
            GateDecision::AllowWithLease => {
                // Dual-sign is already validated (Step 1b) — grant lease
                let lease = LeaseManager::grant(
                    pool,
                    &contract_hash,
                    &agent_ids.first().cloned().unwrap_or_default(),
                    vec!["urn:coevo:action:production:write".to_string()],
                    3, // lease budget
                    mon_sig,
                    diag_sig,
                )
                .await
                .map_err(|e| RedTrackError::LeaseError(e.to_string()))?;
                Some(lease)
            }
            GateDecision::Deny => {
                return Err(RedTrackError::CircuitBreakerTripped {
                    reason: gating.reason,
                });
            }
            _ => None,
        };
        let lease = finalize_red_gate(&gating, lease)?;

        // ---- Step 9-10: Execute under lease ----
        let traceparent = format!(
            "00-{}-{}-01",
            hex::encode(uuid::Uuid::new_v4().as_bytes()),
            hex::encode(rand::random::<[u8; 8]>())
        );

        let meta = CommonMetadataHeader {
            caller_identity_proof: Some(identity_proof.to_string()),
            ..CommonMetadataHeader::new(
                contract_hash.clone(),
                contract.institution_policy_hash.clone(),
                tenant_id.to_string(),
                route_result.plan_hash.clone(),
                "Proposer".to_string(),
            )
        };

        let mut entries = vec![];

        // If lease exists, consume operations for each write
        if let Some(ref l) = lease {
            for i in 0..l.lease_budget {
                match LeaseManager::try_consume(
                    pool,
                    &l.lease_id,
                    "urn:coevo:action:production:write",
                )
                .await
                {
                    Ok(()) => {
                        let entry_value =
                            serde_json::json!({"operation": i, "status": "executed_under_lease"});
                        // Real Ed25519-signed provenance (no literal
                        // "red-track-signature"); the signature binds to
                        // `entry_value`.
                        let provenance = ProvenanceSigner::new(
                            agent_ids.first().cloned().unwrap_or_default(),
                            "urn:mcp:tool:deploy-production",
                        )
                        .with_scope(coevo_core::cognitive::EnvironmentalScope {
                            environment: coevo_core::cognitive::Environment::Production,
                            tenant_id: tenant_id.to_string(),
                        })
                        .with_ttl_seconds(900) // 15 minutes
                        .with_verification_report(serde_json::json!({"deployment": "staged"}))
                        .sign(&entry_value);

                        // Brand-new key (fresh UUID) ⇒ expected_version = 0 per OCC.
                        let receipt = CognitiveCustoms::propose(
                            pool,
                            &format!("red-result-{}-{}", uuid::Uuid::new_v4(), i),
                            0,
                            &entry_value,
                            CognitiveLayer::Hypothesis,
                            &provenance,
                            &meta,
                            &contract.evidence_requirement,
                            &[],
                        )
                        .await
                        .map_err(|e| RedTrackError::CustomsRejected(e.to_string()))?;

                        entries.push(format!("{}@v{}", receipt.key, receipt.new_version));
                    }
                    Err(e) => {
                        tracing::warn!("Lease operation {} failed: {}", i, e);
                        break;
                    }
                }
            }
        }

        Ok(RedTrackResult {
            contract_hash,
            plan_hash: route_result.plan_hash,
            traceparent,
            lease,
            decision: canonical_gate_decision(gating.decision).to_string(),
            requires_mfa: gating.decision == GateDecision::RequireHumanApproval,
            entries_created: entries,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RedTrackError {
    #[error("caller_identity_proof is required for Red Track")]
    MissingIdentityProof,
    #[error("compilation failed: {0}")]
    CompilationFailed(String),
    #[error("state machine error: {0}")]
    StateMachineError(#[from] coevo_mcl::state_machine::StateMachineError),
    #[error("routing failed: {0}")]
    RoutingFailed(String),
    #[error("cognitive customs rejected: {0}")]
    CustomsRejected(String),
    #[error("circuit breaker tripped: {reason}")]
    CircuitBreakerTripped { reason: String },
    #[error("lease error: {0}")]
    LeaseError(String),
    #[error("human approval required: {reason}")]
    HumanApprovalRequired { reason: String },
    #[error("missing dual-sign signature: {0}")]
    MissingDualSign(String),
    #[error("storage error: {0}")]
    StorageError(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[cfg(test)]
mod tests {
    use super::RedTrackRunner;
    use coevo_core::decision::{GateDecision, GateDecisionSpec};
    use coevo_risk::lease::{LeaseManager, LEASE_ROLE_DIAGNOSTIC, LEASE_ROLE_MONITORING};
    use coevo_store::{
        migrate::run_migrations, pool::create_test_pool, repos::risk_repo::RiskRepo,
    };

    async fn setup() -> sqlx::SqlitePool {
        std::env::set_var("COEVO_ENABLE_MOCK_POLICY_ENGINE", "1");
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        pool
    }

    fn dual_sign(agent_id: &str, scope: &[String]) -> (String, String) {
        (
            LeaseManager::sign_attestation(agent_id, scope, LEASE_ROLE_MONITORING),
            LeaseManager::sign_attestation(agent_id, scope, LEASE_ROLE_DIAGNOSTIC),
        )
    }

    #[test]
    fn finalize_red_gate_fails_closed_for_blocked_paths() {
        let approval_gate = GateDecisionSpec {
            decision: GateDecision::RequireHumanApproval,
            required_confidence: 0.9,
            available_confidence: 0.1,
            action_risk: 0.95,
            inaction_risk: 0.99,
            reason: "human approval required".to_string(),
            mfa_auth_url: None,
            task_status_url: None,
            lease: None,
        };

        let err = super::finalize_red_gate(&approval_gate, None)
            .expect_err("blocked red gate must not return a success result");
        assert!(matches!(
            err,
            super::RedTrackError::HumanApprovalRequired { .. }
        ));

        let lease_gate = GateDecisionSpec {
            decision: GateDecision::AllowWithLease,
            required_confidence: 0.9,
            available_confidence: 0.1,
            action_risk: 0.95,
            inaction_risk: 0.99,
            reason: "lease required".to_string(),
            mfa_auth_url: None,
            task_status_url: None,
            lease: None,
        };

        let err = super::finalize_red_gate(&lease_gate, None)
            .expect_err("AllowWithLease without a persisted lease must fail closed");
        assert!(matches!(err, super::RedTrackError::LeaseError(_)));
    }

    #[tokio::test]
    async fn run_persists_risk_decision_even_when_human_approval_is_required() {
        let pool = setup().await;
        let scope = vec!["urn:coevo:action:production:write".to_string()];
        let (monitoring, diagnostic) = dual_sign("agent-red-1", &scope);

        let err = RedTrackRunner::run(
            &pool,
            "Delete production customer data",
            vec!["agent-red-1".to_string()],
            "tenant-red",
            Some("mock-signature:agent-red-1"),
            Some(&monitoring),
            Some(&diagnostic),
        )
        .await
        .expect_err("red track should fail closed when the gate requires human approval");
        assert!(matches!(
            err,
            super::RedTrackError::HumanApprovalRequired { .. }
        ));

        let contract_hash = sqlx::query_scalar::<_, String>(
            "SELECT contract_hash FROM risk_decisions ORDER BY decided_at_ms DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let rows = RiskRepo::find_by_contract(&pool, &contract_hash)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action_urn, "urn:coevo:action:production:write");
        assert!(!rows[0].decision.trim().is_empty());
        assert!(!rows[0].reason.trim().is_empty());
        let anchor: Option<String> = sqlx::query_scalar(
            "SELECT responsibility_anchor_json FROM adr_records WHERE mcl_reference = ?",
        )
        .bind(&contract_hash)
        .fetch_optional(&pool)
        .await
        .unwrap();
        let anchor = anchor.expect("red track ADR-A anchor must be persisted");
        assert!(anchor.contains("mfa_signature_fingerprint"));
    }
}
