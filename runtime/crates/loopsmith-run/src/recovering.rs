//! Typed recovery: what the engine does about each class of failure.
//!
//! Before 1.0 the loop had one reaction to trouble — keep iterating until a
//! stop gate fired. That is right for a node that produced a poor result and
//! wrong for almost everything else: an unreachable provider wants a retry, a
//! judge that ignored the output contract wants to be asked again, and a
//! safety violation wants the run stopped *now* rather than two iterations of
//! budget later.
//!
//! The policy is data (`safety.recovery`, one action per [`FailureClass`]).
//! This module turns a class and an attempt count into a [`Response`] and
//! nothing more; carrying the response out is the dispatcher's job. Keeping
//! the decision pure is what lets every row of the table be tested without
//! running a provider.

use crate::logging::Recorder;
use crate::state::RunState;
use loopsmith_core::{FailureClass, Recovery, RecoveryAction};
use loopsmith_memory::{LedgerKind, Store};

/// What the engine does next about one failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Response {
    /// Dispatch the same node again, unchanged, after this many seconds.
    Retry { delay_seconds: u64 },
    /// Dispatch again at once, telling the node what was wrong last time.
    Revise,
    /// Accept the failure for this iteration and carry on. The node is
    /// eligible again next iteration.
    Continue,
    /// Stop dispatching this node for the rest of the run and put the
    /// question to a human. The run itself carries on.
    Escalate,
    /// Stop the run in this state.
    Halt(RunState),
}

impl Response {
    /// Whether this response sends the node round again.
    pub(crate) fn redispatches(&self) -> bool {
        matches!(self, Response::Retry { .. } | Response::Revise)
    }
}

/// Decide the response to a failure and write it to the ledger, in one step,
/// so no call site can do one without the other.
///
/// `may_redispatch` is false once dispatch has stopped for the iteration — a
/// halt, a budget ceiling — and turns a retry or revision into `Continue`: the
/// policy asked for another attempt, and there is no longer an attempt to
/// give. The ledger line says which was decided.
pub(crate) fn answer<S: Store>(
    rec: &Recorder<S>,
    policy: &Recovery,
    failure: &Failure,
    may_redispatch: bool,
) -> Response {
    let mut response = respond(policy, failure.class, failure.attempt);
    if response.redispatches() && !may_redispatch {
        response = Response::Continue;
    }
    rec.entry(
        failure.iteration,
        LedgerKind::Recovered,
        format!(
            "{}: {} — {}",
            failure.subject,
            failure.detail,
            describe(failure.class, &response, failure.attempt)
        ),
        failure.node.clone(),
    );
    response
}

/// One failure, as `answer` needs to see it.
pub(crate) struct Failure {
    pub class: FailureClass,
    /// How many times this has now happened, 1-based.
    pub attempt: u32,
    /// What failed, as the ledger names it: "`build`", "the checkpoint".
    pub subject: String,
    pub detail: String,
    pub node: Option<String>,
    pub iteration: u32,
}

/// The response to a failure that has now happened `attempt` times.
///
/// `max_attempts` counts dispatches, not retries: the default of 3 means the
/// first try and two more. `Fallback` has nothing left to do by the time a
/// failure reaches here — the provider cascade *is* the fallback, and it has
/// already been walked — so it continues.
pub(crate) fn respond(policy: &Recovery, class: FailureClass, attempt: u32) -> Response {
    match policy.action_for(class) {
        RecoveryAction::Retry {
            max_attempts,
            base_delay_seconds,
            backoff,
        } => {
            if attempt < max_attempts {
                Response::Retry {
                    delay_seconds: backoff.delay_seconds(base_delay_seconds, attempt),
                }
            } else {
                Response::Continue
            }
        }
        RecoveryAction::Revise { max_attempts } => {
            if attempt < max_attempts {
                Response::Revise
            } else {
                Response::Continue
            }
        }
        RecoveryAction::Fallback => Response::Continue,
        RecoveryAction::Escalate => Response::Escalate,
        RecoveryAction::Pause => Response::Halt(RunState::Paused),
        RecoveryAction::Stop => Response::Halt(RunState::Failed),
        RecoveryAction::RestoreCheckpoint => Response::Halt(RunState::RolledBack),
    }
}

