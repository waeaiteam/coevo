//! MCL Compiler — translates unstructured user intent into a declarative MCLSpec.
//! Per coevo whitepaper Section 2.1 and Section 4.

use coevo_core::contract::*;
use coevo_core::metadata::CommonMetadataHeader;
use coevo_policy::traits::PolicyEngine;
use sha2::{Digest, Sha256};

/// Result of MCL compilation.
pub struct CompileResult {
    pub contract: MCLSpec,
    pub contract_hash: String,
    pub ambiguity_score: f64,
    pub compile_warnings: Vec<String>,
}

/// The MCL Compiler.
pub struct MCLCompiler {
    /// Optional institution policy engine for validation.
    policy_engine: Option<Box<dyn PolicyEngine>>,
}

impl MCLCompiler {
    pub fn new() -> Self {
        Self {
            policy_engine: None,
        }
    }

    pub fn with_policy_engine(mut self, engine: Box<dyn PolicyEngine>) -> Self {
        self.policy_engine = Some(engine);
        self
    }

    /// Compile user intent into an MCL contract.
    /// If `requested_mode` is ACTIVE, policy validation is enforced.
    /// If `requested_mode` is DRAFT, only compilation warnings are returned.
    pub async fn compile(
        &self,
        user_intent: &str,
        requested_mode: &str,
        parent_contract_hash: Option<&str>,
        metadata: &CommonMetadataHeader,
    ) -> Result<CompileResult, CompileError> {
        let mut warnings: Vec<String> = vec![];

        // ---- Phase 1: Intent parsing ----
        let parsed = parse_intent(user_intent)?;
        if parsed.ambiguity_score > 0.7 {
            return Err(CompileError::AmbiguityTooHigh {
                score: parsed.ambiguity_score,
                detail: "Intent is too ambiguous; please provide more specific instructions".to_string(),
            });
        }
        if parsed.ambiguity_score > 0.3 {
            warnings.push(format!(
                "Intent ambiguity score {:.2} — consider clarifying the objective",
                parsed.ambiguity_score
            ));
        }

        // ---- Phase 2: Build MCLSpec ----
        let parent_hash = parent_contract_hash
            .unwrap_or("0000000000000000000000000000000000000000000000000000000000000000")
            .to_string();

        let contract = MCLSpec {
            mcl_version: "1.0".to_string(),
            mcl_state: ContractState::DraftContract,
            parent_contract_hash: parent_hash,
            goal_tree: build_goal_tree(&parsed),
            institution_policy_hash: metadata.policy_version.clone(),
            data_boundary: infer_data_boundaries(&parsed),
            allowed_action_modes: infer_action_modes(&parsed),
            human_approval_policy: infer_approval_policy(&parsed),
            evidence_requirement: infer_evidence_requirement(&parsed),
            risk_tolerance_profile: infer_risk_tolerance(&parsed),
            termination_policy: infer_termination_policy(&parsed),
            responsibility_anchor_policy: infer_responsibility_policy(&parsed),
        };

        // ---- Phase 3: Policy validation ----
        if requested_mode == "ACTIVE" {
            if let Some(ref engine) = self.policy_engine {
                let policy_result = engine
                    .validate_contract(&contract)
                    .await
                    .map_err(|e| CompileError::PolicyEngineError(e.to_string()))?;

                if !policy_result.passed {
                    let violation_details: Vec<String> = policy_result
                        .violations
                        .iter()
                        .map(|v| format!("{}: {}", v.policy_urn, v.description))
                        .collect();
                    return Err(CompileError::InstitutionViolation {
                        violations: violation_details,
                        contract_hash: "not-yet-hashed".to_string(),
                    });
                }
            }
        } else {
            // DRAFT mode: do a dry-run to collect warnings
            if let Some(ref engine) = self.policy_engine {
                if let Ok(dry_run) = engine.dry_run(&contract).await {
                    for v in &dry_run.violations {
                        warnings.push(format!("Policy warning [{}]: {}", v.policy_urn, v.description));
                    }
                }
            }
        }

        // ---- Phase 4: Hash the contract ----
        let contract_json = serde_json::to_string(&contract)
            .map_err(|e| CompileError::SerializationError(e.to_string()))?;
        let mut hasher = Sha256::new();
        hasher.update(contract_json.as_bytes());
        let contract_hash = hex::encode(hasher.finalize());

        Ok(CompileResult {
            contract,
            contract_hash,
            ambiguity_score: parsed.ambiguity_score,
            compile_warnings: warnings,
        })
    }
}

