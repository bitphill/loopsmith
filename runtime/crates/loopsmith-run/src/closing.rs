//! Closing: the run's outcome, written down once.
//!
//! Whatever stopped the run — a stop gate, a halting rule, a failure the
//! recovery policy would not retry — ends here: the outcome state is entered,
//! the checkpoint is saved with the accounting that was current, the stop is
//! written to the ledger, and a success package is exported if and only if the
//! gate certified success.

use crate::context::Run;
use crate::export;
use crate::recovering;
use crate::running::{Progress, Stopped};
use crate::state::RunState;
use crate::stop::StopReason;
use crate::metrics::RunMetrics;
use crate::RunOutcome;
use loopsmith_core::{Baseline, FailureClass, Recovery};
use loopsmith_gate::BaselineVerdict;
use loopsmith_memory::{LedgerKind, Store};

/// The outcome state a stop reason sends the run to.
///
/// A budget or iteration ceiling is resource exhaustion, answered by
/// `safety.recovery.resource_exhaustion` — `pause` by default, because the run
/// did nothing wrong and a bigger budget is the ordinary next step. A run with
/// nothing moving is blocked, because resuming it unchanged would stall the
/// same way. Either becomes `escalated` when the run has questions open for a
/// human: that, not the ceiling, is what it is actually waiting on.
pub(crate) fn outcome_for(
    reason: &StopReason,
    policy: &Recovery,
    open_escalations: bool,
) -> RunState {
    let state = match reason {
        StopReason::OverallSuccess => return RunState::Succeeded,
        StopReason::Halted { state, .. } => return *state,
        StopReason::IterationCap(_)
        | StopReason::WallClock(_)
        | StopReason::TokenBudget(_)
        | StopReason::CostBudget(_) => {
            recovering::run_outcome(policy, FailureClass::ResourceExhaustion, RunState::Paused)
        }
        StopReason::NoProgress(_) => RunState::Blocked,
    };
    if open_escalations && matches!(state, RunState::Paused | RunState::Blocked) {
        RunState::Escalated
    } else {
        state
    }
}

pub(crate) fn close<S: Store>(
    mut run: Run<S>,
    mut progress: Progress,
    stopped: Stopped,
) -> Result<RunOutcome, String> {
    let Stopped { reason, verdicts } = stopped;
    let outcome = outcome_for(
        &reason,
        &run.cfg.safety.recovery,
        !run.checkpoint.escalations.is_empty(),
    );

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

    // Wall clock and spend can cross an alert's line in the last moments of a
    // run, so the alerts get a final look at the numbers the outcome reports.
    crate::metrics::watch(&run, &mut progress, it);
    let metrics = crate::metrics::measure(&run, &progress);
    let baseline = judge_against_baseline(&run, &metrics, outcome, it);

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
        metrics,
        alerts: progress.alerts,
        baseline,
    })
}

/// The regression gate, applied to what this run measured.
///
/// Cost, latency, and iterations are only comparable between runs that
/// succeeded — "mean cost of a successful run" is what the baseline holds — so
/// a run that did not succeed is measured on completion and pass rate alone.
fn judge_against_baseline<S: Store>(
    run: &Run<S>,
    m: &RunMetrics,
    outcome: RunState,
    it: u32,
) -> BaselineVerdict {
    let succeeded = outcome == RunState::Succeeded;
    let measured = Baseline {
        completion_rate: Some(if succeeded { 1.0 } else { 0.0 }),
        validation_pass_rate: m.validation_pass_rate,
        cost_usd: succeeded.then_some(m.cost_usd),
        latency_seconds: succeeded.then_some(m.wall_clock_seconds as f64),
        iterations_to_success: succeeded.then_some(m.iterations as f64),
        measured_at: None,
    };
    let verdict = loopsmith_gate::compare_to_baseline(run.cfg, &measured);
    let line = match &verdict {
        BaselineVerdict::Off => return verdict,
        BaselineVerdict::NoBaseline => {
            "no evolution baseline is frozen, so this run cannot show an improvement — \
             proposals are recorded, not adoptable"
                .to_string()
        }
        BaselineVerdict::Held => "held the evolution baseline on every metric it names".into(),
        BaselineVerdict::Regressed(r) => format!("regressed against the evolution baseline: {}", r.join("; ")),
    };
    run.rec.entry(it, LedgerKind::GateEvaluated, line, None);
    verdict
}
