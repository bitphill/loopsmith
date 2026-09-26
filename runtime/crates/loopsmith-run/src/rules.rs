//! Entry, approval, and rollback rules, applied.
//!
//! The gate decides each rule; this module writes the ruling down and turns a
//! failed rule into what its `on_fail` asks for. The three kinds are checked at
//! three different moments — entry before planning, approval before the first
//! dispatch, rollback after every gate ruling — but a failed rule means the
//! same thing wherever it is.

use crate::context::Run;
use crate::state::RunState;
use crate::state::Halt;
use loopsmith_core::{GateKind, GateOutcome};
use loopsmith_gate::Evidence;
use loopsmith_memory::{LedgerKind, Store};

/// Evaluate every rule of `kind`, record each ruling, and return the first
/// halt a failed rule calls for.
///
/// A `warn` rule is recorded and does not halt. A `rollback` outcome on an
/// entry or approval rule has nothing to undo — nothing has run yet — so it
/// fails the run instead, and says so.
pub(crate) fn apply<S: Store>(
    run: &mut Run<S>,
    kind: GateKind,
    ev: &Evidence,
    it: u32,
) -> Option<Halt> {
    let verdicts = loopsmith_gate::check_rules(run.cfg, kind, ev);
    let mut halt = None;
    for v in verdicts {
        run.rec.entry(
            it,
            LedgerKind::RuleEvaluated,
            format!(
                "{} rule `{}` {}: {}",
                kind.as_str(),
                v.id,
                if v.passed { "held" } else { "failed" },
                v.evidence
            ),
            None,
        );
        if v.passed || halt.is_some() {
            continue;
        }
        let state = match (kind, RunState::for_gate_outcome(v.on_fail)) {
            (_, None) => continue,
            (GateKind::Entry | GateKind::Approval, Some(RunState::RolledBack)) => {
                run.rec.entry(
                    it,
                    LedgerKind::RuleEvaluated,
                    format!(
                        "{} rule `{}` asks for a rollback, but nothing has run to roll back; \
                         failing the run instead",
                        kind.as_str(),
                        v.id
                    ),
                    None,
                );
                RunState::Failed
            }
            (_, Some(state)) => state,
        };
        halt = Some(Halt {
            state,
            why: format!(
                "{} rule `{}` failed ({}): {}",
                kind.as_str(),
                v.id,
                outcome_name(v.on_fail),
                v.statement
            ),
        });
    }
    halt
}

fn outcome_name(o: GateOutcome) -> &'static str {
    match o {
        GateOutcome::Stop => "stop",
        GateOutcome::Escalate => "escalate",
        GateOutcome::Pause => "pause",
        GateOutcome::Rollback => "rollback",
        GateOutcome::Warn => "warn",
    }
}
