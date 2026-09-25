//! What to do about each class of failure.
//!
//! Before this section existed the loop had exactly one reaction to trouble:
//! keep iterating until a stop gate fired. That is the right answer for a node
//! that produced a poor result and the wrong answer for almost everything else
//! — a provider that is briefly unreachable wants a retry, a corrupted store
//! wants a checkpoint restore, and a safety violation wants the run to stop
//! *now* rather than after two more iterations of budget.
//!
//! Failures are classified by cause, not by which node hit them, because the
//! right response to an unreachable provider is the same wherever it happens.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How a retry spaces its attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Backoff {
    /// Same delay every time.
    Fixed,
    /// Delay grows by the base each attempt.
    Linear,
    /// Delay doubles each attempt. The right default against a rate limit.
    Exponential,
}

impl Backoff {
    /// Delay before retry `n` (1-based), given a base delay in seconds. The
    /// first retry waits exactly `base` under every strategy; they differ in
    /// how the wait grows after that.
    pub fn delay_seconds(self, base: u64, attempt: u32) -> u64 {
        let n = attempt.max(1);
        match self {
            Backoff::Fixed => base,
            Backoff::Linear => base.saturating_mul(n as u64),
            // Shift rather than pow so a large attempt count cannot overflow
            // into a tiny delay. Capped at 2^16 × base, which is already far
            // past any sane wall-clock budget.
            Backoff::Exponential => base.saturating_mul(1u64 << (n - 1).min(16)),
        }
    }
}

/// What the run does when a given failure class occurs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryAction {
    /// Re-dispatch the same work unchanged. For failures that are about the
    /// world, not the output.
    Retry {
        /// Dispatches in total, counting the one that failed: 3 is the first
        /// try and two more.
        #[serde(default = "default_attempts")]
        max_attempts: u32,
        /// Delay before the first retry; `backoff` decides the rest.
        #[serde(default = "default_base_delay")]
        base_delay_seconds: u64,
        #[serde(default = "default_backoff")]
        backoff: Backoff,
    },
    /// Re-dispatch, telling the node what was wrong with its last output. For
    /// failures that are about the output.
    Revise {
        /// Dispatches in total, counting the one that was refused.
        #[serde(default = "default_attempts")]
        max_attempts: u32,
    },
    /// Move to the next provider in the tier's cascade.
    Fallback,
    /// Halt this node and record an escalation for a human to answer.
    Escalate,
    /// Halt the whole run but keep the state resumable.
    Pause,
    /// Roll the store back to the last good checkpoint and resume from there.
    RestoreCheckpoint,
    /// Stop the run immediately. No further dispatch, no perturbation.
    Stop,
}

fn default_attempts() -> u32 {
    3
}
fn default_base_delay() -> u64 {
    2
}
fn default_backoff() -> Backoff {
    Backoff::Exponential
}

/// The failure-class to action map.
///
/// The defaults encode the reference specification's recovery policy, which is
/// also what the corpus arrived at independently: retry the world, revise the
/// output, escalate a pattern, and never negotiate with a safety violation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recovery {
    /// The provider or a tool was briefly unavailable, timed out, or returned
    /// a rate-limit response.
    #[serde(default = "default_transient")]
    pub transient_error: RecoveryAction,
    /// The node returned something, but it did not satisfy the contract — a
    /// malformed artifact, a missing file it claimed to write.
    #[serde(default = "default_invalid_output")]
    pub invalid_output: RecoveryAction,
    /// A provider in the cascade is not installed or not reachable at all.
    #[serde(default = "default_tool_unavailable")]
    pub tool_unavailable: RecoveryAction,
    /// The same node has failed `max_revisions_per_node` times.
    #[serde(default = "default_repeated_failure")]
    pub repeated_failure: RecoveryAction,
    /// A node attempted something `safety.limits` forbids.
    #[serde(default = "default_safety_violation")]
    pub safety_violation: RecoveryAction,
    /// A budget ceiling was reached mid-iteration.
    #[serde(default = "default_resource_exhaustion")]
    pub resource_exhaustion: RecoveryAction,
    /// The store failed to read back what it wrote.
    #[serde(default = "default_corrupted_state")]
    pub corrupted_state: RecoveryAction,
}

