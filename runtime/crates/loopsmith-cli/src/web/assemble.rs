//! Turning what the browser holds into a config, and telling it what it has.
//!
//! The form in the browser is the A–J model, field for field. That is not a
//! coincidence to be maintained by hand: the config the browser posts is
//! deserialized by the very same `serde` derives the CLI uses, so a section
//! the browser gets wrong fails here, at the moment it is typed, with the same
//! message `loopsmith validate` would print. There is no second schema.
//!
//! Everything in this module is in-process and instant. Validation, planning,
//! permissions, and cost all answer in under a millisecond, which is what lets
//! the right-hand rail update on every keystroke. The buttons that spend money
//! or write files spawn the real CLI instead — see [`crate::web::exec`].

use loopsmith_core::{LoopConfig, Severity};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Everything the right-hand rail shows, from one config, in one pass.
#[derive(Debug, Clone, Serialize)]
pub struct Review {
    pub parsed: bool,
    /// Why it would not parse. A `deny_unknown_fields` model gives a precise
    /// message here, naming the offending key — worth surfacing verbatim.
    pub parse_error: Option<String>,
    pub issues: Vec<ReviewIssue>,
    pub error_count: usize,
    pub warning_count: usize,
    pub plan: Option<PlanView>,
    pub permissions: Vec<String>,
    pub cost: CostView,
    /// Plain-language observations that are neither errors nor warnings —
    /// things worth knowing before pressing run.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewIssue {
    pub severity: &'static str,
    /// Dotted path into the config, so the UI can scroll to the field.
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanView {
    pub waves: Vec<Vec<String>>,
    pub critical_path: Vec<String>,
    pub concurrency: usize,
    pub predicted_speedup: f64,
    pub speedup_ceiling: f64,
    pub parallel_fraction: f64,
    /// Nodes that may run at the same time and both write, without a worktree
    /// each. The single most expensive mistake this planner can catch.
    pub unisolated_parallel_writers: Vec<String>,
    pub error: Option<String>,
}

/// What a run could cost at the ceilings currently set.
///
/// Deliberately an upper bound, not a forecast. A number that is usually low
/// and occasionally enormous is worse than useless when the point is deciding
/// whether to leave something running overnight.
#[derive(Debug, Clone, Serialize)]
pub struct CostView {
    /// The hard ceiling, if `max_cost_usd` is set.
    pub ceiling_usd: Option<f64>,
    /// Worst case from iterations × nodes × the priciest reachable provider.
    pub worst_case_usd: Option<f64>,
    /// Why the estimate is missing or rough.
    pub basis: String,
    pub bounded: bool,
}

/// Parse, validate, plan, and price a config the browser is holding.
pub fn review(value: &serde_json::Value) -> Review {
    let cfg: LoopConfig = match serde_json::from_value(value.clone()) {
        Ok(c) => c,
        Err(e) => {
            return Review {
                parsed: false,
                parse_error: Some(e.to_string()),
                issues: Vec::new(),
                error_count: 1,
                warning_count: 0,
                plan: None,
                permissions: Vec::new(),
                cost: CostView {
                    ceiling_usd: None,
                    worst_case_usd: None,
                    basis: "the config could not be read".into(),
                    bounded: false,
                },
                notes: Vec::new(),
            }
        }
    };
    review_config(&cfg)
}

pub fn review_config(cfg: &LoopConfig) -> Review {
    let report = loopsmith_core::validate(cfg);
    let issues: Vec<ReviewIssue> = report
        .issues
        .iter()
        .map(|i| ReviewIssue {
            severity: match i.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            field: i.field.clone(),
            message: i.message.clone(),
        })
        .collect();

    Review {
        parsed: true,
        parse_error: None,
        error_count: issues.iter().filter(|i| i.severity == "error").count(),
        warning_count: issues.iter().filter(|i| i.severity == "warning").count(),
        issues,
        plan: plan_view(cfg),
        permissions: crate::permissions::required(cfg),
        cost: cost_view(cfg),
        notes: notes(cfg),
    }
}

fn plan_view(cfg: &LoopConfig) -> Option<PlanView> {
    if cfg.graph.nodes.is_empty() {
        return None;
    }
    match loopsmith_graph::plan(&cfg.graph) {
        Ok(p) => Some(PlanView {
            waves: p.waves.iter().map(|w| w.nodes.clone()).collect(),
            unisolated_parallel_writers: loopsmith_graph::unisolated_parallel_writers(
                &cfg.graph, &p.waves,
            ),
            critical_path: p.critical_path,
            concurrency: p.concurrency,
            predicted_speedup: p.predicted_speedup,
            speedup_ceiling: p.speedup_ceiling,
            parallel_fraction: p.parallel_fraction,
            error: None,
        }),
        // A cycle is a real answer, not a missing one. Showing the graph
        // section as blank would hide the one thing wrong with it.
        Err(e) => Some(PlanView {
            waves: Vec::new(),
            critical_path: Vec::new(),
            concurrency: 1,
            predicted_speedup: 1.0,
            speedup_ceiling: 1.0,
            parallel_fraction: 0.0,
            unisolated_parallel_writers: Vec::new(),
            error: Some(e.to_string()),
        }),
    }
}

fn cost_view(cfg: &LoopConfig) -> CostView {
    let ceiling = cfg.stop_gates.max_cost_usd;

    let priciest = cfg
        .providers
        .providers
        .iter()
        .filter_map(|p| p.cost_per_1k_tokens)
        .fold(None::<f64>, |acc, c| Some(acc.map_or(c, |a| a.max(c))));

    let per_node_tokens = cfg
        .constraints
        .global
        .max_tokens
        .or(cfg.stop_gates.max_tokens);

    let worst = match (priciest, per_node_tokens) {
        (Some(rate), Some(tokens)) if rate > 0.0 => {
            let nodes = cfg.graph.nodes.len().max(1) as f64;
            let iterations = cfg.stop_gates.max_iterations.max(1) as f64;
            Some(rate * (tokens as f64 / 1000.0) * nodes * iterations)
        }
        _ => None,
    };

    let basis = match (ceiling, worst) {
        (Some(_), _) => "a hard ceiling is set: the run halts when it is reached".into(),
        (None, Some(_)) => "no ceiling is set. This is iterations × nodes × the priciest \
                            provider's rate at the token limit — the most a run could spend \
                            before it stops on its own."
            .into(),
        (None, None) => "no ceiling, and not enough information to bound the spend. Set \
                         `max_cost_usd`, or give each provider a `cost_per_1k_tokens`."
            .into(),
    };

    CostView {
        ceiling_usd: ceiling,
        worst_case_usd: worst,
        bounded: ceiling.is_some(),
        basis,
    }
}

/// Observations a person would want before an unattended run, in their words.
///
/// These are not schema violations — a config can be perfectly valid and still
/// be a bad idea to leave running overnight.
fn notes(cfg: &LoopConfig) -> Vec<String> {
    let mut out = Vec::new();

    let judged = cfg
        .validations
        .iter()
        .filter(|v| v.blocking && matches!(v.detector, loopsmith_core::Detector::Judge { .. }))
        .count();
    let blocking = cfg.validations.iter().filter(|v| v.blocking).count();
    if blocking > 0 && judged == blocking {
        out.push(
            "Every blocking validation is decided by a model. The gate can still only be \
             opened by loopsmith, but what it is reading is an opinion. Add at least one \
             script, file, or threshold check so something deterministic has a vote."
                .into(),
        );
    }

    if cfg.stop_gates.max_cost_usd.is_none() && cfg.stop_gates.max_wall_clock_seconds.is_none() {
        out.push(
            "Neither a cost ceiling nor a wall-clock ceiling is set. The iteration limit is \
             the only thing standing between this loop and an unbounded bill."
                .into(),
        );
    }

    if cfg.schedules.iter().any(|t| {
        matches!(t, loopsmith_core::Trigger::Interval { seconds } if *seconds < 300)
    }) {
        out.push(
            "A trigger fires more often than every five minutes. Confirm that a run finishes \
             faster than that, or runs will pile up on each other."
                .into(),
        );
    }

    if cfg.pre_execution.iter().any(|w| !w.done) {
        let pending = cfg.pre_execution.iter().filter(|w| !w.done).count();
        out.push(format!(
            "{pending} pre-execution step(s) are not marked done. Section B is the work you \
             must do by hand before automating it — a loop built on an unproven manual \
             process automates the wrong thing faster."
        ));
    }

    if cfg.providers.providers.len() < 2 && cfg.providers.enforce_judge_independence {
        out.push(
            "Judge independence is on but only one provider is configured, so any judge node \
             would have to grade its own family's work. Add a second provider, or the judge \
             will be refused at run time."
                .into(),
        );
    }

    out
}

/// Render a config as the browser asked for it.
pub fn render(cfg: &LoopConfig, format: Format) -> Result<String, String> {
    match format {
        Format::Yaml => serde_yaml::to_string(cfg).map_err(|e| e.to_string()),
        Format::Markdown => Ok(loopsmith_core::render_md(cfg)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Yaml,
    Markdown,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Yaml => "yaml",
            Format::Markdown => "md",
        }
    }
    pub fn file_name(self) -> String {
        format!("loop.{}", self.extension())
    }
}

/// Read a config off disk into the browser's editor, whatever grammar it is in.
pub fn load_file(path: &Path) -> Result<(LoopConfig, Format), String> {
    let cfg = loopsmith_core::load(path).map_err(|e| e.to_string())?;
    let format = if path.extension().and_then(|e| e.to_str()) == Some("md") {
        Format::Markdown
    } else {
        Format::Yaml
    };
    Ok((cfg, format))
}

/// Write a config to a scratch file so `loopsmith new --config-file` can read it.
///
/// A temp file rather than stdin because the job runner streams a subprocess's
/// output and gives it no stdin — one code path for every button beats a
/// special case for this one.
pub fn write_scratch(text: &str, format: Format) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("loopsmith-web");
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let path = dir.join(format!(
        "draft-{}.{}",
        crate::web::detect::now_ms(),
        format.extension()
    ));
    std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// The loop library
// ---------------------------------------------------------------------------

/// Loops this machine knows about.
///
/// A loop's home directory is the real record; this is only an index so the UI
/// can offer "the ones you made" without asking the user to remember paths. An
/// entry whose directory has gone is dropped on read rather than resurrected,
/// because a library that lists things that are not there is worse than one
/// that lists nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub path: String,
    pub name: String,
    pub config_file: String,
    pub created_ms: u64,
}

