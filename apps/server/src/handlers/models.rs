use axum::{extract::State, Json, http::StatusCode};
use serde::Deserialize;
use coevo_models::types::*;
use coevo_models::gateway::select_gateway;
use coevo_models::router::{ModelRouter, default_model_profiles, ModelRoutingRequest};
use coevo_store::repos::model_config_repo::ModelConfigRepo;
use crate::state::AppState;

macro_rules! ok { ($v:expr) => { (StatusCode::OK, Json($v)) } }
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

#[derive(Deserialize)] pub struct ChatRequest { pub role: Option<String>, pub messages: Vec<ModelMessage>, pub temperature: Option<f64>, pub max_tokens: Option<u32> }
#[derive(Deserialize)] pub struct StructuredRequest { pub role: Option<String>, pub messages: Vec<ModelMessage>, pub schema: Option<serde_json::Value>, pub temperature: Option<f64>, pub max_tokens: Option<u32> }
#[derive(Deserialize)] pub struct PutConfigRequest { pub provider_id: String, pub kind: String, pub base_url: Option<String>, pub api_key: Option<String>, pub clear_api_key: Option<bool>, pub default_model: Option<String>, pub fast_model: Option<String>, pub reasoning_model: Option<String>, pub structured_output_model: Option<String>, pub max_tokens: Option<i64>, pub temperature: Option<f64>, pub timeout_ms: Option<i64>, pub max_cost_per_task_usd: Option<f64> }

pub async fn get_config_handler(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match ModelConfigRepo::get_active_config_or_seed(&s.pool).await {
        Ok(c) => ok!(serde_json::json!({
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
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("MODEL_CONFIG_INVALID_KIND") || msg.contains("invalid type") {
                err!(StatusCode::UNPROCESSABLE_ENTITY, msg)
            } else { err!(StatusCode::INTERNAL_SERVER_ERROR, msg) }
        }
    }
}

pub async fn put_config_handler(State(s): State<AppState>, Json(req): Json<PutConfigRequest>) -> (StatusCode, Json<serde_json::Value>) {
    let kind_str = req.kind.clone();
    let valid_kinds = ["Mock","OpenAICompatible","OpenAI","Anthropic","Gemini","DeepSeek","Ollama","Local"];
    if !valid_kinds.contains(&kind_str.as_str()) {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, format!("MODEL_CONFIG_INVALID_KIND: {}", kind_str));
    }
    if kind_str == "OpenAICompatible" {
        let bu = req.base_url.as_deref().unwrap_or("");
        if bu.is_empty() { return err!(StatusCode::UNPROCESSABLE_ENTITY, "INVALID_BASE_URL"); }
        if req.default_model.as_deref().map(|m| m.is_empty()).unwrap_or(true) { return err!(StatusCode::UNPROCESSABLE_ENTITY, "MODEL_CONFIG_INVALID: default_model required"); }
        let temp = req.temperature.unwrap_or(0.7);
        if temp < 0.0 || temp > 2.0 { return err!(StatusCode::UNPROCESSABLE_ENTITY, format!("INVALID_TEMPERATURE: {}", temp)); }
        let to = req.timeout_ms.unwrap_or(30000);
        if to <= 0 { return err!(StatusCode::UNPROCESSABLE_ENTITY, format!("INVALID_TIMEOUT: {}", to)); }
        let mt = req.max_tokens.unwrap_or(4096);
        if mt <= 0 { return err!(StatusCode::UNPROCESSABLE_ENTITY, format!("INVALID_MAX_TOKENS: {}", mt)); }
        if let Some(cost) = req.max_cost_per_task_usd { if cost < 0.0 { return err!(StatusCode::UNPROCESSABLE_ENTITY, format!("INVALID_COST: {}", cost)); } }
    }
    // Get existing key
    let existing_key = if req.clear_api_key.unwrap_or(false) {
        String::new()
    } else if req.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) {
        req.api_key.clone().unwrap_or_default()
    } else {
        ModelConfigRepo::get_active_config(&s.pool).await.ok().flatten()
            .map(|c| c.api_key).unwrap_or_default()
    };
    if kind_str == "OpenAICompatible" && existing_key.is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "MISSING_API_KEY");
    }
    let mask = ModelConfigRepo::mask_key(&existing_key);
    let now = chrono::Utc::now().timestamp_millis();
    let pid = req.provider_id.clone();
    if let Err(e) = ModelConfigRepo::upsert_config(&s.pool, &pid, &kind_str, &req.base_url.unwrap_or_default(), &existing_key, &mask, &req.default_model.unwrap_or_default(), &req.fast_model.unwrap_or_default(), &req.reasoning_model.unwrap_or_default(), &req.structured_output_model.unwrap_or_default(), req.max_tokens.unwrap_or(4096), req.temperature.unwrap_or(0.7), req.timeout_ms.unwrap_or(30000), req.max_cost_per_task_usd.unwrap_or(0.0)).await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {}", e));
    }
    if let Err(e) = ModelConfigRepo::deactivate_others(&s.pool, &pid).await {
        return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {}", e));
    }
    ok!(serde_json::json!({"ok":true,"provider_id":pid,"kind":kind_str,"api_key_masked":mask}))
}

