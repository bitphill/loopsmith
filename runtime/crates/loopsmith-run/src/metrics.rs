//! The numbers a run keeps about itself, and the alerts that watch them.
//!
//! Every run reports the same eight numbers in its outcome, whether or not any
//! alert is configured, so a scheduler or dashboard reading `RunOutcome` does
//! not have to scrape the ledger. `safety.alerts` puts thresholds on them;
//! each alert fires at most once per run, at the first iteration its metric
//! crosses the line.

use crate::context::Run;
use crate::running::Progress;
use loopsmith_core::Metric;
use loopsmith_gate::TargetVerdict;
use loopsmith_memory::{LedgerKind, Store};
use std::collections::BTreeMap;

/// What a run measured about itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunMetrics {
    pub iterations: u32,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub wall_clock_seconds: u64,
    pub failed_dispatches: u32,
    pub retries: u32,
    pub stale_iterations: u32,
    /// Blocking checks passing at the latest ruling, as a fraction. `None`
    /// before the first ruling, or when no check is blocking.
    pub validation_pass_rate: Option<f64>,
}

impl RunMetrics {
    pub fn get(&self, m: Metric) -> Option<f64> {
        Some(match m {
            Metric::Iterations => self.iterations as f64,
            Metric::TokensUsed => self.tokens_used as f64,
            Metric::CostUsd => self.cost_usd,
            Metric::WallClockSeconds => self.wall_clock_seconds as f64,
            Metric::FailedDispatches => self.failed_dispatches as f64,
            Metric::Retries => self.retries as f64,
            Metric::StaleIterations => self.stale_iterations as f64,
            Metric::ValidationPassRate => return self.validation_pass_rate,
        })
    }
}

/// An alert that fired.
#[derive(Debug, Clone, PartialEq)]
pub struct RaisedAlert {
    pub id: String,
    pub metric: Metric,
    pub value: f64,
    pub iteration: u32,
    pub message: String,
}

/// Blocking checks passing across every target, as a fraction.
pub(crate) fn pass_rate(verdicts: &BTreeMap<String, TargetVerdict>) -> Option<f64> {
    let (passed, total) = verdicts
        .values()
        .flat_map(|v| v.checks.iter().filter(|c| c.blocking))
        .fold((0usize, 0usize), |(p, t), c| (p + c.passed as usize, t + 1));
    (total > 0).then(|| passed as f64 / total as f64)
}

/// The metrics as they stand.
pub(crate) fn measure<S: Store>(run: &Run<S>, progress: &Progress) -> RunMetrics {
    RunMetrics {
        iterations: run.checkpoint.iteration,
        tokens_used: run.checkpoint.tokens_used,
        cost_usd: run.checkpoint.cost_usd,
        wall_clock_seconds: run.started.elapsed().as_secs(),
        failed_dispatches: progress.failed_dispatches,
        retries: progress.retries,
        stale_iterations: progress.stale_iterations,
        validation_pass_rate: progress.previous_verdicts.as_ref().and_then(pass_rate),
    }
}

/// Raise every configured alert whose metric has crossed its threshold and
/// that has not fired yet this run.
pub(crate) fn watch<S: Store>(run: &Run<S>, progress: &mut Progress, it: u32) {
    if run.cfg.safety.alerts.is_empty() {
        return;
    }
    let now = measure(run, progress);
    for alert in &run.cfg.safety.alerts {
        if progress.alerts.iter().any(|a| a.id == alert.id) {
            continue;
        }
        let Some(value) = now.get(alert.metric) else {
            continue;
        };
        if !alert.fires_at(value) {
            continue;
        }
        let message = alert.describe(value);
        run.rec.entry(
            it,
            LedgerKind::AlertRaised,
            format!("alert `{}`: {message}", alert.id),
            None,
        );
        progress.alerts.push(RaisedAlert {
            id: alert.id.clone(),
            metric: alert.metric,
            value,
            iteration: it,
            message,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_gate::CheckResult;

    fn verdict(checks: &[(bool, bool)]) -> TargetVerdict {
        TargetVerdict {
            target: "t".into(),
            satisfied: false,
            checks: checks
                .iter()
                .map(|(passed, blocking)| CheckResult {
                    name: "c".into(),
                    text: String::new(),
                    passed: *passed,
                    blocking: *blocking,
                    evidence: String::new(),
                })
                .collect(),
            passed: 0,
            failed: 0,
            total: checks.len(),
            reason: String::new(),
        }
    }

    #[test]
    fn the_pass_rate_counts_blocking_checks_only() {
        let mut v = BTreeMap::new();
        v.insert("a".into(), verdict(&[(true, true), (false, true), (false, false)]));
        v.insert("b".into(), verdict(&[(true, true), (true, true)]));
        assert_eq!(pass_rate(&v), Some(0.75));
    }

    #[test]
    fn no_blocking_check_means_no_rate_not_a_perfect_one() {
        let mut v = BTreeMap::new();
        v.insert("a".into(), verdict(&[(true, false)]));
        assert_eq!(pass_rate(&v), None);
    }
}