impl Default for MCLCompiler {
    fn default() -> Self {
        Self::new()
    }
}

// ---- Internal intent parsing ----

struct ParsedIntent {
    objective: String,
    sub_goals: Vec<String>,
    environment: String,
    risk_level: String,
    ambiguity_score: f64,
    actions: Vec<String>,
    data_domains: Vec<String>,
    estimated_duration_ms: u64,
    estimated_hops: u32,
}

/// Simple keyword-based intent parser.
/// In production this would use an LLM for semantic understanding.
fn parse_intent(user_intent: &str) -> Result<ParsedIntent, CompileError> {
    let intent = user_intent.trim().to_lowercase();

    if intent.is_empty() {
        return Err(CompileError::EmptyIntent);
    }

    // Detect environment
    let environment = if intent.contains("production") || intent.contains("prod") {
        "production"
    } else if intent.contains("staging") {
        "staging"
    } else {
        "development"
    };

    // Detect risk level
    let risk_level = if intent.contains("high risk")
        || intent.contains("dangerous")
        || intent.contains("critical")
    {
        "high"
    } else if intent.contains("medium risk") || intent.contains("moderate") {
        "medium"
    } else {
        "low"
    };

    // Detect actions
    let mut actions = vec![];
    if intent.contains("read") || intent.contains("query") || intent.contains("analyze") {
        actions.push("DRAFT_ONLY".to_string());
    }
    if intent.contains("write") || intent.contains("update") || intent.contains("modify") {
        actions.push("MUTABLE_WRITE".to_string());
    }
    if intent.contains("deploy") || intent.contains("commit") || intent.contains("execute") {
        actions.push("COMMIT_READY".to_string());
    }
    if actions.is_empty() {
        actions.push("DRAFT_ONLY".to_string());
    }

    // Detect data domains
    let mut data_domains = vec!["urn:coevo:data:default".to_string()];
    if intent.contains("database") || intent.contains("db") {
        data_domains.push("urn:coevo:data:database".to_string());
    }
    if intent.contains("file") || intent.contains("storage") {
        data_domains.push("urn:coevo:data:filesystem".to_string());
    }
    if intent.contains("network") || intent.contains("http") {
        data_domains.push("urn:coevo:data:network".to_string());
    }

    // Heuristic ambiguity: shorter/more vague = more ambiguous
    let word_count = intent.split_whitespace().count();
    let action_verb_count = intent
        .split_whitespace()
        .filter(|w| {
            matches!(
                *w,
                "read" | "write" | "create" | "delete" | "update" | "analyze" | "deploy" | "query"
            )
        })
        .count();
    let ambiguity_score = if word_count < 3 || action_verb_count == 0 {
        0.6
    } else if word_count < 8 {
        0.4
    } else {
        0.2
    };

    Ok(ParsedIntent {
        objective: user_intent.to_string(),
        sub_goals: vec!["Complete the requested task".to_string()],
        environment: environment.to_string(),
        risk_level: risk_level.to_string(),
        ambiguity_score,
        actions,
        data_domains,
        estimated_duration_ms: 60_000,
        estimated_hops: 3,
    })
}

