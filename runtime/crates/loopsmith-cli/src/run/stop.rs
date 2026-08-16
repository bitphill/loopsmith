//! The stop-gate ladder.
//!
//! This is deliberately a pure function over a snapshot. The gates are the
//! mechanical answer to "may this run continue", and a mechanical answer must
//! not be reachable by anything a node said — so nothing here touches the
//! store, the providers, or the clock. It is handed numbers and returns a
//! verdict.
//!
//! It used to be eight inline `if` blocks in the middle of `execute()`, each
//! ending in the same two lines (`verdicts = current; break …`). Adding a gate
//! meant remembering that dance; forgetting it meant the borrow checker
//! complained about something unrelated three screens away.

use loopsmith_core::{LoopConfig, StopGates};
use loopsmith_gate::TargetVerdict;
use std::collections::BTreeMap;

/// Why the loop stopped. Every variant except `OverallSuccess` is an
/// escalation: the run ended without meeting the bar, and the reason is
/// recorded so the human sees what was tried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    OverallSuccess,
    IterationCap(u32),
    WallClock(u64),
    TokenBudget(u64),
    CostBudget(String),
    NoProgress(u32),
}

impl StopReason {
    pub fn is_success(&self) -> bool {
        matches!(self, StopReason::OverallSuccess)
    }

    pub fn describe(&self) -> String {
        match self {
            StopReason::OverallSuccess => "all overall success scenarios met".into(),
            StopReason::IterationCap(n) => format!("iteration cap reached ({n})"),
            StopReason::WallClock(s) => format!("wall-clock budget exhausted ({s}s)"),
            StopReason::TokenBudget(t) => format!("token budget exhausted ({t})"),
            StopReason::CostBudget(c) => format!("cost budget exhausted ({c})"),
            StopReason::NoProgress(n) => {
                format!("no measurable change for {n} iterations; stopping the line")
            }
        }
    }
}

/// Everything the ladder is allowed to look at.
pub struct StopInputs<'a> {
    pub cfg: &'a LoopConfig,
    pub gates: &'a StopGates,
    pub verdicts: &'a BTreeMap<String, TargetVerdict>,
    pub iteration: u32,
    pub stale_iterations: u32,
    pub elapsed_seconds: u64,
    pub tokens_used: u64,
    pub cost_usd: f64,
}

/// Ask whether the run may continue. `None` means keep going.
///
/// Order matters and is not arbitrary: success is checked first so a run that
/// met the bar on its last permitted iteration reports success rather than
/// "iteration cap". Everything after that is a budget, cheapest signal first.
pub fn should_stop(inp: &StopInputs) -> Option<StopReason> {
    let g = inp.gates;

    if g.stop_on_overall_success && loopsmith_gate::overall_success(inp.cfg, inp.verdicts) {
        return Some(StopReason::OverallSuccess);
    }
    if g.no_progress_iterations > 0 && inp.stale_iterations >= g.no_progress_iterations {
        return Some(StopReason::NoProgress(inp.stale_iterations));
    }
    if inp.iteration >= g.max_iterations {
        return Some(StopReason::IterationCap(g.max_iterations));
    }
    if let Some(limit) = g.max_wall_clock_seconds {
        if inp.elapsed_seconds >= limit {
            return Some(StopReason::WallClock(limit));
        }
    }
    if let Some(limit) = g.max_tokens {
        if inp.tokens_used >= limit {
            return Some(StopReason::TokenBudget(limit));
        }
    }
    if let Some(limit) = g.max_cost_usd {
        if inp.cost_usd >= limit {
            return Some(StopReason::CostBudget(format!("${limit:.2}")));
        }
    }
    None
}

