//! One walker per config section: the questions it asks, and the pure step of
//! turning the answers into the typed struct the rest of loopsmith already uses.
//!
//! Every struct built here is a `loopsmith_core` type with `deny_unknown_fields`,
//! so there is no second schema to drift: a field the wizard forgets is a field
//! that is simply left at its serde default, and a field it spells wrong would
//! not compile. The final [`super::run`] pass hands the assembled `LoopConfig`
//! to `loopsmith_core::validate`, which is the same check `loopsmith validate`
//! prints — so nothing the wizard writes is trusted further than the CLI trusts
//! a hand-written file.

use super::detect;
use super::form::{self, Answers, Step};
use super::io::{Choice, Io, Nav};
use loopsmith_core::{
    CompareOp, Concurrency, ConstraintSet, Detector, Goal, Guideline, InfoItem, LoopConfig, Mode,
    NodeSpec, ProviderKind, ProviderSpec, Role, SkillOrigin, StopGates, SuccessScenario, Tier,
    Trigger, Validation, WorkItem,
};

use super::detect::Found;

// ===========================================================================
// Identity — name, description, version
// ===========================================================================

pub fn identity(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Loop identity");
    let seed = [
        ("name", cfg.name.clone()),
        ("description", cfg.description.clone()),
        ("version", cfg.version.clone()),
    ]
    .into_iter()
    .filter(|(_, v)| !v.is_empty())
    .map(|(k, v)| (k.to_string(), v))
    .collect();

    let steps = vec![
        Step::text("name", "Loop name", cfg.name.clone())
            .help(&[
                "A short identifier for this loop. Becomes the generated skill name,",
                "so lower-case and hyphens travel best (e.g. weekly-competitor-brief).",
            ])
            .validated(|v| {
                if v.trim().is_empty() {
                    Err("a name is required".into())
                } else {
                    Ok(())
                }
            }),
        Step::optional_text("description", "One-line description", cfg.description.clone())
            .help(&["What this loop is for, in a sentence. For the humans, never sent to a node."]),
        Step::text("version", "Version", if cfg.version.is_empty() { "0.1.0".into() } else { cfg.version.clone() })
            .help(&["Semantic version for your own tracking. 0.1.0 is a fine start."]),
    ];

    let a = form::run_seeded(io, &steps, seed)?;
    cfg.name = a.s("name");
    cfg.description = a.s("description");
    cfg.version = a.s("version");
    io.success(&format!("loop `{}`", cfg.name));
    Ok(())
}

// ===========================================================================
// Providers — the CLIs a loop is allowed to spend
// ===========================================================================

pub fn providers(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Providers — the agent CLIs this loop may call");
    io.note("A provider is one command template loopsmith can route work to.");
    io.note("Ones already on this machine are marked ✓ and come pre-filled.");

    let found = detect::scan();
    let installed = found.iter().filter(|f| f.present).count();
    if installed == 0 {
        io.note("None of the known CLIs were found on PATH — you can still add one by hand.");
    }

    manage_list(
        io,
        "provider",
        &mut cfg.providers.providers,
        1,
        |p| format!("{} ({})", p.id, kind_name(p.kind)),
        |io, existing| build_provider(io, &found, existing),
    )?;

    if cfg.providers.providers.len() >= 2 {
        // Only worth asking once there are two families to keep apart.
        io.heading("Judge independence");
        cfg.providers.enforce_judge_independence = io.ask_bool(
            "Refuse a judge that runs on the same provider as the work it grades?",
            &[
                "A model grading its own family's output is not an independent check.",
                "On is the safe default; turn it off only if you know you want it.",
            ],
            cfg.providers.enforce_judge_independence,
        )?;
    }
    Ok(())
}

fn build_provider(
    io: &mut Io,
    found: &[Found],
    existing: Option<&ProviderSpec>,
) -> Result<Option<ProviderSpec>, Nav> {
    // Menu: every known CLI (installed first-class, others still offerable),
    // then a fully manual BYOK entry.
    let mut choices: Vec<Choice> = found
        .iter()
        .map(|f| {
            let mark = match &f.path {
                Some(p) => format!("✓ {}", p.display()),
                None => "not found".to_string(),
            };
            Choice::new(f.known.id, f.known.label).noted(mark)
        })
        .collect();
    choices.push(Choice::new("__byok__", "Something else (enter the command yourself)"));

    let default = existing
        .and_then(|e| found.iter().position(|f| f.known.id == e.id))
        .unwrap_or(0);

    let pick = io.ask_select(
        "Which provider?",
        &["Pick a CLI to pre-fill, or the last option to type a custom command."],
        &choices,
        Some(default),
    )?;

    if pick == "__byok__" {
        return build_byok_provider(io, existing);
    }

    let known = crate::catalog::find(&pick).expect("menu only offers catalog ids");
    if !found.iter().any(|f| f.known.id == pick && f.present) {
        io.note(&format!(
            "{} was not found on PATH. Its argv is a template — confirm it before a long run.",
            known.label
        ));
    }

    // Model choice, when the CLI offers a list. Free-text is always allowed so a
    // newer model than the binary shipped with is still reachable.
    let model = if known.models.is_empty() {
        if known.discovers_models {
            io.note("This CLI reports its own models; leaving the model blank uses its default.");
        }
        None
    } else {
        let mut mchoices: Vec<Choice> =
            known.models.iter().map(|m| Choice::new(*m, *m)).collect();
        mchoices.push(Choice::new("__custom__", "Type a different model id"));
        let chosen = io.ask_select(
            &format!("Model for {}", known.label),
            &[],
            &mchoices,
            Some(0),
        )?;
        if chosen == "__custom__" {
            Some(io.ask_text("Model id", &[], None, &|_| Ok(()))?)
        } else {
            Some(chosen)
        }
    };

    let id = io.ask_text(
        "Provider id (how nodes refer to it)",
        &["A short handle used in the config. The CLI name is a fine default."],
        Some(existing.map(|e| e.id.as_str()).unwrap_or(known.id)),
        &nonempty,
    )?;

    Ok(Some(ProviderSpec {
        id,
        kind: parse_kind(known.kind),
        tiers: known.tiers.iter().filter_map(|t| parse_tier(t)).collect(),
        command: known.bin.to_string(),
        args: known.args.iter().map(|s| s.to_string()).collect(),
        model,
        requires_env: known.requires_env.iter().map(|s| s.to_string()).collect(),
        timeout_seconds: None,
        prompt_on_stdin: known.prompt_on_stdin,
        usage_regex: None,
        cost_per_1k_tokens: known.cost_per_1k,
    }))
}

