//! A–H validation.
//!
//! The rules here are the corpus rules made mechanical. The most important one
//! is that every goal carries at least one blocking validation: a goal you
//! cannot check is a goal the loop can never honestly finish.

use crate::config::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    /// Dotted path into the config, e.g. `goals[2].name`.
    pub field: String,
    pub message: String,
}

impl Issue {
    fn err(field: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            severity: Severity::Error,
            field: field.into(),
            message: message.into(),
        }
    }
    fn warn(field: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            severity: Severity::Warning,
            field: field.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }
    pub fn errors(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error)
    }
    pub fn warnings(&self) -> impl Iterator<Item = &Issue> {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
    }
    pub fn render(&self) -> String {
        let mut out = String::new();
        for i in &self.issues {
            let tag = match i.severity {
                Severity::Error => "error",
                Severity::Warning => "warn ",
            };
            out.push_str(&format!("  {tag}  {}: {}\n", i.field, i.message));
        }
        out
    }
}

pub fn validate(cfg: &LoopConfig) -> ValidationReport {
    let mut r = ValidationReport::default();

    if cfg.name.trim().is_empty() {
        r.issues.push(Issue::err("name", "must not be empty"));
    }
    if cfg.goals.is_empty() {
        r.issues
            .push(Issue::err("goals", "a loop needs at least one goal"));
    }

    let goal_names: BTreeSet<&str> = cfg.goals.iter().map(|g| g.name.as_str()).collect();
    check_goals(cfg, &goal_names, &mut r);
    check_pre_execution(cfg, &mut r);
    check_validations(cfg, &goal_names, &mut r);
    check_success(cfg, &goal_names, &mut r);
    check_stop_gates(cfg, &mut r);
    check_graph(cfg, &goal_names, &mut r);
    check_providers(cfg, &mut r);
    r
}

fn check_goals(cfg: &LoopConfig, names: &BTreeSet<&str>, r: &mut ValidationReport) {
    let mut seen = BTreeSet::new();
    for (i, g) in cfg.goals.iter().enumerate() {
        let f = format!("goals[{i}]");
        if g.name.trim().is_empty() {
            r.issues.push(Issue::err(format!("{f}.name"), "must not be empty"));
        }
        if g.name == OVERALL {
            r.issues.push(Issue::err(
                format!("{f}.name"),
                format!("`{OVERALL}` is reserved for whole-loop targets"),
            ));
        }
        if !seen.insert(g.name.as_str()) {
            r.issues.push(Issue::err(
                format!("{f}.name"),
                format!("duplicate goal name `{}`", g.name),
            ));
        }
        if g.description.trim().len() < 12 {
            r.issues.push(Issue::warn(
                format!("{f}.description"),
                "very short; a vague goal produces a vague verdict",
            ));
        }
        for d in &g.depends_on {
            if !names.contains(d.as_str()) {
                r.issues.push(Issue::err(
                    format!("{f}.depends_on"),
                    format!("unknown goal `{d}`"),
                ));
            }
        }
        if g.depends_on.iter().any(|d| d == &g.name) {
            r.issues
                .push(Issue::err(format!("{f}.depends_on"), "goal depends on itself"));
        }
    }
}

fn check_pre_execution(cfg: &LoopConfig, r: &mut ValidationReport) {
    if cfg.pre_execution.is_empty() {
        r.issues.push(Issue::warn(
            "pre_execution",
            "empty. The corpus rule is to do the task manually first — the manual runs are the spec",
        ));
        return;
    }
    let undone: Vec<&str> = cfg
        .pre_execution
        .iter()
        .filter(|w| !w.done)
        .map(|w| w.step.as_str())
        .collect();
    if !undone.is_empty() {
        r.issues.push(Issue::err(
            "pre_execution",
            format!(
                "{} step(s) not marked done: {}. Automating before understanding produces fast, confident garbage",
                undone.len(),
                undone.join("; ")
            ),
        ));
    }
}

