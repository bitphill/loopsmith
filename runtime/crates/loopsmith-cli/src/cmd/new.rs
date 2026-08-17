//! `loopsmith new` — scaffold a purpose-specific loop.
//!
//! Config can be handed over whole (`--config-file`, `--config-stdin`) or left
//! as the starter for you to edit. Nothing here ever blocks on a prompt: a
//! command that waits for a keypress cannot be run from a script, a Makefile,
//! or the agent that is setting the loop up for you.

use crate::scaffold::{self, ProvidedConfig};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

pub struct NewArgs {
    pub path: PathBuf,
    pub name: Option<String>,
    pub purpose: String,
    pub force: bool,
    pub config_file: Option<PathBuf>,
    pub config_stdin: bool,
    pub markdown: bool,
}

pub fn execute(args: NewArgs) -> Result<ExitCode, String> {
    let name = args
        .name
        .clone()
        .unwrap_or_else(|| scaffold::name_from_path(&args.path));

    let config = read_provided(&args)?;

    let s = scaffold::scaffold(&scaffold::NewLoopArgs {
        path: args.path.clone(),
        name: name.clone(),
        purpose: args.purpose,
        force: args.force,
        config,
    })
    .map_err(|e| e.to_string())?;

    let dir = args.path.display();
    let cfg = format!("{dir}/{}", s.config_file);

    println!("Created loop `{name}` at {dir}\n");
    for f in &s.written {
        println!("  {}", f.display());
    }

    println!("\n── configure ──────────────────────────────────────────────");
    println!("1. Edit the config:\n     {cfg}");
    println!(
        "2. Finish `pre_execution`. Every step must say `done: true` before the\n   \
         loop will run — automating a process you cannot describe produces fast,\n   \
         confident garbage."
    );
    // `set` on Windows, `export` elsewhere. Printing the wrong one is a small
    // thing that tells the reader this tool was not written with their machine in
    // mind, and the correct spelling costs one branch.
    let set_var = if loopsmith_util::platform::Os::detect().is_windows() {
        "set OPENAI_API_KEY=..."
    } else {
        "export OPENAI_API_KEY=..."
    };
    println!(
        "3. Export the keys your providers name under `requires_env`:\n     \
         {set_var}\n   \
         loopsmith checks only that these variables EXIST; it never reads their\n   \
         values, so a key cannot reach a prompt, a log, or the ledger."
    );
    println!(
        "\n   ⚠  Never paste an API key into a chat window, a config file, or an\n   \
         issue. If one ends up somewhere it should not be, rotate it — deleting\n   \
         the message is not enough."
    );

    println!("\n── check ──────────────────────────────────────────────────");
    println!("  loopsmith validate {cfg}");
    println!("  loopsmith plan     {cfg}");

    // Both launchers are always written, so name the one this host can actually
    // run. Telling a Windows user to run `run.sh` sends them to a file cmd.exe
    // cannot execute, while the `.cmd` sitting beside it would have worked.
    let (run, resume) = if loopsmith_util::platform::Os::detect().is_windows() {
        ("run.cmd", "resume.cmd")
    } else {
        ("run.sh", "resume.sh")
    };
    // Joined onto the real path, not interpolated with a `/`, so the separator is
    // this platform's.
    println!("\n── run ────────────────────────────────────────────────────");
    println!("  {}", args.path.join(run).display());
    println!("\nIf it stops before it is done, resume from the last checkpoint:");
    println!("  {} <run-id>", args.path.join(resume).display());

    Ok(ExitCode::SUCCESS)
}

/// Read a config handed over whole, if one was.
fn read_provided(args: &NewArgs) -> Result<Option<ProvidedConfig>, String> {
    match (&args.config_file, args.config_stdin) {
        (Some(_), true) => {
            Err("pass either --config-file or --config-stdin, not both".into())
        }
        (Some(path), false) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("could not read {}: {e}", path.display()))?;
            Ok(Some(ProvidedConfig {
                // The file's own extension decides the grammar. Guessing from
                // content would mean a stray `#` heading silently reinterpreting
                // someone's YAML.
                markdown: args.markdown || loopsmith_core::is_markdown(path),
                text,
            }))
        }
        (None, true) => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| format!("could not read stdin: {e}"))?;
            if text.trim().is_empty() {
                return Err("--config-stdin was given but stdin was empty".into());
            }
            Ok(Some(ProvidedConfig {
                markdown: args.markdown,
                text,
            }))
        }
        (None, false) => Ok(None),
    }
}
