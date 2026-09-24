//! `loopsmith loop guided` — build a loop in the terminal.
//!
//! This is the third front end onto the exact same config that `loopsmith loop
//! new` hands you as a file and `loopsmith web` paints in a browser. It exists
//! for the machine with no browser and for the person who would rather not
//! start from a blank file.
//!
//! Two ways through it, and the choice is remembered (q57 b, q58 a):
//!
//! - **Guided** walks every question with its explanation in place, offering
//!   the agent CLIs it already found. The questions live in
//!   [`loopsmith_wizard`], shared with the browser.
//! - **Expert** hands you a filled-in config in `$EDITOR` and validates what
//!   comes back. It writes your file, comments and all, rather than
//!   re-rendering it from a struct — which is the point: someone who wants an
//!   editor wants their own formatting kept.
//!
//! What lives here is what only a terminal front end does with the answer:
//! write it back over the file being edited, or scaffold a new loop directory
//! around it and offer to run it.

use loopsmith_core::LoopConfig;
use loopsmith_wizard::preferences::{self, Level};
use loopsmith_wizard::{Choice, Io, Outcome};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Entry point from `cmd::dispatch`.
pub fn execute(
    path: Option<PathBuf>,
    edit: Option<PathBuf>,
    level: Option<Level>,
) -> Result<ExitCode, String> {
    let mut io = Io::new();

    // Starting point: an existing config to revise, or nothing.
    let start = match &edit {
        Some(file) => Some(
            loopsmith_core::load(file)
                .map_err(|e| format!("could not read {} to edit: {e}", file.display()))?,
        ),
        None => None,
    };

    // An edit overwrites the file it read, so the grammar is already settled
    // by that file's own extension.
    let grammar = edit.as_deref().map(loopsmith_core::is_markdown);

    let level = level_for(&mut io, level)?;
    let outcome = match level {
        Level::Novice => loopsmith_wizard::interview(&mut io, start, grammar)?,
        Level::Expert => expert::run(&mut io, start, grammar)?,
    };

    let (cfg, text, markdown) = match outcome {
        Outcome::Ready { cfg, text, markdown } => (cfg, text, markdown),
        Outcome::Quit { saved } => {
            if let Some(p) = saved {
                println!(
                    "\nDraft saved to {}. Resume with:\n  loopsmith loop guided --edit {}",
                    p.display(),
                    p.display()
                );
            } else {
                println!("\nNothing written.");
            }
            return Ok(ExitCode::SUCCESS);
        }
        Outcome::Declined => return Ok(ExitCode::SUCCESS),
    };

    match edit {
        Some(file) => write_back(&mut io, &file, &text),
        None => create_loop(&mut io, &cfg, path, text, markdown),
    }
}

/// Which way through: the flag, then what was remembered, then ask.
///
/// The answer is only remembered when it was asked for. A `--expert` on one
/// run is that run's business; picking expert when asked is a preference.
fn level_for(io: &mut Io, flag: Option<Level>) -> Result<Level, String> {
    if let Some(level) = flag {
        return Ok(level);
    }
    if let Some(level) = preferences::level() {
        io.note(&format!(
            "Using the {} path you picked last time. `--novice` or `--expert` \
             overrides it for this run; `--ask` forgets it.",
            level.as_str()
        ));
        return Ok(level);
    }
    if !io.is_interactive() {
        return Ok(Level::Novice);
    }
    let choices = vec![
        Choice::new("novice", "Walk me through it")
            .noted("every question, with what it is for"),
        Choice::new("expert", "Give me a config in my editor")
            .noted("filled in and validated on the way out"),
    ];
    let pick = io
        .ask_select("How would you like to build this loop?", &[], &choices, Some(0))
        .unwrap_or_else(|_| "novice".into());
    let level = if pick == "expert" {
        Level::Expert
    } else {
        Level::Novice
    };
    preferences::remember(level);
    io.note("Remembered. `--novice` or `--expert` overrides it for one run, `--ask` forgets it.");
    Ok(level)
}

/// A `:quit` at one of the final prompts, after the config is assembled but
/// before it is on disk. Nothing is written; this is a clean exit, not an error.
fn cancelled(io: &Io) -> ExitCode {
    io.note("cancelled — nothing written");
    ExitCode::SUCCESS
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
    println!("\nCheck it:\n  loopsmith loop validate {}", file.display());
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
    let wants_git_default = cfg.execution.graph.nodes.iter().any(|n| n.isolation.needs_worktree());
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
    println!("  check:  loopsmith loop validate {}", config_path.display());
    println!("  plan:   loopsmith loop plan {}", config_path.display());

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
}

/// The expert path: a filled-in config, your editor, and the validator.
mod expert {
    use super::*;

