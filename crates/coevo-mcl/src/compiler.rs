//! MCL Compiler — translates unstructured user intent into a declarative MCLSpec.
//! Per coevo whitepaper Section 2.1 and Section 4.

use coevo_core::contract::*;
use coevo_core::metadata::CommonMetadataHeader;
use coevo_policy::traits::PolicyEngine;

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
                detail: "Intent is too ambiguous; please provide more specific instructions"
                    .to_string(),
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
                        warnings.push(format!(
                            "Policy warning [{}]: {}",
                            v.policy_urn, v.description
                        ));
                    }
                }
            }
        }

        // ---- Phase 4: Hash the contract ----
        let contract_hash = hash_contract(&contract)
            .map_err(|e| CompileError::SerializationError(e.to_string()))?;

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

/// A semantic concept the intent parser can recognise, defined by a bilingual
/// keyword table. ASCII terms are matched case-insensitively; CJK terms are
/// matched against the raw intent string (lowercasing is a no-op for CJK and
/// would only risk normalising away the characters we want to match).
struct Concept {
    /// Stable identifier, used by the classification logic below.
    id: ConceptId,
    /// English / ASCII terms, lowercase. Matched against the lowercased intent.
    en: &'static [&'static str],
    /// Chinese / CJK terms. Matched against the raw intent.
    zh: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConceptId {
    Read,
    Write,
    Delete,
    Deploy,
    Production,
    Staging,
    Database,
    File,
    Payment,
    CustomerData,
    EmailNotify,
    Shell,
    ExternalApi,
}

/// Bilingual, data-driven concept table (English + Chinese).
///
/// Each concept maps a normalised meaning to the surface terms that express it
/// in either language. The parser classifies a mission by the *union* of every
/// concept whose English or Chinese terms appear in the intent, so a mixed
/// mission like "部署 backend 到 production" lights up both `Deploy` (zh) and
/// `Production` (en).
const CONCEPTS: &[Concept] = &[
    Concept {
        id: ConceptId::Read,
        en: &[
            "read",
            "readonly",
            "read-only",
            "query",
            "analyze",
            "analyse",
            "analysis",
            "summarize",
            "summarise",
            "summary",
            "retrieve",
            "search",
            "view",
            "inspect",
            "fetch",
            "list",
        ],
        zh: &["读", "读取", "查看", "分析", "总结", "查询", "检索"],
    },
    Concept {
        id: ConceptId::Write,
        en: &[
            "write", "create", "modify", "update", "edit", "generate", "add", "insert", "append",
            "patch",
        ],
        zh: &["写", "写入", "修改", "创建", "新建", "编辑", "生成"],
    },
    Concept {
        id: ConceptId::Delete,
        en: &[
            "delete", "remove", "drop", "purge", "clear", "truncate", "wipe",
        ],
        zh: &["删", "删除", "移除", "清空", "清除"],
    },
    Concept {
        id: ConceptId::Deploy,
        en: &[
            "deploy",
            "release",
            "publish",
            "ship",
            "rollout",
            "roll out",
            "rollback",
            "roll back",
            "canary",
        ],
        zh: &["部署", "发布", "上线", "灰度"],
    },
    Concept {
        id: ConceptId::Production,
        en: &["production", "prod", "live", "live env"],
        zh: &["生产", "线上", "正式环境"],
    },
    Concept {
        id: ConceptId::Staging,
        en: &[
            "staging",
            "stage",
            "test env",
            "test environment",
            "sandbox",
            "preprod",
            "pre-prod",
            "pre-release",
        ],
        zh: &["预发", "测试环境", "沙箱"],
    },
    Concept {
        id: ConceptId::Database,
        en: &["database", "db", "sql", "table", "schema"],
        zh: &["数据库", "库表", "sql"],
    },
    Concept {
        id: ConceptId::File,
        en: &["file", "directory", "folder", "storage", "filesystem"],
        zh: &["文件", "目录", "文件夹"],
    },
    Concept {
        id: ConceptId::Payment,
        en: &[
            "payment", "pay", "transfer", "refund", "billing", "invoice", "charge", "payout",
        ],
        zh: &["支付", "付款", "转账", "退款", "账单"],
    },
    Concept {
        id: ConceptId::CustomerData,
        en: &[
            "customer data",
            "user data",
            "personal info",
            "personal information",
            "privacy",
            "pii",
        ],
        zh: &["客户数据", "用户数据", "个人信息", "隐私"],
    },
    Concept {
        id: ConceptId::EmailNotify,
        en: &[
            "email",
            "e-mail",
            "notify",
            "notification",
            "broadcast",
            "alert",
        ],
        zh: &["邮件", "发邮件", "通知", "群发"],
    },
    Concept {
        id: ConceptId::Shell,
        en: &[
            "shell",
            "command",
            "script",
            "terminal",
            "bash",
            "cmd",
            "run command",
        ],
        zh: &["命令", "脚本", "终端", "执行命令"],
    },
    Concept {
        id: ConceptId::ExternalApi,
        en: &[
            "call api",
            "external api",
            "api",
            "endpoint",
            "http request",
            "rest",
            "webhook",
        ],
        zh: &["调用接口", "api", "请求"],
    },
];