fn build_byok_provider(
    io: &mut Io,
    existing: Option<&ProviderSpec>,
) -> Result<Option<ProviderSpec>, Nav> {
    let kinds = [
        ("byok", "BYOK — any OpenAI-compatible or bespoke command"),
        ("claude_code", "Claude Code"),
        ("openai", "OpenAI / Codex"),
        ("gemini", "Google Gemini"),
        ("grok_cli", "Grok CLI"),
        ("hermes", "Hermes"),
        ("ollama", "Ollama"),
        ("mcp", "MCP server over stdio"),
    ];
    let kind_choices: Vec<Choice> = kinds.iter().map(|(v, l)| Choice::new(*v, *l)).collect();

    let steps = vec![
        Step::text("id", "Provider id", existing.map(|e| e.id.clone()).unwrap_or_default())
            .help(&["A short handle nodes use to refer to this provider."])
            .validated(nonempty),
        Step::select("kind", "Provider family", kind_choices, 0)
            .help(&["Its wire family. BYOK is the escape hatch for anything else."]),
        Step::text("command", "Command (the binary to run)", existing.map(|e| e.command.clone()).unwrap_or_default())
            .help(&["The executable, e.g. `my-agent` or an absolute path."])
            .validated(nonempty),
        Step::optional_text(
            "args",
            "Arguments (space-separated)",
            existing.map(|e| e.args.join(" ")).unwrap_or_default(),
        )
        .help(&[
            "Argv tokens, split on spaces. Use the placeholders {prompt} {system}",
            "{model} {tier} {node} — loopsmith substitutes them at spawn time.",
        ]),
        Step::optional_text("model", "Model id", existing.and_then(|e| e.model.clone()).unwrap_or_default())
            .help(&["Substituted for {model}. Leave blank if the command needs none."]),
        Step::optional_text(
            "requires_env",
            "Required env vars (comma-separated)",
            existing.map(|e| e.requires_env.join(", ")).unwrap_or_default(),
        )
        .help(&["Names only — e.g. OPENAI_API_KEY. loopsmith checks presence, never reads values."]),
        Step::boolean("stdin", "Send the prompt on stdin instead of in the arguments?", existing.map(|e| e.prompt_on_stdin).unwrap_or(false)),
        Step::optional_text("cost", "Cost per 1000 tokens in USD", existing.and_then(|e| e.cost_per_1k_tokens).map(|c| c.to_string()).unwrap_or_default())
            .help(&["Powers the cost ceiling. Leave blank if it runs locally or is unknown."])
            .validated(opt_float),
    ];

    let a = match form::run(io, &steps) {
        Ok(a) => a,
        Err(Nav::Back) => return Ok(None),
        Err(Nav::Quit) => return Err(Nav::Quit),
    };

    Ok(Some(ProviderSpec {
        id: a.s("id"),
        kind: parse_kind(&a.s("kind")),
        tiers: Vec::new(),
        command: a.s("command"),
        args: split_ws(&a.s("args")),
        model: a.opt("model"),
        requires_env: split_commas(&a.s("requires_env")),
        timeout_seconds: None,
        prompt_on_stdin: a.flag("stdin"),
        usage_regex: None,
        cost_per_1k_tokens: a.opt("cost").and_then(|c| c.parse().ok()),
    }))
}

// ===========================================================================
// C — Goals
// ===========================================================================

pub fn goals(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Goals — what this loop is trying to achieve");
    io.note("Name each outcome in plain language. Add at least one.");
    manage_list(
        io,
        "goal",
        &mut cfg.goals,
        1,
        |g| format!("{} — {}", g.name, truncate(&g.description, 48)),
        build_goal,
    )?;
    Ok(())
}

fn build_goal(io: &mut Io, existing: Option<&Goal>) -> Result<Option<Goal>, Nav> {
    let steps = vec![
        Step::text("name", "Goal name", existing.map(|g| g.name.clone()).unwrap_or_default())
            .help(&["A short handle for this goal, referenced by validations and nodes."])
            .validated(nonempty),
        Step::text("description", "Description", existing.map(|g| g.description.clone()).unwrap_or_default())
            .help(&["Natural language. Subjective phrasing is fine here — the validation is what must be checkable."])
            .validated(nonempty),
        Step::optional_text("depends_on", "Depends on (comma-separated goal names)", existing.map(|g| g.depends_on.join(", ")).unwrap_or_default())
            .help(&["Other goals that must be satisfied first. Leave blank for none."]),
        Step::optional_text("priority", "Priority (integer, optional)", existing.and_then(|g| g.priority).map(|p| p.to_string()).unwrap_or_default())
            .help(&["Lower runs first when it matters. Blank leaves it unordered."])
            .validated(opt_uint),
    ];
    let a = match form::run(io, &steps) {
        Ok(a) => a,
        Err(Nav::Back) => return Ok(None),
        Err(Nav::Quit) => return Err(Nav::Quit),
    };
    Ok(Some(Goal {
        name: a.s("name"),
        description: a.s("description"),
        depends_on: split_commas(&a.s("depends_on")),
        priority: a.opt("priority").and_then(|p| p.parse().ok()),
    }))
}