fn check_validations(cfg: &LoopConfig, names: &BTreeSet<&str>, r: &mut ValidationReport) {
    let mut covered: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, v) in cfg.validations.iter().enumerate() {
        let f = format!("validations[{i}]");
        if v.target != OVERALL && !names.contains(v.target.as_str()) {
            r.issues.push(Issue::err(
                format!("{f}.target"),
                format!("unknown target `{}` (expected a goal name or `{OVERALL}`)", v.target),
            ));
        }
        if v.statement.trim().is_empty() {
            r.issues
                .push(Issue::err(format!("{f}.statement"), "must not be empty"));
        }
        match &v.detector {
            Detector::Script { command, .. } if command.trim().is_empty() => {
                r.issues
                    .push(Issue::err(format!("{f}.detector.command"), "must not be empty"));
            }
            Detector::Threshold { metric, .. } if metric.trim().is_empty() => {
                r.issues
                    .push(Issue::err(format!("{f}.detector.metric"), "must not be empty"));
            }
            Detector::Judge { standard, .. } => {
                if standard.trim().is_empty() {
                    r.issues.push(Issue::err(
                        format!("{f}.detector.standard"),
                        "name the external standard the judge checks against; an unnamed standard is an opinion",
                    ));
                }
                if v.blocking && v.mode == Mode::Objective {
                    r.issues.push(Issue::warn(
                        f.clone(),
                        "objective mode with a model judge — prefer a script detector so the verdict is not a model's opinion",
                    ));
                }
            }
            _ => {}
        }
        if v.blocking {
            *covered.entry(v.target.as_str()).or_insert(0) += 1;
        }
    }

    for g in &cfg.goals {
        if covered.get(g.name.as_str()).copied().unwrap_or(0) == 0 {
            r.issues.push(Issue::err(
                format!("validations[target={}]", g.name),
                "goal has no blocking validation; it could never be honestly satisfied",
            ));
        }
    }
    if covered.get(OVERALL).copied().unwrap_or(0) == 0 {
        r.issues.push(Issue::warn(
            format!("validations[target={OVERALL}]"),
            "no overall validation; the loop can only finish per-goal",
        ));
    }
}

fn check_success(cfg: &LoopConfig, names: &BTreeSet<&str>, r: &mut ValidationReport) {
    for (i, s) in cfg.success.iter().enumerate() {
        let f = format!("success[{i}]");
        if s.target != OVERALL && !names.contains(s.target.as_str()) {
            r.issues.push(Issue::err(
                format!("{f}.target"),
                format!("unknown target `{}`", s.target),
            ));
        }
        match (s.mode, s.threshold) {
            (Mode::Percentage, None) => r.issues.push(Issue::err(
                format!("{f}.threshold"),
                "percentage mode requires a threshold between 0.0 and 1.0",
            )),
            (Mode::Percentage, Some(t)) if !(0.0..=1.0).contains(&t) => r.issues.push(Issue::err(
                format!("{f}.threshold"),
                format!("{t} is outside 0.0..=1.0"),
            )),
            _ => {}
        }
    }
}

fn check_stop_gates(cfg: &LoopConfig, r: &mut ValidationReport) {
    let g = &cfg.stop_gates;
    if g.max_iterations == 0 {
        r.issues
            .push(Issue::err("stop_gates.max_iterations", "must be at least 1"));
    }
    if g.max_iterations > 100 {
        r.issues.push(Issue::warn(
            "stop_gates.max_iterations",
            "very high; a loop that cannot converge in 100 iterations usually has a miscalibrated verifier",
        ));
    }
    if g.no_progress_iterations == 0 {
        r.issues.push(Issue::warn(
            "stop_gates.no_progress_iterations",
            "disabled; the loop can spin without changing anything",
        ));
    }
    if g.max_tokens.is_none() && g.max_cost_usd.is_none() && g.max_wall_clock_seconds.is_none() {
        r.issues.push(Issue::warn(
            "stop_gates",
            "no budget ceiling of any kind; an unsolvable task will bill until someone notices",
        ));
    }
}

fn check_graph(cfg: &LoopConfig, goal_names: &BTreeSet<&str>, r: &mut ValidationReport) {
    let ids: BTreeSet<&str> = cfg.graph.nodes.iter().map(|n| n.id.as_str()).collect();
    if cfg.graph.nodes.is_empty() {
        r.issues.push(Issue::warn(
            "graph.nodes",
            "no nodes; the loop will run a single implicit builder per goal",
        ));
        return;
    }
    let mut seen = BTreeSet::new();
    let mut has_judge = false;
    for (i, n) in cfg.graph.nodes.iter().enumerate() {
        let f = format!("graph.nodes[{i}]");
        if !seen.insert(n.id.as_str()) {
            r.issues
                .push(Issue::err(format!("{f}.id"), format!("duplicate node id `{}`", n.id)));
        }
        if n.instruction.trim().len() < 16 {
            r.issues.push(Issue::warn(
                format!("{f}.instruction"),
                "thin instruction; vague roles produce whatever the model felt like",
            ));
        }
        if n.weight <= 0.0 {
            r.issues
                .push(Issue::err(format!("{f}.weight"), "must be greater than zero"));
        }
        for d in &n.depends_on {
            if !ids.contains(d.as_str()) {
                r.issues
                    .push(Issue::err(format!("{f}.depends_on"), format!("unknown node `{d}`")));
            }
            if d == &n.id {
                r.issues
                    .push(Issue::err(format!("{f}.depends_on"), "node depends on itself"));
            }
        }
        for g in &n.goals {
            if !goal_names.contains(g.as_str()) {
                r.issues
                    .push(Issue::err(format!("{f}.goals"), format!("unknown goal `{g}`")));
            }
        }
        if let Some(p) = &n.provider {
            if cfg.provider(p).is_none() {
                r.issues.push(Issue::err(
                    format!("{f}.provider"),
                    format!("unknown provider `{p}`"),
                ));
            }
        }
        if n.role == Role::Judge {
            has_judge = true;
        }
    }
    if !has_judge {
        r.issues.push(Issue::warn(
            "graph.nodes",
            "no judge node; verification will fall back to detectors only",
        ));
    }
    if let Concurrency::Fixed { max_parallel } = cfg.graph.concurrency {
        if max_parallel == 0 {
            r.issues.push(Issue::err(
                "graph.concurrency.max_parallel",
                "must be at least 1",
            ));
        }
    }
    // Parallel writers without isolation clobber each other.
    let parallel_possible = !matches!(cfg.graph.concurrency, Concurrency::Sequential);
    if parallel_possible {
        let unisolated: Vec<&str> = cfg
            .graph
            .nodes
            .iter()
            .filter(|n| !n.isolated && matches!(n.role, Role::Builder))
            .map(|n| n.id.as_str())
            .collect();
        if unisolated.len() > 1 {
            r.issues.push(Issue::warn(
                "graph.nodes[].isolated",
                format!(
                    "{} builder nodes may run in parallel without worktree isolation: {}",
                    unisolated.len(),
                    unisolated.join(", ")
                ),
            ));
        }
    }
}

