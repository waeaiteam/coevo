//! Skill Generator: produce SkillEvolutionProposal from failure analysis.

use coevo_core::skills::*;
use coevo_models::{
    gateway::select_gateway,
    openai::extract_structured_json_text,
    types::{ModelMessage, ModelProviderConfig, ModelRequest, ModelRole, ResponseFormat},
};
use coevo_store::repos::model_config_repo::ModelConfigRepo;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct SkillGenerator;

impl SkillGenerator {
    pub async fn generate_from_failure(
        model_pool: &SqlitePool,
        analysis: &FailureAnalysis,
        target_skill_id: &str,
        created_by_agent: &str,
    ) -> Result<SkillEvolutionProposal, String> {
        let proposal_type = match analysis.category {
            FailureCategory::MissingCapability => EvolutionProposalType::CreateNewSkill,
            FailureCategory::BadPromptProcedure | FailureCategory::WrongToolUse => {
                EvolutionProposalType::PatchSkill
            }
            FailureCategory::MemoryStale | FailureCategory::PolicyViolation => {
                EvolutionProposalType::PatchSkill
            }
            _ => EvolutionProposalType::PatchSkill,
        };

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let generated = Self::generate_content(
            model_pool,
            analysis,
            target_skill_id,
            created_by_agent,
            proposal_type,
        )
        .await
        .ok_or_else(|| {
            format!(
                "skill proposal generation failed for {target_skill_id}: provider returned no valid structured proposal"
            )
        })?;

        Ok(SkillEvolutionProposal {
            proposal_id: format!("evol-{}", Uuid::new_v4()),
            source_type: EvolutionSourceType::Failure,
            source_refs: vec![],
            target_skill_id: target_skill_id.to_string(),
            proposal_type,
            diagnosis: generated.diagnosis,
            proposed_changes: generated.proposed_changes,
            expected_benefit: generated.expected_benefit,
            risk_assessment: generated.risk_assessment,
            generated_tests: generated.generated_tests,
            status: EvolutionProposalStatus::Draft,
            created_by_agent: created_by_agent.to_string(),
            created_at_ms: now,
        })
    }

    async fn generate_content(
        model_pool: &SqlitePool,
        analysis: &FailureAnalysis,
        target_skill_id: &str,
        created_by_agent: &str,
        proposal_type: EvolutionProposalType,
    ) -> Option<GeneratedProposalContent> {
        let config = ModelConfigRepo::get_active_config_or_seed(model_pool)
            .await
            .ok()?;
        if config.kind == coevo_models::types::ModelProviderKind::Mock {
            return None;
        }
        let gateway = select_gateway(config.kind);
        let request = ModelRequest {
            config: config.clone(),
            role: ModelRole::SkillGenerator,
            model: preferred_skill_model(&config),
            messages: vec![
                ModelMessage {
                    role: "system".to_string(),
                    content: "You generate governed skill evolution proposals for a backend agent platform. Return concise JSON only. Never suggest permission escalation, policy bypass, or unrelated refactors.".to_string(),
                    ..Default::default()
                },
                ModelMessage {
                    role: "user".to_string(),
                    content: format!(
                        "Create a skill evolution proposal.\nTarget skill: {target_skill_id}\nCreated by: {created_by_agent}\nProposal type: {:?}\nFailure category: {:?}\nRoot cause: {}\nSuspected missing skill: {}\nSuspected skill bug: {}\nRequires memory update: {}\nRequires policy update: {}\nFallback risk assessment: {}\nReturn fields diagnosis, proposed_changes, expected_benefit, risk_assessment, generated_tests. generated_tests must be an array of objects with description and pass_criteria.",
                        proposal_type,
                        analysis.category,
                        analysis.root_cause,
                        analysis
                            .suspected_missing_skill
                            .as_deref()
                            .unwrap_or(""),
                        analysis
                            .suspected_skill_bug
                            .as_deref()
                            .unwrap_or(""),
                        analysis.required_memory_update,
                        analysis.required_policy_update,
                        Self::assess_risk(analysis),
                    ),
                    ..Default::default()
                },
            ],
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            response_format: ResponseFormat::Json,
            stream: false,
            tools: vec![],
            tool_choice: None,
        };
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "diagnosis": { "type": "string" },
                "proposed_changes": { "type": "string" },
                "expected_benefit": { "type": "string" },
                "risk_assessment": { "type": "string" },
                "generated_tests": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" },
                            "pass_criteria": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        },
                        "required": ["description", "pass_criteria"]
                    }
                }
            },
            "required": [
                "diagnosis",
                "proposed_changes",
                "expected_benefit",
                "risk_assessment",
                "generated_tests"
            ]
        });

        let response = gateway.structured(&request, &schema).await.ok()?;
        let json = response.json.or_else(|| {
            extract_structured_json_text(&response.content)
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        })?;
        parse_generated_content(&json, target_skill_id)
    }

    fn assess_risk(analysis: &FailureAnalysis) -> String {
        match analysis.category {
            FailureCategory::PolicyViolation | FailureCategory::ExternalExecutorFailure => {
                "HIGH - requires human review".to_string()
            }
            FailureCategory::HallucinatedFact | FailureCategory::OverConfidentDecision => {
                "MEDIUM - needs verifier validation".to_string()
            }
            _ => "LOW - auto-verifiable".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct GeneratedProposalContent {
    diagnosis: String,
    proposed_changes: String,
    expected_benefit: String,
    risk_assessment: String,
    generated_tests: Vec<SkillTestCase>,
}

fn preferred_skill_model(config: &ModelProviderConfig) -> String {
    if !config.structured_output_model.is_empty() {
        config.structured_output_model.clone()
    } else if !config.reasoning_model.is_empty() {
        config.reasoning_model.clone()
    } else {
        config.default_model.clone()
    }
}

fn parse_generated_content(
    json: &serde_json::Value,
    target_skill_id: &str,
) -> Option<GeneratedProposalContent> {
    let diagnosis = json.get("diagnosis")?.as_str()?.trim().to_string();
    let proposed_changes = json.get("proposed_changes")?.as_str()?.trim().to_string();
    let expected_benefit = json.get("expected_benefit")?.as_str()?.trim().to_string();
    let risk_assessment = json.get("risk_assessment")?.as_str()?.trim().to_string();
    let generated_tests_json = coerce_generated_tests(json.get("generated_tests")?)?;
    let generated_tests = generated_tests_json
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let description = item.get("description")?.as_str()?.trim();
            if description.is_empty() {
                return None;
            }
            let pass_criteria = coerce_pass_criteria(item.get("pass_criteria")?)?;
            Some(skill_test_case(
                target_skill_id,
                &format!("llm-{}", idx + 1),
                description.to_string(),
                pass_criteria.join("; ").as_str(),
            ))
        })
        .collect::<Vec<_>>();
    if diagnosis.is_empty()
        || proposed_changes.is_empty()
        || expected_benefit.is_empty()
        || risk_assessment.is_empty()
        || generated_tests.is_empty()
    {
        return None;
    }
    Some(GeneratedProposalContent {
        diagnosis,
        proposed_changes,
        expected_benefit,
        risk_assessment,
        generated_tests,
    })
}