// ===========================================================================
// D — Validations
// ===========================================================================

pub fn validations(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Validations — how each goal is checked");
    io.note("A validation is what actually decides 'done'. Add at least one.");
    io.note("Prefer a deterministic check (script, file, threshold) over a model judge.");

    let targets = goal_target_choices(cfg);
    manage_list(
        io,
        "validation",
        &mut cfg.validations,
        1,
        |v| format!("{} → {} [{}]", v.target, v.name, detector_name(&v.detector)),
        |io, existing| build_validation(io, &targets, existing),
    )?;
    Ok(())
}

fn build_validation(
    io: &mut Io,
    targets: &[Choice],
    existing: Option<&Validation>,
) -> Result<Option<Validation>, Nav> {
    let target_default = existing
        .and_then(|v| targets.iter().position(|c| c.value == v.target))
        .unwrap_or(0);
    let mode_choices = vec![
        Choice::new("objective", "objective — pass/fail on evidence"),
        Choice::new("subjective", "subjective — a judged opinion"),
        Choice::new("percentage", "percentage — a fraction of checks"),
    ];
    let detector_choices = vec![
        Choice::new("script", "script — run a command, exit 0 passes (strongest)"),
        Choice::new("file_exists", "file_exists — a path must exist"),
        Choice::new("regex_match", "regex_match — a pattern must match an artifact"),
        Choice::new("threshold", "threshold — a number vs a limit"),
        Choice::new("judge", "judge — a model verdict against a named standard"),
    ];

    // Head fields, then detector-specific fields, in two forms so the detector
    // choice can steer the second set.
    let head = vec![
        Step::select("target", "Which goal does this check?", clone_choices(targets), target_default)
            .help(&["The goal this validation gates, or `overall` for the whole loop."]),
        Step::text("name", "Validation name", existing.map(|v| v.name.clone()).unwrap_or_default())
            .help(&["A short handle for this check."])
            .validated(nonempty),
        Step::select("mode", "Mode", mode_choices, mode_index(existing))
            .help(&["How the result is read. Objective is the usual choice."]),
        Step::text("statement", "Statement (what is being checked)", existing.map(|v| v.statement.clone()).unwrap_or_default())
            .help(&["Natural-language description of the condition."])
            .validated(nonempty),
        Step::select("detector", "How is it decided?", detector_choices, detector_index(existing))
            .help(&["The mechanism. Deterministic detectors are stronger than a judge."]),
    ];
    let a = match form::run(io, &head) {
        Ok(a) => a,
        Err(Nav::Back) => return Ok(None),
        Err(Nav::Quit) => return Err(Nav::Quit),
    };

    let detector = match a.s("detector").as_str() {
        "script" => {
            let d = detector_form(io, &[
                Step::text("command", "Command to run", existing_script_command(existing)).validated(nonempty),
                Step::optional_text("args", "Arguments (space-separated)", "")
                    .help(&["Argv tokens for the command."]),
                Step::optional_text("expect_exit", "Expected exit code (default 0)", "")
                    .validated(opt_int),
            ])?;
            let Some(d) = d else { return Ok(None) };
            Detector::Script {
                command: d.s("command"),
                args: split_ws(&d.s("args")),
                expect_exit: d.opt("expect_exit").and_then(|s| s.parse().ok()),
            }
        }
        "file_exists" => {
            let d = detector_form(io, &[
                Step::text("path", "Path that must exist", "").validated(nonempty),
                Step::boolean("non_empty", "Must it also be non-empty?", false),
            ])?;
            let Some(d) = d else { return Ok(None) };
            Detector::FileExists { path: d.s("path"), non_empty: d.flag("non_empty") }
        }
        "regex_match" => {
            let d = detector_form(io, &[
                Step::text("artifact", "Artifact (file) to search", "").validated(nonempty),
                Step::text("pattern", "Regular expression", "").validated(nonempty),
            ])?;
            let Some(d) = d else { return Ok(None) };
            Detector::RegexMatch { artifact: d.s("artifact"), pattern: d.s("pattern") }
        }
        "threshold" => {
            let op_choices = vec![
                Choice::new("gte", "≥ at least"),
                Choice::new("gt", "> greater than"),
                Choice::new("lte", "≤ at most"),
                Choice::new("lt", "< less than"),
                Choice::new("eq", "= equal to"),
            ];
            let d = detector_form(io, &[
                Step::text("metric", "Metric name", "").validated(nonempty),
                Step::select("op", "Comparison", op_choices, 0),
                Step::text("value", "Threshold value", "").validated(req_float),
            ])?;
            let Some(d) = d else { return Ok(None) };
            Detector::Threshold {
                metric: d.s("metric"),
                op: parse_op(&d.s("op")),
                value: d.s("value").parse().unwrap_or(0.0),
            }
        }
        _ => {
            let d = detector_form(io, &[
                Step::text("standard", "The standard the judge checks against", "")
                    .help(&["Naming a standard is what turns an opinion into a check."])
                    .validated(nonempty),
                Step::optional_text("min_score", "Minimum score to pass (0-1, optional)", "")
                    .validated(opt_float),
            ])?;
            let Some(d) = d else { return Ok(None) };
            Detector::Judge {
                standard: d.s("standard"),
                min_score: d.opt("min_score").and_then(|s| s.parse().ok()),
            }
        }
    };

    let blocking = io.ask_bool(
        "Must this pass for the goal to count as satisfied?",
        &["A blocking validation holds the gate shut; a non-blocking one is only recorded."],
        existing.map(|v| v.blocking).unwrap_or(true),
    )?;

    Ok(Some(Validation {
        target: a.s("target"),
        name: a.s("name"),
        mode: parse_mode(&a.s("mode")),
        statement: a.s("statement"),
        detector,
        blocking,
    }))
}

