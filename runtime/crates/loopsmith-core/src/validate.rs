//! A–J validation.
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
    check_execution_guidelines(cfg, &mut r);
    check_graph(cfg, &goal_names, &mut r);
    check_providers(cfg, &mut r);
    r
}

/// Section I. A phase graph that cannot be scheduled is a config bug, and it is
/// far cheaper to say so now than to discover it when the first arrow turns out
/// to point at a typo three hours into an unattended run.
fn check_execution_guidelines(cfg: &LoopConfig, r: &mut ValidationReport) {
    let g = &cfg.execution_guidelines;

    let mut seen = BTreeSet::new();
    for (i, item) in g.items.iter().enumerate() {
        if item.name.trim().is_empty() {
            r.issues
                .push(Issue::err(format!("execution_guidelines.items[{i}].name"), "must not be empty"));
        }
        if !seen.insert(item.name.as_str()) {
            r.issues.push(Issue::err(
                format!("execution_guidelines.items[{i}].name"),
                format!("duplicate guideline name `{}`", item.name),
            ));
        }
        if item.guideline.trim().len() < 12 {
            r.issues.push(Issue::warn(
                format!("execution_guidelines.items[{i}].guideline"),
                "too short to steer a node; say what this phase is for and what it must not do",
            ));
        }
    }

    if !g.dependency.is_empty() && g.items.is_empty() {
        r.issues.push(Issue::err(
            "execution_guidelines.dependency",
            "orders guidelines that do not exist; `items` is empty",
        ));
        return;
    }

    let edges = match g.edges() {
        Ok(e) => e,
        Err(msg) => {
            r.issues.push(Issue::err("execution_guidelines.dependency", msg));
            return;
        }
    };
    for (from, to) in &edges {
        for name in [from, to] {
            if !seen.contains(name.as_str()) {
                r.issues.push(Issue::err(
                    "execution_guidelines.dependency",
                    format!("`{name}` is ordered but is not one of: {}", g.names().join(", ")),
                ));
            }
        }
        if from == to {
            r.issues.push(Issue::err(
                "execution_guidelines.dependency",
                format!("`{from}` cannot come before itself"),
            ));
        }
    }

    // Cycles are found by trying to schedule. Reusing the scheduler rather than
    // writing a second traversal is the point of the `DagNode` trait.
    if let Ok(phases) = g.phases() {
        if let Err(e) = topo_order(&phases) {
            r.issues.push(Issue::err("execution_guidelines.dependency", e));
        }
    }

    // A node pointing at a phase that does not exist would simply never run.
    for (i, n) in cfg.graph.nodes.iter().enumerate() {
        if let Some(stage) = &n.stage {
            if !seen.contains(stage.as_str()) {
                r.issues.push(Issue::err(
                    format!("graph.nodes[{i}].stage"),
                    format!(
                        "`{stage}` is not a guideline in `execution_guidelines.items`; \
                         this node would never be dispatched"
                    ),
                ));
            }
        }
    }
}

/// Kahn's algorithm over phase names. `loopsmith-graph` owns the real
/// scheduler, but `loopsmith-core` cannot depend on it (the dependency runs the
/// other way), so cycle detection for validation lives here.
fn topo_order(phases: &[crate::Phase]) -> Result<(), String> {
    use std::collections::BTreeMap;
    let names: BTreeSet<&str> = phases.iter().map(|p| p.name.as_str()).collect();
    let mut indegree: BTreeMap<&str, usize> = names.iter().map(|n| (*n, 0)).collect();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for p in phases {
        for d in &p.depends_on {
            if !names.contains(d.as_str()) {
                continue; // already reported as an unknown name
            }
            *indegree.get_mut(p.name.as_str()).unwrap() += 1;
            dependents.entry(d.as_str()).or_default().push(p.name.as_str());
        }
    }
    let mut ready: Vec<&str> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(n, _)| *n)
        .collect();
    let mut placed = 0usize;
    while let Some(n) = ready.pop() {
        placed += 1;
        for d in dependents.get(n).into_iter().flatten() {
            let e = indegree.get_mut(*d).unwrap();
            *e -= 1;
            if *e == 0 {
                ready.push(d);
            }
        }
    }
    if placed != phases.len() {
        let stuck: Vec<&str> = indegree
            .iter()
            .filter(|(_, &d)| d > 0)
            .map(|(n, _)| *n)
            .collect();
        return Err(format!(
            "these guidelines depend on each other in a cycle: {}",
            stuck.join(", ")
        ));
    }
    Ok(())
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

