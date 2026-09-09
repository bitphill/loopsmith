//! `loopsmith guided` / `loopsmith --guided` — build a loop by answering
//! questions in the terminal, one field at a time.
//!
//! This is the third front end onto the exact same A–J config that `loopsmith
//! new` hands you as a file and `loopsmith web` paints in a browser. It exists
//! for the machine with no browser and the person who would rather not open a
//! text editor: over SSH, in a bare TTY, it walks every section with the field's
//! own explanation in place, offers the agent CLIs it already found, and writes
//! nothing until the whole thing passes `loopsmith_core::validate`.
//!
//! The pieces:
//!
//!  - [`io`] — the terminal: reading, colour, and the four `:commands`.
//!  - [`form`] — a driver turning an ordered field list into answers, where
//!    `:back` moves a cursor rather than losing progress.
//!  - [`sections`] — one walker per config section, plus the answer→struct step.
//!  - [`detect`] — a synchronous `PATH` scan for the known CLIs (no async, so it
//!    works in a `--no-default-features` build with no `web`).
//!
//! Nothing here blocks a script: a piped stdin is consumed as an answer stream,
//! which is also how the tests drive the whole wizard end to end.

mod detect;
mod form;
mod io;
mod sections;

use io::{Choice, Io, Nav};
use loopsmith_core::LoopConfig;
use std::path::PathBuf;
use std::process::ExitCode;

/// A section of the wizard: a title, an optional yes/no gate (advanced sections
/// are opt-in, per the design), and the walker that fills it in.
struct Stage {
    /// The yes/no question shown for an opt-in section. `None` for a core
    /// section that always runs.
    gate: Option<&'static str>,
    run: fn(&mut Io, &mut LoopConfig) -> Result<(), Nav>,
}

/// Entry point from `cmd::dispatch`.
pub fn execute(path: Option<PathBuf>, edit: Option<PathBuf>) -> Result<ExitCode, String> {
    let mut io = Io::new();

    // Starting point: an existing config to revise, or a blank skeleton.
    let (mut cfg, edit_target) = match &edit {
        Some(file) => {
            let cfg = loopsmith_core::load(file).map_err(|e| {
                format!("could not read {} to edit: {e}", file.display())
            })?;
            (cfg, Some(file.clone()))
        }
        None => (skeleton(), None),
    };

    intro(&io, edit_target.is_some());

    match run_wizard(&mut io, &mut cfg)? {
        Flow::Completed => {}
        Flow::Quit { saved } => {
            if let Some(p) = saved {
                println!("\nDraft saved to {}. Resume with:\n  loopsmith guided --edit {}", p.display(), p.display());
            } else {
                println!("\nNothing written.");
            }
            return Ok(ExitCode::SUCCESS);
        }
    }

    // Final gate: the same check `loopsmith validate` prints. Nothing is written
    // until this passes or the user knowingly overrides it.
    if !final_review(&mut io, &mut cfg)? {
        return Ok(ExitCode::SUCCESS);
    }

    // Grammar, then write.
    let markdown = match ask_format(&mut io)? {
        Some(m) => m,
        None => return Ok(cancelled(&io)),
    };
    let text = render(&cfg, markdown)?;

    match edit_target {
        Some(file) => write_back(&mut io, &file, &text),
        None => create_loop(&mut io, &cfg, path, text, markdown),
    }
}

/// A `:quit` at one of the final prompts, after the config is assembled but
/// before it is on disk. Nothing is written; this is a clean exit, not an error.
fn cancelled(io: &Io) -> ExitCode {
    io.note("cancelled — nothing written");
    ExitCode::SUCCESS
}

enum Flow {
    Completed,
    Quit { saved: Option<PathBuf> },
}

/// The outcome of the `:quit` menu.
enum QuitChoice {
    /// Go back to the stage we were on.
    Resume,
    /// Stop; the wizard writes nothing (a draft may have been saved).
    Leave(Option<PathBuf>),
}

