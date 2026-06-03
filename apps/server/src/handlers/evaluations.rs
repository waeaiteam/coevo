use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_models::gateway::select_gateway;
use coevo_models::types::{
    ModelMessage, ModelProviderConfig, ModelRequest, ModelRole, ResponseFormat,
};
use coevo_store::{
    migrate::run_migrations,
    pool::create_pool,
    repos::{eval_repo, eval_repo::EvalRepo, model_config_repo::ModelConfigRepo},
    repos_opc::agent_employee_repo,
};
use serde::Deserialize;
use sqlx::Row;

use crate::state::AppState;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

#[derive(Deserialize)]
pub struct CreateDatasetRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateCaseRequest {
    pub input: String,
    pub expected: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct EvalTarget {
    pub kind: String,
    pub agent_id: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Deserialize)]
pub struct EvalRunRequest {
    pub target: EvalTarget,
    pub dataset_id: String,
    pub judge_model: String,
    pub exec_model: String,
    pub metrics: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct EvalCompareRequest {
    pub agent_id: String,
    pub version_a: i32,
    pub version_b: i32,
    pub dataset_id: String,
    pub judge_model: String,
    pub exec_model: String,
}

async fn company_pool(
    state: &AppState,
    opc_id: &str,
) -> Result<sqlx::SqlitePool, (StatusCode, Json<serde_json::Value>)> {
    let company_dir = state.company_workspace.company_dir(opc_id);
    if !company_dir.exists() {
        return Err(err!(StatusCode::NOT_FOUND, "company not found"));
    }
    let pool = create_pool(
        &state
            .company_workspace
            .company_db_path(opc_id)
            .to_string_lossy(),
    )
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    run_migrations(&pool)
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(pool)
}

fn employee_prompt_path(state: &AppState, opc_id: &str, agent_id: &str) -> std::path::PathBuf {
    state
        .company_workspace
        .company_dir(opc_id)
        .join("employees")
        .join(agent_id)
        .join("prompt.md")
}

fn employee_prompt_version_path(
    state: &AppState,
    opc_id: &str,
    agent_id: &str,
    version: i32,
) -> std::path::PathBuf {
    state
        .company_workspace
        .company_dir(opc_id)
        .join("employees")
        .join(agent_id)
        .join("prompt_versions")
        .join(format!("v{version}.md"))
}

async fn resolve_target_prompt(
    state: &AppState,
    opc_id: &str,
    pool: &sqlx::SqlitePool,
    target: &EvalTarget,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    match target.kind.as_str() {
        "prompt" => Ok(target.system_prompt.clone().unwrap_or_default()),
        "agent" => {
            let Some(agent_id) = target.agent_id.as_deref() else {
                return Err(err!(StatusCode::UNPROCESSABLE_ENTITY, "agent_id is required"));
            };
            let path = employee_prompt_path(state, opc_id, agent_id);
            if path.exists() {
                return std::fs::read_to_string(path)
                    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            }
            let employee = agent_employee_repo::AgentEmployeeRepo::get(pool, agent_id)
                .await
                .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            match employee {
                Some(employee) => Ok(employee.system_prompt),
                None => Err(err!(StatusCode::NOT_FOUND, "Employee not found")),
            }
        }
        _ => Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "target.kind must be agent or prompt"
        )),
    }
}

async fn generate_output(
    state: &AppState,
    system_prompt: &str,
    input: &str,
    exec_model: &str,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let config = ModelConfigRepo::get_active_config_or_seed(&state.pool)
        .await
        .unwrap_or_else(|_| ModelProviderConfig::mock());
    let gateway = select_gateway(config.kind);
    let request = ModelRequest {
        config: config.clone(),
        role: ModelRole::Synthesizer,
        model: if exec_model.is_empty() {
            config.default_model.clone()
        } else {
            exec_model.to_string()
        },
        messages: vec![
            ModelMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ModelMessage {
                role: "user".to_string(),
                content: input.to_string(),
            },
        ],
        temperature: config.temperature,
        max_tokens: config.max_tokens,
        response_format: ResponseFormat::Text,
    };
    gateway
        .chat(&request)
        .await
        .map(|response| response.content)
        .map_err(|e| err!(StatusCode::BAD_REQUEST, e.to_string()))
}

