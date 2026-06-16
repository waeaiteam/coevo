use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_models::gateway::select_gateway;
use coevo_models::router::{default_model_profiles, ModelRouter, ModelRoutingRequest};
use coevo_models::types::*;
use coevo_store::repos::model_config_repo::ModelConfigRepo;
use coevo_store::{migrate::run_migrations, pool::create_pool, repos_opc::agent_employee_repo};
use serde::Deserialize;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

const MODEL_PROVIDER_NOT_CONFIGURED: &str =
    "MODEL_PROVIDER_NOT_CONFIGURED: configure a real model provider before using model endpoints";

#[derive(Deserialize)]
pub struct ChatRequest {
    pub role: Option<String>,
    pub messages: Vec<ModelMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}
#[derive(Deserialize)]
pub struct StructuredRequest {
    pub role: Option<String>,
    pub messages: Vec<ModelMessage>,
    pub schema: Option<serde_json::Value>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}
#[derive(Deserialize, Clone)]
pub struct PutConfigRequest {
    pub provider_id: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub clear_api_key: Option<bool>,
    pub default_model: Option<String>,
    pub fast_model: Option<String>,
    pub reasoning_model: Option<String>,
    pub structured_output_model: Option<String>,
    pub max_tokens: Option<i64>,
    pub temperature: Option<f64>,
    pub timeout_ms: Option<i64>,
    pub max_cost_per_task_usd: Option<f64>,
}
#[derive(Deserialize)]
pub struct TestConfigRequest {
    pub config: Option<PutConfigRequest>,
}
#[derive(Deserialize)]
pub struct PlaygroundRunRequest {
    pub agent_id: Option<String>,
    pub system_prompt: Option<String>,
    pub user_input: String,
    pub variables: Option<std::collections::BTreeMap<String, String>>,
    pub models: Vec<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

fn parse_provider_kind(kind: &str) -> Result<ModelProviderKind, String> {
    match kind {
        "Mock" => Ok(ModelProviderKind::Mock),
        "OpenAICompatible" => Ok(ModelProviderKind::OpenAICompatible),
        "OpenAI" => Ok(ModelProviderKind::OpenAI),
        "Anthropic" => Ok(ModelProviderKind::Anthropic),
        "Gemini" => Ok(ModelProviderKind::Gemini),
        "DeepSeek" => Ok(ModelProviderKind::DeepSeek),
        "Ollama" => Ok(ModelProviderKind::Ollama),
        "Local" => Ok(ModelProviderKind::Local),
        _ => Err(format!("MODEL_CONFIG_INVALID_KIND: {}", kind)),
    }
}

fn model_config_error(e: sqlx::Error) -> (StatusCode, String) {
    match e {
        sqlx::Error::RowNotFound => (
            StatusCode::CONFLICT,
            MODEL_PROVIDER_NOT_CONFIGURED.to_string(),
        ),
        other => {
            let msg = other.to_string();
            if msg.contains("MODEL_CONFIG_INVALID_KIND") || msg.contains("invalid type") {
                (StatusCode::UNPROCESSABLE_ENTITY, msg)
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Config error: {}", msg),
                )
            }
        }
    }
}

pub async fn get_config_handler(
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match ModelConfigRepo::get_active_config(&s.pool).await {
        Ok(Some(c)) => ok!(serde_json::json!({
            "provider_id": c.provider_id,
            "kind": c.kind,
            "base_url": c.base_url,
            "api_key_masked": c.mask_key(),
            "has_api_key": !c.api_key.is_empty(),
            "default_model": c.default_model,
            "fast_model": c.fast_model,
            "reasoning_model": c.reasoning_model,
            "structured_output_model": c.structured_output_model,
            "max_tokens": c.max_tokens,
            "temperature": c.temperature,
            "timeout_ms": c.timeout_ms,
            "max_cost_per_task_usd": c.max_cost_per_task_usd,
        })),
        Ok(None) => err!(StatusCode::CONFLICT, MODEL_PROVIDER_NOT_CONFIGURED),
        Err(e) => {
            let (code, msg) = model_config_error(e);
            err!(code, msg)
        }
    }
}