/// The section flow, with `:back` walking between sections and `:quit` offering
/// to save a draft or resume.
fn run_wizard(io: &mut Io, cfg: &mut LoopConfig) -> Result<Flow, String> {
    let stages = stages();
    let mut i = 0usize;
    while i < stages.len() {
        // An opt-in section asks first; a "no" (or `:back`) skips it.
        let proceed = match stages[i].gate {
            None => true,
            Some(q) => match io.ask_bool(q, &["(advanced — press Enter to skip)"], false) {
                Ok(b) => b,
                Err(Nav::Back) => {
                    i = i.saturating_sub(1);
                    continue;
                }
                Err(Nav::Quit) => match quit_menu(io, cfg)? {
                    QuitChoice::Resume => continue,
                    QuitChoice::Leave(saved) => return Ok(Flow::Quit { saved }),
                },
            },
        };
        if !proceed {
            i += 1;
            continue;
        }
        match (stages[i].run)(io, cfg) {
            Ok(()) => i += 1,
            Err(Nav::Back) => i = i.saturating_sub(1),
            Err(Nav::Quit) => match quit_menu(io, cfg)? {
                // Resume re-runs the current stage; sections read the config as
                // their defaults, so nothing from earlier stages is lost.
                QuitChoice::Resume => continue,
                QuitChoice::Leave(saved) => return Ok(Flow::Quit { saved }),
            },
        }
    }
    Ok(Flow::Completed)
}

/// The `:quit` menu: save a partial draft, discard, or resume where we left off.
fn quit_menu(io: &mut Io, cfg: &LoopConfig) -> Result<QuitChoice, String> {
    // In a non-interactive run there is no one to answer; treat quit/EOF as a
    // clean discard so a truncated script does not hang.
    if !io.is_interactive() {
        return Ok(QuitChoice::Leave(None));
    }
    let choices = vec![
        Choice::new("resume", "Resume — go back to where I was"),
        Choice::new("save", "Save a draft and leave"),
        Choice::new("discard", "Discard everything and leave"),
    ];
    // A nested Nav here (another Ctrl-C) collapses to discard.
    let pick = io
        .ask_select("Leave the wizard?", &[], &choices, Some(0))
        .unwrap_or_else(|_| "discard".into());
    match pick.as_str() {
        "resume" => Ok(QuitChoice::Resume),
        "save" => Ok(QuitChoice::Leave(Some(save_draft(cfg)?))),
        _ => Ok(QuitChoice::Leave(None)),
    }
}

/// Write a partial config to `loop.draft.<ext>` in the current directory.
fn save_draft(cfg: &LoopConfig) -> Result<PathBuf, String> {
    let text = render(cfg, true)?;
    let path = PathBuf::from("loop.draft.md");
    std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(path)
}

fn skeleton() -> LoopConfig {
    LoopConfig {
        name: String::new(),
        version: "0.1.0".into(),
        description: String::new(),
        information: Vec::new(),
        pre_execution: Vec::new(),
        goals: Vec::new(),
        validations: Vec::new(),
        success: Vec::new(),
        stop_gates: Default::default(),
        schedules: Vec::new(),
        constraints: Default::default(),
        execution_guidelines: Default::default(),
        default_skills: Vec::new(),
        graph: Default::default(),
        providers: Default::default(),
        skills: Default::default(),
        context: Default::default(),
    }
}

fn stages() -> Vec<Stage> {
    vec![
        Stage { gate: None, run: sections::identity },
        Stage { gate: None, run: sections::providers },
        Stage { gate: None, run: sections::goals },
        Stage { gate: None, run: sections::validations },
        Stage { gate: None, run: sections::stop_gates },
        Stage { gate: Some("Define an execution graph of work nodes now?"), run: sections::graph },
        Stage { gate: Some("Add static information every node receives (section A)?"), run: sections::information },
        Stage { gate: Some("Record the manual pre-execution steps (section B)?"), run: sections::pre_execution },
        Stage { gate: Some("Define explicit success scenarios (section E)?"), run: sections::success },
        Stage { gate: Some("Add schedules/triggers (section G)?"), run: sections::schedules },
        Stage { gate: Some("Set global constraints (section H)?"), run: sections::constraints },
        Stage { gate: Some("Define execution-guideline phases (section I)?"), run: sections::execution_guidelines },
        Stage { gate: Some("Declare default skills to install (section J)?"), run: sections::default_skills },
        Stage { gate: Some("Tune the context/memory policy?"), run: sections::context },
    ]
}

