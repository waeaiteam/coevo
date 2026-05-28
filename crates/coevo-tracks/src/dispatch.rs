//! Track dispatcher — classifies tasks and routes to Green/Yellow/Red.
//! Per coevo whitepaper Section 11.

use coevo_core::track::Track;
use sqlx::SqlitePool;

use crate::green::{GreenTrackResult, GreenTrackRunner};
use crate::red::{RedTrackResult, RedTrackRunner};
use crate::yellow::{YellowTrackResult, YellowTrackRunner};

/// Generic dispatch result with optional typed inner results.
pub struct DispatchResult {
    pub track: Track,
    pub green_result: Option<GreenTrackResult>,
    pub yellow_result: Option<YellowTrackResult>,
    pub red_result: Option<RedTrackResult>,
    pub message: String,
}

pub async fn dispatch_green(
    pool: &SqlitePool,
    user_intent: &str,
    agent_ids: Vec<String>,
    tenant_id: &str,
) -> Result<DispatchResult, TrackError> {
    let runner = GreenTrackRunner::new();
    let result = runner
        .run(pool, user_intent, agent_ids, tenant_id)
        .await
        .map_err(|e| TrackError::GreenError(e.to_string()))?;

    Ok(DispatchResult {
        track: Track::Green,
        green_result: Some(result),
        yellow_result: None,
        red_result: None,
        message: "Green Track completed successfully".to_string(),
    })
}

pub async fn dispatch_yellow(
    pool: &SqlitePool,
    user_intent: &str,
    agent_ids: Vec<String>,
    tenant_id: &str,
    environment: &str,
) -> Result<DispatchResult, TrackError> {
    let result = YellowTrackRunner::run(pool, user_intent, agent_ids, tenant_id, environment)
        .await
        .map_err(|e| TrackError::YellowError(e.to_string()))?;

    Ok(DispatchResult {
        track: Track::Yellow,
        green_result: None,
        yellow_result: Some(result),
        red_result: None,
        message: "Yellow Track completed".to_string(),
    })
}

pub async fn dispatch_red(
    pool: &SqlitePool,
    user_intent: &str,
    agent_ids: Vec<String>,
    tenant_id: &str,
    caller_identity_proof: Option<&str>,
    monitoring_signature: Option<&str>,
    diagnostic_signature: Option<&str>,
) -> Result<DispatchResult, TrackError> {
    let result = RedTrackRunner::run(
        pool,
        user_intent,
        agent_ids,
        tenant_id,
        caller_identity_proof,
        monitoring_signature,
        diagnostic_signature,
    )
    .await
    .map_err(|e| TrackError::RedError(e.to_string()))?;

    Ok(DispatchResult {
        track: Track::Red,
        green_result: None,
        yellow_result: None,
        red_result: Some(result),
        message: "Red Track completed".to_string(),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum TrackError {
    #[error("green track error: {0}")]
    GreenError(String),
    #[error("yellow track error: {0}")]
    YellowError(String),
    #[error("red track error: {0}")]
    RedError(String),
}
