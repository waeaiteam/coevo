//! coevo-evolution: Agent Skill Evolution Loop.
//! Observe → Diagnose → Propose → Verify → Approve → Publish → Monitor → Rollback.
//! Skills cannot auto-escalate permissions. All evolution governed by MCL/RiskGate/ADR-A.

pub mod analyzer;
pub mod generator;
pub mod scheduler;
pub mod verifier;