fn detector_form(
    io: &mut Io,
    steps: &[Step],
) -> Result<Option<std::collections::BTreeMap<String, String>>, Nav> {
    match form::run(io, steps) {
        Ok(a) => Ok(Some(a)),
        Err(Nav::Back) => Ok(None),
        Err(Nav::Quit) => Err(Nav::Quit),
    }
}

// ===========================================================================
// F — Stop gates
// ===========================================================================

pub fn stop_gates(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Stop gates — the ceilings that end a run");
    io.note("At least one hard limit keeps an unattended loop from running away.");
    let g = &cfg.stop_gates;
    let steps = vec![
        Step::text("max_iterations", "Max whole-loop iterations", g.max_iterations.to_string())
            .help(&["Hard ceiling on passes over the graph."])
            .validated(req_uint),
        Step::text("max_revisions_per_node", "Max revisions per node", g.max_revisions_per_node.to_string())
            .help(&["One stuck node cannot burn the whole budget past this."])
            .validated(req_uint),
        Step::optional_text("max_wall_clock_seconds", "Wall-clock budget in seconds", g.max_wall_clock_seconds.map(|v| v.to_string()).unwrap_or_default())
            .help(&["Whole-run time limit. Blank for none."])
            .validated(opt_uint),
        Step::optional_text("max_tokens", "Token budget for the whole run", g.max_tokens.map(|v| v.to_string()).unwrap_or_default())
            .validated(opt_uint),
        Step::optional_text("max_cost_usd", "Cost ceiling in USD", g.max_cost_usd.map(|v| v.to_string()).unwrap_or_default())
            .help(&["The clearest safety limit for an overnight run. Strongly recommended."])
            .validated(opt_float),
        Step::text("no_progress_iterations", "Halt after this many no-change iterations", g.no_progress_iterations.to_string())
            .help(&["Stop the line rather than spin when nothing is improving."])
            .validated(req_uint),
        Step::boolean("stop_on_overall_success", "Stop as soon as every overall success is met?", g.stop_on_overall_success),
    ];
    let a = form::run(io, &steps)?;
    cfg.stop_gates = StopGates {
        max_iterations: a.s("max_iterations").parse().unwrap_or(10),
        max_revisions_per_node: a.s("max_revisions_per_node").parse().unwrap_or(3),
        max_wall_clock_seconds: a.opt("max_wall_clock_seconds").and_then(|s| s.parse().ok()),
        max_tokens: a.opt("max_tokens").and_then(|s| s.parse().ok()),
        max_cost_usd: a.opt("max_cost_usd").and_then(|s| s.parse().ok()),
        no_progress_iterations: a.s("no_progress_iterations").parse().unwrap_or(3),
        no_progress_iterations_randomness: g.no_progress_iterations_randomness,
        stop_on_overall_success: a.flag("stop_on_overall_success"),
    };
    Ok(())
}

// ===========================================================================
// Advanced sections — each behind a yes/no gate (Q3)
// ===========================================================================

/// A — static context every node receives.
pub fn information(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Information — static context every node receives (section A)");
    manage_list(io, "info item", &mut cfg.information, 0, |i| format!("{} = {}", i.key, truncate(&i.value, 40)), |io, e| {
        let steps = vec![
            Step::text("key", "Key", e.map(|i: &InfoItem| i.key.clone()).unwrap_or_default()).validated(nonempty),
            Step::text("value", "Value", e.map(|i| i.value.clone()).unwrap_or_default()).validated(nonempty),
            Step::optional_text("note", "Note (optional)", e.and_then(|i| i.note.clone()).unwrap_or_default()),
        ];
        Ok(match form::run(io, &steps) {
            Ok(a) => Some(InfoItem { key: a.s("key"), value: a.s("value"), note: a.opt("note") }),
            Err(Nav::Back) => None,
            Err(Nav::Quit) => return Err(Nav::Quit),
        })
    })?;
    Ok(())
}

/// B — the manual work that must be done before automating.
pub fn pre_execution(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Pre-execution — the manual work to prove first (section B)");
    io.note("You cannot automate a process you cannot yet describe by hand.");
    manage_list(io, "step", &mut cfg.pre_execution, 0, |w| format!("[{}] {}", if w.done { "x" } else { " " }, truncate(&w.step, 44)), |io, e| {
        let steps = vec![
            Step::text("step", "Manual step", e.map(|w: &WorkItem| w.step.clone()).unwrap_or_default()).validated(nonempty),
            Step::boolean("done", "Have you actually done this by hand yet?", e.map(|w| w.done).unwrap_or(false)),
            Step::optional_text("evidence", "Evidence (optional)", e.and_then(|w| w.evidence.clone()).unwrap_or_default()),
        ];
        Ok(match form::run(io, &steps) {
            Ok(a) => Some(WorkItem { step: a.s("step"), done: a.flag("done"), evidence: a.opt("evidence") }),
            Err(Nav::Back) => None,
            Err(Nav::Quit) => return Err(Nav::Quit),
        })
    })?;
    Ok(())
}