pub async fn put_config_handler(
    State(s): State<AppState>,
    Json(req): Json<PutConfigRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let existing_key = if req.clear_api_key.unwrap_or(false) {
        String::new()
    } else if req.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) {
        req.api_key.clone().unwrap_or_default()
    } else {
        ModelConfigRepo::get_active_config(&s.pool)
            .await
            .ok()
            .flatten()
            .map(|c| c.api_key)
            .unwrap_or_default()
    };
    let config = match validate_config_request(&req, &existing_key) {
        Ok(c) => c,
        Err((code, msg)) => return err!(code, msg),
    };
    let kind_str = req.kind.clone();
    let mask = ModelConfigRepo::mask_key(&config.api_key);
    let pid = config.provider_id.clone();
    if let Err(e) = ModelConfigRepo::upsert_config(
        &s.pool,
        &pid,
        &kind_str,
        &config.base_url,
        &config.api_key,
        &mask,
        &config.default_model,
        &config.fast_model,
        &config.reasoning_model,
        &config.structured_output_model,
        config.max_tokens as i64,
        config.temperature,
        config.timeout_ms as i64,
        config.max_cost_per_task_usd,
    )
    .await
    {
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e)
        );
    }
    if let Err(e) = ModelConfigRepo::deactivate_others(&s.pool, &pid).await {
        return err!(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e)
        );
    }
    ok!(serde_json::json!({"ok":true,"provider_id":pid,"kind":kind_str,"api_key_masked":mask}))
}

fn validate_config_request(
    req: &PutConfigRequest,
    existing_key: &str,
) -> Result<ModelProviderConfig, (StatusCode, String)> {
    let kind = parse_provider_kind(&req.kind).map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let base_url = req.base_url.clone().unwrap_or_default();
    let default_model = req.default_model.clone().unwrap_or_default();
    let fast_model = req.fast_model.clone().unwrap_or_default();
    let reasoning_model = req.reasoning_model.clone().unwrap_or_default();
    let structured_output_model = req.structured_output_model.clone().unwrap_or_default();
    let max_tokens = req.max_tokens.unwrap_or(4096);
    let temperature = req.temperature.unwrap_or(0.7);
    let timeout_ms = req.timeout_ms.unwrap_or(30000);
    let max_cost_per_task_usd = req.max_cost_per_task_usd.unwrap_or(0.0);
    let api_key = if req.clear_api_key.unwrap_or(false) {
        String::new()
    } else if req.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) {
        req.api_key.clone().unwrap_or_default()
    } else {
        existing_key.to_string()
    };
    if kind != ModelProviderKind::Mock {
        if base_url.is_empty() {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_BASE_URL".to_string(),
            ));
        }
        if default_model.is_empty() {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "MODEL_CONFIG_INVALID: default_model required".to_string(),
            ));
        }
        if !(0.0..=2.0).contains(&temperature) {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("INVALID_TEMPERATURE: {}", temperature),
            ));
        }
        if timeout_ms <= 0 {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("INVALID_TIMEOUT: {}", timeout_ms),
            ));
        }
        if max_tokens <= 0 {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("INVALID_MAX_TOKENS: {}", max_tokens),
            ));
        }
        if max_cost_per_task_usd < 0.0 {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("INVALID_COST: {}", max_cost_per_task_usd),
            ));
        }
    }
    if matches!(
        kind,
        ModelProviderKind::OpenAICompatible
            | ModelProviderKind::OpenAI
            | ModelProviderKind::Anthropic
            | ModelProviderKind::Gemini
            | ModelProviderKind::DeepSeek
    ) {
        if api_key.is_empty() {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "MISSING_API_KEY".to_string(),
            ));
        }
    }
    Ok(ModelProviderConfig {
        provider_id: req.provider_id.clone(),
        kind,
        base_url,
        api_key,
        default_model,
        fast_model,
        reasoning_model,
        structured_output_model,
        max_tokens: max_tokens as u32,
        temperature,
        timeout_ms: timeout_ms as u64,
        max_cost_per_task_usd,
    })
}