fn default_transient() -> RecoveryAction {
    RecoveryAction::Retry {
        max_attempts: default_attempts(),
        base_delay_seconds: default_base_delay(),
        backoff: Backoff::Exponential,
    }
}
fn default_invalid_output() -> RecoveryAction {
    RecoveryAction::Revise { max_attempts: 2 }
}
fn default_tool_unavailable() -> RecoveryAction {
    RecoveryAction::Fallback
}
fn default_repeated_failure() -> RecoveryAction {
    RecoveryAction::Escalate
}
fn default_safety_violation() -> RecoveryAction {
    RecoveryAction::Stop
}
fn default_resource_exhaustion() -> RecoveryAction {
    RecoveryAction::Pause
}
fn default_corrupted_state() -> RecoveryAction {
    RecoveryAction::RestoreCheckpoint
}

impl Default for Recovery {
    fn default() -> Self {
        Self {
            transient_error: default_transient(),
            invalid_output: default_invalid_output(),
            tool_unavailable: default_tool_unavailable(),
            repeated_failure: default_repeated_failure(),
            safety_violation: default_safety_violation(),
            resource_exhaustion: default_resource_exhaustion(),
            corrupted_state: default_corrupted_state(),
        }
    }
}

/// The classes a failure can be sorted into. Kept apart from [`Recovery`] so
/// the engine can name a class without owning the policy for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    TransientError,
    InvalidOutput,
    ToolUnavailable,
    RepeatedFailure,
    SafetyViolation,
    ResourceExhaustion,
    CorruptedState,
}

impl FailureClass {
    pub const ALL: [FailureClass; 7] = [
        FailureClass::TransientError,
        FailureClass::InvalidOutput,
        FailureClass::ToolUnavailable,
        FailureClass::RepeatedFailure,
        FailureClass::SafetyViolation,
        FailureClass::ResourceExhaustion,
        FailureClass::CorruptedState,
    ];

    /// The name the config uses for this class.
    pub fn key(self) -> &'static str {
        match self {
            FailureClass::TransientError => "transient_error",
            FailureClass::InvalidOutput => "invalid_output",
            FailureClass::ToolUnavailable => "tool_unavailable",
            FailureClass::RepeatedFailure => "repeated_failure",
            FailureClass::SafetyViolation => "safety_violation",
            FailureClass::ResourceExhaustion => "resource_exhaustion",
            FailureClass::CorruptedState => "corrupted_state",
        }
    }
}

impl Recovery {
    /// The configured action for a class.
    pub fn action_for(&self, class: FailureClass) -> RecoveryAction {
        match class {
            FailureClass::TransientError => self.transient_error,
            FailureClass::InvalidOutput => self.invalid_output,
            FailureClass::ToolUnavailable => self.tool_unavailable,
            FailureClass::RepeatedFailure => self.repeated_failure,
            FailureClass::SafetyViolation => self.safety_violation,
            FailureClass::ResourceExhaustion => self.resource_exhaustion,
            FailureClass::CorruptedState => self.corrupted_state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff_cannot_overflow_into_a_short_delay() {
        // A naive `base * 2u64.pow(attempt)` wraps for large attempt counts and
        // produces a *shorter* delay than the attempt before it — turning a
        // backoff into a hot loop against whatever was rate-limiting us.
        let big = Backoff::Exponential.delay_seconds(2, 4096);
        let small = Backoff::Exponential.delay_seconds(2, 4);
        assert!(big >= small, "{big} should not be shorter than {small}");
    }

    #[test]
    fn the_first_retry_waits_the_base_delay_under_every_strategy() {
        for b in [Backoff::Fixed, Backoff::Linear, Backoff::Exponential] {
            assert_eq!(b.delay_seconds(3, 1), 3, "{b:?}");
        }
        assert_eq!(Backoff::Exponential.delay_seconds(3, 2), 6);
        assert_eq!(Backoff::Exponential.delay_seconds(3, 3), 12);
    }

    #[test]
    fn every_class_is_named_as_the_config_spells_it() {
        for class in FailureClass::ALL {
            let json = serde_json::to_string(&class).unwrap();
            assert_eq!(json.trim_matches('"'), class.key());
        }
    }

    #[test]
    fn a_safety_violation_stops_rather_than_retries() {
        // The one class where "try again" is never the right answer.
        assert_eq!(
            Recovery::default().action_for(FailureClass::SafetyViolation),
            RecoveryAction::Stop
        );
    }
}
