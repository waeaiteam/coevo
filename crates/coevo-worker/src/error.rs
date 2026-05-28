use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("WorkOrder not found: {0}")]
    WorkOrderNotFound(String),
    #[error("Worker not found: {0}")]
    WorkerNotFound(String),
    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),
    #[error("Session busy")]
    SessionBusy,
    #[error("Session lock timeout")]
    SessionLockTimeout,
    #[error("Red Track blocked: {0}")]
    RedTrackBlocked(String),
    #[error("Yellow approval required")]
    YellowApprovalRequired,
    #[error("Tool denied by policy")]
    ToolDeniedByPolicy,
    #[error("Tool unavailable: {0}")]
    ToolUnavailable(String),
    #[error("Tool credential missing")]
    ToolCredentialMissing,
    #[error("Tool risk too high")]
    ToolRiskTooHigh,
    #[error("Skill not found: {0}")]
    SkillNotFound(String),
    #[error("Skill risk too high")]
    SkillRiskTooHigh,
    #[error("Skill permission escalation denied")]
    SkillPermissionEscalation,
    #[error("Memory write denied")]
    MemoryWriteDenied,
    #[error("Fact write denied")]
    FactWriteDenied,
    #[error("Decision write denied")]
    DecisionWriteDenied,
    #[error("Executor denied")]
    ExecutorDenied,
    #[error("Timeout")]
    Timeout,
    #[error("Cancelled")]
    Cancelled,
    #[error("GitHub read failed: {0}")]
    GitHubReadFailed(String),
    #[error("File read denied")]
    FileReadDenied,
    #[error("Path traversal denied")]
    PathTraversalDenied,
    #[error("Internal error: {0}")]
    Internal(String),
}
