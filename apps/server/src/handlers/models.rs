use axum::{Json, http::StatusCode};
use serde::Deserialize;
use coevo_models::types::*;
use coevo_models::gateway::select_gateway;
use coevo_models::router::{ModelRouter, default_model_profiles, ModelRoutingRequest};
use std::sync::Mutex;

macro_rules! ok { ($v:expr) => { (StatusCode::OK, Json($v)) } }

#[derive(Deserialize)] pub struct ChatRequest { pub role: Option<String>, pub messages: Vec<ModelMessage>, pub temperature: Option<f64>, pub max_tokens: Option<u32> }
#[derive(Deserialize)] pub struct StructuredRequest { pub role: Option<String>, pub messages: Vec<ModelMessage>, pub schema: Option<serde_json::Value>, pub temperature: Option<f64>, pub max_tokens: Option<u32> }

static CONFIG: Mutex<Option<ModelProviderConfig>> = Mutex::new(None);

fn get_config() -> ModelProviderConfig {
    CONFIG.lock().unwrap().clone().unwrap_or_else(ModelProviderConfig::mock)
}

pub async fn get_config_handler() -> (StatusCode, Json<serde_json::Value>) {
    let mut c = get_config();
    c.api_key = c.mask_key();
    ok!(serde_json::to_value(&c).unwrap())
}

pub async fn put_config_handler(Json(c): Json<ModelProviderConfig>) -> (StatusCode, Json<serde_json::Value>) {
    *CONFIG.lock().unwrap() = Some(c);
    ok!(serde_json::json!({"ok":true}))
}

pub async fn test_connection() -> (StatusCode, Json<serde_json::Value>) {
    let config = get_config();
    let gateway = select_gateway(config.kind);
    match gateway.test_connection(&config).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

pub async fn chat(Json(req): Json<ChatRequest>) -> (StatusCode, Json<serde_json::Value>) {
    let config = get_config();
    let gateway = select_gateway(config.kind);
    let role: ModelRole = serde_json::from_str(&format!("\"{}\"", req.role.unwrap_or_default())).unwrap_or(ModelRole::Synthesizer);
    let mr = ModelRequest { config: config.clone(), role, model: config.default_model.clone(), messages: req.messages, temperature: req.temperature.unwrap_or(config.temperature), max_tokens: req.max_tokens.unwrap_or(config.max_tokens), response_format: ResponseFormat::Text };
    match gateway.chat(&mr).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

pub async fn structured(Json(req): Json<StructuredRequest>) -> (StatusCode, Json<serde_json::Value>) {
    let config = get_config();
    let gateway = select_gateway(config.kind);
    let role: ModelRole = serde_json::from_str(&format!("\"{}\"",req.role.unwrap_or_default())).unwrap_or(ModelRole::MissionDraft);
    let mr = ModelRequest { config: config.clone(), role, model: config.structured_output_model.clone(), messages: req.messages, temperature: req.temperature.unwrap_or(config.temperature), max_tokens: req.max_tokens.unwrap_or(config.max_tokens), response_format: ResponseFormat::Json };
    let schema = req.schema.unwrap_or(serde_json::json!({"type":"object"}));
    match gateway.structured(&mr, &schema).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

pub async fn list_model_profiles() -> (StatusCode, Json<serde_json::Value>) {
    ok!(serde_json::to_value(default_model_profiles()).unwrap())
}

pub async fn route_model(Json(req): Json<ModelRoutingRequest>) -> (StatusCode, Json<serde_json::Value>) {
    match ModelRouter::route(&req, &default_model_profiles(), None) {
        Ok(decision) => ok!(serde_json::to_value(&decision).unwrap()),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}