    pub fn run(
        io: &mut Io,
        start: Option<LoopConfig>,
        grammar: Option<bool>,
    ) -> Result<Outcome, String> {
        io.heading("Expert");
        let markdown = match grammar {
            Some(known) => known,
            None => match super::ask_grammar(io)? {
                Some(m) => m,
                None => return Ok(Outcome::Declined),
            },
        };

        let cfg = match start {
            Some(cfg) => cfg,
            None => {
                let name = io.ask_text(
                    "What is this loop called?",
                    &["Lower-case and hyphens travel best; it becomes the directory name."],
                    Some("my-loop"),
                    &|v| {
                        if v.trim().is_empty() {
                            Err("this one is needed".into())
                        } else {
                            Ok(())
                        }
                    },
                ).map_err(|_| "cancelled".to_string())?;
                let purpose = io
                    .ask_text("What is it for, in a sentence?", &[], Some(""), &|_| Ok(()))
                    .unwrap_or_default();
                crate::scaffold::starter_config(&name, &purpose)
            }
        };

        let mut text = loopsmith_wizard::render(&cfg, markdown)?;
        let file = draft_path(markdown);
        loop {
            std::fs::write(&file, &text)
                .map_err(|e| format!("could not write {}: {e}", file.display()))?;
            open_editor(io, &file)?;
            text = std::fs::read_to_string(&file)
                .map_err(|e| format!("could not read {} back: {e}", file.display()))?;

            match loopsmith_core::parse_str(&text, &file.display().to_string()) {
                Ok(cfg) => {
                    if !report(io, &cfg) {
                        continue;
                    }
                    let _ = std::fs::remove_file(&file);
                    return Ok(Outcome::Ready {
                        cfg: Box::new(cfg),
                        text,
                        markdown,
                    });
                }
                Err(e) => {
                    io.error(&format!("that file is not a loop config yet: {e}"));
                    if !io.ask_bool("Open it again?", &[], true).unwrap_or(false) {
                        let _ = std::fs::remove_file(&file);
                        return Ok(Outcome::Declined);
                    }
                }
            }
        }
    }

    /// Print the validator's findings. `true` when the config should be
    /// written — cleanly, or because the author said so anyway.
    fn report(io: &mut Io, cfg: &LoopConfig) -> bool {
        let report = loopsmith_core::validate(cfg);
        if report.issues.is_empty() {
            io.success("config is valid, no warnings");
            return true;
        }
        for issue in &report.issues {
            let sev = match issue.severity {
                loopsmith_core::Severity::Error => io.red("error"),
                loopsmith_core::Severity::Warning => io.yellow("warn "),
            };
            io.println(&format!("  {sev} {}: {}", io.dim(&issue.field), issue.message));
        }
        let errors = report.errors().count();
        if errors == 0 {
            return io.ask_bool("Write it?", &[], true).unwrap_or(false);
        }
        !io.ask_bool(
            &format!("{errors} error(s). Open it again?"),
            &["Answering no writes it as it stands."],
            true,
        )
        .unwrap_or(true)
    }

    /// Where the draft lives while it is being edited. Not in the loop
    /// directory: that does not exist yet on a fresh `guided`, and a failed
    /// edit should leave nothing behind in the one place a person looks.
    fn draft_path(markdown: bool) -> PathBuf {
        let ext = if markdown { "md" } else { "yaml" };
        std::env::temp_dir().join(format!("loopsmith-draft-{}.{ext}", std::process::id()))
    }

    /// `$VISUAL`, then `$EDITOR`, then ask them to do it themselves.
    ///
    /// A machine with neither is common enough to matter — a container, a
    /// stripped CI image — and refusing there would make the expert path
    /// unusable on exactly the machines its users work on.
    fn open_editor(io: &mut Io, file: &Path) -> Result<(), String> {
        let editor = std::env::var("VISUAL")
            .ok()
            .or_else(|| std::env::var("EDITOR").ok())
            .filter(|e| !e.trim().is_empty());

        let Some(editor) = editor else {
            io.note(&format!("No $EDITOR is set. The draft is at {}", file.display()));
            let _ = io.ask_text("Edit it, then press Enter", &[], Some(""), &|_| Ok(()));
            return Ok(());
        };

        // Split on spaces so `code --wait` and `emacsclient -nw` work; the
        // shell is not involved, so nothing here is interpreted.
        let mut parts = editor.split_whitespace();
        let program = parts.next().unwrap_or("vi");
        let status = std::process::Command::new(program)
            .args(parts)
            .arg(file)
            .status()
            .map_err(|e| format!("could not start `{editor}`: {e}"))?;
        if !status.success() {
            io.note(&format!("`{editor}` exited {}", status.code().unwrap_or(-1)));
        }
        Ok(())
    }
}

/// Markdown or YAML, asked before the editor opens so the draft arrives in the
/// grammar it will be written in.
fn ask_grammar(io: &mut Io) -> Result<Option<bool>, String> {
    let choices = vec![
        Choice::new("md", "Markdown — reads like a brief, easiest to hand-edit later"),
        Choice::new("yaml", "YAML — terser, closer to the schema"),
    ];
    match io.ask_select("Which grammar for the config file?", &[], &choices, Some(0)) {
        Ok(pick) => Ok(Some(pick == "md")),
        Err(_) => Ok(None),
    }
}