fn heuristic_scores(output: &str, expected: Option<&str>) -> (i64, i64, String) {
    let expected = expected.unwrap_or("").trim();
    if expected.is_empty() {
        return (
            75,
            80,
            "Judge fallback: no expected answer supplied, so relevance was estimated from non-empty output.".to_string(),
        );
    }
    let output_lower = output.to_lowercase();
    let expected_lower = expected.to_lowercase();
    if output_lower.contains(&expected_lower) {
        (
            95,
            92,
            "Judge fallback: output contains the expected answer directly.".to_string(),
        )
    } else {
        let overlap = expected_lower
            .split_whitespace()
            .filter(|token| output_lower.contains(token))
            .count() as i64;
        let total = expected_lower.split_whitespace().count().max(1) as i64;
        let accuracy = ((overlap * 100) / total).clamp(20, 85);
        let relevance = (accuracy + 10).clamp(30, 90);
        (
            accuracy,
            relevance,
            "Judge fallback: estimated by token overlap between output and expected answer."
                .to_string(),
        )
    }
}

async fn judge_output(
    state: &AppState,
    input: &str,
    output: &str,
    expected: Option<&str>,
    judge_model: &str,
    metrics: &[String],
) -> Result<(i64, i64, String), (StatusCode, Json<serde_json::Value>)> {
    let config = ModelConfigRepo::get_active_config_or_seed(&state.pool)
        .await
        .unwrap_or_else(|_| ModelProviderConfig::mock());
    let gateway = select_gateway(config.kind);
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "accuracy": {"type": "number"},
            "relevance": {"type": "number"},
            "judge_reasoning": {"type": "string"}
        },
        "required": ["accuracy", "relevance", "judge_reasoning"]
    });
    let metrics_line = if metrics.is_empty() {
        "accuracy,relevance".to_string()
    } else {
        metrics.join(",")
    };
    let request = ModelRequest {
        config: config.clone(),
        role: ModelRole::StructuredOutput,
        model: if judge_model.is_empty() {
            config.structured_output_model.clone()
        } else {
            judge_model.to_string()
        },
        messages: vec![ModelMessage {
            role: "user".to_string(),
            content: format!(
                "Evaluate this output. Metrics: {metrics_line}\nInput: {input}\nOutput: {output}\nExpected: {}",
                expected.unwrap_or("")
            ),
        }],
        temperature: 0.0,
        max_tokens: config.max_tokens,
        response_format: ResponseFormat::Json,
    };

    let response = gateway.structured(&request, &schema).await.ok();
    if let Some(response) = response {
        if let Some(json) = response.json {
            let accuracy = json.get("accuracy").and_then(|v| v.as_f64()).map(|v| v.round() as i64);
            let relevance =
                json.get("relevance").and_then(|v| v.as_f64()).map(|v| v.round() as i64);
            let reasoning = json
                .get("judge_reasoning")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let (Some(accuracy), Some(relevance), Some(reasoning)) =
                (accuracy, relevance, reasoning)
            {
                return Ok((accuracy, relevance, reasoning));
            }
        }
    }
    Ok(heuristic_scores(output, expected))
}