/// The outcome state for a failure that belongs to the whole run rather than
/// to one node — a budget ceiling, a node out of revisions.
///
/// Retrying or revising a run-level condition means nothing (a budget does not
/// refill by being asked twice), so those actions fall back to `otherwise`,
/// which is what the run would have done before recovery existed.
pub(crate) fn run_outcome(policy: &Recovery, class: FailureClass, otherwise: RunState) -> RunState {
    match respond(policy, class, 1) {
        Response::Halt(state) => state,
        Response::Escalate => RunState::Escalated,
        Response::Retry { .. } | Response::Revise | Response::Continue => otherwise,
    }
}

/// A one-line account of a response, for the ledger.
pub(crate) fn describe(class: FailureClass, response: &Response, attempt: u32) -> String {
    let class = class_name(class);
    match response {
        Response::Retry { delay_seconds } => {
            format!("{class}; retrying (attempt {}) in {delay_seconds}s", attempt + 1)
        }
        Response::Revise => format!("{class}; asking again (attempt {})", attempt + 1),
        Response::Continue => format!("{class}; recovery exhausted, carrying on without it"),
        Response::Escalate => format!("{class}; escalated to a human"),
        Response::Halt(state) => format!("{class}; halting the run as `{state}`"),
    }
}

/// The class in prose: its config key with the underscores spoken.
pub(crate) fn class_name(class: FailureClass) -> String {
    class.key().replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_core::Backoff;

    fn policy() -> Recovery {
        Recovery::default()
    }

    #[test]
    fn a_transient_error_is_retried_with_growing_delays_then_let_go() {
        let p = policy();
        let first = respond(&p, FailureClass::TransientError, 1);
        let second = respond(&p, FailureClass::TransientError, 2);
        match (&first, &second) {
            (Response::Retry { delay_seconds: a }, Response::Retry { delay_seconds: b }) => {
                assert!(b > a, "exponential backoff must grow: {a}s then {b}s")
            }
            other => panic!("expected two retries, got {other:?}"),
        }
        // Three dispatches is the default budget; the third failure is final.
        assert_eq!(respond(&p, FailureClass::TransientError, 3), Response::Continue);
    }

    #[test]
    fn invalid_output_is_revised_not_retried_blind() {
        assert_eq!(respond(&policy(), FailureClass::InvalidOutput, 1), Response::Revise);
        assert_eq!(respond(&policy(), FailureClass::InvalidOutput, 2), Response::Continue);
    }

    #[test]
    fn a_safety_violation_halts_as_failed_on_the_first_occurrence() {
        assert_eq!(
            respond(&policy(), FailureClass::SafetyViolation, 1),
            Response::Halt(RunState::Failed)
        );
    }

    #[test]
    fn fallback_has_nothing_left_to_do_once_the_cascade_is_walked() {
        assert_eq!(respond(&policy(), FailureClass::ToolUnavailable, 1), Response::Continue);
    }

    #[test]
    fn a_budget_ceiling_pauses_unless_the_policy_says_otherwise() {
        let mut p = policy();
        assert_eq!(
            run_outcome(&p, FailureClass::ResourceExhaustion, RunState::Paused),
            RunState::Paused
        );
        p.resource_exhaustion = RecoveryAction::Stop;
        assert_eq!(
            run_outcome(&p, FailureClass::ResourceExhaustion, RunState::Paused),
            RunState::Failed
        );
        // A budget does not refill by being asked twice.
        p.resource_exhaustion = RecoveryAction::Retry {
            max_attempts: 5,
            base_delay_seconds: 1,
            backoff: Backoff::Fixed,
        };
        assert_eq!(
            run_outcome(&p, FailureClass::ResourceExhaustion, RunState::Paused),
            RunState::Paused
        );
    }

    #[test]
    fn a_node_out_of_revisions_is_escalated_by_default() {
        assert_eq!(
            run_outcome(&policy(), FailureClass::RepeatedFailure, RunState::Blocked),
            RunState::Escalated
        );
    }
}