/// E — what counts as success.
pub fn success(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Success scenarios — what counts as done (section E)");
    let targets = goal_target_choices(cfg);
    manage_list(io, "scenario", &mut cfg.success, 0, |s| format!("{} → {}", s.target, s.name), |io, e| {
        let td = e.and_then(|s: &SuccessScenario| targets.iter().position(|c| c.value == s.target)).unwrap_or(0);
        let modes = vec![
            Choice::new("objective", "objective"),
            Choice::new("subjective", "subjective"),
            Choice::new("percentage", "percentage — needs a threshold"),
        ];
        let md = e.map(|s| mode_value_index(s.mode)).unwrap_or(0);
        let steps = vec![
            Step::select("target", "Target goal (or overall)", clone_choices(&targets), td),
            Step::text("name", "Scenario name", e.map(|s| s.name.clone()).unwrap_or_default()).validated(nonempty),
            Step::select("mode", "Mode", modes, md),
            Step::text("statement", "Statement", e.map(|s| s.statement.clone()).unwrap_or_default()).validated(nonempty),
            Step::optional_text("threshold", "Threshold (0-1, only for percentage)", e.and_then(|s| s.threshold).map(|t| t.to_string()).unwrap_or_default()).validated(opt_float),
        ];
        Ok(match form::run(io, &steps) {
            Ok(a) => Some(SuccessScenario {
                target: a.s("target"),
                name: a.s("name"),
                mode: parse_mode(&a.s("mode")),
                statement: a.s("statement"),
                threshold: a.opt("threshold").and_then(|t| t.parse().ok()),
            }),
            Err(Nav::Back) => None,
            Err(Nav::Quit) => return Err(Nav::Quit),
        })
    })?;
    Ok(())
}

/// G — triggers.
pub fn schedules(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Schedules — when the loop fires on its own (section G)");
    manage_list(io, "trigger", &mut cfg.schedules, 0, |t| trigger_name(t).to_string(), |io, _e| build_trigger(io))?;
    Ok(())
}

fn build_trigger(io: &mut Io) -> Result<Option<Trigger>, Nav> {
    let kinds = vec![
        Choice::new("interval", "interval — every N seconds"),
        Choice::new("cron", "cron — a five-field schedule"),
        Choice::new("file_change", "file_change — when a path changes"),
        Choice::new("goal_satisfied", "goal_satisfied — when a goal becomes true"),
        Choice::new("manual", "manual — only on demand"),
    ];
    let kind = io.ask_select("Trigger type", &[], &kinds, Some(0))?;
    let t = match kind.as_str() {
        "interval" => {
            let s = io.ask_text("Interval in seconds", &["Runs must finish faster than this or they pile up."], Some("3600"), &req_uint)?;
            Trigger::Interval { seconds: s.parse().unwrap_or(3600) }
        }
        "cron" => {
            let e = io.ask_text("Cron expression (5 fields)", &["e.g. `0 9 * * 1` for 09:00 every Monday."], None, &nonempty)?;
            Trigger::Cron { expr: e }
        }
        "file_change" => {
            let p = io.ask_text("Path to watch", &[], None, &nonempty)?;
            Trigger::FileChange { path: p }
        }
        "goal_satisfied" => {
            let g = io.ask_text("Upstream goal name", &[], None, &nonempty)?;
            Trigger::GoalSatisfied { goal: g }
        }
        _ => Trigger::Manual,
    };
    Ok(Some(t))
}

/// H — global constraints.
pub fn constraints(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Constraints — global rules applied to every node (section H)");
    io.note("Per-node overrides stay a file/web job; this sets the global set.");
    let g = &cfg.constraints.global;
    let steps = vec![
        Step::optional_text("rules", "Rules (one per line, use ; to separate)", g.rules.join("; "))
            .help(&["Literal instructions injected into every node's prompt."]),
        Step::optional_text("forbidden_paths", "Forbidden paths (comma-separated)", g.forbidden_paths.join(", ")),
        Step::optional_text("forbidden_commands", "Forbidden commands (comma-separated)", g.forbidden_commands.join(", ")),
        Step::optional_text("max_tokens", "Per-node token cap", g.max_tokens.map(|v| v.to_string()).unwrap_or_default()).validated(opt_uint),
        Step::optional_text("max_seconds", "Per-node time cap in seconds", g.max_seconds.map(|v| v.to_string()).unwrap_or_default()).validated(opt_uint),
        Step::optional_text("human_checkpoint", "Actions needing a human (comma-separated)", g.human_checkpoint.join(", "))
            .help(&["Irreversible actions do not get made at machine speed."]),
    ];
    let a = form::run(io, &steps)?;
    cfg.constraints.global = ConstraintSet {
        rules: split_semis(&a.s("rules")),
        forbidden_paths: split_commas(&a.s("forbidden_paths")),
        forbidden_commands: split_commas(&a.s("forbidden_commands")),
        max_tokens: a.opt("max_tokens").and_then(|s| s.parse().ok()),
        max_seconds: a.opt("max_seconds").and_then(|s| s.parse().ok()),
        human_checkpoint: split_commas(&a.s("human_checkpoint")),
    };
    Ok(())
}

/// I — execution guidelines (phases).
pub fn execution_guidelines(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Execution guidelines — named phases (section I)");
    io.note("A phase is a stretch of the run with a standing instruction; order them with arrows.");
    manage_list(io, "phase", &mut cfg.execution_guidelines.items, 0, |g| g.name.clone(), |io, e| {
        let steps = vec![
            Step::text("name", "Phase name", e.map(|g: &Guideline| g.name.clone()).unwrap_or_default()).validated(nonempty),
            Step::text("guideline", "Standing instruction", e.map(|g| g.guideline.clone()).unwrap_or_default()).validated(nonempty),
            Step::optional_text("note", "Note (optional)", e.and_then(|g| g.note.clone()).unwrap_or_default()),
        ];
        Ok(match form::run(io, &steps) {
            Ok(a) => Some(Guideline { name: a.s("name"), guideline: a.s("guideline"), note: a.opt("note") }),
            Err(Nav::Back) => None,
            Err(Nav::Quit) => return Err(Nav::Quit),
        })
    })?;
    let names: Vec<String> = cfg.execution_guidelines.items.iter().map(|g| g.name.clone()).collect();
    if names.len() >= 2 {
        let existing = cfg.execution_guidelines.dependency.join("; ");
        let line = io.ask_text(
            "Ordering (e.g. `gather -> draft -> review`; ; separates chains)",
            &[&format!("Known phases: {}", names.join(", ")), "Blank leaves them all parallel."],
            Some(if existing.is_empty() { "" } else { &existing }),
            &|_| Ok(()),
        )?;
        cfg.execution_guidelines.dependency = split_semis(&line);
    }
    Ok(())
}