async fn company_pool(
    state: &AppState,
    opc_id: &str,
) -> Result<sqlx::SqlitePool, (StatusCode, Json<serde_json::Value>)> {
    if !is_plain_identifier(opc_id) {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "opc_id must be a plain identifier"
        ));
    }
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

async fn resolve_playground_prompt(
    state: &AppState,
    opc_id: &str,
    pool: &sqlx::SqlitePool,
    req: &PlaygroundRunRequest,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    if let Some(system_prompt) = req.system_prompt.as_ref() {
        return Ok(system_prompt.clone());
    }
    let Some(agent_id) = req.agent_id.as_deref() else {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_id or system_prompt is required"
        ));
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
        Some(_) => Err(err!(
            StatusCode::CONFLICT,
            format!("PROMPT_FILE_MISSING: employee {agent_id} prompt.md is missing")
        )),
        None => Err(err!(StatusCode::NOT_FOUND, "Employee not found")),
    }
}

fn apply_playground_variables(
    prompt: &str,
    variables: Option<&std::collections::BTreeMap<String, String>>,
) -> String {
    let mut rendered = prompt.to_string();
    if let Some(variables) = variables {
        for (key, value) in variables {
            rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
        }
    }
    rendered
}

fn model_unit_cost_usd(model_id: &str) -> Option<(f64, f64)> {
    let lower = model_id.to_ascii_lowercase();
    if lower.contains("deepseek-v4-flash")
        || lower.contains("deepseek-chat")
        || (lower.contains("deepseek") && !lower.contains("reasoner"))
    {
        return Some((0.00014, 0.00028));
    }
    if lower.contains("deepseek-reasoner") {
        return Some((0.00055, 0.00219));
    }
    if lower.contains("gpt-4o-mini") {
        return Some((0.00015, 0.0006));
    }
    if lower.contains("gpt-4o") {
        return Some((0.0025, 0.01));
    }
    if lower.contains("o3-mini") {
        return Some((0.0011, 0.0044));
    }
    None
}

fn estimated_model_cost_usd(model_id: &str, prompt_tokens: u64, completion_tokens: u64) -> f64 {
    model_unit_cost_usd(model_id)
        .map(|(input_cost_per_1k, output_cost_per_1k)| {
            ((prompt_tokens as f64 / 1000.0) * input_cost_per_1k)
                + ((completion_tokens as f64 / 1000.0) * output_cost_per_1k)
        })
        .unwrap_or(0.0)
}