/// Artifact names a `regex_match` detector can refer to.
///
/// Evidence is collected from the files this config's own `file_exists`
/// detectors name, registered under both the full path and the stem. A regex
/// naming anything else has nothing to match and fails closed for the whole
/// life of the loop, which reads as "the work is not done" rather than as "this
/// check was never wired up".
fn available_artifacts(cfg: &LoopConfig) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for v in &cfg.validations {
        if let Detector::FileExists { path, .. } = &v.detector {
            if let Some(stem) = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
            {
                out.insert(stem.to_string());
            }
            out.insert(path.clone());
        }
    }
    out
}

fn check_validations(cfg: &LoopConfig, names: &BTreeSet<&str>, r: &mut ValidationReport) {
    let artifacts = available_artifacts(cfg);
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
            Detector::RegexMatch { artifact, .. } if !artifacts.contains(artifact) => {
                r.issues.push(Issue::err(
                    format!("{f}.detector.artifact"),
                    format!(
                        "no `file_exists` detector produces `{artifact}`, so this check can \
                         never match. Artifacts are named by the file's stem or its full path; \
                         available here: {}",
                        if artifacts.is_empty() {
                            "none".to_string()
                        } else {
                            artifacts
                                .iter()
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                    ),
                ));
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
    if let Some(rand_at) = g.no_progress_iterations_randomness {
        let field = "stop_gates.no_progress_iterations_randomness";
        if rand_at == 0 {
            r.issues.push(Issue::err(
                field,
                "must be at least 1; remove it entirely to disable perturbation",
            ));
        } else if g.no_progress_iterations == 0 {
            r.issues.push(Issue::err(
                field,
                "no_progress_iterations is 0, so staleness is never counted and this can never fire",
            ));
        } else if rand_at >= g.no_progress_iterations {
            r.issues.push(Issue::err(
                field,
                format!(
                    "must be less than no_progress_iterations ({}); at {rand_at} the loop halts \
                     before it ever tries something different",
                    g.no_progress_iterations
                ),
            ));
        }
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
    // Parallel writers without isolation clobber each other — but only the ones
    // that can actually overlap. Two unisolated builders in a dependency chain
    // never run at the same time, and warning about them trains the reader to
    // ignore the warning that matters.
    let parallel_possible = !matches!(cfg.graph.concurrency, Concurrency::Sequential);
    if parallel_possible {
        let levels = wave_levels(&cfg.graph.nodes);
        let mut by_wave: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
        for n in cfg
            .graph
            .nodes
            .iter()
            .filter(|n| !n.isolated && matches!(n.role, Role::Builder))
        {
            let wave = levels.get(n.id.as_str()).copied().unwrap_or(0);
            by_wave.entry(wave).or_default().push(n.id.as_str());
        }
        for (wave, ids) in by_wave.iter().filter(|(_, ids)| ids.len() > 1) {
            r.issues.push(Issue::warn(
                "graph.nodes[].isolated",
                format!(
                    "{} builder nodes run together in wave {} without worktree isolation: {}",
                    ids.len(),
                    wave + 1,
                    ids.join(", ")
                ),
            ));
        }
    }
}

/// Which wave each node lands in: the longest dependency chain ending at it.
///
/// Nodes sharing a wave have no path between them, so they are exactly the ones
/// that can be dispatched at the same time. `loopsmith-graph` computes the same
/// thing with Kahn's algorithm, but the dependency runs the other way — core
/// cannot reach it — so this is a small relaxation pass instead.
fn wave_levels(nodes: &[NodeSpec]) -> BTreeMap<&str, usize> {
    let ids: BTreeSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut level: BTreeMap<&str, usize> = nodes.iter().map(|n| (n.id.as_str(), 0)).collect();

    // The node count bounds the longest possible chain. A cyclic graph stops
    // improving rather than looping forever; cycles are reported at plan time.
    for _ in 0..nodes.len() {
        let mut changed = false;
        for n in nodes {
            let want = n
                .depends_on
                .iter()
                .filter(|d| ids.contains(d.as_str()))
                .map(|d| level.get(d.as_str()).copied().unwrap_or(0) + 1)
                .max()
                .unwrap_or(0);
            if want > level.get(n.id.as_str()).copied().unwrap_or(0) {
                level.insert(n.id.as_str(), want);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    level
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
    fn a_regex_naming_an_artifact_nobody_produces_is_an_error() {
        // Evidence is collected from the files `file_exists` detectors name.
        // A regex naming anything else has nothing to match and fails closed
        // for the life of the loop, which reads as "the work is not done"
        // rather than as "this check was never wired up".
        let mut c = minimal();
        c.validations.push(crate::Validation {
            target: "g1".into(),
            name: "cited".into(),
            mode: Mode::Objective,
            statement: "the notes carry source URLs".into(),
            detector: Detector::RegexMatch {
                artifact: "notes".into(),
                pattern: "https?://".into(),
            },
            blocking: true,
        });
        let r = validate(&c);
        assert!(r.has_errors(), "{}", r.render());
        assert!(
            r.render().contains("can never match"),
            "the error must say why: {}",
            r.render()
        );
    }

    #[test]
    fn a_regex_over_a_file_the_config_declares_is_accepted() {
        let mut c = minimal();
        c.validations.push(crate::Validation {
            target: "g1".into(),
            name: "notes-exist".into(),
            mode: Mode::Objective,
            statement: "the notes exist".into(),
            detector: Detector::FileExists {
                path: "out/notes.md".into(),
                non_empty: true,
            },
            blocking: true,
        });
        // Both spellings resolve: the file's stem and its full path.
        for artifact in ["notes", "out/notes.md"] {
            let mut c = c.clone();
            c.validations.push(crate::Validation {
                target: "g1".into(),
                name: "cited".into(),
                mode: Mode::Objective,
                statement: "the notes carry source URLs".into(),
                detector: Detector::RegexMatch {
                    artifact: artifact.into(),
                    pattern: "https?://".into(),
                },
                blocking: true,
            });
            let r = validate(&c);
            assert!(!r.has_errors(), "`{artifact}` should resolve:\n{}", r.render());
        }
    }

    #[test]
    fn goal_without_blocking_validation_is_an_error() {
        let mut c = minimal();
        c.validations[0].blocking = false;
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("no blocking validation"));
    }

    fn builder(id: &str, deps: &[&str]) -> NodeSpec {
        NodeSpec {
            id: id.into(),
            role: Role::Builder,
            instruction: "produce the thing described in the goal".into(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            goals: vec![],
            tier: Tier::Standard,
            provider: None,
            stage: None,
            skills: vec![],
            weight: 1.0,
            isolated: false,
        }
    }

    #[test]
    fn chained_builders_are_not_reported_as_parallel_writers() {
        // `make-media -> publish` cannot overlap, so warning about it trains the
        // reader to ignore the warning that matters.
        let mut c = minimal();
        c.graph.nodes = vec![
            builder("draft", &[]),
            builder("make-media", &["draft"]),
            builder("publish", &["make-media"]),
        ];
        c.graph.concurrency = Concurrency::Auto {
            cap: 4,
            min_marginal_gain: 0.05,
        };
        assert!(
            !validate(&c).render().contains("without worktree isolation"),
            "a straight chain has no parallel writers:\n{}",
            validate(&c).render()
        );
    }

    #[test]
    fn builders_that_really_can_overlap_are_still_reported() {
        let mut c = minimal();
        c.graph.nodes = vec![
            builder("survey", &[]),
            builder("refactor-a", &["survey"]),
            builder("refactor-b", &["survey"]),
        ];
        c.graph.concurrency = Concurrency::Auto {
            cap: 4,
            min_marginal_gain: 0.05,
        };
        let report = validate(&c).render();
        assert!(report.contains("run together in wave 2"), "got:\n{report}");
        assert!(report.contains("refactor-a, refactor-b"), "got:\n{report}");
        assert!(
            !report.contains("survey,"),
            "the node they both depend on is not one of them:\n{report}"
        );
    }

    #[test]
    fn sequential_concurrency_silences_the_warning_entirely() {
        let mut c = minimal();
        c.graph.nodes = vec![builder("a", &[]), builder("b", &[])];
        c.graph.concurrency = Concurrency::Sequential;
        assert!(!validate(&c).render().contains("without worktree isolation"));
    }

    #[test]
    fn wave_levels_follow_the_longest_chain() {
        // `d` depends on both a one-hop and a two-hop path; the longest wins,
        // which is what decides whether two nodes can share a wave.
        let nodes = vec![
            builder("a", &[]),
            builder("b", &["a"]),
            builder("c", &["b"]),
            builder("d", &["a", "c"]),
        ];
        let levels = wave_levels(&nodes);
        assert_eq!(levels["a"], 0);
        assert_eq!(levels["b"], 1);
        assert_eq!(levels["c"], 2);
        assert_eq!(levels["d"], 3);
    }

    #[test]
    fn a_cycle_does_not_hang_the_wave_computation() {
        // Cycles are reported at plan time; this must terminate, not loop.
        let nodes = vec![builder("a", &["b"]), builder("b", &["a"])];
        let levels = wave_levels(&nodes);
        assert_eq!(levels.len(), 2);
    }

    #[test]
    fn a_randomness_threshold_at_or_past_the_halt_point_is_refused() {
        // At or past `no_progress_iterations` the loop halts first, so the
        // perturbation would never fire and the author would never find out.
        let mut c = minimal();
        c.stop_gates.no_progress_iterations = 3;
        for at in [3u32, 4] {
            c.stop_gates.no_progress_iterations_randomness = Some(at);
            let r = validate(&c);
            assert!(r.has_errors(), "{at} should be refused against a halt of 3");
            assert!(r.render().contains("must be less than no_progress_iterations"));
        }

        c.stop_gates.no_progress_iterations_randomness = Some(2);
        assert!(
            !validate(&c)
                .render()
                .contains("no_progress_iterations_randomness"),
            "2 is below the halt point and should be accepted"
        );
    }

    #[test]
    fn randomness_is_refused_when_staleness_is_never_counted() {
        let mut c = minimal();
        c.stop_gates.no_progress_iterations = 0;
        c.stop_gates.no_progress_iterations_randomness = Some(1);
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("staleness is never counted"));
    }

    #[test]
    fn an_execution_guideline_cycle_is_refused() {
        let mut c = minimal();
        c.execution_guidelines = ExecutionGuidelines {
            items: vec![
                Guideline {
                    name: "a".into(),
                    guideline: "the first phase of a cycle".into(),
                    note: None,
                },
                Guideline {
                    name: "b".into(),
                    guideline: "the second phase of a cycle".into(),
                    note: None,
                },
            ],
            dependency: vec!["a -> b".into(), "b -> a".into()],
        };
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("cycle"));
    }

    #[test]
    fn an_unknown_guideline_name_in_an_arrow_is_refused() {
        let mut c = minimal();
        c.execution_guidelines = ExecutionGuidelines {
            items: vec![Guideline {
                name: "gather".into(),
                guideline: "collect the sources first".into(),
                note: None,
            }],
            dependency: vec!["gather -> drfat".into()],
        };
        let r = validate(&c);
        assert!(r.has_errors());
        assert!(r.render().contains("drfat"));
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