/// J — default skills.
pub fn default_skills(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Default skills — sub-agents installed before the loop starts (section J)");
    manage_list(io, "skill", &mut cfg.default_skills, 0, |s| s.name.clone(), |io, e| {
        let origins = vec![
            Choice::new("marketplace", "marketplace — claudemarketplaces.com / skills CLI"),
            Choice::new("github", "github — an https git repo"),
            Choice::new("local", "local — already on disk"),
        ];
        let od = e.map(|s: &loopsmith_core::DefaultSkill| origin_index(s.source)).unwrap_or(0);
        let steps = vec![
            Step::text("name", "Skill name / install directory", e.map(|s| s.name.clone()).unwrap_or_default()).validated(nonempty),
            Step::select("source", "Where does it come from?", origins, od),
            Step::optional_text("url", "URL or owner/repo@skill spec", e.and_then(|s| s.url.clone()).unwrap_or_default())
                .help(&["Required for github (https only); optional for marketplace; ignored for local."]),
            Step::optional_text("init_command", "Setup command (argv, optional)", e.and_then(|s| s.init_command.clone()).unwrap_or_default())
                .help(&["Split on spaces and run directly — NOT a shell line, so &&, |, $() are literal."]),
            Step::optional_text("note", "Note (optional)", e.and_then(|s| s.note.clone()).unwrap_or_default()),
        ];
        Ok(match form::run(io, &steps) {
            Ok(a) => Some(loopsmith_core::DefaultSkill {
                name: a.s("name"),
                source: parse_origin(&a.s("source")),
                url: a.opt("url"),
                init_command: a.opt("init_command"),
                note: a.opt("note"),
            }),
            Err(Nav::Back) => None,
            Err(Nav::Quit) => return Err(Nav::Quit),
        })
    })?;
    Ok(())
}

/// The execution graph.
pub fn graph(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Execution graph — the units of work and their real dependencies");
    io.note("Nodes are steps; an edge means 'this node reads that node's output'.");
    let providers: Vec<String> = cfg.providers.providers.iter().map(|p| p.id.clone()).collect();
    let goals: Vec<String> = cfg.goals.iter().map(|g| g.name.clone()).collect();
    manage_list(io, "node", &mut cfg.graph.nodes, 0, |n| format!("{} ({}, {})", n.id, role_name(n.role), tier_name(n.tier)), |io, e| build_node(io, &providers, &goals, e))?;

    if !cfg.graph.nodes.is_empty() {
        let modes = vec![
            Choice::new("auto", "auto — derive parallelism from the graph"),
            Choice::new("sequential", "sequential — one node at a time"),
            Choice::new("fixed", "fixed — a set width"),
        ];
        let cur = match cfg.graph.concurrency {
            Concurrency::Auto { .. } => 0,
            Concurrency::Sequential => 1,
            Concurrency::Fixed { .. } => 2,
        };
        let mode = io.ask_select("Concurrency", &["How much to run at once."], &modes, Some(cur))?;
        cfg.graph.concurrency = match mode.as_str() {
            "sequential" => Concurrency::Sequential,
            "fixed" => {
                let w = io.ask_text("Max parallel nodes", &[], Some("4"), &req_uint)?;
                Concurrency::Fixed { max_parallel: w.parse().unwrap_or(4) }
            }
            _ => Concurrency::default(),
        };
    }
    Ok(())
}

fn build_node(
    io: &mut Io,
    providers: &[String],
    goals: &[String],
    e: Option<&NodeSpec>,
) -> Result<Option<NodeSpec>, Nav> {
    let roles = vec![
        Choice::new("builder", "builder — produces the work"),
        Choice::new("judge", "judge — grades it against a standard"),
        Choice::new("manager", "manager — routes on the verdict"),
        Choice::new("adversary", "adversary — argues the other side"),
        Choice::new("researcher", "researcher — gathers material"),
    ];
    let tiers = vec![
        Choice::new("cheap", "cheap — high volume, low judgment"),
        Choice::new("standard", "standard"),
        Choice::new("strong", "strong — low volume, high judgment"),
    ];
    let mut steps = vec![
        Step::text("id", "Node id", e.map(|n| n.id.clone()).unwrap_or_default()).validated(nonempty),
        Step::select("role", "Role", roles, e.map(|n| role_index(n.role)).unwrap_or(0)),
        Step::text("instruction", "Instruction (what this node does)", e.map(|n| n.instruction.clone()).unwrap_or_default()).validated(nonempty),
        Step::optional_text("depends_on", "Depends on (comma-separated node ids)", e.map(|n| n.depends_on.join(", ")).unwrap_or_default())
            .help(&["Only list an edge if this node truly reads that node's output."]),
        Step::select("tier", "Tier", tiers, e.map(|n| tier_index(n.tier)).unwrap_or(1)),
    ];
    if !goals.is_empty() {
        steps.push(Step::optional_text("goals", "Goals this node advances (comma-separated)", e.map(|n| n.goals.join(", ")).unwrap_or_default())
            .help(&[&format!("Known goals: {}", goals.join(", "))]));
    }
    if !providers.is_empty() {
        let mut pchoices = vec![Choice::new("", "(none — route by tier)")];
        pchoices.extend(providers.iter().map(|p| Choice::new(p.clone(), p.clone())));
        let pd = e.and_then(|n| n.provider.as_ref()).and_then(|p| providers.iter().position(|x| x == p)).map(|i| i + 1).unwrap_or(0);
        steps.push(Step::select("provider", "Pin a provider?", pchoices, pd));
    }
    steps.push(Step::boolean("isolated", "Run in its own git worktree? (required for parallel writers)", e.map(|n| n.isolated).unwrap_or(false)));

    let a = match form::run(io, &steps) {
        Ok(a) => a,
        Err(Nav::Back) => return Ok(None),
        Err(Nav::Quit) => return Err(Nav::Quit),
    };
    Ok(Some(NodeSpec {
        id: a.s("id"),
        role: parse_role(&a.s("role")),
        instruction: a.s("instruction"),
        depends_on: split_commas(&a.s("depends_on")),
        goals: split_commas(&a.s("goals")),
        tier: parse_tier(&a.s("tier")).unwrap_or_default(),
        provider: a.opt("provider"),
        skills: e.map(|n| n.skills.clone()).unwrap_or_default(),
        stage: e.and_then(|n| n.stage.clone()),
        weight: e.map(|n| n.weight).unwrap_or(1.0),
        isolated: a.flag("isolated"),
    }))
}

