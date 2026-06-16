use crate::error::WorkerError;
use crate::types::*;
use coevo_evolution::{analyzer::FailureAnalyzer, generator::SkillGenerator};
use coevo_store::repos_opc::{agent_memory_repo, skill_evolution_repo};
use sqlx::SqlitePool;

pub struct SelfUpgradeLoop;
impl SelfUpgradeLoop {
    pub async fn run(
        model_pool: &SqlitePool,
        proposal_pool: &SqlitePool,
        run: &WorkerRun,
        reflection: &ReflectionRecord,
        feedback: Option<&str>,
    ) -> Result<Option<String>, WorkerError> {
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Write to AgentMemory
        if let Ok(Some(mut am)) =
            agent_memory_repo::AgentMemoryRepo::get(proposal_pool, &run.agent_id).await
        {
            if run.status == WorkerRunStatus::Completed {
                am.successful_patterns = vec!["worker harness completed".into()];
            } else {
                am.recurring_failures = vec!["worker harness failed".into()];
            }
            agent_memory_repo::AgentMemoryRepo::upsert(proposal_pool, &am)
                .await
                .map_err(|e| WorkerError::Internal(e.to_string()))?;
        }

        // Create SkillEvolutionProposal if needed
        let skill_updates: Vec<String> = reflection
            .skill_to_update_json
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let reflection_failures: Vec<String> = reflection
            .what_failed_json
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let has_feedback = feedback.map(|f| !f.is_empty()).unwrap_or(false);

        if !skill_updates.is_empty() || has_feedback || reflection.needs_human_review {
            let diagnosis_source = if has_feedback {
                feedback.unwrap_or("").to_string()
            } else if !reflection_failures.is_empty() {
                reflection_failures.join("; ")
            } else {
                format!("Skill update needed: {:?}", skill_updates)
            };
            let analysis = FailureAnalyzer::analyze(&diagnosis_source, false, false, None);
            let mut proposal = match SkillGenerator::generate_from_failure(
                model_pool,
                &analysis,
                "skill-mission-draft",
                &run.agent_id,
            )
            .await
            {
                Ok(proposal) => proposal,
                Err(error) => {
                    return Err(WorkerError::Internal(format!(
                        "skill proposal generation failed for {}: {}",
                        run.agent_id, error
                    )))
                }
            };
            proposal.source_refs = vec![run.run_id.clone()];
            proposal.created_at_ms = now;
            skill_evolution_repo::SkillEvolutionRepo::create_proposal(proposal_pool, &proposal)
                .await
                .map_err(|e| WorkerError::Internal(e.to_string()))?;
            return Ok(Some(proposal.proposal_id));
        }
        Ok(None)
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
            .bind("local-self-upgrade-test")
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

    async fn start_self_upgrade_server() -> (String, tokio::sync::oneshot::Sender<()>) {
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
                                    "diagnosis": "The worker failed because the recovery instructions did not require evidence-backed validation after the denied tool step.",
                                    "proposed_changes": "Add a recovery branch that records the denied tool step, asks for alternate evidence, and performs a validation checkpoint before returning the final answer.",
                                    "expected_benefit": "Keeps self-upgrade proposals grounded in the actual failure and reduces repeat denial loops.",
                                    "risk_assessment": "LOW",
                                    "generated_tests": [{
                                        "description": "should require alternate evidence after a denied tool step",
                                        "pass_criteria": ["The proposal tells the worker to seek alternate evidence and validate it before answering."]
                                    }]
                                }).to_string()
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": {
                            "prompt_tokens": 13,
                            "completion_tokens": 8,
                            "total_tokens": 21
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
    async fn self_upgrade_loop_should_not_store_placeholder_patch_text() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let (base_url, shutdown_tx) = start_self_upgrade_server().await;
        configure_active_openai_compatible(&pool, &base_url, "local-test-key").await;

        let run = WorkerRun {
            run_id: "run-self-upgrade-placeholder".to_string(),
            work_order_id: "wo-self-upgrade-placeholder".to_string(),
            agent_id: "agent-pm-01".to_string(),
            worker_id: "worker-agent-pm-01".to_string(),
            session_id: "session-self-upgrade-placeholder".to_string(),
            status: WorkerRunStatus::Failed,
            result_json: serde_json::json!({}),
            memory_ids_json: serde_json::json!([]),
            errors_json: serde_json::json!(["tool missing"]),
            audit_ref: None,
            started_at_ms: chrono::Utc::now().timestamp_millis(),
            ended_at_ms: None,
        };
        let reflection = ReflectionRecord {
            reflection_id: "reflection-self-upgrade-placeholder".to_string(),
            work_order_id: run.work_order_id.clone(),
            run_id: run.run_id.clone(),
            agent_id: run.agent_id.clone(),
            worker_id: run.worker_id.clone(),
            what_worked_json: serde_json::json!([]),
            what_failed_json: serde_json::json!(["tool missing"]),
            memory_to_add_json: serde_json::json!([]),
            skill_to_update_json: serde_json::json!(["skill-mission-draft"]),
            user_preference_observed_json: serde_json::json!([]),
            needs_human_review: false,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };

