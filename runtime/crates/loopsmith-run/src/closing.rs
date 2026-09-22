//! Closing: the run's outcome, written down once.
//!
//! Whatever stopped the run — a stop gate, a halting rule, a failure the
//! recovery policy would not retry — ends here: the outcome state is entered,
//! the checkpoint is saved with the accounting that was current, the stop is
//! written to the ledger, and a success package is exported if and only if the
//! gate certified success.

use crate::context::Run;
use crate::export;
use crate::running::{Progress, Stopped};
use crate::state::RunState;
use crate::stop::StopReason;
use crate::RunOutcome;
use loopsmith_memory::{LedgerKind, Store};

/// The outcome state a stop reason sends the run to.
///
/// A budget ceiling pauses rather than fails: the run did nothing wrong, it
/// ran out of what it was given, and resuming with a larger budget is the
/// ordinary next step. A run with nothing moving is blocked, because resuming
/// it unchanged would stall the same way.
pub(crate) fn outcome_for(reason: &StopReason) -> RunState {
    match reason {
        StopReason::OverallSuccess => RunState::Succeeded,
        StopReason::IterationCap(_)
        | StopReason::WallClock(_)
        | StopReason::TokenBudget(_)
        | StopReason::CostBudget(_) => RunState::Paused,
        StopReason::NoProgress(_) => RunState::Blocked,
    }
}

pub(crate) fn close<S: Store>(
    mut run: Run<S>,
    progress: Progress,
    stopped: Stopped,
) -> Result<RunOutcome, String> {
    let Stopped { reason, verdicts } = stopped;
    let outcome = outcome_for(&reason);

    progress.store_into(&mut run.checkpoint, &verdicts);
    run.enter(outcome, reason.describe())?;

    let it = run.checkpoint.iteration;
    run.rec.entry(
        it,
        if reason.is_success() {
            LedgerKind::RunFinished
        } else {
            LedgerKind::StopGateTriggered
        },
        reason.describe(),
        None,
    );

    // The export is gated on the gate. `reason.is_success()` is true only for
    // `StopReason::OverallSuccess`, which only `should_stop` produces, and only
    // from `loopsmith_gate::overall_success`.
    let export_path = if reason.is_success() {
        match export::export_success(
            run.cfg,
            run.root(),
            &verdicts,
            &run.store.summaries(&run.opts.run_id).unwrap_or_default(),
            it,
            &run.opts.config_file,
        ) {
            Ok(p) => {
                run.rec.entry(
                    it,
                    LedgerKind::RunFinished,
                    format!("reusable success package written to {}", p.display()),
                    None,
                );
                Some(p)
            }
            Err(e) => {
                run.rec.entry(
                    it,
                    LedgerKind::NodeFailed,
                    format!("could not write the success package: {e}"),
                    None,
                );
                None
            }
        }
    } else {
        None
    };

    run.enter(RunState::Closed, "")?;
    let _ = run.store.flush();

    Ok(RunOutcome {
        run_id: run.opts.run_id.clone(),
        iterations: it,
        state: outcome,
        stop: reason,
        verdicts,
        tokens_used: run.checkpoint.tokens_used,
        tokens_estimated: progress.any_estimated,
        cost_usd: run.checkpoint.cost_usd,
        proposals: progress.proposals_written,
        log_path: run.rec.log.path().map(|p| p.to_path_buf()),
        export_path,
    })
}
