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

use crate::handlers::identifiers::is_plain_identifier;
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

fn validate_agent_id(agent_id: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if agent_id.trim().is_empty() {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_id is required"
        ));
    }
    if !is_plain_identifier(agent_id) {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_id must be a plain employee identifier"
        ));
    }
    Ok(())
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
                return Err(err!(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "agent_id is required"
                ));
            };
            validate_agent_id(agent_id)?;
            let path = employee_prompt_path(state, opc_id, agent_id);
            if path.exists() {
                return std::fs::read_to_string(path)
                    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            }
            let employee = agent_employee_repo::AgentEmployeeRepo::get(pool, agent_id)
                .await
                .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            match employee {
                Some(_) => Err(err!(
                    StatusCode::CONFLICT,
                    format!("PROMPT_FILE_MISSING: employee {agent_id} prompt.md is missing")
                )),
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
    let config = ModelConfigRepo::get_active_config(&state.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(ModelProviderConfig::mock);
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
                ..Default::default()
            },
            ModelMessage {
                role: "user".to_string(),
                content: input.to_string(),
                ..Default::default()
            },
        ],
        temperature: config.temperature,
        max_tokens: config.max_tokens,
        response_format: ResponseFormat::Text,
        stream: false,
        tools: vec![],
        tool_choice: None,
    };
    gateway
        .chat(&request)
        .await
        .map(|response| response.content)
        .map_err(|e| err!(StatusCode::BAD_REQUEST, e.to_string()))
}

fn build_judge_prompt(
    input: &str,
    output: &str,
    expected: Option<&str>,
    metrics: &[String],
) -> String {
    let metrics_line = if metrics.is_empty() {
        "accuracy,relevance".to_string()
    } else {
        metrics.join(",")
    };
    format!(
        "Evaluate this output and return a JSON object only. Metrics: {metrics_line}\nInput: {input}\nOutput: {output}\nExpected: {}",
        expected.unwrap_or("")
    )
}

fn parse_judge_score(value: &serde_json::Value) -> Option<i64> {
    if let Some(number) = value.as_f64() {
        return Some(number.round() as i64);
    }
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Ok(number) = trimmed.parse::<f64>() {
            return Some(number.round() as i64);
        }
        if trimmed.eq_ignore_ascii_case("true") {
            return Some(1);
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return Some(0);
        }
        return None;
    }
    value.as_bool().map(|flag| if flag { 1 } else { 0 })
}

