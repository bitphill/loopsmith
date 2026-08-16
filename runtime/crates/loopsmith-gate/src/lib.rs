//! The verification gate.
//!
//! This crate exists to make one property true: **a model cannot talk its way
//! to "done"**. The gate is plain Rust, it evaluates the config's validations
//! against collected evidence, and it is the only place a [`GoalState`] with
//! `satisfied: true` is constructed.
//!
//! Two consequences worth stating explicitly:
//!
//! - **It can revoke.** Re-running the gate on fresh evidence may flip a
//!   previously satisfied target back to unsatisfied. A system that can only
//!   promote is a burndown chart with extra steps.
//! - **A judge must be independent.** A `Judge` detector whose verdict came
//!   from the same provider that produced the work is refused, not downgraded.
//!   Sharing a provider between builder and judge means sharing blind spots,
//!   which is the failure the whole architecture exists to avoid.

use loopsmith_core::{
    CompareOp, Detector, LoopConfig, Mode, SuccessScenario, Validation, OVERALL,
};
use loopsmith_memory::{now_ms, GoalState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("detector could not run: {0}")]
    Detector(String),
    #[error("invalid regex in validation `{name}`: {source}")]
    Regex {
        name: String,
        #[source]
        source: regex::Error,
    },
}

/// A model's verdict on a subjective validation. Carries the provenance the
/// gate needs to decide whether the verdict is independent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Judgment {
    /// Which validation this answers.
    pub validation: String,
    /// Provider that produced the judgment.
    pub provider_id: String,
    /// Provider that produced the work being judged.
    pub builder_provider_id: String,
    pub passed: bool,
    #[serde(default)]
    pub score: Option<f64>,
    /// The external standard the judge claims to have applied.
    #[serde(default)]
    pub standard: String,
    #[serde(default)]
    pub evidence: String,
}

/// Everything the gate is allowed to look at. Nothing else is in scope —
/// notably, the builder's own claim that it finished is not evidence.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// Named artifacts (file contents, transcripts) available to regex checks.
    pub artifacts: BTreeMap<String, String>,
    /// Numeric metrics available to threshold checks.
    pub metrics: BTreeMap<String, f64>,
    /// Model verdicts, one or more per subjective validation.
    pub judgments: Vec<Judgment>,
    /// Working directory for script and file detectors.
    pub workdir: PathBuf,
}

impl Evidence {
    pub fn new(workdir: impl AsRef<Path>) -> Self {
        Self {
            workdir: workdir.as_ref().to_path_buf(),
            ..Default::default()
        }
    }
    pub fn with_metric(mut self, k: &str, v: f64) -> Self {
        self.metrics.insert(k.into(), v);
        self
    }
    pub fn with_artifact(mut self, k: &str, v: &str) -> Self {
        self.artifacts.insert(k.into(), v.into());
        self
    }
    pub fn with_judgment(mut self, j: Judgment) -> Self {
        self.judgments.push(j);
        self
    }
}

/// Result of one validation. Field names mirror the grading schema used by
/// the existing eval viewer (`text` / `passed` / `evidence`) so reports can be
/// read by tooling that already exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub text: String,
    pub passed: bool,
    pub blocking: bool,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetVerdict {
    pub target: String,
    pub satisfied: bool,
    pub checks: Vec<CheckResult>,
    pub passed: usize,
    pub failed: usize,
    pub total: usize,
    pub reason: String,
}

impl TargetVerdict {
    pub fn blocking_pass_rate(&self) -> f64 {
        let blocking: Vec<&CheckResult> = self.checks.iter().filter(|c| c.blocking).collect();
        if blocking.is_empty() {
            return 0.0;
        }
        blocking.iter().filter(|c| c.passed).count() as f64 / blocking.len() as f64
    }

    /// Convert to the persisted form. This is the only constructor of a
    /// satisfied [`GoalState`] in the workspace.
    pub fn to_goal_state(&self, iteration: u32) -> GoalState {
        GoalState {
            target: self.target.clone(),
            satisfied: self.satisfied,
            passed: self.passed,
            failed: self.failed,
            total: self.total,
            reason: self.reason.clone(),
            iteration,
            updated_ms: now_ms(),
        }
    }
}