/// A fingerprint of the current verdicts. If it does not change between
/// iterations, the loop is spinning rather than progressing.
pub fn progress_signature(verdicts: &BTreeMap<String, TargetVerdict>) -> String {
    let mut parts: Vec<String> = verdicts
        .iter()
        .map(|(k, v)| format!("{k}:{}:{}/{}", v.satisfied, v.passed, v.total))
        .collect();
    parts.sort();
    parts.join("|")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(overall_passes: bool) -> LoopConfig {
        let detector = if overall_passes { "true" } else { "false" };
        loopsmith_core::parse_str(
            &format!(
                r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
validations:
  - target: overall
    name: ov
    mode: objective
    statement: always
    detector: {{ type: script, command: "{detector}" }}
success:
  - target: overall
    name: done
    mode: objective
    statement: the overall validations pass
"#
            ),
            "test",
        )
        .unwrap()
    }

    fn verdicts(satisfied: bool) -> BTreeMap<String, TargetVerdict> {
        let mut m = BTreeMap::new();
        m.insert(
            "overall".to_string(),
            TargetVerdict {
                target: "overall".into(),
                satisfied,
                checks: vec![],
                passed: if satisfied { 1 } else { 0 },
                failed: if satisfied { 0 } else { 1 },
                total: 1,
                reason: "test".into(),
            },
        );
        m
    }

    fn inputs<'a>(
        cfg: &'a LoopConfig,
        v: &'a BTreeMap<String, TargetVerdict>,
    ) -> StopInputs<'a> {
        StopInputs {
            cfg,
            gates: &cfg.stop_gates,
            verdicts: v,
            iteration: 1,
            stale_iterations: 0,
            elapsed_seconds: 0,
            tokens_used: 0,
            cost_usd: 0.0,
        }
    }

    #[test]
    fn an_unfinished_run_within_every_budget_continues() {
        let cfg = cfg_with(false);
        let v = verdicts(false);
        assert_eq!(should_stop(&inputs(&cfg, &v)), None);
    }

    #[test]
    fn success_is_checked_before_the_iteration_cap() {
        // A run that meets the bar on its final permitted iteration must
        // report success, not "iteration cap reached".
        let cfg = cfg_with(true);
        let v = verdicts(true);
        let mut inp = inputs(&cfg, &v);
        inp.iteration = cfg.stop_gates.max_iterations;
        assert_eq!(should_stop(&inp), Some(StopReason::OverallSuccess));
    }

    #[test]
    fn the_iteration_cap_fires() {
        let cfg = cfg_with(false);
        let v = verdicts(false);
        let mut inp = inputs(&cfg, &v);
        inp.iteration = cfg.stop_gates.max_iterations;
        assert_eq!(
            should_stop(&inp),
            Some(StopReason::IterationCap(cfg.stop_gates.max_iterations))
        );
    }

    #[test]
    fn zero_disables_the_no_progress_gate() {
        let mut cfg = cfg_with(false);
        cfg.stop_gates.no_progress_iterations = 0;
        let v = verdicts(false);
        let mut inp = inputs(&cfg, &v);
        inp.gates = &cfg.stop_gates;
        inp.stale_iterations = 9_999;
        assert_eq!(should_stop(&inp), None, "0 must mean disabled, not instant");
    }

    #[test]
    fn each_budget_fires_on_its_own() {
        let base = cfg_with(false);
        let v = verdicts(false);

        let mut cfg = base.clone();
        cfg.stop_gates.max_wall_clock_seconds = Some(60);
        let mut inp = inputs(&cfg, &v);
        inp.gates = &cfg.stop_gates;
        inp.elapsed_seconds = 60;
        assert_eq!(should_stop(&inp), Some(StopReason::WallClock(60)));

        let mut cfg = base.clone();
        cfg.stop_gates.max_tokens = Some(100);
        let mut inp = inputs(&cfg, &v);
        inp.gates = &cfg.stop_gates;
        inp.tokens_used = 100;
        assert_eq!(should_stop(&inp), Some(StopReason::TokenBudget(100)));

        let mut cfg = base;
        cfg.stop_gates.max_cost_usd = Some(1.5);
        let mut inp = inputs(&cfg, &v);
        inp.gates = &cfg.stop_gates;
        inp.cost_usd = 1.5;
        assert_eq!(
            should_stop(&inp),
            Some(StopReason::CostBudget("$1.50".into()))
        );
    }

    #[test]
    fn a_signature_only_changes_when_a_verdict_does() {
        let a = progress_signature(&verdicts(false));
        let b = progress_signature(&verdicts(false));
        let c = progress_signature(&verdicts(true));
        assert_eq!(a, b, "identical verdicts must fingerprint identically");
        assert_ne!(a, c, "a changed verdict must change the fingerprint");
    }
}
