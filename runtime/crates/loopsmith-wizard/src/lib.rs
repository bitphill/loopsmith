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
//!  - [`spec`] — every question, as data. The browser fetches the same list.
//!  - [`answers`] — answers in, `LoopConfig` out, typed here and nowhere else.
//!  - [`interview`] — walking that list in a terminal, where `:back` moves a
//!    cursor rather than losing progress.
//!  - [`io`] — the terminal: reading, colour, and the four `:commands`.
//!
//! Nothing here blocks a script: a piped stdin is consumed as an answer stream,
//! which is also how the tests drive the whole wizard end to end.

pub mod answers;
pub mod catalog;
pub mod detect;
pub mod interview;
pub mod io;
pub mod preferences;
pub mod spec;

pub use io::{Choice, Io, Nav};
use answers::Answers;
use loopsmith_core::LoopConfig;
use spec::Spec;
use std::path::PathBuf;

/// Which of the two grammars a config is written in.
///
/// A `bool` did this job until 1.0, and every signature carrying it had to
/// say in prose which way round it was. Both grammars are the same model —
/// the loader reads either — so the choice is a genuine two-valued thing
/// rather than a flag with an obvious default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grammar {
    /// Reads like a brief. Easiest to hand-edit later.
    Markdown,
    /// Terser, closer to the schema.
    Yaml,
}

impl Grammar {
    /// The grammar a file is in, by its extension — the same rule the loader
    /// uses to decide how to read it.
    pub fn of(path: &std::path::Path) -> Self {
        if loopsmith_core::is_markdown(path) {
            Grammar::Markdown
        } else {
            Grammar::Yaml
        }
    }

    /// The extension a file in this grammar carries.
    pub fn extension(self) -> &'static str {
        match self {
            Grammar::Markdown => "md",
            Grammar::Yaml => "yaml",
        }
    }

    pub fn is_markdown(self) -> bool {
        self == Grammar::Markdown
    }
}

/// How an interview ended.
pub enum Outcome {
    /// A config the user asked to have written, already rendered in the
    /// grammar they picked.
    Ready {
        cfg: Box<LoopConfig>,
        text: String,
        grammar: Grammar,
    },
    /// The user left mid-way. `saved` is the draft they asked to keep, if any.
    Quit { saved: Option<PathBuf> },
    /// The user reached the end and chose not to write anything.
    Declined,
}

/// Run the whole interview, starting from `start` (an existing config to
/// revise) or from nothing.
///
/// `grammar` settles the output format in advance; `None` asks for it.
/// `--edit` passes the grammar of the file it loaded, because that file is
/// what gets overwritten — asking there would let someone answer "Markdown"
/// and leave Markdown inside a `.yaml` the loader then refuses to read.
///
/// Nothing is written except a draft the user explicitly asks for at `:quit`.
/// The config is validated before it is returned; a `Ready` config with errors
/// is one the user knowingly chose to write anyway.
pub fn interview(
    io: &mut Io,
    start: Option<LoopConfig>,
    grammar: Option<Grammar>,
) -> Result<Outcome, String> {
    let spec = spec::spec();
    let editing = start.is_some();
    let mut answers = start
        .as_ref()
        .map(|cfg| answers::unpack(&spec, cfg))
        .unwrap_or_default();

    intro(io, editing);

    loop {
        match interview::walk(io, &spec, &mut answers)? {
            interview::Flow::Quit => {
                return Ok(Outcome::Quit {
                    saved: offer_draft(io, &spec, &answers)?,
                })
            }
            interview::Flow::Completed => {}
        }

        let cfg = match answers::assemble_over(&spec, &answers, start.as_ref()) {
            Ok(cfg) => cfg,
            Err(issues) => {
                // Only reachable when an answer is refused by something the
                // loader knows and the spec does not — a duplicate goal name,
                // a graph with a cycle. Going round again is the only useful
                // offer, since the wizard is where those answers live.
                io.heading("Not yet a loop");
                for issue in &issues {
                    let where_ = if issue.key.is_empty() {
                        String::new()
                    } else {
                        format!("{}: ", issue.key)
                    };
                    io.error(&format!("{where_}{}", issue.message));
                }
                if io.ask_bool("Go back through the questions?", &[], true).unwrap_or(false) {
                    continue;
                }
                return Ok(Outcome::Declined);
            }
        };

        // Final gate: the same check `loopsmith validate` prints. Nothing is
        // returned for writing until this passes or the user knowingly
        // overrides it.
        match final_review(io, &cfg) {
            Review::Write => {}
            Review::Edit => continue,
            Review::Leave => return Ok(Outcome::Declined),
        }

        let grammar = match grammar {
            Some(known) => known,
            None => match ask_grammar(io, true) {
                Some(picked) => picked,
                None => {
                    io.note("cancelled — nothing written");
                    return Ok(Outcome::Declined);
                }
            },
        };
        let text = render(&cfg, grammar)?;
        return Ok(Outcome::Ready {
            cfg: Box::new(cfg),
            text,
            grammar,
        });
    }
}

