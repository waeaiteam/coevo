//! Three-track runtime types.
//! Per coevo whitepaper Section 11.

use serde::{Deserialize, Serialize};

/// The three coevo execution tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Track {
    /// Green: BR=0, IR=0 — fast, no human approval.
    Green,
    /// Yellow: IR=1 — async with approval window.
    Yellow,
    /// Red: IR=3 — physical circuit breaker, emergency lease.
    Red,
}

/// Track classification based on blast radius and irreversibility.
pub fn classify_track(blast_radius: u8, irreversibility: u8) -> Track {
    match (blast_radius, irreversibility) {
        (0, 0) => Track::Green,
        (0..=1, 1..=2) | (1, 0) => Track::Yellow,
        _ => Track::Red,
    }
}

/// Track execution context carrying all state needed for a track run.
#[derive(Debug, Clone)]
pub struct TrackContext {
    pub track: Track,
    pub contract: super::contract::MCLSpec,
    pub plan: super::plan::ExecutionPlanSpec,
    pub metadata: super::metadata::CommonMetadataHeader,
}
