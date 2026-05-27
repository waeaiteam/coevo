//! Stance matrix specification for resolution engine.
//! Per coevo whitepaper Section 10.

use serde::{Deserialize, Serialize};

/// Stance matrix: captures every node's position, blockers, and compromises.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StanceMatrixSpec {
    /// Per-agent stance entries.
    pub stances: Vec<StanceEntry>,
    /// The issue/prompt under contention.
    pub issue: String,
    /// Context reference (contract hash, plan hash, etc.).
    pub context_ref: String,
    /// Maximum allowed debate rounds.
    pub max_rounds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StanceEntry {
    pub agent_id: String,
    /// SUPPORT or OPPOSE.
    pub position: StancePosition,
    /// Confidence weight (reputation-backed).
    pub weight: f64,
    /// Evidence URNs backing this stance.
    pub evidence_urns: Vec<String>,
    /// Whether this agent has veto power.
    pub has_veto: bool,
    /// Proposed compromise, if any.
    pub compromise_proposal: Option<String>,
    /// Stance round number.
    pub round: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StancePosition {
    Support,
    Oppose,
}

/// Resolution output from the ResolutionEngine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionDecisionSpec {
    pub decision: ResolutionVerdict,
    /// If resolved, the chosen path.
    pub resolved_path: Option<String>,
    /// If deadlocked, the blocking nodes.
    pub blocking_nodes: Vec<String>,
    /// ADR-A record for this resolution.
    pub adr: Option<super::decision::AdrA>,
    /// Recommended escalation if unresolved.
    pub escalation: Option<EscalationAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionVerdict {
    Resolved,
    Deadlocked,
    Escalated,
    Compromised,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationAction {
    pub target: String,
    pub reason: String,
    pub requires_human_arbitration: bool,
}