fn intro(io: &Io, editing: bool) {
    if !io.is_interactive() {
        return;
    }
    println!();
    println!("{}", io.bold(&io.cyan("loopsmith — guided setup")));
    if editing {
        io.note("Editing an existing config. Each field shows its current value in [brackets].");
    } else {
        io.note("Answer one field at a time. Each shows a default in [brackets] — Enter accepts it.");
    }
    io.note("At any prompt: :back  ·  :next (keep default)  ·  :help  ·  :quit");
    println!();
}

/// Run the real validator, print what it found, and decide whether to write.
fn final_review(io: &mut Io, cfg: &mut LoopConfig) -> Result<bool, String> {
    loop {
        let report = loopsmith_core::validate(cfg);
        let errors = report
            .issues
            .iter()
            .filter(|i| matches!(i.severity, loopsmith_core::Severity::Error))
            .count();
        let warnings = report.issues.len() - errors;

        io.heading("Review");
        if report.issues.is_empty() {
            io.success("config is valid, no warnings");
            return Ok(true);
        }
        for issue in &report.issues {
            let sev = match issue.severity {
                loopsmith_core::Severity::Error => io.red("error"),
                loopsmith_core::Severity::Warning => io.yellow("warn "),
            };
            io.println(&format!("  {sev} {}: {}", io.dim(&issue.field), issue.message));
        }
        io.println("");

        if errors == 0 {
            io.note(&format!("{warnings} warning(s), no errors."));
            // Warnings do not block a write; ask, defaulting to yes. A `:quit`
            // here means "do not write", not an error.
            return Ok(io.ask_bool("Write the config?", &[], true).unwrap_or(false));
        }

        let choices = vec![
            Choice::new("edit", "Go back through the sections and fix them"),
            Choice::new("write", "Write it anyway (errors and all)"),
            Choice::new("quit", "Leave without writing"),
        ];
        match io.ask_select(&format!("{errors} error(s) block a clean config."), &[], &choices, Some(0)) {
            Ok(a) => match a.as_str() {
                "edit" => match run_wizard(io, cfg)? {
                    Flow::Completed => continue,
                    Flow::Quit { .. } => return Ok(false),
                },
                "write" => return Ok(true),
                _ => return Ok(false),
            },
            Err(_) => return Ok(false),
        }
    }
}

/// `Some(true)` = Markdown, `Some(false)` = YAML, `None` = the user quit.
fn ask_format(io: &mut Io) -> Result<Option<bool>, String> {
    let choices = vec![
        Choice::new("md", "Markdown — reads like a brief, easiest to hand-edit later"),
        Choice::new("yaml", "YAML — terser, closer to the schema"),
    ];
    io.heading("Output");
    match io.ask_select("Which grammar for the config file?", &[], &choices, Some(0)) {
        Ok(pick) => Ok(Some(pick == "md")),
        Err(_) => Ok(None),
    }
}

fn render(cfg: &LoopConfig, markdown: bool) -> Result<String, String> {
    if markdown {
        Ok(loopsmith_core::render_md(cfg))
    } else {
        serde_yaml::to_string(cfg).map_err(|e| e.to_string())
    }
}

/// Overwrite the file that was loaded with `--edit`.
fn write_back(io: &mut Io, file: &std::path::Path, text: &str) -> Result<ExitCode, String> {
    io.heading("Save");
    if io.is_interactive() {
        // A `:quit` at the overwrite prompt leaves the original untouched.
        let ok = io.ask_bool(&format!("Overwrite {}?", file.display()), &[], true).unwrap_or(false);
        if !ok {
            io.note("Left the original file untouched.");
            return Ok(ExitCode::SUCCESS);
        }
    }
    std::fs::write(file, text).map_err(|e| format!("could not write {}: {e}", file.display()))?;
    io.success(&format!("wrote {}", file.display()));
    println!("\nCheck it:\n  loopsmith validate {}", file.display());
    Ok(ExitCode::SUCCESS)
}

