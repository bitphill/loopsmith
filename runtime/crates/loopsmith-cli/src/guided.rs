//! `loopsmith guided` / `loopsmith --guided` — build a loop by answering
//! questions in the terminal, one field at a time.
//!
//! This is the third front end onto the exact same config that `loopsmith new`
//! hands you as a file and `loopsmith web` paints in a browser. It exists for
//! the machine with no browser and the person who would rather not open a text
//! editor: over SSH, in a bare TTY, it walks every section with the field's own
//! explanation in place, offers the agent CLIs it already found, and writes
//! nothing until the whole thing passes `loopsmith_core::validate`.
//!
//! The questions live in [`loopsmith_wizard`], shared with the browser. What
//! lives here is what only a terminal front end does with the answer: write it
//! back over the file being edited, or scaffold a new loop directory around it
//! and offer to run it.

use loopsmith_core::LoopConfig;
use loopsmith_wizard::{Io, Outcome};
use std::path::PathBuf;
use std::process::ExitCode;

/// Entry point from `cmd::dispatch`.
pub fn execute(path: Option<PathBuf>, edit: Option<PathBuf>) -> Result<ExitCode, String> {
    let mut io = Io::new();

    // Starting point: an existing config to revise, or a blank skeleton.
    let start = match &edit {
        Some(file) => Some(loopsmith_core::load(file).map_err(|e| {
            format!("could not read {} to edit: {e}", file.display())
        })?),
        None => None,
    };

    let (cfg, text, markdown) = match loopsmith_wizard::interview(&mut io, start)? {
        Outcome::Ready { cfg, text, markdown } => (cfg, text, markdown),
        Outcome::Quit { saved } => {
            if let Some(p) = saved {
                println!("\nDraft saved to {}. Resume with:\n  loopsmith guided --edit {}", p.display(), p.display());
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
}