fn check_providers(cfg: &LoopConfig, r: &mut ValidationReport) {
    if cfg.providers.providers.is_empty() {
        r.issues.push(Issue::warn(
            "providers.providers",
            "none declared; nodes cannot be dispatched until at least one exists",
        ));
        return;
    }
    let mut seen = BTreeSet::new();
    for (i, p) in cfg.providers.providers.iter().enumerate() {
        let f = format!("providers.providers[{i}]");
        if !seen.insert(p.id.as_str()) {
            r.issues
                .push(Issue::err(format!("{f}.id"), format!("duplicate provider id `{}`", p.id)));
        }
        if p.command.trim().is_empty() {
            r.issues
                .push(Issue::err(format!("{f}.command"), "must not be empty"));
        }
    }
    for (tier, ids) in &cfg.providers.cascade {
        if !matches!(tier.as_str(), "cheap" | "standard" | "strong") {
            r.issues.push(Issue::err(
                format!("providers.cascade.{tier}"),
                "tier must be one of cheap, standard, strong",
            ));
        }
        for id in ids {
            if cfg.provider(id).is_none() {
                r.issues.push(Issue::err(
                    format!("providers.cascade.{tier}"),
                    format!("unknown provider `{id}`"),
                ));
            }
        }
    }
    if cfg.providers.enforce_judge_independence {
        let distinct: BTreeSet<&str> = cfg
            .providers
            .providers
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        if distinct.len() < 2 {
            r.issues.push(Issue::warn(
                "providers",
                "judge independence is enforced but only one provider exists; judges will fall back to detector-only verdicts",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> LoopConfig {
        crate::parse_str(
            r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
validations:
  - target: g1
    name: v1
    mode: objective
    statement: tests pass
    detector: { type: script, command: "true" }
pre_execution:
  - step: ran it by hand
    done: true
"#,
            "test",
        )
        .expect("parses")
    }

    #[test]
    fn minimal_config_is_valid() {
        let r = validate(&minimal());
        assert!(!r.has_errors(), "unexpected errors:\n{}", r.render());
    }

    #[test]
    fn goal_without_blocking_validation_is_an_error() {
        let mut c = minimal();
        c.validations[0].blocking = false;
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("no blocking validation"));
    }

    #[test]
    fn undone_pre_execution_blocks_the_run() {
        let mut c = minimal();
        c.pre_execution[0].done = false;
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("not marked done"));
    }

    #[test]
    fn overall_is_reserved_as_a_goal_name() {
        let mut c = minimal();
        c.goals[0].name = OVERALL.into();
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("reserved"));
    }

    #[test]
    fn unknown_validation_target_is_an_error() {
        let mut c = minimal();
        c.validations[0].target = "nope".into();
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("unknown target"));
    }

    #[test]
    fn judge_detector_requires_a_named_standard() {
        let mut c = minimal();
        c.validations[0].detector = Detector::Judge {
            standard: "  ".into(),
            min_score: None,
        };
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("name the external standard"));
    }

    #[test]
    fn constraint_merge_appends_rules_and_overrides_limits() {
        let g = ConstraintSet {
            rules: vec!["a".into()],
            max_tokens: Some(10),
            ..Default::default()
        };
        let n = ConstraintSet {
            rules: vec!["b".into()],
            max_tokens: Some(20),
            ..Default::default()
        };
        let m = ConstraintSet::merged(&g, Some(&n));
        assert_eq!(m.rules, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(m.max_tokens, Some(20));
    }
}