/// Context policy.
pub fn context(io: &mut Io, cfg: &mut LoopConfig) -> Result<(), Nav> {
    io.heading("Context — what each iteration remembers");
    let c = &cfg.context;
    let steps = vec![
        Step::text("carry_summaries", "How many past iteration summaries to carry", c.carry_summaries.to_string())
            .help(&["0 disables carry-forward; 2 lets a node see its last two tries."])
            .validated(req_uint),
        Step::optional_text("summary_provider", "Provider id to write summary prose (optional)", c.summary_provider.clone().unwrap_or_default())
            .help(&["Prose costs tokens every iteration; blank keeps only the deterministic facts."]),
        Step::text("max_summary_chars", "Max characters per summary", c.max_summary_chars.to_string()).validated(req_uint),
    ];
    let a = form::run(io, &steps)?;
    cfg.context = loopsmith_core::ContextPolicy {
        carry_summaries: a.s("carry_summaries").parse().unwrap_or(2),
        summary_provider: a.opt("summary_provider"),
        max_summary_chars: a.s("max_summary_chars").parse().unwrap_or(1200),
    };
    Ok(())
}

// ===========================================================================
// The generic list-management menu
// ===========================================================================

/// Run the Add / Edit / Remove / Done menu over a `Vec<T>`.
///
/// `min` is the smallest count that lets the user finish. `describe` renders one
/// entry for the list; `build` constructs or edits one entry, returning `None`
/// when the user backs out of that entry.
fn manage_list<T>(
    io: &mut Io,
    singular: &str,
    entries: &mut Vec<T>,
    min: usize,
    describe: impl Fn(&T) -> String,
    mut build: impl FnMut(&mut Io, Option<&T>) -> Result<Option<T>, Nav>,
) -> Result<(), Nav> {
    loop {
        if !entries.is_empty() {
            io.println("");
            for (i, e) in entries.iter().enumerate() {
                io.println(&format!("  {}. {}", i + 1, describe(e)));
            }
        }
        let mut choices = vec![Choice::new("add", format!("Add a {singular}"))];
        if !entries.is_empty() {
            choices.push(Choice::new("edit", format!("Edit a {singular}")));
            choices.push(Choice::new("remove", format!("Remove a {singular}")));
        }
        if entries.len() >= min {
            choices.push(Choice::new("done", "Done with this section"));
        } else {
            io.note(&format!("Add at least {min} to continue."));
        }
        // Default to Done when allowed and there is something, else Add.
        let default = if entries.len() >= min && !entries.is_empty() {
            choices.len() - 1
        } else {
            0
        };
        match io.ask_select(&format!("{}s: choose", cap(singular)), &[], &choices, Some(default)) {
            Ok(action) => match action.as_str() {
                "add" => {
                    if let Some(item) = build(io, None)? {
                        io.success(&format!("added {singular}"));
                        entries.push(item);
                    }
                }
                "edit" => {
                    if let Some(i) = pick_index(io, entries.len())? {
                        if let Some(item) = build(io, Some(&entries[i]))? {
                            entries[i] = item;
                        }
                    }
                }
                "remove" => {
                    if let Some(i) = pick_index(io, entries.len())? {
                        entries.remove(i);
                        io.success(&format!("removed {singular}"));
                    }
                }
                _ => return Ok(()),
            },
            // `:back` at the section menu leaves the section (the orchestrator
            // decides whether that returns to the previous section).
            Err(Nav::Back) => return Err(Nav::Back),
            Err(Nav::Quit) => return Err(Nav::Quit),
        }
    }
}

fn pick_index(io: &mut Io, len: usize) -> Result<Option<usize>, Nav> {
    match io.ask_text(
        &format!("Which number (1-{len})?"),
        &[],
        None,
        &move |v| match v.parse::<usize>() {
            Ok(n) if (1..=len).contains(&n) => Ok(()),
            _ => Err(format!("enter a number from 1 to {len}")),
        },
    ) {
        Ok(s) => Ok(Some(s.parse::<usize>().unwrap() - 1)),
        Err(Nav::Back) => Ok(None),
        Err(Nav::Quit) => Err(Nav::Quit),
    }
}

// ===========================================================================
// Validators (shared)
// ===========================================================================

fn nonempty(v: &str) -> Result<(), String> {
    if v.trim().is_empty() {
        Err("this field is required".into())
    } else {
        Ok(())
    }
}
fn req_uint(v: &str) -> Result<(), String> {
    v.trim().parse::<u64>().map(|_| ()).map_err(|_| "enter a whole number".into())
}
fn opt_uint(v: &str) -> Result<(), String> {
    if v.trim().is_empty() { return Ok(()); }
    req_uint(v)
}
fn req_float(v: &str) -> Result<(), String> {
    v.trim().parse::<f64>().map(|_| ()).map_err(|_| "enter a number".into())
}
fn opt_float(v: &str) -> Result<(), String> {
    if v.trim().is_empty() { return Ok(()); }
    req_float(v)
}
fn opt_int(v: &str) -> Result<(), String> {
    if v.trim().is_empty() { return Ok(()); }
    v.trim().parse::<i64>().map(|_| ()).map_err(|_| "enter a whole number".into())
}