/// The concepts that represent a concrete *action* the mission wants taken.
/// A mission that matches none of these has no recognisable verb and is treated
/// as highly ambiguous (and therefore conservatively).
const ACTION_CONCEPTS: [ConceptId; 4] = [
    ConceptId::Read,
    ConceptId::Write,
    ConceptId::Delete,
    ConceptId::Deploy,
];

/// Returns true if `concept` is expressed anywhere in the intent.
/// `lower` is the lowercased intent (for ASCII terms); `raw` is the original
/// (for CJK terms). ASCII terms are also lowercased so the comparison is
/// case-insensitive on both sides.
fn concept_matches(concept: &Concept, lower: &str, raw: &str) -> bool {
    concept.en.iter().any(|term| lower.contains(term))
        || concept.zh.iter().any(|term| raw.contains(term))
}

/// Count of CJK (Han) ideographs in the string. Used to approximate a word
/// count for missions written in Chinese, where whitespace tokenisation
/// under-counts severely (often the whole mission is a single "word").
fn cjk_char_count(s: &str) -> usize {
    s.chars().filter(|c| is_cjk(*c)).count()
}

/// Whether a char is a CJK ideograph (covers the common BMP + Ext-A ranges and
/// the SIP via the surrogate-free `char` value).
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF      // CJK Unified Ideographs Extension A
        | 0x4E00..=0x9FFF    // CJK Unified Ideographs
        | 0xF900..=0xFAFF    // CJK Compatibility Ideographs
        | 0x2_0000..=0x2_A6DF // Extension B
        | 0x2_A700..=0x2_EBEF // Extensions C–F
    )
}

/// Effective word count that works across scripts: ASCII/space-delimited tokens
/// plus roughly one word per two CJK characters (Chinese averages ~2 chars per
/// lexical word).
fn effective_word_count(intent: &str) -> usize {
    let cjk = cjk_char_count(intent);
    // Whitespace tokens that aren't purely CJK (CJK is counted separately).
    let ascii_words = intent
        .split_whitespace()
        .filter(|w| w.chars().any(|c| !is_cjk(c)))
        .count();
    ascii_words + cjk.div_ceil(2)
}

