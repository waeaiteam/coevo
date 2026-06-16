//! Policy-engine selection for track runners.
//!
//! Mirrors `coevo-worker`'s GovernGate: the live default is the fail-closed
//! [`DenyAllPolicyEngine`]; the keyword [`MockPolicyEngine`] is only used under
//! `cfg!(test)` (this crate's own unit tests) or when
//! `COEVO_ENABLE_MOCK_POLICY_ENGINE=1`. With no env var and outside tests, the
//! default is fail-closed — a track can never silently authorize an action
//! through an absent policy engine.

use coevo_policy::fail_closed::DenyAllPolicyEngine;
use coevo_policy::mock::MockPolicyEngine;
use coevo_policy::traits::PolicyEngine;

/// Return whether the keyword mock policy engine is permitted in this build.
fn mock_allowed() -> bool {
    cfg!(test)
        || matches!(
            std::env::var("COEVO_ENABLE_MOCK_POLICY_ENGINE"),
            Ok(value) if value == "1"
        )
}

/// Select the policy engine a track runner should gate with.
///
/// Fail-closed [`DenyAllPolicyEngine`] by default; [`MockPolicyEngine`] only
/// under tests or `COEVO_ENABLE_MOCK_POLICY_ENGINE=1`.
pub fn select_policy_engine() -> Box<dyn PolicyEngine> {
    if mock_allowed() {
        Box::new(MockPolicyEngine::new())
    } else {
        Box::new(DenyAllPolicyEngine)
    }
}