pub async fn test_connection(
    State(s): State<AppState>,
    Json(req): Json<TestConfigRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let config = if let Some(candidate) = req.config {
        let existing_key = ModelConfigRepo::get_active_config(&s.pool)
            .await
            .ok()
            .flatten()
            .map(|c| c.api_key)
            .unwrap_or_default();
        match validate_config_request(&candidate, &existing_key) {
            Ok(c) => c,
            Err((code, msg)) => return err!(code, msg),
        }
    } else {
        match ModelConfigRepo::get_active_config(&s.pool).await {
            Ok(Some(c)) => c,
            Ok(None) => return err!(StatusCode::CONFLICT, MODEL_PROVIDER_NOT_CONFIGURED),
            Err(e) => {
                let (code, msg) = model_config_error(e);
                return err!(code, msg);
            }
        }
    };
    let gateway = select_gateway(config.kind);
    match gateway.test_connection(&config).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

pub async fn discover_models(
    State(s): State<AppState>,
    Json(req): Json<TestConfigRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let candidate = match req.config {
        Some(c) => c,
        None => {
            return err!(
                StatusCode::UNPROCESSABLE_ENTITY,
                "MODEL_DISCOVERY_REQUIRES_CANDIDATE_CONFIG"
            )
        }
    };
    let existing_key = ModelConfigRepo::get_active_config(&s.pool)
        .await
        .ok()
        .flatten()
        .map(|c| c.api_key)
        .unwrap_or_default();
    let config = match validate_config_request(&candidate, &existing_key) {
        Ok(c) => c,
        Err((code, msg)) => return err!(code, msg),
    };
    let gateway = select_gateway(config.kind);
    match gateway.discover_models(&config).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

pub async fn chat(
    State(s): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let config = match ModelConfigRepo::get_active_config(&s.pool).await {
        Ok(Some(c)) => c,
        Ok(None) => return err!(StatusCode::CONFLICT, MODEL_PROVIDER_NOT_CONFIGURED),
        Err(e) => {
            let (code, msg) = model_config_error(e);
            return err!(code, msg);
        }
    };
    let gateway = select_gateway(config.kind);
    let role_str = req.role.clone().unwrap_or_default();
    let role: ModelRole = match serde_json::from_str(&format!("\"{}\"", &role_str)) {
        Ok(r) => r,
        Err(_) => {
            return err!(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("MODEL_ROLE_INVALID: {}", role_str)
            )
        }
    };
    let mr = ModelRequest {
        config: config.clone(),
        role,
        model: config.default_model.clone(),
        messages: req.messages,
        temperature: req.temperature.unwrap_or(config.temperature),
        max_tokens: req.max_tokens.unwrap_or(config.max_tokens),
        response_format: ResponseFormat::Text,
        stream: false,
        tools: vec![],
        tool_choice: None,
    };
    match gateway.chat(&mr).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

pub async fn company_playground_run(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<PlaygroundRunRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.models.is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "models must not be empty");
    }
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let base_prompt = match resolve_playground_prompt(&s, &opc_id, &pool, &req).await {
        Ok(prompt) => prompt,
        Err(err) => {
            pool.close().await;
            return err;
        }
    };
    pool.close().await;

    let system_prompt = apply_playground_variables(&base_prompt, req.variables.as_ref());
    let config = match ModelConfigRepo::get_active_config(&s.pool).await {
        Ok(Some(c)) => c,
        Ok(None) => return err!(StatusCode::CONFLICT, MODEL_PROVIDER_NOT_CONFIGURED),
        Err(e) => {
            let (code, msg) = model_config_error(e);
            return err!(code, msg);
        }
    };
    let gateway = select_gateway(config.kind);
    let mut results = Vec::new();
    for model in &req.models {
        let request = ModelRequest {
            config: config.clone(),
            role: ModelRole::Synthesizer,
            model: model.clone(),
            messages: vec![
                ModelMessage {
                    role: "system".to_string(),
                    content: system_prompt.clone(),
                    ..Default::default()
                },
                ModelMessage {
                    role: "user".to_string(),
                    content: req.user_input.clone(),
                    ..Default::default()
                },
            ],
            temperature: req.temperature.unwrap_or(config.temperature),
            max_tokens: req.max_tokens.unwrap_or(config.max_tokens),
            response_format: ResponseFormat::Text,
            stream: false,
            tools: vec![],
            tool_choice: None,
        };
        match gateway.chat(&request).await {
            Ok(response) => results.push(serde_json::json!({
                "model": model,
                "output": response.content,
                "input_tokens": response.usage.prompt_tokens,
                "output_tokens": response.usage.completion_tokens,
                "cost_usd": estimated_model_cost_usd(
                    model,
                    response.usage.prompt_tokens,
                    response.usage.completion_tokens,
                ),
                "latency_ms": response.latency_ms,
                "error": serde_json::Value::Null,
            })),
            Err(e) => results.push(serde_json::json!({
                "model": model,
                "output": "",
                "input_tokens": 0,
                "output_tokens": 0,
                "cost_usd": 0.0,
                "latency_ms": 0,
                "error": e.to_string(),
            })),
        }
    }

    ok!(serde_json::json!({
        "run_id": format!("playground-{}", uuid::Uuid::new_v4().simple()),
        "results": results
    }))
}

pub async fn structured(
    State(s): State<AppState>,
    Json(req): Json<StructuredRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let config = match ModelConfigRepo::get_active_config(&s.pool).await {
        Ok(Some(c)) => c,
        Ok(None) => return err!(StatusCode::CONFLICT, MODEL_PROVIDER_NOT_CONFIGURED),
        Err(e) => {
            let (code, msg) = model_config_error(e);
            return err!(code, msg);
        }
    };
    let gateway = select_gateway(config.kind);
    let role_str = req.role.clone().unwrap_or_default();
    let role: ModelRole = match serde_json::from_str(&format!("\"{}\"", &role_str)) {
        Ok(r) => r,
        Err(_) => {
            return err!(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("MODEL_ROLE_INVALID: {}", role_str)
            )
        }
    };
    let mr = ModelRequest {
        config: config.clone(),
        role,
        model: config.structured_output_model.clone(),
        messages: req.messages,
        temperature: req.temperature.unwrap_or(config.temperature),
        max_tokens: req.max_tokens.unwrap_or(config.max_tokens),
        response_format: ResponseFormat::Json,
        stream: false,
        tools: vec![],
        tool_choice: None,
    };
    let schema = req.schema.unwrap_or(serde_json::json!({"type":"object"}));
    match gateway.structured(&mr, &schema).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

pub async fn list_model_profiles() -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::to_value(default_model_profiles()).unwrap())
}