        let proposal_id =
            SelfUpgradeLoop::run(&pool, &pool, &run, &reflection, Some("tool missing"))
                .await
                .unwrap()
                .unwrap();

        let stored_changes: String = sqlx::query_scalar(
            "SELECT proposed_changes FROM skill_evolution_proposals WHERE proposal_id = ?",
        )
        .bind(proposal_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_ne!(stored_changes, "Auto-patch from SelfUpgradeLoop");
        assert!(stored_changes.contains("validation checkpoint"));
        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn self_upgrade_loop_generates_proposal_from_reflection_failures_without_skill_list() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let (base_url, shutdown_tx) = start_self_upgrade_server().await;
        configure_active_openai_compatible(&pool, &base_url, "local-test-key").await;

        let run = WorkerRun {
            run_id: "run-self-upgrade-failure-only".to_string(),
            work_order_id: "wo-self-upgrade-failure-only".to_string(),
            agent_id: "agent-pm-01".to_string(),
            worker_id: "worker-agent-pm-01".to_string(),
            session_id: "session-self-upgrade-failure-only".to_string(),
            status: WorkerRunStatus::Failed,
            result_json: serde_json::json!({}),
            memory_ids_json: serde_json::json!([]),
            errors_json: serde_json::json!(["file read denied"]),
            audit_ref: None,
            started_at_ms: chrono::Utc::now().timestamp_millis(),
            ended_at_ms: None,
        };
        let reflection = ReflectionRecord {
            reflection_id: "reflection-self-upgrade-failure-only".to_string(),
            work_order_id: run.work_order_id.clone(),
            run_id: run.run_id.clone(),
            agent_id: run.agent_id.clone(),
            worker_id: run.worker_id.clone(),
            what_worked_json: serde_json::json!([]),
            what_failed_json: serde_json::json!(["Step CallTool: File read denied"]),
            memory_to_add_json: serde_json::json!([]),
            skill_to_update_json: serde_json::json!([]),
            user_preference_observed_json: serde_json::json!([]),
            needs_human_review: true,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };

        let proposal_id = SelfUpgradeLoop::run(&pool, &pool, &run, &reflection, None)
            .await
            .expect("self-upgrade should succeed")
            .expect("proposal should be created");

        let stored_changes: String = sqlx::query_scalar(
            "SELECT proposed_changes FROM skill_evolution_proposals WHERE proposal_id = ?",
        )
        .bind(proposal_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(!stored_changes.trim().is_empty());
        assert!(stored_changes.contains("alternate evidence"));
        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn self_upgrade_loop_returns_error_when_generation_is_unavailable() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        let run = WorkerRun {
            run_id: "run-self-upgrade-no-provider".to_string(),
            work_order_id: "wo-self-upgrade-no-provider".to_string(),
            agent_id: "agent-pm-01".to_string(),
            worker_id: "worker-agent-pm-01".to_string(),
            session_id: "session-self-upgrade-no-provider".to_string(),
            status: WorkerRunStatus::Failed,
            result_json: serde_json::json!({}),
            memory_ids_json: serde_json::json!([]),
            errors_json: serde_json::json!(["tool missing"]),
            audit_ref: None,
            started_at_ms: chrono::Utc::now().timestamp_millis(),
            ended_at_ms: None,
        };
        let reflection = ReflectionRecord {
            reflection_id: "reflection-self-upgrade-no-provider".to_string(),
            work_order_id: run.work_order_id.clone(),
            run_id: run.run_id.clone(),
            agent_id: run.agent_id.clone(),
            worker_id: run.worker_id.clone(),
            what_worked_json: serde_json::json!([]),
            what_failed_json: serde_json::json!(["tool missing"]),
            memory_to_add_json: serde_json::json!([]),
            skill_to_update_json: serde_json::json!(["skill-mission-draft"]),
            user_preference_observed_json: serde_json::json!([]),
            needs_human_review: false,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };

        let err = SelfUpgradeLoop::run(&pool, &pool, &run, &reflection, Some("tool missing"))
            .await
            .expect_err("self-upgrade loop should surface generation failure");
        assert!(
            err.to_string().contains("skill proposal generation failed"),
            "unexpected error: {err}"
        );

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skill_evolution_proposals")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
