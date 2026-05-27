//! Track dispatcher — classifies tasks and routes to Green/Yellow/Red.
//! Per coevo whitepaper Section 11.

use coevo_core::track::{classify_track, Track};
use sqlx::SqlitePool;

use crate::green::GreenTrackResult;
use crate::green::GreenTrackRunner;

/// Track dispatch result.
pub struct DispatchResult {
    pub track: Track,
    pub green_result: Option<GreenTrackResult>,
    pub message: String,
}

/// Route a task to the appropriate track based on blast radius and irreversibility.
pub async fn dispatch_green(
    pool: &SqlitePool,
    user_intent: &str,
    agent_ids: Vec<String>,
    tenant_id: &str,
) -> Result<DispatchResult, TrackError> {
    let track = classify_track(0, 0); // Green = BR 0, IR 0
    let runner = GreenTrackRunner::new();
    let result = runner
        .run(pool, user_intent, agent_ids, tenant_id)
        .await?;

    Ok(DispatchResult {
        track,
        green_result: Some(result),
        message: "Green Track completed successfully".to_string(),
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