fn row_to_json_list(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<serde_json::Value> {
    rows.iter().map(eval_repo::row_to_json).collect()
}

pub async fn list_datasets(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = EvalRepo::list_datasets(&pool).await;
    pool.close().await;
    match result {
        Ok(rows) => ok!(serde_json::Value::Array(row_to_json_list(&rows))),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_dataset(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<CreateDatasetRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let now = chrono::Utc::now().timestamp_millis();
    let dataset_id = format!("ds-{}", uuid::Uuid::new_v4().simple());
    let result = EvalRepo::create_dataset(
        &pool,
        &dataset_id,
        &req.name,
        req.description.as_deref().unwrap_or(""),
        now,
    )
    .await;
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::json!({
            "dataset_id": dataset_id,
            "name": req.name,
            "description": req.description.unwrap_or_default()
        })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_dataset_cases(
    State(s): State<AppState>,
    Path((opc_id, dataset_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = EvalRepo::list_cases(&pool, &dataset_id).await;
    pool.close().await;
    match result {
        Ok(rows) => ok!(serde_json::Value::Array(row_to_json_list(&rows))),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_dataset_case(
    State(s): State<AppState>,
    Path((opc_id, dataset_id)): Path<(String, String)>,
    Json(req): Json<CreateCaseRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let now = chrono::Utc::now().timestamp_millis();
    let case_id = format!("case-{}", uuid::Uuid::new_v4().simple());
    let tags_json = serde_json::to_string(&req.tags.unwrap_or_default()).unwrap_or_else(|_| "[]".to_string());
    let result = EvalRepo::create_case(
        &pool,
        &case_id,
        &dataset_id,
        &req.input,
        req.expected.as_deref(),
        &tags_json,
        now,
    )
    .await;
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::json!({
            "case_id": case_id,
            "input": req.input,
            "expected": req.expected,
            "tags": serde_json::from_str::<serde_json::Value>(&tags_json).unwrap_or_else(|_| serde_json::json!([])),
        })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn delete_dataset_case(
    State(s): State<AppState>,
    Path((opc_id, _dataset_id, case_id)): Path<(String, String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = EvalRepo::delete_case(&pool, &case_id).await;
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn run_eval_internal(
    state: &AppState,
    opc_id: &str,
    target_kind: &str,
    agent_id: Option<&str>,
    system_prompt: Option<&str>,
    dataset_id: &str,
    judge_model: &str,
    exec_model: &str,
    metrics: &[String],
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let pool = company_pool(state, opc_id).await?;
    if !EvalRepo::dataset_exists(&pool, dataset_id)
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        pool.close().await;
        return Err(err!(StatusCode::NOT_FOUND, "Dataset not found"));
    }
    let target = EvalTarget {
        kind: target_kind.to_string(),
        agent_id: agent_id.map(str::to_string),
        system_prompt: system_prompt.map(str::to_string),
    };
    let resolved_prompt = resolve_target_prompt(state, opc_id, &pool, &target).await?;
    let cases = EvalRepo::list_cases(&pool, dataset_id)
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let experiment_id = format!("exp-{}", uuid::Uuid::new_v4().simple());
    let created_at_ms = chrono::Utc::now().timestamp_millis();
    EvalRepo::create_experiment(
        &pool,
        &experiment_id,
        target_kind,
        agent_id,
        Some(&resolved_prompt),
        dataset_id,
        judge_model,
        exec_model,
        "running",
        created_at_ms,
    )
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut scores = Vec::new();
    for row in &cases {
        let case_id: String = row.get("case_id");
        let input_text: String = row.get("input_text");
        let expected_text: Option<String> = row.try_get("expected_text").ok();
        let output_text = generate_output(state, &resolved_prompt, &input_text, exec_model).await?;
        let (accuracy, relevance, judge_reasoning) = judge_output(
            state,
            &input_text,
            &output_text,
            expected_text.as_deref(),
            judge_model,
            metrics,
        )
        .await?;
        let overall = ((accuracy + relevance) as f64) / 2.0;
        scores.push((accuracy, relevance, overall));
        EvalRepo::append_case_result(
            &pool,
            &format!("res-{}", uuid::Uuid::new_v4().simple()),
            &experiment_id,
            &case_id,
            &input_text,
            &output_text,
            &serde_json::json!({
                "accuracy": accuracy,
                "relevance": relevance
            })
            .to_string(),
            &judge_reasoning,
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let count = scores.len().max(1) as f64;
    let accuracy_avg = scores.iter().map(|(a, _, _)| *a as f64).sum::<f64>() / count;
    let relevance_avg = scores.iter().map(|(_, r, _)| *r as f64).sum::<f64>() / count;
    let overall_score = scores.iter().map(|(_, _, s)| *s).sum::<f64>() / count;
    let aggregate = serde_json::json!({
        "accuracy": accuracy_avg.round(),
        "relevance": relevance_avg.round()
    });
    EvalRepo::complete_experiment(
        &pool,
        &experiment_id,
        &aggregate.to_string(),
        overall_score,
        chrono::Utc::now().timestamp_millis(),
    )
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    pool.close().await;
    Ok(experiment_id)
}

pub async fn run_eval(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<EvalRunRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match run_eval_internal(
        &s,
        &opc_id,
        &req.target.kind,
        req.target.agent_id.as_deref(),
        req.target.system_prompt.as_deref(),
        &req.dataset_id,
        &req.judge_model,
        &req.exec_model,
        &req.metrics.unwrap_or_default(),
    )
    .await
    {
        Ok(experiment_id) => ok!(serde_json::json!({"experiment_id": experiment_id, "status": "running"})),
        Err(err) => err,
    }
}

pub async fn list_experiments(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = EvalRepo::list_experiments(&pool).await;
    pool.close().await;
    match result {
        Ok(rows) => ok!(serde_json::Value::Array(row_to_json_list(&rows))),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_experiment(
    State(s): State<AppState>,
    Path((opc_id, experiment_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let experiment = match EvalRepo::get_experiment(&pool, &experiment_id).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, "Experiment not found");
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    let case_results = EvalRepo::list_case_results(&pool, &experiment_id).await;
    pool.close().await;
    match case_results {
        Ok(results) => {
            let aggregate_json: String = experiment.get("aggregate_json");
            let aggregate = serde_json::from_str::<serde_json::Value>(&aggregate_json)
                .unwrap_or_else(|_| serde_json::json!({}));
            ok!(serde_json::json!({
                "experiment_id": experiment.get::<String, _>("experiment_id"),
                "status": experiment.get::<String, _>("status"),
                "dataset_id": experiment.get::<String, _>("dataset_id"),
                "case_results": row_to_json_list(&results),
                "aggregate": aggregate,
                "overall_score": experiment.get::<f64, _>("overall_score"),
                "created_at_ms": experiment.get::<i64, _>("created_at_ms"),
                "completed_at_ms": experiment.try_get::<Option<i64>, _>("completed_at_ms").ok().flatten()
            }))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn compare_eval(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<EvalCompareRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let prompt_a_path = employee_prompt_version_path(&s, &opc_id, &req.agent_id, req.version_a);
    let prompt_b_path = employee_prompt_version_path(&s, &opc_id, &req.agent_id, req.version_b);
    let prompt_a = match std::fs::read_to_string(prompt_a_path) {
        Ok(content) => content,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let prompt_b = match std::fs::read_to_string(prompt_b_path) {
        Ok(content) => content,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let exp_a = match run_eval_internal(
        &s,
        &opc_id,
        "prompt",
        Some(&req.agent_id),
        Some(&prompt_a),
        &req.dataset_id,
        &req.judge_model,
        &req.exec_model,
        &[],
    )
    .await
    {
        Ok(id) => id,
        Err(err) => return err,
    };
    let exp_b = match run_eval_internal(
        &s,
        &opc_id,
        "prompt",
        Some(&req.agent_id),
        Some(&prompt_b),
        &req.dataset_id,
        &req.judge_model,
        &req.exec_model,
        &[],
    )
    .await
    {
        Ok(id) => id,
        Err(err) => return err,
    };
    ok!(serde_json::json!({"experiment_a": exp_a, "experiment_b": exp_b}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{router::build_router, state::AppState};
    use axum::{body::Body, http::Request};
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};
    use tower::ServiceExt;

    #[tokio::test]
    async fn company_eval_flow_creates_dataset_runs_experiment_and_returns_judge_reasoning() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-company-eval-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool, root.clone()));

        let create_company = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Eval Co","mission":"Run eval"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let company: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_company.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let opc_id = company["opc_id"].as_str().unwrap();

        let create_dataset = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/eval/datasets"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Smoke","description":"Eval smoke"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_dataset.status(), StatusCode::OK);
        let dataset: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_dataset.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let dataset_id = dataset["dataset_id"].as_str().unwrap();

        let create_case = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/eval/datasets/{dataset_id}/cases"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "input":"Say alpha",
                            "expected":"alpha",
                            "tags":["smoke"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_case.status(), StatusCode::OK);

        let run_eval = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/eval/run"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "target":{"kind":"prompt","system_prompt":"Answer briefly with alpha."},
                            "dataset_id": dataset_id,
                            "judge_model": "mock-model",
                            "exec_model": "mock-model",
                            "metrics": ["accuracy","relevance"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(run_eval.status(), StatusCode::OK);
        let experiment_resp: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(run_eval.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let experiment_id = experiment_resp["experiment_id"].as_str().unwrap();

        let detail = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/eval/experiments/{experiment_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        let detail_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(detail.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(detail_body["status"], "completed");
        assert!(detail_body["case_results"][0]["judge_reasoning"]
            .as_str()
            .unwrap_or_default()
            .len()
            > 0);

        std::fs::remove_dir_all(root).ok();
    }
}
