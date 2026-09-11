use std::time::Duration;

use crate::capability::{ActionFailure, FailureKind};

#[derive(Debug, Clone, Copy)]
pub enum Backoff {
    Fixed(Duration),
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff: Backoff,
}

impl RetryPolicy {
    pub fn allows(&self, attempt: u32) -> bool {
        attempt <= self.max_attempts
    }

    pub fn delay_for(&self, _attempt: u32) -> Duration {
        match self.backoff {
            Backoff::Fixed(duration) => duration,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDecision {
    Retry,
    Exhausted,
}

/// The only place that decides whether a failure gets another attempt.
/// Pure and deterministic — no LLM, no I/O, nothing that can block. A
/// permanent failure is never retried, regardless of budget remaining
/// ("PermanentFailure -> emit operational failure").
pub fn decide(attempt: u32, failure: &ActionFailure, policy: &RetryPolicy) -> RecoveryDecision {
    if failure.kind == FailureKind::Permanent {
        return RecoveryDecision::Exhausted;
    }
    if policy.allows(attempt) {
        RecoveryDecision::Retry
    } else {
        RecoveryDecision::Exhausted
    }
}