pub async fn test_connection(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let config = match ModelConfigRepo::get_active_config_or_seed(&s.pool).await {
        Ok(c) => c,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Config error: {}", e)),
    };
    let gateway = select_gateway(config.kind);
    match gateway.test_connection(&config).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

pub async fn chat(State(s): State<AppState>, Json(req): Json<ChatRequest>) -> (StatusCode, Json<serde_json::Value>) {
    let config = match ModelConfigRepo::get_active_config_or_seed(&s.pool).await {
        Ok(c) => c,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Config error: {}", e)),
    };
    let gateway = select_gateway(config.kind);
    let role_str = req.role.clone().unwrap_or_default();
    let role: ModelRole = match serde_json::from_str(&format!("\"{}\"", &role_str)) {
        Ok(r) => r,
        Err(_) => return err!(StatusCode::UNPROCESSABLE_ENTITY, format!("MODEL_ROLE_INVALID: {}", role_str)),
    };
    let mr = ModelRequest { config: config.clone(), role, model: config.default_model.clone(), messages: req.messages, temperature: req.temperature.unwrap_or(config.temperature), max_tokens: req.max_tokens.unwrap_or(config.max_tokens), response_format: ResponseFormat::Text };
    match gateway.chat(&mr).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

pub async fn structured(State(s): State<AppState>, Json(req): Json<StructuredRequest>) -> (StatusCode, Json<serde_json::Value>) {
    let config = match ModelConfigRepo::get_active_config_or_seed(&s.pool).await {
        Ok(c) => c,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, format!("Config error: {}", e)),
    };
    let gateway = select_gateway(config.kind);
    let role_str = req.role.clone().unwrap_or_default();
    let role: ModelRole = match serde_json::from_str(&format!("\"{}\"", &role_str)) {
        Ok(r) => r,
        Err(_) => return err!(StatusCode::UNPROCESSABLE_ENTITY, format!("MODEL_ROLE_INVALID: {}", role_str)),
    };
    let mr = ModelRequest { config: config.clone(), role, model: config.structured_output_model.clone(), messages: req.messages, temperature: req.temperature.unwrap_or(config.temperature), max_tokens: req.max_tokens.unwrap_or(config.max_tokens), response_format: ResponseFormat::Json };
    let schema = req.schema.unwrap_or(serde_json::json!({"type":"object"}));
    match gateway.structured(&mr, &schema).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

pub async fn list_model_profiles() -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::to_value(default_model_profiles()).unwrap())
}

pub async fn route_model(Json(req): Json<ModelRoutingRequest>) -> (StatusCode, Json<serde_json::Value>) {
    match ModelRouter::route(&req, &default_model_profiles(), None) {
        Ok(decision) => ok!(serde_json::to_value(&decision).unwrap()),
        Err(e) => err!(StatusCode::BAD_REQUEST, e.to_string()),
    }
}
