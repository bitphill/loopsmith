//! The run's lifecycle, and the only place a transition is legal.
//!
//! Before 1.0 a run had no state of its own. It was "the loop is executing"
//! until it returned a [`StopReason`](crate::StopReason), and everything a
//! reader wanted to know — did it pause or give up, is it waiting for someone,
//! can it be resumed — had to be inferred from which stop gate fired. That
//! inference lived in every front end separately.
//!
//! Now there is one answer, persisted in the checkpoint, and this module owns
//! it:
//!
//! ```text
//! Created → Validating → Planning → [AwaitingApproval] → Running
//! Running ⇄ Retrying
//! Running → Paused | Blocked | Escalated | RolledBack | Failed | Succeeded
//! any of those → Closed → Validating   (resume)
//! ```
//!
//! Every other module asks [`Lifecycle::advance`] to move, and an illegal move
//! is an error rather than a silent overwrite. That is the property worth
//! having: a run cannot be recorded as `Succeeded` from `Planning`, because
//! the only road to `Succeeded` runs through `Running`, which runs through the
//! gate.

use loopsmith_core::GateOutcome;
use serde::{Deserialize, Serialize};

/// Where a run is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Nothing has happened yet.
    Created,
    /// Entry gates and config preflight.
    Validating,
    /// The execution graph is being scheduled.
    Planning,
    /// Approval gates are declared and have not all passed.
    AwaitingApproval,
    /// Iterating: dispatching, gating, deciding whether to go on.
    Running,
    /// A node's dispatch failed with a transient error and is being retried.
    Retrying,
    /// Stopped on purpose, resumable as-is — usually a budget ceiling.
    Paused,
    /// Stopped because nothing is moving. Needs a change before a resume
    /// will do anything different.
    Blocked,
    /// Stopped with a question for a human: a node that cannot make progress,
    /// or a gate that says a person must decide.
    Escalated,
    /// A rollback gate failed. Progress from the iteration that tripped it
    /// was discarded; spend was not.
    RolledBack,
    /// Stopped by a failure that must not be retried: a safety violation, an
    /// entry gate, an unschedulable graph.
    Failed,
    /// The gate certified overall success.
    Succeeded,
    /// Finished and written down. The state before this is the outcome.
    Closed,
}

impl RunState {
    pub const ALL: [RunState; 13] = [
        RunState::Created,
        RunState::Validating,
        RunState::Planning,
        RunState::AwaitingApproval,
        RunState::Running,
        RunState::Retrying,
        RunState::Paused,
        RunState::Blocked,
        RunState::Escalated,
        RunState::RolledBack,
        RunState::Failed,
        RunState::Succeeded,
        RunState::Closed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Created => "created",
            RunState::Validating => "validating",
            RunState::Planning => "planning",
            RunState::AwaitingApproval => "awaiting_approval",
            RunState::Running => "running",
            RunState::Retrying => "retrying",
            RunState::Paused => "paused",
            RunState::Blocked => "blocked",
            RunState::Escalated => "escalated",
            RunState::RolledBack => "rolled_back",
            RunState::Failed => "failed",
            RunState::Succeeded => "succeeded",
            RunState::Closed => "closed",
        }
    }

    pub fn parse(s: &str) -> Option<RunState> {
        RunState::ALL.into_iter().find(|st| st.as_str() == s)
    }

    /// The states this one may move to. The whole machine is this table.
    pub fn successors(self) -> &'static [RunState] {
        use RunState::*;
        match self {
            Created => &[Validating],
            Validating => &[Planning, Failed, Paused, Blocked, Escalated],
            Planning => &[AwaitingApproval, Running, Failed],
            AwaitingApproval => &[Running, Failed, Paused, Blocked, Escalated],
            Running => &[
                Retrying, Paused, Blocked, Escalated, RolledBack, Failed, Succeeded,
            ],
            Retrying => &[Running, Paused, Escalated, Failed],
            Paused | Blocked | Escalated | RolledBack | Failed | Succeeded => &[Closed],
            // Resuming re-enters through validation: the config may have been
            // edited while the run was stopped, and an entry gate that now
            // refuses must get the chance to.
            Closed => &[Validating],
        }
    }

    pub fn can_move_to(self, next: RunState) -> bool {
        self.successors().contains(&next)
    }

    /// A state a run ends in, just before it is closed.
    pub fn is_outcome(self) -> bool {
        self.successors() == [RunState::Closed]
    }

    /// The state a halting gate rule sends the run to. `None` for `warn`,
    /// which records and carries on.
    pub fn for_gate_outcome(outcome: GateOutcome) -> Option<RunState> {
        match outcome {
            GateOutcome::Stop => Some(RunState::Failed),
            GateOutcome::Escalate => Some(RunState::Escalated),
            GateOutcome::Pause => Some(RunState::Paused),
            GateOutcome::Rollback => Some(RunState::RolledBack),
            GateOutcome::Warn => None,
        }
    }
}

impl std::fmt::Display for RunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A move the table does not allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllegalTransition {
    pub from: RunState,
    pub to: RunState,
}

