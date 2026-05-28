//! RFC 9457 Problem Details for HTTP APIs.
//! All error responses MUST conform to this format.

use serde::{Deserialize, Serialize};

/// RFC 9457 compliant problem detail.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProblemDetails {
    /// A URI reference that identifies the problem type.
    #[serde(rename = "type")]
    pub type_uri: String,
    /// A short, human-readable summary of the problem type.
    pub title: String,
    /// The HTTP status code.
    pub status: u16,
    /// A human-readable explanation specific to this occurrence.
    pub detail: String,
    /// A URI reference that identifies the specific occurrence.
    pub instance: String,
    /// coevo internal fine-grained error code.
    pub error_code: String,
}

impl ProblemDetails {
    pub fn new(
        type_uri: &str,
        title: &str,
        status: u16,
        detail: &str,
        instance: &str,
        error_code: &str,
    ) -> Self {
        Self {
            type_uri: type_uri.to_string(),
            title: title.to_string(),
            status,
            detail: detail.to_string(),
            instance: instance.to_string(),
            error_code: error_code.to_string(),
        }
    }

    // ---- Pre-built constructors matching the whitepaper error table ----

    pub fn mcl_compilation_error(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/mcl-compilation-error",
            "MCL Compilation Failed",
            422,
            detail,
            instance,
            "MCL_COMPILATION_ERROR",
        )
    }

    pub fn mcl_institution_violation(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/mcl-institution-violation",
            "Institution Policy Violation",
            403,
            detail,
            instance,
            "MCL_INSTITUTION_VIOLATION",
        )
    }

    pub fn routing_no_path(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/routing-no-path",
            "No Compliant Routing Path",
            422,
            detail,
            instance,
            "ROUTING_NO_PATH",
        )
    }

    pub fn budget_exceeded(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/budget-exceeded",
            "Token Budget Exceeded",
            422,
            detail,
            instance,
            "BUDGET_EXCEEDED",
        )
    }

    pub fn version_required(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/version-required",
            "Precondition Required — expected_version missing",
            428,
            detail,
            instance,
            "VERSION_REQUIRED",
        )
    }

    pub fn version_mismatch(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/version-mismatch",
            "Precondition Failed — version mismatch",
            412,
            detail,
            instance,
            "VERSION_MISMATCH",
        )
    }

    pub fn cognitive_write_conflict(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/cognitive-write-conflict",
            "Cognitive Write Conflict",
            409,
            detail,
            instance,
            "COGNITIVE_WRITE_CONFLICT",
        )
    }

    pub fn cognitive_bound_violation(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/cognitive-bound-violation",
            "Cognitive Boundary Violation",
            403,
            detail,
            instance,
            "COGNITIVE_BOUND_VIOLATION",
        )
    }

    pub fn risk_denied(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/risk-denied",
            "Risk Threshold Not Met",
            403,
            detail,
            instance,
            "RISK_DENIED",
        )
    }

    pub fn approval_required(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/approval-required",
            "Human Approval Required",
            202,
            detail,
            instance,
            "APPROVAL_REQUIRED",
        )
    }

    pub fn deadlock_detected(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/deadlock-detected",
            "Irreconcilable Deadlock Detected",
            422,
            detail,
            instance,
            "DEADLOCK_DETECTED",
        )
    }

    pub fn internal_error(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/internal-error",
            "Internal Server Error",
            500,
            detail,
            instance,
            "INTERNAL_ERROR",
        )
    }

    pub fn forbidden(instance: &str, detail: &str) -> Self {
        Self::new(
            "https://coevo.dev/errors/forbidden",
            "Forbidden",
            403,
            detail,
            instance,
            "FORBIDDEN",
        )
    }
}

/// Axum-compatible response conversion.
impl axum::response::IntoResponse for ProblemDetails {
    fn into_response(self) -> axum::response::Response {
        let status = axum::http::StatusCode::from_u16(self.status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::Json(self);
        (status, body).into_response()
    }
}