fn coerce_generated_tests(value: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    match value {
        serde_json::Value::Array(items) if !items.is_empty() => Some(items.clone()),
        serde_json::Value::Object(_) => Some(vec![value.clone()]),
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return coerce_generated_tests(&parsed);
            }
            Some(vec![serde_json::json!({
                "description": trimmed,
                "pass_criteria": [trimmed]
            })])
        }
        _ => None,
    }
}

fn coerce_pass_criteria(value: &serde_json::Value) -> Option<Vec<String>> {
    match value {
        serde_json::Value::Array(items) => {
            let collected = items
                .iter()
                .filter_map(|v| v.as_str().map(str::trim))
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            (!collected.is_empty()).then_some(collected)
        }
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then_some(vec![trimmed.to_string()])
        }
        _ => None,
    }
}

fn skill_test_case(
    target_skill_id: &str,
    suffix: &str,
    description: String,
    pass_criterion: &str,
) -> SkillTestCase {
    SkillTestCase {
        test_id: format!("{target_skill_id}-{suffix}"),
        description,
        input: serde_json::json!({ "skill_id": target_skill_id }),
        expected_output_schema: serde_json::json!({ "type": "object" }),
        forbidden_behaviors: vec![
            "permission escalation".to_string(),
            "policy bypass".to_string(),
        ],
        required_evidence: vec!["proposal review".to_string()],
        pass_criteria: vec![pass_criterion.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn configure_active_openai_compatible(
        pool: &sqlx::SqlitePool,
        base_url: &str,
        api_key: &str,
    ) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("local-skillgen-test")
            .bind("OpenAICompatible")
            .bind(base_url)
            .bind(api_key)
            .bind(format!("{}***", &api_key[..api_key.len().min(4)]))
            .bind("gpt-4o")
            .bind("gpt-4o-mini")
            .bind("o3-mini")
            .bind("gpt-4o")
            .bind(16384)
            .bind(0.2)
            .bind(30000)
            .bind(5.0)
            .bind(1)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn start_skill_generator_server() -> (String, tokio::sync::oneshot::Sender<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tokio::select! {
                _ = async move {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut buf = Vec::new();
                    let mut chunk = [0_u8; 4096];
                    let mut content_length = None;
                    loop {
                        let n = socket.read(&mut chunk).await.unwrap();
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        let text = String::from_utf8_lossy(&buf);
                        if let Some(header_end) = text.find("\r\n\r\n") {
                            if content_length.is_none() {
                                let headers = &text[..header_end];
                                for line in headers.lines() {
                                    let lower = line.to_ascii_lowercase();
                                    if let Some(value) = lower.strip_prefix("content-length:") {
                                        content_length = value.trim().parse::<usize>().ok();
                                        break;
                                    }
                                }
                            }
                            if let Some(expected) = content_length {
                                let body_len = buf.len().saturating_sub(header_end + 4);
                                if body_len >= expected {
                                    break;
                                }
                            }
                        }
                    }
                    let body = serde_json::json!({
                        "choices": [{
                            "message": {
                                "content": serde_json::json!({
                                    "diagnosis": "The worker skipped required validation before answering.",
                                    "proposed_changes": "Add an explicit validation checkpoint before final answer emission, and require the skill to name the selected employee and executor boundary in its final summary.",
                                    "expected_benefit": "Improves traceability and prevents vague completions when governance-sensitive steps are skipped.",
                                    "risk_assessment": "LOW",
                                    "generated_tests": [{
                                        "description": "should mention the validation checkpoint",
                                        "pass_criteria": ["The generated proposal requires a validation checkpoint before final answer emission."]
                                    }]
                                }).to_string()
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": {
                            "prompt_tokens": 11,
                            "completion_tokens": 7,
                            "total_tokens": 18
                        }
                    }).to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                } => {}
                _ = async move {
                    let _ = shutdown_rx.await;
                } => {}
            }
        });
        (format!("http://{addr}/v1"), shutdown_tx)
    }

    #[tokio::test]
    async fn generate_from_failure_should_not_leave_placeholder_patch_text() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let (base_url, shutdown_tx) = start_skill_generator_server().await;
        configure_active_openai_compatible(&pool, &base_url, "local-test-key").await;

        let analysis = FailureAnalysis {
            category: FailureCategory::BadPromptProcedure,
            root_cause: "The worker skipped required validation and returned a vague answer."
                .to_string(),
            suspected_missing_skill: Some("skill-mission-draft".to_string()),
            suspected_skill_bug: Some("prompt-procedure-bug".to_string()),
            required_memory_update: false,
            required_policy_update: false,
        };

        let proposal = SkillGenerator::generate_from_failure(
            &pool,
            &analysis,
            "skill-mission-draft",
            "agent-pm-01",
        )
        .await
        .expect("proposal should be generated");

        assert_ne!(
            proposal.proposed_changes,
            "Auto-generated patch based on failure analysis"
        );
        assert!(
            proposal.proposed_changes.contains("validation checkpoint"),
            "{}",
            proposal.proposed_changes
        );
        assert!(!proposal.generated_tests.is_empty());
        assert!(!proposal.expected_benefit.is_empty());
        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn generate_from_failure_reads_provider_config_from_separate_pool() {
        let model_pool = create_test_pool().await.unwrap();
        run_migrations(&model_pool).await.unwrap();
        let (base_url, shutdown_tx) = start_skill_generator_server().await;
        configure_active_openai_compatible(&model_pool, &base_url, "local-test-key").await;

        let proposal_pool = create_test_pool().await.unwrap();
        run_migrations(&proposal_pool).await.unwrap();

        let analysis = FailureAnalysis {
            category: FailureCategory::BadPromptProcedure,
            root_cause: "The worker skipped required validation and returned a vague answer."
                .to_string(),
            suspected_missing_skill: Some("skill-mission-draft".to_string()),
            suspected_skill_bug: Some("prompt-procedure-bug".to_string()),
            required_memory_update: false,
            required_policy_update: false,
        };

        let proposal = SkillGenerator::generate_from_failure(
            &model_pool,
            &analysis,
            "skill-mission-draft",
            "agent-pm-01",
        )
        .await
        .expect("proposal should use provider config from global pool");

        assert!(!proposal.proposed_changes.trim().is_empty());
        assert!(!proposal.generated_tests.is_empty());
        proposal_pool.close().await;
        model_pool.close().await;
        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn generate_from_failure_returns_error_when_only_mock_provider_is_available() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        let analysis = FailureAnalysis {
            category: FailureCategory::BadPromptProcedure,
            root_cause: "The worker skipped required validation and returned a vague answer."
                .to_string(),
            suspected_missing_skill: Some("skill-mission-draft".to_string()),
            suspected_skill_bug: Some("prompt-procedure-bug".to_string()),
            required_memory_update: false,
            required_policy_update: false,
        };

        let result = SkillGenerator::generate_from_failure(
            &pool,
            &analysis,
            "skill-mission-draft",
            "agent-pm-01",
        )
        .await;

        assert!(result.is_err());
    }

    #[test]
    fn parse_generated_content_requires_non_empty_pass_criteria() {
        let json = serde_json::json!({
            "diagnosis": "The task skipped a governed validation step.",
            "proposed_changes": "Insert an explicit validation checkpoint before final answer emission.",
            "expected_benefit": "Keeps governed answers grounded in verified evidence.",
            "risk_assessment": "LOW",
            "generated_tests": [
                {
                    "description": "should mention the validation checkpoint",
                    "pass_criteria": []
                }
            ]
        });

        let parsed = parse_generated_content(&json, "skill-mission-draft");

        assert!(parsed.is_none());
    }
}
