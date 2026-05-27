//! Structured audit logger.
//! Per coevo whitepaper — deterministic, read-only decision logging.

use coevo_store::repos::audit_repo::AuditRepo;
use sqlx::SqlitePool;

/// Audit event types.
#[derive(Debug, Clone, Copy)]
pub enum AuditEventType {
    ContractCompiled,
    ContractActivated,
    ContractSuspended,
    ContractClosed,
    PlanCreated,
    PlanRevised,
    FactProposed,
    FactPromoted,
    FactRevoked,
    FactStaled,
    RiskEvaluated,
    LeaseGranted,
    LeaseRevoked,
    ResolutionCompleted,
    AdrGenerated,
    HumanOverridden,
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditEventType::ContractCompiled => "contract.compiled",
            AuditEventType::ContractActivated => "contract.activated",
            AuditEventType::ContractSuspended => "contract.suspended",
            AuditEventType::ContractClosed => "contract.closed",
            AuditEventType::PlanCreated => "plan.created",
            AuditEventType::PlanRevised => "plan.revised",
            AuditEventType::FactProposed => "fact.proposed",
            AuditEventType::FactPromoted => "fact.promoted",
            AuditEventType::FactRevoked => "fact.revoked",
            AuditEventType::FactStaled => "fact.staled",
            AuditEventType::RiskEvaluated => "risk.evaluated",
            AuditEventType::LeaseGranted => "lease.granted",
            AuditEventType::LeaseRevoked => "lease.revoked",
            AuditEventType::ResolutionCompleted => "resolution.completed",
            AuditEventType::AdrGenerated => "adr.generated",
            AuditEventType::HumanOverridden => "human.overridden",
        }
    }
}

/// The audit logger.
pub struct AuditLogger;

impl AuditLogger {
    /// Log a structured audit event.
    pub async fn log(
        pool: &SqlitePool,
        event_type: AuditEventType,
        contract_hash: Option<&str>,
        agent_id: Option<&str>,
        traceparent: Option<&str>,
        tenant_id: &str,
        event_data: &serde_json::Value,
    ) -> Result<(), AuditError> {
        AuditRepo::insert(
            pool,
            event_type.as_str(),
            contract_hash,
            agent_id,
            traceparent,
            tenant_id,
            &serde_json::to_string(event_data).unwrap_or_default(),
        )
        .await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
