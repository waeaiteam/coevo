use crate::error::WorkerError;
use crate::types::*;

pub struct WorkerStateMachine;
impl WorkerStateMachine {
    pub fn transition(
        current: WorkerStatus,
        next: WorkerStatus,
        ctx: &TransitionContext,
    ) -> Result<WorkerStatus, WorkerError> {
        use WorkerStatus::*;
        match (current, next) {
            (Idle, Assigned) => Ok(Assigned),
            (Assigned, Planning) => Ok(Planning),
            (Planning, Executing) => match ctx.track.as_str() {
                "red" => Err(WorkerError::RedTrackBlocked(
                    "Red Track requires identity, dual-sign, and lease".into(),
                )),
                "yellow" if !ctx.has_approval_receipt => Ok(WaitingApproval),
                "yellow" | "green" => Ok(Executing),
                _ => Ok(Executing),
            },
            (Executing, Reflecting) => Ok(Reflecting),
            (Reflecting, Completed) => Ok(Completed),
            (Executing, Failed) => Ok(Failed),
            (Failed, Reflecting) => Ok(Reflecting),
            (Executing, WaitingApproval) => Ok(WaitingApproval),
            (WaitingApproval, Executing) if ctx.has_approval_receipt => Ok(Executing),
            (WaitingApproval, Cancelled) => Ok(Cancelled),
            (s, Cancelled) if !matches!(s, Completed | Cancelled) => Ok(Cancelled),
            (Completed, _) | (Cancelled, _) => Err(WorkerError::InvalidStateTransition(format!(
                "Cannot transition from {:?}",
                current
            ))),
            _ => Err(WorkerError::InvalidStateTransition(format!(
                "{:?} → {:?} not allowed",
                current, next
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_to_completed() {
        let ctx = TransitionContext {
            track: "green".into(),
            has_approval_receipt: false,
            has_valid_lease: false,
            reason: "test".into(),
        };
        assert!(
            WorkerStateMachine::transition(WorkerStatus::Idle, WorkerStatus::Assigned, &ctx)
                .is_ok()
        );
        assert!(WorkerStateMachine::transition(
            WorkerStatus::Assigned,
            WorkerStatus::Planning,
            &ctx
        )
        .is_ok());
        assert!(WorkerStateMachine::transition(
            WorkerStatus::Planning,
            WorkerStatus::Executing,
            &ctx
        )
        .is_ok());
        assert!(WorkerStateMachine::transition(
            WorkerStatus::Executing,
            WorkerStatus::Reflecting,
            &ctx
        )
        .is_ok());
        assert!(WorkerStateMachine::transition(
            WorkerStatus::Reflecting,
            WorkerStatus::Completed,
            &ctx
        )
        .is_ok());
    }
    #[test]
    fn red_blocked() {
        let ctx = TransitionContext {
            track: "red".into(),
            has_approval_receipt: false,
            has_valid_lease: false,
            reason: "test".into(),
        };
        assert!(WorkerStateMachine::transition(
            WorkerStatus::Planning,
            WorkerStatus::Executing,
            &ctx
        )
        .is_err());
    }
    #[test]
    fn yellow_waiting() {
        let ctx = TransitionContext {
            track: "yellow".into(),
            has_approval_receipt: false,
            has_valid_lease: false,
            reason: "test".into(),
        };
        assert_eq!(
            WorkerStateMachine::transition(WorkerStatus::Planning, WorkerStatus::Executing, &ctx)
                .unwrap(),
            WorkerStatus::WaitingApproval
        );
    }
    #[test]
    fn completed_no_transition() {
        let ctx = TransitionContext {
            track: "green".into(),
            has_approval_receipt: false,
            has_valid_lease: false,
            reason: "test".into(),
        };
        assert!(WorkerStateMachine::transition(
            WorkerStatus::Completed,
            WorkerStatus::Assigned,
            &ctx
        )
        .is_err());
    }
}