/// Keyword-based, bilingual intent parser.
///
/// Classification is driven entirely by the [`CONCEPTS`] table (English +
/// Chinese), so the two languages are treated identically and mixed-language
/// missions classify by the union of their matches. In production this would be
/// backed by an LLM for semantic understanding.
fn parse_intent(user_intent: &str) -> Result<ParsedIntent, CompileError> {
    let raw = user_intent.trim();
    if raw.is_empty() {
        return Err(CompileError::EmptyIntent);
    }
    let lower = raw.to_lowercase();

    // ---- Concept detection (union of English + Chinese matches) ----
    let has = |id: ConceptId| -> bool {
        CONCEPTS
            .iter()
            .find(|c| c.id == id)
            .map(|c| concept_matches(c, &lower, raw))
            .unwrap_or(false)
    };

    let read = has(ConceptId::Read);
    let write = has(ConceptId::Write);
    let delete = has(ConceptId::Delete);
    let deploy = has(ConceptId::Deploy);
    let production = has(ConceptId::Production);
    let staging = has(ConceptId::Staging);
    let database = has(ConceptId::Database);
    let file = has(ConceptId::File);
    let payment = has(ConceptId::Payment);
    let customer = has(ConceptId::CustomerData);
    let notify = has(ConceptId::EmailNotify);
    let shell = has(ConceptId::Shell);
    let external_api = has(ConceptId::ExternalApi);

    // ---- Environment ----
    let environment = if production {
        "production"
    } else if staging {
        "staging"
    } else {
        "development"
    };

    // ---- Explicit risk markers (bilingual) ----
    let explicit_high = ["high risk", "dangerous", "critical"]
        .iter()
        .any(|t| lower.contains(t))
        || ["高风险", "危险", "严重", "紧急"]
            .iter()
            .any(|t| raw.contains(t));
    let explicit_medium = ["medium risk", "moderate"]
        .iter()
        .any(|t| lower.contains(t))
        || ["中风险", "谨慎"].iter().any(|t| raw.contains(t));

    // ---- Risk level: data-driven from concepts, then explicit markers ----
    // High: irreversible / regulated operations, or a mutation aimed at prod.
    let mutating = write || deploy || delete;
    let risk_level = if delete || payment || customer || (mutating && production) || explicit_high {
        "high"
    } else if write || deploy || shell || staging || explicit_medium {
        "medium"
    } else {
        "low"
    };

    // ---- Actions (allowed action modes) ----
    let mut actions = vec![];
    if read {
        actions.push("DRAFT_ONLY".to_string());
    }
    if write {
        actions.push("MUTABLE_WRITE".to_string());
    }
    if deploy || delete {
        actions.push("COMMIT_READY".to_string());
    }
    if actions.is_empty() {
        // No recognised action verb — default to the least-privileged mode.
        actions.push("DRAFT_ONLY".to_string());
    }

    // ---- Data domains ----
    let mut data_domains = vec!["urn:coevo:data:default".to_string()];
    if database {
        data_domains.push("urn:coevo:data:database".to_string());
    }
    if file {
        data_domains.push("urn:coevo:data:filesystem".to_string());
    }
    if payment {
        data_domains.push("urn:coevo:data:payment".to_string());
    }
    if customer {
        data_domains.push("urn:coevo:data:customer".to_string());
    }
    if notify || external_api {
        data_domains.push("urn:coevo:data:network".to_string());
    }
    if shell {
        data_domains.push("urn:coevo:data:shell".to_string());
    }

    // ---- Ambiguity (CJK-aware) ----
    // A mission that matches no action concept has no recognisable verb: it is
    // maximally ambiguous and must trip the hard ambiguity gate so it is
    // treated conservatively rather than silently passing as low-risk draft.
    let action_concepts_matched = ACTION_CONCEPTS.iter().filter(|id| has(**id)).count();
    let words = effective_word_count(raw);
    let ambiguity_score = if action_concepts_matched == 0 {
        // > 0.7 so `compile()` rejects with AmbiguityTooHigh. Strictly higher
        // than any classified mission below.
        0.8
    } else if words < 3 {
        0.5
    } else if words < 8 {
        0.35
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

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_core::metadata::CommonMetadataHeader;

    fn meta() -> CommonMetadataHeader {
        CommonMetadataHeader::new(
            "0".repeat(64),
            "0".repeat(64),
            "test".to_string(),
            "0".repeat(64),
            "Synthesizer".to_string(),
        )
    }

    async fn compile(intent: &str) -> Result<CompileResult, CompileError> {
        MCLCompiler::new()
            .compile(intent, "DRAFT", None, &meta())
            .await
    }

    // --- Concept-level (parser) assertions ---

    #[test]
    fn chinese_read_classifies_as_low_risk_draft_only() {
        let p = parse_intent("分析数据库表并总结查询结果").unwrap();
        assert_eq!(p.risk_level, "low", "Chinese read intent must be low risk");
        assert_eq!(
            p.actions,
            vec!["DRAFT_ONLY".to_string()],
            "Chinese read intent must be draft-only"
        );
        assert!(
            p.ambiguity_score <= 0.7,
            "classified Chinese read must not trip the ambiguity gate, got {}",
            p.ambiguity_score
        );
    }

    #[test]
    fn english_and_chinese_read_agree() {
        let en = parse_intent("read and analyze the database query results").unwrap();
        let zh = parse_intent("读取并分析数据库查询结果").unwrap();
        assert_eq!(en.risk_level, zh.risk_level);
        assert_eq!(en.actions, zh.actions);
    }

    #[test]
    fn chinese_deploy_to_production_is_highest_risk_like_english() {
        let zh = parse_intent("部署服务到生产环境").unwrap();
        let en = parse_intent("deploy the service to production").unwrap();
        assert_eq!(
            zh.risk_level, "high",
            "Chinese deploy→prod must be high risk"
        );
        assert_eq!(
            zh.risk_level, en.risk_level,
            "Chinese deploy→prod must match English risk level"
        );
        assert_eq!(zh.environment, "production");
        assert!(zh.actions.contains(&"COMMIT_READY".to_string()));
    }

    #[test]
    fn chinese_delete_database_is_high_risk() {
        let p = parse_intent("删除生产数据库").unwrap();
        assert_eq!(
            p.risk_level, "high",
            "Chinese delete-database must be high risk"
        );
        assert!(p.actions.contains(&"COMMIT_READY".to_string()));
        assert!(p
            .data_domains
            .contains(&"urn:coevo:data:database".to_string()));
    }

    #[test]
    fn chinese_payment_and_pii_are_high_risk() {
        assert_eq!(parse_intent("处理客户退款转账").unwrap().risk_level, "high");
        assert_eq!(
            parse_intent("导出用户数据和个人信息").unwrap().risk_level,
            "high"
        );
    }

    #[test]
    fn mixed_language_classifies_by_union() {
        // "部署 backend 到 production" → Deploy(zh) ∪ Production(en).
        let p = parse_intent("部署 backend 到 production").unwrap();
        assert_eq!(p.environment, "production");
        assert!(p.actions.contains(&"COMMIT_READY".to_string()));
        assert_eq!(p.risk_level, "high");
    }

    #[test]
    fn mixed_language_chinese_verb_english_noun() {
        // 修改 (write, zh) + production (en) → high-risk mutation against prod.
        let p = parse_intent("修改 production database 配置").unwrap();
        assert_eq!(p.environment, "production");
        assert!(p.actions.contains(&"MUTABLE_WRITE".to_string()));
        assert_eq!(p.risk_level, "high");
    }

    #[test]
    fn gibberish_chinese_is_highly_ambiguous() {
        // No action concept → must exceed the hard ambiguity gate (0.7).
        let p = parse_intent("天气怎么样啊今天").unwrap();
        assert!(
            p.ambiguity_score >= 0.7,
            "unclassifiable Chinese must be highly ambiguous, got {}",
            p.ambiguity_score
        );
    }

    #[test]
    fn gibberish_ascii_is_highly_ambiguous() {
        let p = parse_intent("the quick brown fox jumps").unwrap();
        assert!(p.ambiguity_score >= 0.7, "got {}", p.ambiguity_score);
    }

    #[test]
    fn unclassifiable_is_not_more_permissive_than_a_write_mission() {
        // A classified write/deploy mission must be MORE permissive (lower
        // ambiguity) than an unclassifiable one — the latter must not slip
        // through as a silent low-risk draft.
        let unclassified = parse_intent("随便看看这个东西嘛").unwrap();
        let write = parse_intent("修改测试环境的配置文件").unwrap();
        let deploy = parse_intent("部署到预发环境").unwrap();
        assert!(unclassified.ambiguity_score > write.ambiguity_score);
        assert!(unclassified.ambiguity_score > deploy.ambiguity_score);
        assert!(write.ambiguity_score <= 0.7);
        assert!(deploy.ambiguity_score <= 0.7);
    }

    #[test]
    fn cjk_word_count_does_not_over_penalise_length() {
        // A short-but-classified Chinese mission segments by CJK chars rather
        // than collapsing to a single whitespace "word".
        assert!(effective_word_count("读取并分析数据库查询结果") >= 4);
        assert!(effective_word_count("read the file") >= 3);
        // Mixed counts both scripts.
        assert!(effective_word_count("部署 backend 到 production") >= 4);
    }

    // --- End-to-end (compile) assertions ---

    #[tokio::test]
    async fn chinese_deploy_to_production_requires_explicit_approval() {
        let result = compile("部署关键修复到生产数据库")
            .await
            .expect("classified Chinese deploy must compile");
        assert_eq!(
            result.contract.human_approval_policy.approval_mode,
            ApprovalMode::ExplicitApproval,
            "Chinese deploy→prod MUST require EXPLICIT_APPROVAL, mirroring English"
        );
        assert!(result.contract.risk_tolerance_profile.allow_emergency_lease);
    }

    #[tokio::test]
    async fn english_deploy_to_production_still_requires_explicit_approval() {
        // Regression guard: English behaviour is unchanged.
        let result = compile("Deploy critical production hotfix to fix the database")
            .await
            .expect("English deploy must compile");
        assert_eq!(
            result.contract.human_approval_policy.approval_mode,
            ApprovalMode::ExplicitApproval
        );
    }

    #[tokio::test]
    async fn chinese_read_compiles_green_equivalent() {
        let result = compile("分析并总结数据库查询结果")
            .await
            .expect("Chinese read must compile");
        assert_eq!(
            result.contract.human_approval_policy.approval_mode,
            ApprovalMode::NegativeConsent,
            "Chinese read should be Green-equivalent (negative consent)"
        );
        assert_eq!(
            result.contract.allowed_action_modes,
            vec![ActionMode::DraftOnly]
        );
    }

    #[tokio::test]
    async fn gibberish_chinese_is_rejected_as_too_ambiguous() {
        let outcome = compile("天气怎么样啊今天").await;
        assert!(
            matches!(outcome, Err(CompileError::AmbiguityTooHigh { .. })),
            "unclassifiable Chinese must be rejected as too ambiguous, not silently drafted"
        );
    }

    #[tokio::test]
    async fn empty_intent_is_rejected() {
        assert!(matches!(
            compile("   ").await,
            Err(CompileError::EmptyIntent)
        ));
    }
}