/// Evaluate every validation aimed at `target`.
pub fn evaluate(cfg: &LoopConfig, target: &str, ev: &Evidence) -> TargetVerdict {
    let vals: Vec<&Validation> = cfg.validations.iter().filter(|v| v.target == target).collect();

    let mut checks = Vec::new();
    for v in &vals {
        let (passed, evidence) = match run_detector(cfg, v, ev) {
            Ok(pair) => pair,
            Err(e) => (false, format!("detector error: {e}")),
        };
        checks.push(CheckResult {
            name: v.name.clone(),
            text: v.statement.clone(),
            passed,
            blocking: v.blocking,
            evidence,
        });
    }

    let total = checks.len();
    let passed = checks.iter().filter(|c| c.passed).count();
    let failed = total - passed;

    let blocking_failures: Vec<&CheckResult> =
        checks.iter().filter(|c| c.blocking && !c.passed).collect();

    // A target with no blocking validations can never be satisfied. This is
    // deliberate: silence is not success.
    let has_blocking = checks.iter().any(|c| c.blocking);

    let (satisfied, reason) = if !has_blocking {
        (
            false,
            format!("no blocking validation targets `{target}`; nothing to satisfy"),
        )
    } else if blocking_failures.is_empty() {
        (true, format!("all {} blocking checks passed", checks.iter().filter(|c| c.blocking).count()))
    } else {
        (
            false,
            format!(
                "{} blocking check(s) failed: {}",
                blocking_failures.len(),
                blocking_failures
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    };

    TargetVerdict {
        target: target.to_string(),
        satisfied,
        checks,
        passed,
        failed,
        total,
        reason,
    }
}

/// Evaluate every goal plus `overall`.
pub fn evaluate_all(cfg: &LoopConfig, ev: &Evidence) -> BTreeMap<String, TargetVerdict> {
    let mut out = BTreeMap::new();
    for g in &cfg.goals {
        out.insert(g.name.clone(), evaluate(cfg, &g.name, ev));
    }
    out.insert(OVERALL.to_string(), evaluate(cfg, OVERALL, ev));
    out
}

/// Does the success scenario hold, given the verdict for its target?
pub fn success_met(s: &SuccessScenario, verdict: &TargetVerdict) -> bool {
    match s.mode {
        Mode::Percentage => {
            let t = s.threshold.unwrap_or(1.0);
            verdict.blocking_pass_rate() >= t
        }
        Mode::Objective | Mode::Subjective => verdict.satisfied,
    }
}

/// Are all `overall` success scenarios met? Used by the stop gates.
pub fn overall_success(cfg: &LoopConfig, verdicts: &BTreeMap<String, TargetVerdict>) -> bool {
    let scenarios: Vec<&SuccessScenario> = cfg
        .success
        .iter()
        .filter(|s| s.target == OVERALL)
        .collect();
    if scenarios.is_empty() {
        // Fall back to the overall verdict when no scenario is declared.
        return verdicts.get(OVERALL).map(|v| v.satisfied).unwrap_or(false);
    }
    scenarios.iter().all(|s| {
        verdicts
            .get(OVERALL)
            .map(|v| success_met(s, v))
            .unwrap_or(false)
    })
}

fn run_detector(
    cfg: &LoopConfig,
    v: &Validation,
    ev: &Evidence,
) -> Result<(bool, String), GateError> {
    match &v.detector {
        Detector::Script {
            command,
            args,
            expect_exit,
        } => {
            let want = expect_exit.unwrap_or(0);
            let out = std::process::Command::new(command)
                .args(args)
                .current_dir(&ev.workdir)
                .output()
                .map_err(|e| GateError::Detector(format!("{command}: {e}")))?;
            let code = out.status.code().unwrap_or(-1);
            let tail = String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("")
                .to_string();
            Ok((
                code == want,
                format!("`{command}` exited {code} (expected {want}){}", if tail.is_empty() { String::new() } else { format!("; {tail}") }),
            ))
        }

        Detector::FileExists { path, non_empty } => {
            let p = ev.workdir.join(path);
            let meta = std::fs::metadata(&p).ok();
            match meta {
                None => Ok((false, format!("{} does not exist", p.display()))),
                Some(m) if *non_empty && m.len() == 0 => {
                    Ok((false, format!("{} exists but is empty", p.display())))
                }
                Some(m) => Ok((true, format!("{} exists ({} bytes)", p.display(), m.len()))),
            }
        }

        Detector::RegexMatch { artifact, pattern } => {
            let re = regex::Regex::new(pattern).map_err(|source| GateError::Regex {
                name: v.name.clone(),
                source,
            })?;
            match ev.artifacts.get(artifact) {
                None => Ok((false, format!("artifact `{artifact}` was not collected"))),
                Some(text) => {
                    let hit = re.is_match(text);
                    Ok((
                        hit,
                        format!(
                            "pattern {} `{artifact}`",
                            if hit { "matched" } else { "did not match" }
                        ),
                    ))
                }
            }
        }

        Detector::Threshold { metric, op, value } => match ev.metrics.get(metric) {
            None => Ok((false, format!("metric `{metric}` was not reported"))),
            Some(actual) => {
                let ok = op.apply(*actual, *value);
                Ok((
                    ok,
                    format!("{metric} = {actual}, required {} {value}", op_str(*op)),
                ))
            }
        },

        Detector::Judge {
            standard,
            min_score,
        } => {
            let judgments: Vec<&Judgment> = ev
                .judgments
                .iter()
                .filter(|j| j.validation == v.name)
                .collect();
            if judgments.is_empty() {
                return Ok((false, format!("no judgment recorded for `{}`", v.name)));
            }
            // Independence check first: a judgment from the builder's own
            // provider is refused outright rather than counted.
            if cfg.providers.enforce_judge_independence {
                if let Some(bad) = judgments
                    .iter()
                    .find(|j| j.provider_id == j.builder_provider_id)
                {
                    return Ok((
                        false,
                        format!(
                            "judgment refused: judge and builder both ran on `{}`; a shared provider shares its blind spots",
                            bad.provider_id
                        ),
                    ));
                }
            }
            let independent: Vec<&&Judgment> = judgments
                .iter()
                .filter(|j| j.provider_id != j.builder_provider_id)
                .collect();
            let pool = if independent.is_empty() {
                judgments.iter().collect::<Vec<_>>()
            } else {
                independent
            };

            if let Some(min) = min_score {
                let scored: Vec<f64> = pool.iter().filter_map(|j| j.score).collect();
                if scored.is_empty() {
                    return Ok((
                        false,
                        format!("`{standard}` requires a score of at least {min}, none reported"),
                    ));
                }
                let mean = scored.iter().sum::<f64>() / scored.len() as f64;
                return Ok((
                    mean >= *min,
                    format!("mean score {mean:.2} against `{standard}` (min {min})"),
                ));
            }

            let all_pass = pool.iter().all(|j| j.passed);
            Ok((
                all_pass,
                format!(
                    "{}/{} independent judgments passed against `{standard}`",
                    pool.iter().filter(|j| j.passed).count(),
                    pool.len()
                ),
            ))
        }
    }
}

fn op_str(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Gt => ">",
        CompareOp::Gte => ">=",
        CompareOp::Lt => "<",
        CompareOp::Lte => "<=",
        CompareOp::Eq => "==",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(validations: &str) -> LoopConfig {
        let text = format!(
            r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
pre_execution:
  - step: did it by hand
    done: true
providers:
  providers:
    - id: builder-p
      kind: ollama
      command: echo
    - id: judge-p
      kind: openai
      command: echo
validations:
{validations}
"#
        );
        loopsmith_core::parse_str(&text, "test").expect("config parses")
    }

    #[test]
    fn a_passing_script_satisfies_the_target() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: tests
    mode: objective
    statement: the suite passes
    detector: { type: script, command: "true" }"#,
        );
        let v = evaluate(&cfg, "g1", &Evidence::new("."));
        assert!(v.satisfied, "{}", v.reason);
        assert_eq!(v.passed, 1);
    }

    #[test]
    fn a_failing_script_holds_the_gate_shut() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: tests
    mode: objective
    statement: the suite passes
    detector: { type: script, command: "false" }"#,
        );
        let v = evaluate(&cfg, "g1", &Evidence::new("."));
        assert!(!v.satisfied);
        assert!(v.reason.contains("blocking check(s) failed"));
    }

    #[test]
    fn a_target_with_no_blocking_validation_is_never_satisfied() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: advisory
    mode: objective
    statement: nice to have
    blocking: false
    detector: { type: script, command: "true" }"#,
        );
        let v = evaluate(&cfg, "g1", &Evidence::new("."));
        assert!(!v.satisfied);
        assert!(v.reason.contains("nothing to satisfy"));
    }

    #[test]
    fn non_blocking_failures_do_not_hold_the_gate() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: must
    mode: objective
    statement: required
    detector: { type: script, command: "true" }
  - target: g1
    name: advisory
    mode: objective
    statement: optional
    blocking: false
    detector: { type: script, command: "false" }"#,
        );
        let v = evaluate(&cfg, "g1", &Evidence::new("."));
        assert!(v.satisfied, "{}", v.reason);
        assert_eq!(v.failed, 1);
    }

    #[test]
    fn threshold_detector_compares_reported_metrics() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: coverage
    mode: percentage
    statement: coverage at least 80 percent
    detector: { type: threshold, metric: coverage, op: gte, value: 0.8 }"#,
        );
        let hi = evaluate(&cfg, "g1", &Evidence::new(".").with_metric("coverage", 0.85));
        assert!(hi.satisfied);
        let lo = evaluate(&cfg, "g1", &Evidence::new(".").with_metric("coverage", 0.5));
        assert!(!lo.satisfied);
    }

    #[test]
    fn a_missing_metric_fails_rather_than_passes_by_default() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: coverage
    mode: percentage
    statement: coverage reported
    detector: { type: threshold, metric: coverage, op: gte, value: 0.8 }"#,
        );
        let v = evaluate(&cfg, "g1", &Evidence::new("."));
        assert!(!v.satisfied);
        assert!(v.checks[0].evidence.contains("not reported"));
    }

    #[test]
    fn judge_on_the_builders_provider_is_refused() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: prose
    mode: subjective
    statement: reads well
    detector: { type: judge, standard: "house style guide" }"#,
        );
        let ev = Evidence::new(".").with_judgment(Judgment {
            validation: "prose".into(),
            provider_id: "builder-p".into(),
            builder_provider_id: "builder-p".into(),
            passed: true,
            score: None,
            standard: "house style guide".into(),
            evidence: "looks great to me".into(),
        });
        let v = evaluate(&cfg, "g1", &ev);
        assert!(!v.satisfied, "self-judgment must not satisfy the gate");
        assert!(v.checks[0].evidence.contains("shares its blind spots"));
    }

    #[test]
    fn an_independent_judge_can_satisfy_a_subjective_check() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: prose
    mode: subjective
    statement: reads well
    detector: { type: judge, standard: "house style guide" }"#,
        );
        let ev = Evidence::new(".").with_judgment(Judgment {
            validation: "prose".into(),
            provider_id: "judge-p".into(),
            builder_provider_id: "builder-p".into(),
            passed: true,
            score: None,
            standard: "house style guide".into(),
            evidence: "checked against the guide".into(),
        });
        let v = evaluate(&cfg, "g1", &ev);
        assert!(v.satisfied, "{}", v.reason);
    }

    #[test]
    fn a_missing_judgment_fails_closed() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: prose
    mode: subjective
    statement: reads well
    detector: { type: judge, standard: "house style guide" }"#,
        );
        let v = evaluate(&cfg, "g1", &Evidence::new("."));
        assert!(!v.satisfied);
        assert!(v.checks[0].evidence.contains("no judgment recorded"));
    }

    #[test]
    fn the_gate_can_take_done_back() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: artifact
    mode: objective
    statement: the report exists
    detector: { type: file_exists, path: report.md, non_empty: true }"#,
        );
        let dir = std::env::temp_dir().join(format!("loopsmith-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("report.md");
        std::fs::write(&file, b"content").unwrap();

        let before = evaluate(&cfg, "g1", &Evidence::new(&dir));
        assert!(before.satisfied);

        // The artifact disappears; re-evaluating must revoke, not remember.
        std::fs::remove_file(&file).unwrap();
        let after = evaluate(&cfg, "g1", &Evidence::new(&dir));
        assert!(!after.satisfied, "gate must be able to revoke");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn percentage_success_uses_blocking_pass_rate() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: a
    mode: objective
    statement: one
    detector: { type: script, command: "true" }
  - target: g1
    name: b
    mode: objective
    statement: two
    detector: { type: script, command: "false" }"#,
        );
        let v = evaluate(&cfg, "g1", &Evidence::new("."));
        assert!((v.blocking_pass_rate() - 0.5).abs() < 1e-9);

        let half = SuccessScenario {
            target: "g1".into(),
            name: "half".into(),
            mode: Mode::Percentage,
            statement: "half is fine".into(),
            threshold: Some(0.5),
        };
        assert!(success_met(&half, &v));

        let all = SuccessScenario {
            threshold: Some(1.0),
            ..half.clone()
        };
        assert!(!success_met(&all, &v));
    }

    #[test]
    fn only_the_gate_builds_a_satisfied_goal_state() {
        let cfg = cfg_with(
            r#"  - target: g1
    name: tests
    mode: objective
    statement: passes
    detector: { type: script, command: "true" }"#,
        );
        let v = evaluate(&cfg, "g1", &Evidence::new("."));
        let st = v.to_goal_state(3);
        assert!(st.satisfied);
        assert_eq!(st.iteration, 3);
        assert_eq!(st.target, "g1");
    }
}
