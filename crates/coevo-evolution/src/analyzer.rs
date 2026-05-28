//! Failure Analyzer: diagnose why a WorkOrder failed.
//! Inputs: WorkOrder, ADR-A, RiskGate decision, Resolution objections, user feedback.

use coevo_core::skills::*;

/// Analyze a failure and produce a structured diagnosis.
pub struct FailureAnalyzer;

impl FailureAnalyzer {
    pub fn analyze(
        error_message: &str,
        was_risk_denied: bool,
        _had_resolution_conflict: bool,
        user_correction: Option<&str>,
    ) -> FailureAnalysis {
        let lower = error_message.to_lowercase();

        let category = if lower.contains("not found") || lower.contains("missing") || lower.contains("no tool") {
            FailureCategory::MissingCapability
        } else if lower.contains("tool") || lower.contains("wrong") || lower.contains("misuse") {
            FailureCategory::WrongToolUse
        } else if lower.contains("prompt") || lower.contains("procedure") || lower.contains("step") {
            FailureCategory::BadPromptProcedure
        } else if lower.contains("evidence") || lower.contains("insufficient") || lower.contains("provenance") {
            FailureCategory::InsufficientEvidence
        } else if lower.contains("policy") || lower.contains("violation") || was_risk_denied {
            FailureCategory::PolicyViolation
        } else if lower.contains("stale") || lower.contains("expired") || lower.contains("memory") {
            FailureCategory::MemoryStale
        } else if lower.contains("executor") || lower.contains("external") || lower.contains("adapter") {
            FailureCategory::ExternalExecutorFailure
        } else if lower.contains("hallucinat") || lower.contains("confident") || lower.contains("fake") {
            FailureCategory::HallucinatedFact
        } else if lower.contains("overconfident") || lower.contains("bypass") {
            FailureCategory::OverConfidentDecision
        } else if user_correction.is_some() {
            FailureCategory::UserPreferenceMismatch
        } else {
            FailureCategory::MissingCapability
        };

        FailureAnalysis {
            category,
            root_cause: error_message.to_string(),
            suspected_missing_skill: Some("auto-diagnosed-skill".to_string()),
            suspected_skill_bug: if category == FailureCategory::BadPromptProcedure {
                Some("prompt-procedure-bug".to_string())
            } else {
                None
            },
            required_memory_update: category == FailureCategory::MemoryStale || category == FailureCategory::UserPreferenceMismatch,
            required_policy_update: category == FailureCategory::PolicyViolation,
        }
    }
}