fn build_goal_tree(parsed: &ParsedIntent) -> GoalTree {
    let mut children: Vec<GoalNode> = parsed
        .sub_goals
        .iter()
        .enumerate()
        .map(|(i, g)| GoalNode {
            id: format!("sub-goal-{}", i + 1),
            description: g.clone(),
            status: GoalStatus::Pending,
            children: vec![],
            depends_on: vec![],
        })
        .collect();

    // Add environment-specific goal
    children.push(GoalNode {
        id: "env-check".to_string(),
        description: format!("Verify execution in {} environment", parsed.environment),
        status: GoalStatus::Pending,
        children: vec![],
        depends_on: vec![],
    });

    GoalTree {
        root: GoalNode {
            id: "root".to_string(),
            description: parsed.objective.clone(),
            status: GoalStatus::Pending,
            children,
            depends_on: vec![],
        },
    }
}

fn infer_data_boundaries(parsed: &ParsedIntent) -> Vec<String> {
    parsed.data_domains.clone()
}

fn infer_action_modes(parsed: &ParsedIntent) -> Vec<ActionMode> {
    parsed
        .actions
        .iter()
        .filter_map(|a| match a.as_str() {
            "DRAFT_ONLY" => Some(ActionMode::DraftOnly),
            "MUTABLE_WRITE" => Some(ActionMode::MutableWrite),
            "COMMIT_READY" => Some(ActionMode::CommitReady),
            _ => None,
        })
        .collect()
}

fn infer_approval_policy(parsed: &ParsedIntent) -> HumanApprovalPolicy {
    if parsed.risk_level == "high" {
        HumanApprovalPolicy {
            approval_mode: ApprovalMode::ExplicitApproval,
            authorized_roles: vec!["Admin".to_string(), "SRE_Lead".to_string()],
            negative_consent_timeout_secs: 0,
            mfa_auth_url: Some("https://coevo.local/mfa".to_string()),
        }
    } else {
        HumanApprovalPolicy {
            approval_mode: ApprovalMode::NegativeConsent,
            authorized_roles: vec!["Admin".to_string()],
            negative_consent_timeout_secs: 300,
            mfa_auth_url: None,
        }
    }
}

fn infer_evidence_requirement(_parsed: &ParsedIntent) -> EvidenceRequirement {
    EvidenceRequirement {
        minimum_level: "unit_tests_passing".to_string(),
        require_json_report: true,
    }
}

fn infer_risk_tolerance(parsed: &ParsedIntent) -> RiskToleranceProfile {
    RiskToleranceProfile {
        max_risk_score: match parsed.risk_level.as_str() {
            "high" => 0.9,
            "medium" => 0.6,
            _ => 0.3,
        },
        allow_emergency_lease: parsed.risk_level == "high",
    }
}

fn infer_termination_policy(parsed: &ParsedIntent) -> TerminationPolicy {
    TerminationPolicy {
        max_token_budget: 100_000,
        max_hops: parsed.estimated_hops + 2,
        max_latency_ms: parsed.estimated_duration_ms * 3,
        max_stance_rounds: 3,
    }
}

fn infer_responsibility_policy(parsed: &ParsedIntent) -> ResponsibilityAnchorPolicy {
    let required_human_roles = if parsed.risk_level == "high" {
        vec!["CISO".to_string(), "SRE_Lead".to_string()]
    } else {
        vec!["Admin".to_string()]
    };
    ResponsibilityAnchorPolicy {
        required_human_roles,
        agent_forbidden_actions: vec![
            "urn:coevo:action:production:delete_customer_data".to_string(),
            "urn:coevo:action:production:financial_transfer".to_string(),
        ],
    }
}

// ---- Compile errors ----

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("empty user intent")]
    EmptyIntent,
    #[error("intent ambiguity too high: {score:.2} — {detail}")]
    AmbiguityTooHigh { score: f64, detail: String },
    #[error("institution policy violation: {violations:?}")]
    InstitutionViolation {
        violations: Vec<String>,
        contract_hash: String,
    },
    #[error("policy engine error: {0}")]
    PolicyEngineError(String),
    #[error("serialization error: {0}")]
    SerializationError(String),
}
