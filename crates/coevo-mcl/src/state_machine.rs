//! MCL Contract state machine — enforces strict one-way transitions.
//! Per coevo whitepaper Section 3.

use coevo_core::contract::ContractState;

/// Transition event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionEvent {
    /// OPA policy validation passed, no conflicts.
    PolicyValidationPass,
    /// Human or responsible role completed MFA + Ed25519 signing.
    ContractActivation,
    /// RiskGate triggered red-zone circuit breaker, or facts were degraded/revoked.
    AuditAlertTriggered,
    /// Execution plan requires revision.
    PlanRevisionRequired,
    /// Resolution engine produced a conflict-free new execution plan.
    ResolutionResolved,
    /// All goals in the goal tree achieved.
    GoalAchieved,
    /// Institution sent a physical revocation signal.
    ContractRevoked,
}

/// Result of a state transition attempt.
#[derive(Debug)]
pub struct TransitionResult {
    pub success: bool,
    pub old_state: ContractState,
    pub new_state: ContractState,
    pub event: TransitionEvent,
    pub message: String,
}

/// The MCL contract state machine.
/// Enforces all guard conditions defined in the whitepaper state transition matrix.
pub struct MCLStateMachine;

impl MCLStateMachine {
    /// Attempt to transition a contract from its current state to the next state
    /// given a triggering event. Returns the result including whether the
    /// transition was valid.
    pub fn transition(
        current: ContractState,
        event: TransitionEvent,
    ) -> Result<TransitionResult, StateMachineError> {
        let target = match (current, event) {
            // DraftContract → ValidatedContract
            (ContractState::DraftContract, TransitionEvent::PolicyValidationPass) => {
                ContractState::ValidatedContract
            }
            // ValidatedContract → ActiveContract
            (ContractState::ValidatedContract, TransitionEvent::ContractActivation) => {
                ContractState::ActiveContract
            }
            // ActiveContract → SuspendedContract
            (ContractState::ActiveContract, TransitionEvent::AuditAlertTriggered)
            | (ContractState::ActiveContract, TransitionEvent::PlanRevisionRequired) => {
                ContractState::SuspendedContract
            }
            // SuspendedContract → ActiveContract
            (ContractState::SuspendedContract, TransitionEvent::ResolutionResolved) => {
                ContractState::ActiveContract
            }
            // ActiveContract → ClosedContract
            (ContractState::ActiveContract, TransitionEvent::GoalAchieved)
            | (ContractState::ActiveContract, TransitionEvent::ContractRevoked) => {
                ContractState::ClosedContract
            }
            // SuspendedContract → ClosedContract
            (ContractState::SuspendedContract, TransitionEvent::GoalAchieved)
            | (ContractState::SuspendedContract, TransitionEvent::ContractRevoked) => {
                ContractState::ClosedContract
            }
            _ => {
                return Err(StateMachineError::InvalidTransition {
                    from: current,
                    event,
                });
            }
        };

        Ok(TransitionResult {
            success: true,
            old_state: current,
            new_state: target,
            event,
            message: format!(
                "Transitioned from {:?} to {:?} via {:?}",
                current, target, event
            ),
        })
    }

    /// Check if a transition is allowed without executing it.
    pub fn can_transition(current: ContractState, event: TransitionEvent) -> bool {
        matches!(
            (current, event),
            (
                ContractState::DraftContract,
                TransitionEvent::PolicyValidationPass
            ) | (
                ContractState::ValidatedContract,
                TransitionEvent::ContractActivation
            ) | (
                ContractState::ActiveContract,
                TransitionEvent::AuditAlertTriggered
            ) | (
                ContractState::ActiveContract,
                TransitionEvent::PlanRevisionRequired
            ) | (
                ContractState::SuspendedContract,
                TransitionEvent::ResolutionResolved
            ) | (ContractState::ActiveContract, TransitionEvent::GoalAchieved)
                | (
                    ContractState::ActiveContract,
                    TransitionEvent::ContractRevoked
                )
                | (
                    ContractState::SuspendedContract,
                    TransitionEvent::GoalAchieved
                )
                | (
                    ContractState::SuspendedContract,
                    TransitionEvent::ContractRevoked
                )
        )
    }

    /// Activate a validated contract — convenience method.
    pub fn activate(current: ContractState) -> Result<TransitionResult, StateMachineError> {
        Self::transition(current, TransitionEvent::ContractActivation)
    }

    /// Suspend an active contract — convenience method.
    pub fn suspend(current: ContractState) -> Result<TransitionResult, StateMachineError> {
        Self::transition(current, TransitionEvent::AuditAlertTriggered)
    }

    /// Close a contract — convenience method.
    pub fn close(current: ContractState) -> Result<TransitionResult, StateMachineError> {
        Self::transition(current, TransitionEvent::GoalAchieved)
    }

    /// Revoke a contract — convenience method.
    pub fn revoke(current: ContractState) -> Result<TransitionResult, StateMachineError> {
        Self::transition(current, TransitionEvent::ContractRevoked)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StateMachineError {
    #[error("invalid state transition: cannot go from {from:?} with event {event:?}")]
    InvalidTransition {
        from: ContractState,
        event: TransitionEvent,
    },
    #[error("contract is in terminal state ClosedContract; no further transitions allowed")]
    TerminalStateReached,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_lifecycle() {
        // Draft → Validated
        let r = MCLStateMachine::transition(
            ContractState::DraftContract,
            TransitionEvent::PolicyValidationPass,
        )
        .unwrap();
        assert_eq!(r.new_state, ContractState::ValidatedContract);

        // Validated → Active
        let r = MCLStateMachine::transition(
            ContractState::ValidatedContract,
            TransitionEvent::ContractActivation,
        )
        .unwrap();
        assert_eq!(r.new_state, ContractState::ActiveContract);

        // Active → Suspended
        let r = MCLStateMachine::transition(
            ContractState::ActiveContract,
            TransitionEvent::AuditAlertTriggered,
        )
        .unwrap();
        assert_eq!(r.new_state, ContractState::SuspendedContract);

        // Suspended → Active
        let r = MCLStateMachine::transition(
            ContractState::SuspendedContract,
            TransitionEvent::ResolutionResolved,
        )
        .unwrap();
        assert_eq!(r.new_state, ContractState::ActiveContract);

        // Active → Closed
        let r = MCLStateMachine::transition(
            ContractState::ActiveContract,
            TransitionEvent::GoalAchieved,
        )
        .unwrap();
        assert_eq!(r.new_state, ContractState::ClosedContract);
    }

    #[test]
    fn test_invalid_transition() {
        let result = MCLStateMachine::transition(
            ContractState::DraftContract,
            TransitionEvent::GoalAchieved,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_can_transition() {
        assert!(MCLStateMachine::can_transition(
            ContractState::DraftContract,
            TransitionEvent::PolicyValidationPass
        ));
        assert!(!MCLStateMachine::can_transition(
            ContractState::ClosedContract,
            TransitionEvent::PolicyValidationPass
        ));
    }
}