// ===========================================================================
// Small parse/label helpers
// ===========================================================================

fn split_ws(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}
fn split_commas(s: &str) -> Vec<String> {
    s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect()
}
fn split_semis(s: &str) -> Vec<String> {
    s.split(';').map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect()
}
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}
fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
fn clone_choices(src: &[Choice]) -> Vec<Choice> {
    src.iter()
        .map(|c| {
            let mut n = Choice::new(c.value.clone(), c.label.clone());
            if let Some(note) = &c.note {
                n = n.noted(note.clone());
            }
            n
        })
        .collect()
}

fn goal_target_choices(cfg: &LoopConfig) -> Vec<Choice> {
    let mut v: Vec<Choice> = cfg
        .goals
        .iter()
        .map(|g| Choice::new(g.name.clone(), g.name.clone()))
        .collect();
    v.push(Choice::new("overall", "overall — the loop as a whole"));
    v
}

fn parse_kind(s: &str) -> ProviderKind {
    serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or(ProviderKind::Byok)
}
fn kind_name(k: ProviderKind) -> &'static str {
    match k {
        ProviderKind::ClaudeCode => "claude_code",
        ProviderKind::Ollama => "ollama",
        ProviderKind::GrokCli => "grok_cli",
        ProviderKind::GrokBuild => "grok_build",
        ProviderKind::Hermes => "hermes",
        ProviderKind::OpenAi => "openai",
        ProviderKind::Gemini => "gemini",
        ProviderKind::Byok => "byok",
        ProviderKind::Mcp => "mcp",
    }
}
fn parse_tier(s: &str) -> Option<Tier> {
    match s {
        "cheap" => Some(Tier::Cheap),
        "standard" => Some(Tier::Standard),
        "strong" => Some(Tier::Strong),
        _ => None,
    }
}
fn tier_name(t: Tier) -> &'static str {
    match t {
        Tier::Cheap => "cheap",
        Tier::Standard => "standard",
        Tier::Strong => "strong",
    }
}
fn tier_index(t: Tier) -> usize {
    match t {
        Tier::Cheap => 0,
        Tier::Standard => 1,
        Tier::Strong => 2,
    }
}
fn parse_role(s: &str) -> Role {
    match s {
        "judge" => Role::Judge,
        "manager" => Role::Manager,
        "adversary" => Role::Adversary,
        "researcher" => Role::Researcher,
        _ => Role::Builder,
    }
}
fn role_name(r: Role) -> &'static str {
    match r {
        Role::Builder => "builder",
        Role::Judge => "judge",
        Role::Manager => "manager",
        Role::Adversary => "adversary",
        Role::Researcher => "researcher",
    }
}
fn role_index(r: Role) -> usize {
    match r {
        Role::Builder => 0,
        Role::Judge => 1,
        Role::Manager => 2,
        Role::Adversary => 3,
        Role::Researcher => 4,
    }
}
fn parse_mode(s: &str) -> Mode {
    match s {
        "subjective" => Mode::Subjective,
        "percentage" => Mode::Percentage,
        _ => Mode::Objective,
    }
}
fn mode_value_index(m: Mode) -> usize {
    match m {
        Mode::Objective => 0,
        Mode::Subjective => 1,
        Mode::Percentage => 2,
    }
}
fn mode_index(v: Option<&Validation>) -> usize {
    v.map(|x| mode_value_index(x.mode)).unwrap_or(0)
}
fn parse_op(s: &str) -> CompareOp {
    match s {
        "gt" => CompareOp::Gt,
        "lt" => CompareOp::Lt,
        "lte" => CompareOp::Lte,
        "eq" => CompareOp::Eq,
        _ => CompareOp::Gte,
    }
}
fn detector_index(v: Option<&Validation>) -> usize {
    match v.map(|x| &x.detector) {
        Some(Detector::Script { .. }) => 0,
        Some(Detector::FileExists { .. }) => 1,
        Some(Detector::RegexMatch { .. }) => 2,
        Some(Detector::Threshold { .. }) => 3,
        Some(Detector::Judge { .. }) => 4,
        None => 0,
    }
}
fn detector_name(d: &Detector) -> &'static str {
    match d {
        Detector::Script { .. } => "script",
        Detector::FileExists { .. } => "file_exists",
        Detector::RegexMatch { .. } => "regex_match",
        Detector::Threshold { .. } => "threshold",
        Detector::Judge { .. } => "judge",
    }
}
fn existing_script_command(v: Option<&Validation>) -> String {
    match v.map(|x| &x.detector) {
        Some(Detector::Script { command, .. }) => command.clone(),
        _ => String::new(),
    }
}
fn trigger_name(t: &Trigger) -> &'static str {
    match t {
        Trigger::Cron { .. } => "cron",
        Trigger::Interval { .. } => "interval",
        Trigger::FileChange { .. } => "file_change",
        Trigger::GoalSatisfied { .. } => "goal_satisfied",
        Trigger::Manual => "manual",
    }
}
fn parse_origin(s: &str) -> SkillOrigin {
    match s {
        "github" => SkillOrigin::Github,
        "local" => SkillOrigin::Local,
        _ => SkillOrigin::Marketplace,
    }
}
fn origin_index(o: SkillOrigin) -> usize {
    match o {
        SkillOrigin::Marketplace => 0,
        SkillOrigin::Github => 1,
        SkillOrigin::Local => 2,
    }
}