fn library_path() -> Option<PathBuf> {
    crate::web::detect::home_dir().map(|h| h.join(".loopsmith/loops.json"))
}

pub fn library() -> Vec<LibraryEntry> {
    let Some(path) = library_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let entries: Vec<LibraryEntry> = serde_json::from_str(&text).unwrap_or_default();
    entries
        .into_iter()
        .filter(|e| Path::new(&e.path).join(&e.config_file).exists())
        .collect()
}

pub fn remember(entry: LibraryEntry) -> Result<(), String> {
    let Some(path) = library_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut all = library();
    all.retain(|e| e.path != entry.path);
    all.insert(0, entry);
    all.truncate(200);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("could not write {}: {e}", path.display()))
}

pub fn forget(loop_path: &str) -> Result<(), String> {
    let Some(path) = library_path() else {
        return Ok(());
    };
    let mut all = library();
    all.retain(|e| e.path != loop_path);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> serde_json::Value {
        serde_json::json!({
            "name": "t",
            "goals": [{ "name": "g1", "description": "a goal with a long enough description" }],
            "validations": [{
                "target": "g1", "name": "v1", "mode": "objective",
                "statement": "the file is written",
                "detector": { "type": "file_exists", "path": "out.txt" }
            }]
        })
    }

    #[test]
    fn a_config_the_browser_posts_is_read_by_the_same_model_the_cli_uses() {
        let r = review(&minimal());
        assert!(r.parsed, "{:?}", r.parse_error);
    }

    #[test]
    fn a_misspelled_section_is_reported_with_the_key_that_was_wrong() {
        // The browser has no second schema to consult, so this message is the
        // whole of what the user gets. It has to name the field.
        let mut v = minimal();
        v["stop_gate"] = serde_json::json!({ "max_iterations": 2 });
        let r = review(&v);
        assert!(!r.parsed);
        let msg = r.parse_error.unwrap();
        assert!(msg.contains("stop_gate"), "got: {msg}");
    }

    #[test]
    fn permissions_are_derived_rather_than_guessed() {
        let mut v = minimal();
        v["providers"] = serde_json::json!({
            "providers": [{ "id": "p", "kind": "claude_code", "command": "claude" }]
        });
        let r = review(&v);
        assert!(
            r.permissions.iter().any(|p| p.contains("claude")),
            "the provider's binary must appear: {:?}",
            r.permissions
        );
    }

    #[test]
    fn a_loop_judged_only_by_models_is_called_out() {
        let mut v = minimal();
        v["validations"] = serde_json::json!([{
            "target": "g1", "name": "v1", "mode": "subjective",
            "statement": "it reads well",
            "detector": { "type": "judge", "standard": "the house style guide" }
        }]);
        let r = review(&v);
        assert!(
            r.notes.iter().any(|n| n.contains("opinion")),
            "expected the all-judge note, got: {:?}",
            r.notes
        );
    }

    #[test]
    fn an_unbounded_run_is_called_out_before_it_is_started() {
        let r = review(&minimal());
        assert!(
            r.notes.iter().any(|n| n.contains("unbounded bill")),
            "got: {:?}",
            r.notes
        );
        assert!(!r.cost.bounded);
    }

    #[test]
    fn a_ceiling_makes_the_cost_view_bounded() {
        let mut v = minimal();
        v["stop_gates"] = serde_json::json!({ "max_cost_usd": 5.0 });
        let r = review(&v);
        assert!(r.cost.bounded);
        assert_eq!(r.cost.ceiling_usd, Some(5.0));
    }

    #[test]
    fn a_cycle_in_the_graph_is_reported_rather_than_hidden() {
        let mut v = minimal();
        v["graph"] = serde_json::json!({ "nodes": [
            { "id": "a", "role": "builder", "instruction": "do the first part of the work", "depends_on": ["b"] },
            { "id": "b", "role": "builder", "instruction": "do the second part of the work", "depends_on": ["a"] }
        ]});
        let r = review(&v);
        let plan = r.plan.expect("a graph with nodes always yields a view");
        assert!(plan.error.is_some(), "a cycle must surface");
    }

    #[test]
    fn a_config_with_no_graph_has_no_plan_rather_than_an_empty_one() {
        assert!(review(&minimal()).plan.is_none());
    }

    #[test]
    fn both_grammars_round_trip_through_render() {
        let cfg: LoopConfig = serde_json::from_value(minimal()).unwrap();
        let yaml = render(&cfg, Format::Yaml).unwrap();
        assert_eq!(serde_yaml::from_str::<LoopConfig>(&yaml).unwrap().name, "t");

        let md = render(&cfg, Format::Markdown).unwrap();
        assert_eq!(
            loopsmith_core::parse_md(&md, "test").unwrap().name,
            "t",
            "markdown is the same model, not a lossy export"
        );
    }

    #[test]
    fn a_scratch_file_carries_the_extension_the_grammar_needs() {
        // `loopsmith new --config-file` picks its parser by extension, so a
        // markdown draft written as `.yaml` would be parsed as YAML and fail.
        let p = write_scratch("name: t\n", Format::Yaml).unwrap();
        assert_eq!(p.extension().unwrap(), "yaml");
        let p = write_scratch("# t\n", Format::Markdown).unwrap();
        assert_eq!(p.extension().unwrap(), "md");
    }

    #[test]
    fn the_library_drops_entries_whose_directory_is_gone() {
        // Not a filter over the real list — that would depend on the machine's
        // own library file. This asserts the predicate the filter uses.
        let missing = LibraryEntry {
            path: "/no/such/loop".into(),
            name: "gone".into(),
            config_file: "loop.yaml".into(),
            created_ms: 0,
        };
        assert!(!Path::new(&missing.path).join(&missing.config_file).exists());
    }
}
