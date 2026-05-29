use axum::{extract::State, Json, http::StatusCode};
use serde::Deserialize;
use sqlx::Row;
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

async fn get_active_config(pool: &sqlx::SqlitePool) -> Result<ModelProviderConfig, sqlx::Error> {
    ModelConfigRepo::seed_mock_if_empty(pool).await?;
    let row = ModelConfigRepo::get_active(pool).await?.ok_or(sqlx::Error::RowNotFound)?;
    Ok(ModelProviderConfig{
        provider_id: row.get("provider_id"),
        kind: serde_json::from_str(&format!("\"{}\"", row.get::<String,_>("kind"))).unwrap_or(ModelProviderKind::Mock),
        base_url: row.get("base_url"),
        api_key: row.get("api_key_ciphertext"),
        default_model: row.get("default_model"),
        fast_model: row.get("fast_model"),
        reasoning_model: row.get("reasoning_model"),
        structured_output_model: row.get("structured_output_model"),
        max_tokens: row.get::<i64,_>("max_tokens") as u32,
        temperature: row.get("temperature"),
        timeout_ms: row.get::<i64,_>("timeout_ms") as u64,
        max_cost_per_task_usd: row.get("max_cost_per_task_usd"),
    })
}

pub async fn get_config_handler(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match get_active_config(&s.pool).await {
        Ok(mut c) => { c.api_key = c.mask_key(); ok!(serde_json::to_value(&c).unwrap()) }
        Err(_) => ok!(serde_json::to_value(&ModelProviderConfig::mock()).unwrap()),
    }
}

pub async fn put_config_handler(State(s): State<AppState>, Json(req): Json<PutConfigRequest>) -> (StatusCode, Json<serde_json::Value>) {
    // Get existing key if not clearing and no new key provided
    let existing_key = if !req.clear_api_key.unwrap_or(false) && req.api_key.as_ref().map(|k| k.is_empty()).unwrap_or(true) {
        ModelConfigRepo::get_active(&s.pool).await.ok().flatten().map(|r| r.get::<String,_>("api_key_ciphertext")).unwrap_or_default()
    } else { req.api_key.clone().unwrap_or_default() };
    let mask = ModelConfigRepo::mask_key(&existing_key);
    let now = chrono::Utc::now().timestamp_millis();
    let pid = req.provider_id.clone();
    let kind = req.kind.clone();
    // Upsert via raw SQL
    sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,1,?,?) ON CONFLICT(provider_id) DO UPDATE SET kind=excluded.kind,base_url=excluded.base_url,api_key_ciphertext=excluded.api_key_ciphertext,api_key_masked=excluded.api_key_masked,default_model=excluded.default_model,fast_model=excluded.fast_model,reasoning_model=excluded.reasoning_model,structured_output_model=excluded.structured_output_model,max_tokens=excluded.max_tokens,temperature=excluded.temperature,timeout_ms=excluded.timeout_ms,max_cost_per_task_usd=excluded.max_cost_per_task_usd,is_active=1,updated_at_ms=excluded.updated_at_ms")
        .bind(&pid).bind(&kind).bind(req.base_url.unwrap_or_default()).bind(&existing_key).bind(&mask)
        .bind(req.default_model.unwrap_or_default()).bind(req.fast_model.unwrap_or_default()).bind(req.reasoning_model.unwrap_or_default()).bind(req.structured_output_model.unwrap_or_default())
        .bind(req.max_tokens.unwrap_or(4096)).bind(req.temperature.unwrap_or(0.7)).bind(req.timeout_ms.unwrap_or(30000)).bind(req.max_cost_per_task_usd.unwrap_or(0.0))
        .bind(now).bind(now).execute(&s.pool).await.map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())).ok();
    // Deactivate others
    sqlx::query("UPDATE model_provider_configs SET is_active=0,updated_at_ms=? WHERE provider_id!=?").bind(now).bind(&pid).execute(&s.pool).await.ok();
    ok!(serde_json::json!({"ok":true,"provider_id":pid,"kind":kind,"api_key_masked":mask}))
}

pub async fn test_connection(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let config = get_active_config(&s.pool).await.unwrap_or_else(|_| ModelProviderConfig::mock());
    let gateway = select_gateway(config.kind);
    match gateway.test_connection(&config).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

pub async fn chat(State(s): State<AppState>, Json(req): Json<ChatRequest>) -> (StatusCode, Json<serde_json::Value>) {
    let config = get_active_config(&s.pool).await.unwrap_or_else(|_| ModelProviderConfig::mock());
    let gateway = select_gateway(config.kind);
    let role: ModelRole = serde_json::from_str(&format!("\"{}\"", req.role.unwrap_or_default())).unwrap_or(ModelRole::Synthesizer);
    let mr = ModelRequest { config: config.clone(), role, model: config.default_model.clone(), messages: req.messages, temperature: req.temperature.unwrap_or(config.temperature), max_tokens: req.max_tokens.unwrap_or(config.max_tokens), response_format: ResponseFormat::Text };
    match gateway.chat(&mr).await {
        Ok(r) => ok!(serde_json::to_value(&r).unwrap()),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

pub async fn structured(State(s): State<AppState>, Json(req): Json<StructuredRequest>) -> (StatusCode, Json<serde_json::Value>) {
    let config = get_active_config(&s.pool).await.unwrap_or_else(|_| ModelProviderConfig::mock());
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