/// On `:quit`, offer to keep what was typed.
///
/// The draft is the answers as far as they go, written as the config document
/// they describe. It may not be a loop yet — that is the point of saving it —
/// so it is written as YAML without being parsed first, and `--edit` reads it
/// back through the ordinary loader like any other file.
fn offer_draft(io: &mut Io, spec: &Spec, answers: &Answers) -> Result<Option<PathBuf>, String> {
    if !io.is_interactive() || answers.is_empty() {
        return Ok(None);
    }
    let choices = vec![
        Choice::new("save", "Save a draft and leave"),
        Choice::new("discard", "Discard everything and leave"),
    ];
    // A nested `:quit` here collapses to discard.
    let pick = io
        .ask_select("Leave the wizard?", &[], &choices, Some(0))
        .unwrap_or_else(|_| "discard".into());
    if pick != "save" {
        return Ok(None);
    }
    let path = PathBuf::from("loop.draft.yaml");
    std::fs::write(&path, answers::draft(spec, answers))
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(Some(path))
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

/// What the reviewer decided about a finished config.
enum Review {
    /// Write it, warnings and all.
    Write,
    /// Go back through the questions.
    Edit,
    /// Leave without writing.
    Leave,
}

/// Run the real validator, print what it found, and decide what happens next.
fn final_review(io: &mut Io, cfg: &LoopConfig) -> Review {
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
        return Review::Write;
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
        return match io.ask_bool("Write the config?", &[], true) {
            Ok(true) => Review::Write,
            _ => Review::Leave,
        };
    }

    let choices = vec![
        Choice::new("edit", "Go back through the questions and fix them"),
        Choice::new("write", "Write it anyway (errors and all)"),
        Choice::new("quit", "Leave without writing"),
    ];
    match io.ask_select(
        &format!("{errors} error(s) block a clean config."),
        &[],
        &choices,
        Some(0),
    ) {
        Ok(a) if a == "edit" => Review::Edit,
        Ok(a) if a == "write" => Review::Write,
        _ => Review::Leave,
    }
}

/// Ask which grammar to write. `None` is the user leaving.
///
/// Both terminal paths ask it — the interview at the end, the expert path
/// before the editor opens — so it lives here rather than once in each.
/// `heading` is the one difference: the interview is inside a run of
/// headed sections and the expert path is not.
pub fn ask_grammar(io: &mut Io, heading: bool) -> Option<Grammar> {
    let choices = vec![
        Choice::new("md", "Markdown — reads like a brief, easiest to hand-edit later"),
        Choice::new("yaml", "YAML — terser, closer to the schema"),
    ];
    if heading {
        io.heading("Output");
    }
    match io.ask_select("Which grammar for the config file?", &[], &choices, Some(0)) {
        Ok(pick) if pick == "md" => Some(Grammar::Markdown),
        Ok(_) => Some(Grammar::Yaml),
        Err(_) => None,
    }
}

/// A config as text, in the grammar asked for.
///
/// Fallible only for YAML: `render_md` cannot fail, and serialising a config
/// that is already in memory can only fail on something pathological.
pub fn render(cfg: &LoopConfig, grammar: Grammar) -> Result<String, String> {
    match grammar {
        Grammar::Markdown => Ok(loopsmith_core::render_md(cfg)),
        Grammar::Yaml => serde_yaml::to_string(cfg).map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_wizard_still_renders_both_grammars() {
        // Draft-saving and the review panel both lean on this: a config that
        // is not finished yet must still be writable as text.
        answers::assemble(&spec::spec(), &Answers::new())
            .expect_err("an empty wizard has nothing to assemble");
        let cfg = loopsmith_core::parse_str("name: unfinished\n", "test")
            .expect("a bare name parses");
        assert!(render(&cfg, Grammar::Markdown).is_ok());
        assert!(render(&cfg, Grammar::Yaml).is_ok());
    }
}
