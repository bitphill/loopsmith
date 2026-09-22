//! The guided wizard: build a loop config by answering questions.
//!
//! Two front ends ask the same questions — the terminal (`loopsmith guided`)
//! and the browser (`loopsmith web`) — and this crate is what they share: the
//! table of agent CLIs loopsmith knows how to drive ([`catalog`]), what is
//! actually installed on this machine ([`detect`]), and the terminal interview
//! itself ([`interview`]).
//!
//! The interview's interface is deliberately one call. It walks every section,
//! lets `:back` and `:quit` move between them, runs the real validator, asks
//! for a grammar, and hands back rendered text — or says the user left. What
//! happens to that text (overwrite a file, scaffold a directory, start a run)
//! is the caller's business, because those are the parts that differ between
//! "edit this file" and "make me a new loop".
//!
//! The pieces:
//!
//!  - [`io`] — the terminal: reading, colour, and the four `:commands`.
//!  - `form` — a driver turning an ordered field list into answers, where
//!    `:back` moves a cursor rather than losing progress.
//!  - `sections` — one walker per config section, plus the answer→struct step.
//!
//! Nothing here blocks a script: a piped stdin is consumed as an answer stream,
//! which is also how the tests drive the whole wizard end to end.

pub mod catalog;
pub mod detect;
mod form;
pub mod io;
mod sections;

pub use io::{Choice, Io, Nav};
use loopsmith_core::LoopConfig;
use std::path::PathBuf;

/// How an interview ended.
pub enum Outcome {
    /// A config the user asked to have written, already rendered in the
    /// grammar they picked.
    Ready {
        cfg: Box<LoopConfig>,
        text: String,
        markdown: bool,
    },
    /// The user left mid-way. `saved` is the draft they asked to keep, if any.
    Quit { saved: Option<PathBuf> },
    /// The user reached the end and chose not to write anything.
    Declined,
}

/// Run the whole interview, starting from `start` (an existing config to
/// revise) or from an empty [`skeleton`].
///
/// Nothing is written except a draft the user explicitly asks for at `:quit`.
/// The config is validated before it is returned; a `Ready` config with errors
/// is one the user knowingly chose to write anyway.
pub fn interview(io: &mut Io, start: Option<LoopConfig>) -> Result<Outcome, String> {
    let editing = start.is_some();
    let mut cfg = start.unwrap_or_else(skeleton);

    intro(io, editing);

    if let Flow::Quit { saved } = run_wizard(io, &mut cfg)? {
        return Ok(Outcome::Quit { saved });
    }

    // Final gate: the same check `loopsmith validate` prints. Nothing is
    // returned for writing until this passes or the user knowingly overrides it.
    if !final_review(io, &mut cfg)? {
        return Ok(Outcome::Declined);
    }

    let Some(markdown) = ask_format(io)? else {
        io.note("cancelled — nothing written");
        return Ok(Outcome::Declined);
    };
    let text = render(&cfg, markdown)?;
    Ok(Outcome::Ready {
        cfg: Box::new(cfg),
        text,
        markdown,
    })
}

/// A section of the wizard: a title, an optional yes/no gate (advanced sections
/// are opt-in, per the design), and the walker that fills it in.
struct Stage {
    /// The yes/no question shown for an opt-in section. `None` for a core
    /// section that always runs.
    gate: Option<&'static str>,
    run: fn(&mut Io, &mut LoopConfig) -> Result<(), Nav>,
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

/// An empty draft the wizard fills in section by section.
///
/// Every bundle is its own `Default`, so a section the author skips is the same
/// as a section they never saw — which is what makes `:back` and an early
/// `:quit` produce a coherent partial draft rather than a half-typed struct.
pub fn skeleton() -> LoopConfig {
    LoopConfig {
        name: String::new(),
        version: "0.1.0".into(),
        description: String::new(),
        environment: Default::default(),
        features: Default::default(),
        intent: Default::default(),
        execution: Default::default(),
        safety: Default::default(),
        evolution: Default::default(),
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

pub fn render(cfg: &LoopConfig, markdown: bool) -> Result<String, String> {
    if markdown {
        Ok(loopsmith_core::render_md(cfg))
    } else {
        serde_yaml::to_string(cfg).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skeleton_is_a_valid_struct_even_when_incomplete() {
        // It must serialize (draft-save relies on this) even before any section
        // is filled in.
        let cfg = skeleton();
        assert!(render(&cfg, true).is_ok());
        assert!(render(&cfg, false).is_ok());
    }
}