pub async fn route_model(
    Json(req): Json<ModelRoutingRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match ModelRouter::route(&req, &default_model_profiles(), None) {
        Ok(decision) => ok!(serde_json::to_value(&decision).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};

    #[tokio::test]
    async fn test_connection_with_candidate_config_does_not_persist_on_failure() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let candidate = PutConfigRequest {
            provider_id: "desktop".to_string(),
            kind: "OpenAICompatible".to_string(),
            base_url: Some("http://127.0.0.1:1/v1".to_string()),
            api_key: Some("sk-test".to_string()),
            clear_api_key: None,
            default_model: Some("gpt-4o".to_string()),
            fast_model: Some("gpt-4o-mini".to_string()),
            reasoning_model: Some("gpt-4o".to_string()),
            structured_output_model: Some("gpt-4o".to_string()),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            timeout_ms: Some(1),
            max_cost_per_task_usd: Some(5.0),
        };

        let (status, _) = test_connection(
            State(state),
            Json(TestConfigRequest {
                config: Some(candidate),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM model_provider_configs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn active_model_paths_report_not_configured_on_fresh_db() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());

        let (test_status, Json(test_body)) = test_connection(
            State(state.clone()),
            Json(TestConfigRequest { config: None }),
        )
        .await;
        assert_eq!(test_status, StatusCode::CONFLICT);
        assert!(test_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("MODEL_PROVIDER_NOT_CONFIGURED"));

        let messages = vec![ModelMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
            ..Default::default()
        }];
        let (chat_status, Json(chat_body)) = chat(
            State(state.clone()),
            Json(ChatRequest {
                role: Some("MissionDraft".to_string()),
                messages: messages.clone(),
                temperature: None,
                max_tokens: None,
            }),
        )
        .await;
        assert_eq!(chat_status, StatusCode::CONFLICT);
        assert!(chat_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("MODEL_PROVIDER_NOT_CONFIGURED"));

        let (structured_status, Json(structured_body)) = structured(
            State(state),
            Json(StructuredRequest {
                role: Some("StructuredOutput".to_string()),
                messages,
                schema: None,
                temperature: None,
                max_tokens: None,
            }),
        )
        .await;
        assert_eq!(structured_status, StatusCode::CONFLICT);
        assert!(structured_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("MODEL_PROVIDER_NOT_CONFIGURED"));
    }

    #[tokio::test]
    async fn company_playground_run_fails_when_employee_prompt_file_is_missing() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-playground-missing-prompt-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool, root.clone());
        let company = state
            .company_workspace
            .create_company(
                "Promptless Co",
                Some("Missing prompt should fail"),
                "default-founder",
            )
            .await
            .unwrap();
        let opc_id = company.opc_id.clone();
        let company_pool = company_pool(&state, &opc_id).await.unwrap();
        coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo::seed(&company_pool)
            .await
            .unwrap();
        company_pool.close().await;

        let prompt_path = state
            .company_workspace
            .company_employee_prompt_path(&opc_id, "agent-pm-01");
        if prompt_path.exists() {
            std::fs::remove_file(&prompt_path).unwrap();
        }

        let (status, Json(body)) = company_playground_run(
            State(state),
            Path(opc_id),
            Json(PlaygroundRunRequest {
                agent_id: Some("agent-pm-01".to_string()),
                system_prompt: None,
                user_input: "hello".to_string(),
                variables: None,
                models: vec!["gpt-4o-mini".to_string()],
                temperature: None,
                max_tokens: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("PROMPT_FILE_MISSING"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_playground_run_rejects_malformed_opc_id() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-playground-bad-opc-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool, root.clone());

        let (status, Json(body)) = company_playground_run(
            State(state),
            Path("../escape".to_string()),
            Json(PlaygroundRunRequest {
                agent_id: None,
                system_prompt: Some("Prompt".to_string()),
                user_input: "hello".to_string(),
                variables: None,
                models: vec!["gpt-4o-mini".to_string()],
                temperature: None,
                max_tokens: None,
            }),
        )
        .await;

        std::fs::remove_dir_all(root).ok();

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("opc_id must be a plain identifier"));
    }

    #[tokio::test]
    async fn discover_models_with_candidate_config_does_not_persist() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let candidate = PutConfigRequest {
            provider_id: "desktop".to_string(),
            kind: "Mock".to_string(),
            base_url: Some(String::new()),
            api_key: Some(String::new()),
            clear_api_key: None,
            default_model: Some("mock-model".to_string()),
            fast_model: Some("mock-model".to_string()),
            reasoning_model: Some("mock-model".to_string()),
            structured_output_model: Some("mock-model".to_string()),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            timeout_ms: Some(30000),
            max_cost_per_task_usd: Some(0.0),
        };

        let (status, Json(body)) = discover_models(
            State(state),
            Json(TestConfigRequest {
                config: Some(candidate),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["models"][0]["id"], "mock-model");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM model_provider_configs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn cloud_provider_candidate_requires_api_key_before_network_call() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());

        let candidate = PutConfigRequest {
            provider_id: "desktop".to_string(),
            kind: "DeepSeek".to_string(),
            base_url: Some("https://api.deepseek.com/v1".to_string()),
            api_key: Some(String::new()),
            clear_api_key: None,
            default_model: Some("deepseek-chat".to_string()),
            fast_model: Some("deepseek-chat".to_string()),
            reasoning_model: Some("deepseek-reasoner".to_string()),
            structured_output_model: Some("deepseek-chat".to_string()),
            max_tokens: Some(8192),
            temperature: Some(0.7),
            timeout_ms: Some(30000),
            max_cost_per_task_usd: Some(5.0),
        };

        let (status, Json(body)) = test_connection(
            State(state),
            Json(TestConfigRequest {
                config: Some(candidate),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "MISSING_API_KEY");
    }

    #[tokio::test]
    async fn real_provider_candidate_requires_base_url_before_network_call() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());

        let candidate = PutConfigRequest {
            provider_id: "desktop".to_string(),
            kind: "DeepSeek".to_string(),
            base_url: Some(String::new()),
            api_key: Some("sk-test".to_string()),
            clear_api_key: None,
            default_model: Some("deepseek-chat".to_string()),
            fast_model: Some("deepseek-chat".to_string()),
            reasoning_model: Some("deepseek-reasoner".to_string()),
            structured_output_model: Some("deepseek-chat".to_string()),
            max_tokens: Some(8192),
            temperature: Some(0.7),
            timeout_ms: Some(30000),
            max_cost_per_task_usd: Some(5.0),
        };

        let (status, Json(body)) = test_connection(
            State(state),
            Json(TestConfigRequest {
                config: Some(candidate),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "INVALID_BASE_URL");
    }
}
