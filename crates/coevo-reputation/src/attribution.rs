//! Failure attribution — split blame between agent, tool, and platform.
//! Per coevo whitepaper Section 6.2.

/// Attribution categories for task failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureAttribution {
    /// Agent's own fault — apply penalty.
    AgentFault,
    /// External API timeout or network issue — exempt agent.
    ExternalApiFailure,
    /// Upstream dirty data contaminated inputs — exempt agent.
    DirtyInput,
    /// Institution policy was self-contradictory — exempt agent, flag platform.
    PolicyContradiction,
    /// MCP tool returned bad data — penalize tool, not agent.
    ToolDataFault,
}

/// Determine attribution by analyzing the execution trace.
pub fn attribute_failure(
    execution_error: &str,
    tool_results: &[bool],
    network_error: bool,
    policy_conflict: bool,
) -> FailureAttribution {
    if network_error {
        return FailureAttribution::ExternalApiFailure;
    }
    if policy_conflict {
        return FailureAttribution::PolicyContradiction;
    }
    if tool_results.iter().any(|r| !*r) {
        return FailureAttribution::ToolDataFault;
    }
    if execution_error.contains("input") || execution_error.contains("corrupted") {
        return FailureAttribution::DirtyInput;
    }
    FailureAttribution::AgentFault
}
