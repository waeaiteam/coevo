//! Resolution Stopper — determines when to halt debate rounds.
//! v1: Rule-based stopper. Wald optional via feature flag.
//! Per coevo whitepaper requirement: Wald only as optional feature.

/// Trait for resolution stopping strategies.
pub trait ResolutionStopper: Send + Sync {
    /// Determine whether the resolution process should stop.
    /// `round`: current debate round
    /// `consensus_ratio`: support / total weight
    /// `max_rounds`: hard limit on rounds
    fn should_stop(&self, round: u32, consensus_ratio: f64, max_rounds: u32) -> bool;
}

/// Rule-based stopper — stops when max rounds reached or consensus is impossible.
pub struct RuleBasedStopper {
    /// Minimum consensus improvement per round to continue.
    min_improvement: f64,
    /// Previous consensus ratio for trend tracking.
    prev_ratio: std::sync::Mutex<Option<f64>>,
}

impl RuleBasedStopper {
    pub fn new(min_improvement: u32) -> Self {
        Self {
            min_improvement: min_improvement as f64 * 0.05,
            prev_ratio: std::sync::Mutex::new(None),
        }
    }
}

impl ResolutionStopper for RuleBasedStopper {
    fn should_stop(&self, round: u32, consensus_ratio: f64, max_rounds: u32) -> bool {
        // Hard stop at max rounds
        if round >= max_rounds {
            return true;
        }

        // Stop if there's been no improvement
        let mut prev = self.prev_ratio.lock().unwrap();
        let should_stop = if let Some(p) = *prev {
            (consensus_ratio - p).abs() < self.min_improvement
        } else {
            false
        };
        *prev = Some(consensus_ratio);
        should_stop
    }
}

/// Wald sequential probability ratio stopper.
/// Only available with `wald` feature flag.
#[cfg(feature = "wald")]
pub struct WaldStopper {
    alpha: f64,
    beta: f64,
}

#[cfg(feature = "wald")]
impl WaldStopper {
    pub fn new(alpha: f64, beta: f64) -> Self {
        Self { alpha, beta }
    }
}

#[cfg(feature = "wald")]
impl ResolutionStopper for WaldStopper {
    fn should_stop(&self, _round: u32, consensus_ratio: f64, max_rounds: u32) -> bool {
        // Wald SPRT: compare likelihood ratio against thresholds
        // Simplified for v1
        let threshold_a = (1.0 - self.beta) / self.alpha;
        let threshold_b = self.beta / (1.0 - self.alpha);

        if consensus_ratio >= 0.8 {
            true
        } else if consensus_ratio <= 0.2 {
            true
        } else {
            false
        }
    }
}