impl std::fmt::Display for IllegalTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a run cannot move from `{}` to `{}`", self.from, self.to)
    }
}

impl std::error::Error for IllegalTransition {}

/// A run's current state, and the one it closed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lifecycle {
    state: RunState,
    /// The outcome state, remembered through `Closed` so a reader can tell a
    /// paused run from a failed one without the ledger.
    outcome: Option<RunState>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    /// A run that has not started.
    pub fn new() -> Self {
        Lifecycle {
            state: RunState::Created,
            outcome: None,
        }
    }

    /// Pick up a run from what its checkpoint recorded.
    ///
    /// A run that closed normally resumes from `Closed`. A checkpoint with no
    /// state at all was written before 1.0, and every such run was closed —
    /// the old engine had no other way to stop. A run whose last recorded
    /// state is anything else did not close: the process died mid-run. That
    /// is reported as the second value, and the run is treated as closed so
    /// the resume goes through validation like any other.
    pub fn resume(stored: Option<RunState>) -> (Lifecycle, Option<RunState>) {
        let crashed = stored.filter(|s| *s != RunState::Closed && !s.is_outcome());
        let outcome = stored.filter(|s| s.is_outcome());
        (
            Lifecycle {
                state: RunState::Closed,
                outcome,
            },
            crashed,
        )
    }

    pub fn state(&self) -> RunState {
        self.state
    }

    /// The state the run closed from, once it has.
    pub fn outcome(&self) -> Option<RunState> {
        self.outcome
    }

    /// Move to `next`, returning the state left behind.
    pub fn advance(&mut self, next: RunState) -> Result<RunState, IllegalTransition> {
        let from = self.state;
        if !from.can_move_to(next) {
            return Err(IllegalTransition { from, to: next });
        }
        if from.is_outcome() && next == RunState::Closed {
            self.outcome = Some(from);
        }
        if next == RunState::Validating {
            self.outcome = None;
        }
        self.state = next;
        Ok(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(path: &[RunState]) -> Result<Lifecycle, IllegalTransition> {
        let mut l = Lifecycle::new();
        for s in path {
            l.advance(*s)?;
        }
        Ok(l)
    }

    #[test]
    fn the_ordinary_road_to_success_is_legal() {
        use RunState::*;
        let l = walk(&[Validating, Planning, Running, Succeeded, Closed]).expect("legal");
        assert_eq!(l.state(), Closed);
        assert_eq!(l.outcome(), Some(Succeeded));
    }

    #[test]
    fn success_is_unreachable_without_running() {
        // The point of the table: nothing can certify a run it never iterated.
        use RunState::*;
        for from in [Created, Validating, Planning, AwaitingApproval, Retrying] {
            assert!(!from.can_move_to(Succeeded), "{from} must not reach succeeded");
        }
    }

    #[test]
    fn an_illegal_move_is_refused_and_leaves_the_state_alone() {
        let mut l = Lifecycle::new();
        let err = l.advance(RunState::Running).expect_err("created cannot jump to running");
        assert_eq!(err.from, RunState::Created);
        assert_eq!(err.to, RunState::Running);
        assert_eq!(l.state(), RunState::Created);
    }

    #[test]
    fn a_closed_run_resumes_through_validation() {
        use RunState::*;
        let (mut l, crashed) = Lifecycle::resume(Some(Closed));
        assert_eq!(crashed, None);
        l.advance(Validating).expect("closed may resume");
        assert_eq!(l.outcome(), None, "a resumed run has no outcome yet");
    }

    #[test]
    fn a_checkpoint_without_a_state_is_a_run_that_closed() {
        let (l, crashed) = Lifecycle::resume(None);
        assert_eq!(l.state(), RunState::Closed);
        assert_eq!(crashed, None);
    }

    #[test]
    fn a_run_that_died_mid_iteration_is_reported_as_such() {
        let (l, crashed) = Lifecycle::resume(Some(RunState::Running));
        assert_eq!(l.state(), RunState::Closed);
        assert_eq!(crashed, Some(RunState::Running));
    }

    #[test]
    fn every_outcome_closes_and_nothing_else_does() {
        use RunState::*;
        let outcomes = [Paused, Blocked, Escalated, RolledBack, Failed, Succeeded];
        for s in RunState::ALL {
            assert_eq!(s.is_outcome(), outcomes.contains(&s), "{s}");
            assert_eq!(s.can_move_to(Closed), outcomes.contains(&s), "{s}");
        }
    }

    #[test]
    fn every_state_round_trips_through_its_name() {
        for s in RunState::ALL {
            assert_eq!(RunState::parse(s.as_str()), Some(s));
        }
        assert_eq!(RunState::parse("finished"), None);
    }

    #[test]
    fn a_warn_rule_does_not_halt() {
        assert_eq!(RunState::for_gate_outcome(GateOutcome::Warn), None);
        assert_eq!(
            RunState::for_gate_outcome(GateOutcome::Stop),
            Some(RunState::Failed)
        );
    }
}