/// Scaffold a brand-new loop directory from the assembled config.
fn create_loop(
    io: &mut Io,
    cfg: &LoopConfig,
    path_arg: Option<PathBuf>,
    text: String,
    markdown: bool,
) -> Result<ExitCode, String> {
    io.heading("Where to create it");
    // A parent directory; the loop nests under its own name, so several loops
    // can share a workspace without colliding.
    let parent = match path_arg {
        Some(p) => p,
        None => match io.ask_text(
            "Parent directory",
            &["The loop is created in a sub-directory named after it."],
            Some("."),
            &|_| Ok(()),
        ) {
            Ok(d) => PathBuf::from(d),
            Err(_) => return Ok(cancelled(io)),
        },
    };
    let dir_name = sanitize(&cfg.name);
    let root = parent.join(&dir_name);

    let force = if root.exists() && dir_nonempty(&root) {
        io.note(&format!("{} already exists and is not empty.", root.display()));
        match io.ask_bool("Write into it anyway?", &[], false) {
            Ok(b) => b,
            Err(_) => return Ok(cancelled(io)),
        }
    } else {
        false
    };

    // Isolated nodes need a repository to get a worktree each; default the git
    // question to yes exactly when the config has one.
    let wants_git_default = cfg.graph.nodes.iter().any(|n| n.isolated);
    let git = match io.ask_bool(
        "Initialise a git repository in the loop?",
        &[if wants_git_default {
            "This config has isolated nodes, which need a repo for their worktrees."
        } else {
            "Lets isolated nodes get a worktree each. Safe to say yes."
        }],
        wants_git_default,
    ) {
        Ok(b) => b,
        Err(_) => return Ok(cancelled(io)),
    };

    let purpose = if cfg.description.is_empty() {
        "a loopsmith loop".to_string()
    } else {
        cfg.description.clone()
    };

    let s = crate::scaffold::scaffold(&crate::scaffold::NewLoopArgs {
        path: root.clone(),
        name: cfg.name.clone(),
        purpose,
        force,
        config: Some(crate::scaffold::ProvidedConfig { text, markdown }),
        git,
    })
    .map_err(|e| e.to_string())?;

    let config_path = root.join(&s.config_file);
    io.success(&format!("created loop `{}` at {}", cfg.name, root.display()));
    if let Some(Err(why)) = &s.git {
        io.note(&format!("git init failed: {why} — isolated nodes will share one directory."));
    }
    println!("\n  config: {}", config_path.display());
    println!("  check:  loopsmith validate {}", config_path.display());
    println!("  plan:   loopsmith plan {}", config_path.display());

    offer_run(io, &config_path)
}

/// Offer to run the loop now (Q20). Defaults to no; if yes, offers a dry run
/// first so a first pass costs nothing.
fn offer_run(io: &mut Io, config_path: &std::path::Path) -> Result<ExitCode, String> {
    io.heading("Run");
    // The loop is already written, so a `:quit` (or EOF) here just means "don't
    // run now" — a clean success, never an error.
    let run_now = io
        .ask_bool("Run the loop now?", &["Or do it later with the command above."], false)
        .unwrap_or(false);
    if !run_now {
        return Ok(ExitCode::SUCCESS);
    }
    let dry = io
        .ask_bool(
            "Dry run first (plan and log, spend nothing)?",
            &["Recommended for a first pass — it invokes no provider."],
            true,
        )
        .unwrap_or(true);
    crate::cmd::run::execute(config_path, None, dry, false, false)
}

// -- small helpers ----------------------------------------------------------

fn dir_nonempty(p: &std::path::Path) -> bool {
    std::fs::read_dir(p).map(|mut d| d.next().is_some()).unwrap_or(false)
}

/// A filesystem-safe directory name from a loop name.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        "loop".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_safe_directory() {
        assert_eq!(sanitize("Weekly Competitor Brief!"), "Weekly-Competitor-Brief");
        assert_eq!(sanitize("  --edge--  "), "edge");
        assert_eq!(sanitize("///"), "loop");
    }

    #[test]
    fn the_skeleton_is_a_valid_struct_even_when_incomplete() {
        // It must serialize (draft-save relies on this) even before any section
        // is filled in.
        let cfg = skeleton();
        assert!(render(&cfg, true).is_ok());
        assert!(render(&cfg, false).is_ok());
    }
}