async fn judge_output(
    state: &AppState,
    input: &str,
    output: &str,
    expected: Option<&str>,
    judge_model: &str,
    metrics: &[String],
) -> Result<(i64, i64, String), (StatusCode, Json<serde_json::Value>)> {
    let judge_model = judge_model.trim();
    if judge_model.is_empty() {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "judge_model is required for formal evaluation"
        ));
    }
    let config = ModelConfigRepo::get_active_config(&state.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(ModelProviderConfig::mock);
    if config.kind == coevo_models::types::ModelProviderKind::Mock {
        return Err(err!(
            StatusCode::BAD_GATEWAY,
            "judge model requires a real configured provider; mock provider is not accepted for formal evaluation"
        ));
    }
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
    let request = ModelRequest {
        config: config.clone(),
        role: ModelRole::StructuredOutput,
        model: judge_model.to_string(),
        messages: vec![ModelMessage {
            role: "user".to_string(),
            content: build_judge_prompt(input, output, expected, metrics),
            ..Default::default()
        }],
        temperature: 0.0,
        max_tokens: config.max_tokens,
        response_format: ResponseFormat::Json,
        stream: false,
        tools: vec![],
        tool_choice: None,
    };

    match gateway.structured(&request, &schema).await {
        Ok(response) => {
            if let Some(json) = response.json {
                let accuracy = json.get("accuracy").and_then(parse_judge_score);
                let relevance = json.get("relevance").and_then(parse_judge_score);
                let reasoning = json
                    .get("judge_reasoning")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or_else(|| response.reasoning_content.clone());
                if let (Some(accuracy), Some(relevance), Some(reasoning)) =
                    (accuracy, relevance, reasoning)
                {
                    return Ok((accuracy, relevance, reasoning));
                }
            }
        }
        Err(error) => {
            return Err(err!(
                StatusCode::BAD_GATEWAY,
                format!("judge model {} failed: {}", judge_model, error)
            ));
        }
    }
    Err(err!(
        StatusCode::BAD_GATEWAY,
        format!(
            "judge model {} returned no valid structured score payload",
            judge_model
        )
    ))
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
    let tags_json =
        serde_json::to_string(&req.tags.unwrap_or_default()).unwrap_or_else(|_| "[]".to_string());
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
    Path((opc_id, dataset_id, case_id)): Path<(String, String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let belongs = match EvalRepo::case_belongs_to_dataset(&pool, &case_id, &dataset_id).await {
        Ok(belongs) => belongs,
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    if !belongs {
        pool.close().await;
        return err!(StatusCode::NOT_FOUND, "Eval case not found");
    }
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
        Ok(experiment_id) => {
            ok!(serde_json::json!({"experiment_id": experiment_id, "status": "running"}))
        }
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
    if let Err(err) = validate_agent_id(&req.agent_id) {
        return err;
    }
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
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("desktop-test")
            .bind("OpenAICompatible")
            .bind("https://api.openai.com/v1")
            .bind("sk-test")
            .bind("sk-t****test")
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
            .execute(&pool)
            .await
            .unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-eval-{}", uuid::Uuid::new_v4()));
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
                    .uri(format!(
                        "/companies/{opc_id}/eval/datasets/{dataset_id}/cases"
                    ))
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
                    .uri(format!(
                        "/companies/{opc_id}/eval/experiments/{experiment_id}"
                    ))
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
        assert!(
            detail_body["case_results"][0]["judge_reasoning"]
                .as_str()
                .unwrap_or_default()
                .len()
                > 0
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn eval_run_fails_when_only_mock_judge_provider_is_available() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-eval-fail-{}", uuid::Uuid::new_v4()));
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
        let dataset: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_dataset.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let dataset_id = dataset["dataset_id"].as_str().unwrap();

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/eval/datasets/{dataset_id}/cases"
                    ))
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

        let run_eval = app
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
        assert_eq!(run_eval.status(), StatusCode::BAD_GATEWAY);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn delete_dataset_case_rejects_dataset_case_mismatch() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-eval-delete-mismatch-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let create_company = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Eval Delete Co","mission":"Delete cases safely"}).to_string(),
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

        let create_dataset_a = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/eval/datasets"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Dataset A","description":"First dataset"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let dataset_a: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_dataset_a.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let dataset_a_id = dataset_a["dataset_id"].as_str().unwrap();

        let create_dataset_b = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/eval/datasets"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Dataset B","description":"Second dataset"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let dataset_b: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_dataset_b.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let dataset_b_id = dataset_b["dataset_id"].as_str().unwrap();

        let create_case = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/eval/datasets/{dataset_a_id}/cases"
                    ))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "input":"Say alpha",
                            "expected":"alpha",
                            "tags":["safety"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_case.status(), StatusCode::OK);
        let case_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_case.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let case_id = case_body["case_id"].as_str().unwrap();

        let delete = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/companies/{opc_id}/eval/datasets/{dataset_b_id}/cases/{case_id}"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(delete.status(), StatusCode::NOT_FOUND);

        let remaining = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{opc_id}/eval/datasets/{dataset_a_id}/cases"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(remaining.status(), StatusCode::OK);
        let remaining_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(remaining.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(remaining_body.as_array().unwrap().len(), 1);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn eval_run_rejects_empty_judge_model() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("desktop-test")
            .bind("OpenAICompatible")
            .bind("https://api.openai.com/v1")
            .bind("sk-test")
            .bind("sk-t****test")
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
            .execute(&pool)
            .await
            .unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-eval-empty-judge-{}",
            uuid::Uuid::new_v4()
        ));
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
        let dataset: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_dataset.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let dataset_id = dataset["dataset_id"].as_str().unwrap();

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/eval/datasets/{dataset_id}/cases"
                    ))
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

        let run_eval = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/eval/run"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "target":{"kind":"prompt","system_prompt":"Answer briefly with alpha."},
                            "dataset_id": dataset_id,
                            "judge_model": "",
                            "exec_model": "mock-model",
                            "metrics": ["accuracy","relevance"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(run_eval.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(run_eval.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("judge_model is required"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn eval_run_fails_when_agent_prompt_file_is_missing() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("desktop-test")
            .bind("OpenAICompatible")
            .bind("https://api.openai.com/v1")
            .bind("sk-test")
            .bind("sk-t****test")
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
            .execute(&pool)
            .await
            .unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-eval-missing-prompt-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let create_company = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Eval Promptless Co","mission":"Run eval"})
                            .to_string(),
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

        let seed_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed_response.status(), StatusCode::OK);

        let prompt_path = root
            .join(opc_id)
            .join("employees")
            .join("agent-pm-01")
            .join("prompt.md");
        if prompt_path.exists() {
            std::fs::remove_file(&prompt_path).unwrap();
        }

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
        let dataset: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_dataset.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let dataset_id = dataset["dataset_id"].as_str().unwrap();

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/eval/datasets/{dataset_id}/cases"
                    ))
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

        let run_eval = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/eval/run"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "target":{"kind":"agent","agent_id":"agent-pm-01"},
                            "dataset_id": dataset_id,
                            "judge_model": "gpt-4o",
                            "exec_model": "gpt-4o-mini",
                            "metrics": ["accuracy","relevance"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(run_eval.status(), StatusCode::CONFLICT);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(run_eval.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("PROMPT_FILE_MISSING"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn eval_routes_reject_agent_id_path_traversal() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("desktop-test")
            .bind("OpenAICompatible")
            .bind("https://api.openai.com/v1")
            .bind("sk-test")
            .bind("sk-t****test")
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
            .execute(&pool)
            .await
            .unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-eval-agent-id-traversal-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));

        let create_alpha = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Eval Alpha","mission":"Alpha"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let alpha: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_alpha.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let alpha_opc = alpha["opc_id"].as_str().unwrap().to_string();

        let create_beta = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Eval Beta","mission":"Beta"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let beta: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_beta.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let beta_opc = beta["opc_id"].as_str().unwrap().to_string();

        for opc_id in [&alpha_opc, &beta_opc] {
            let seed_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/companies/{opc_id}/employees/seed"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(seed_response.status(), StatusCode::OK);
        }

        std::fs::write(
            root.join(&beta_opc)
                .join("employees")
                .join("agent-pm-01")
                .join("prompt.md"),
            "BETA SECRET PROMPT",
        )
        .unwrap();
        let beta_versions = root
            .join(&beta_opc)
            .join("employees")
            .join("agent-pm-01")
            .join("prompt_versions");
        std::fs::create_dir_all(&beta_versions).unwrap();
        std::fs::write(beta_versions.join("v1.md"), "BETA VERSION 1").unwrap();

        let create_dataset = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{alpha_opc}/eval/datasets"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Traversal","description":"Traversal dataset"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let dataset: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_dataset.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let dataset_id = dataset["dataset_id"].as_str().unwrap();

        let add_case = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{alpha_opc}/eval/datasets/{dataset_id}/cases"
                    ))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "input":"Say alpha",
                            "expected":"alpha",
                            "tags":["security"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_case.status(), StatusCode::OK);

        let traversal_agent_id = format!("..\\..\\{beta_opc}\\employees\\agent-pm-01");
        let run_eval = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{alpha_opc}/eval/run"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "target":{"kind":"agent","agent_id": traversal_agent_id},
                            "dataset_id": dataset_id,
                            "judge_model": "gpt-4o",
                            "exec_model": "gpt-4o-mini",
                            "metrics": ["accuracy","relevance"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(run_eval.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let compare = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{alpha_opc}/eval/compare"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "agent_id": format!("..\\..\\{beta_opc}\\employees\\agent-pm-01"),
                            "version_a": 1,
                            "version_b": 1,
                            "dataset_id": dataset_id,
                            "judge_model": "gpt-4o",
                            "exec_model": "gpt-4o-mini"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(compare.status(), StatusCode::UNPROCESSABLE_ENTITY);

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn judge_prompt_mentions_json_for_structured_output_providers() {
        let prompt = build_judge_prompt(
            "Say alpha",
            "alpha",
            Some("alpha"),
            &["accuracy".to_string(), "relevance".to_string()],
        );
        assert!(
            prompt.to_lowercase().contains("json"),
            "judge prompt must explicitly mention JSON for providers like DeepSeek that require it when response_format=json_object: {prompt}"
        );
    }

    #[test]
    fn judge_accepts_provider_reasoning_content_when_json_omits_judge_reasoning() {
        let response = coevo_models::types::ModelResponse {
            content: "{\"accuracy\":1.0,\"relevance\":1.0}".to_string(),
            json: Some(serde_json::json!({
                "accuracy": 1.0,
                "relevance": 1.0
            })),
            usage: coevo_models::types::ModelUsage::default(),
            latency_ms: 1,
            model: "deepseek-v4-flash".to_string(),
            finish_reason: "stop".to_string(),
            provider_kind: coevo_models::types::ModelProviderKind::OpenAICompatible,
            reasoning_content: Some("The output exactly matches the expected answer.".to_string()),
            tool_calls: vec![],
        };
        let json = response.json.clone().unwrap();
        let accuracy = json.get("accuracy").and_then(parse_judge_score);
        let relevance = json.get("relevance").and_then(parse_judge_score);
        let reasoning = json
            .get("judge_reasoning")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| response.reasoning_content.clone());
        assert_eq!(accuracy, Some(1));
        assert_eq!(relevance, Some(1));
        assert_eq!(
            reasoning.as_deref(),
            Some("The output exactly matches the expected answer.")
        );
    }

    #[test]
    fn parse_judge_score_accepts_string_and_boolean_provider_drift() {
        assert_eq!(parse_judge_score(&serde_json::json!("5")), Some(5));
        assert_eq!(parse_judge_score(&serde_json::json!(true)), Some(1));
        assert_eq!(parse_judge_score(&serde_json::json!(false)), Some(0));
        assert_eq!(parse_judge_score(&serde_json::json!(" 4.2 ")), Some(4));
        assert_eq!(parse_judge_score(&serde_json::json!("")), None);
    }
}
