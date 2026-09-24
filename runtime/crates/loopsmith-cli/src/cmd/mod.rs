//! One module per subcommand, plus the three helpers they share.
//!
//! Every command returns `Result<ExitCode, String>` rather than exiting, so a
//! command is testable and the exit code is decided in one place.

use crate::cli::{Command, LoopAction, MemoryAction, RunAction, SkillsAction};
use loopsmith_run::RunOutcome;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub mod convert;
pub mod doctor;
pub mod gate;
pub mod ledger;
pub mod mcp;
pub mod memory;
pub mod new;
pub mod permissions;
pub mod plan;
pub mod proposals;
pub mod providers;
pub mod prune;
pub mod resume;
pub mod run;
pub mod schedule;
pub mod skills;
pub mod status;
pub mod migrate;
mod validate;
pub mod watch;
#[cfg(feature = "web")]
pub mod web;

/// Directory holding the config. `Path::parent()` yields `Some("")` for a
/// bare filename like `loop.yaml`, and an empty path is not a usable working
/// directory — spawning into it fails with ENOENT. Normalise to `.`.
pub fn config_dir(config: &Path) -> PathBuf {
    match config.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// The config's own file name, for scripts generated inside the loop
/// directory. They `cd` there first, so a relative name is what they want.
pub fn config_file_name(config: &Path) -> String {
    config
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("loop.yaml")
        .to_string()
}

pub fn open_store(config: &Path) -> Result<loopsmith_memory::SledStore, String> {
    loopsmith_memory::open(config_dir(config).join("state")).map_err(|e| e.to_string())
}

pub fn report_outcome(out: &RunOutcome) {
    println!(
        "\nrun {} finished after {} iteration(s)",
        out.run_id, out.iterations
    );
    println!("outcome:     {}", out.state);
    println!("stop reason: {}", out.stop.describe());
    if out.tokens_used > 0 || out.cost_usd > 0.0 {
        println!(
            "spend:       {} tokens{}, ${:.4}",
            out.tokens_used,
            if out.tokens_estimated {
                " (estimated: no provider reported usage)"
            } else {
                ""
            },
            out.cost_usd
        );
    }
    for a in &out.alerts {
        println!("alert:       `{}` {} (iteration {})", a.id, a.message, a.iteration);
    }
    if let Some(line) = out.baseline.describe() {
        println!("baseline:    {line}");
    }
    if out.proposals > 0 {
        println!(
            "proposals:   {} written — review them, the loop cannot apply them itself",
            out.proposals
        );
    }
    for (target, v) in &out.verdicts {
        println!(
            "  {:<20} {:<14} {}/{}",
            target,
            if v.satisfied {
                "SATISFIED"
            } else {
                "not satisfied"
            },
            v.passed,
            v.total
        );
    }
    if let Some(p) = &out.log_path {
        println!("log:         {}", p.display());
    }
    if let Some(p) = &out.export_path {
        println!(
            "reusable:    {}\n             the config that converged, its evidence, and its artifacts",
            p.display()
        );
    }
    if !out.stop.is_success() {
        println!(
            "\nThis run did not meet the bar. The ledger holds what was tried:\n  loopsmith run ledger <config> {}",
            out.run_id
        );
    }
}

/// Route a resolved command to its module.
pub fn dispatch(command: Command) -> Result<ExitCode, String> {
    match command {
        Command::Loop { action } => loop_noun(action),
        Command::Run { action } => run_noun(action),
        Command::Memory { action } => match action {
            MemoryAction::List { config, namespace } => memory::list(&config, namespace.as_deref()),
            MemoryAction::Promote {
                config,
                namespace,
                key,
            } => memory::promote(&config, &namespace, &key),
            MemoryAction::Forget {
                config,
                namespace,
                key,
            } => memory::forget(&config, &namespace, &key),
        },
        Command::Skills { action } => match action {
            SkillsAction::List { config, all } => skills::list(&config, all),
            SkillsAction::Search {
                terms,
                min_stars,
                limit,
            } => skills::search(&terms, min_stars, limit),
            SkillsAction::Acquire { config, name } => skills::acquire(&config, &name),
            SkillsAction::Install { config } => skills::install(&config),
            SkillsAction::Scores { config } => skills::scores(&config),
        },
        Command::Doctor { config } => doctor::execute(config.as_deref()),
        Command::Providers { config } => providers::execute(&config),
        Command::Mcp { state } => mcp::execute(state),
        #[cfg(feature = "web")]
        Command::Web { port, no_open } => web::execute(port, no_open),
        // Built without the `web` feature: say which flag brings it back
        // rather than pretending the command does not exist.
        #[cfg(not(feature = "web"))]
        Command::Web { .. } => Err("this build has no web UI. Rebuild with the `web` feature: \
             `cargo install loopsmith` (it is on by default), or \
             `cargo build --features web` from a checkout."
            .into()),
    }
}

/// `loopsmith loop …` — the config file itself.
fn loop_noun(action: LoopAction) -> Result<ExitCode, String> {
    match action {
        LoopAction::New {
            path,
            name,
            purpose,
            force,
            config_file,
            config_stdin,
            markdown,
            git,
        } => new::execute(new::NewArgs {
            path,
            name,
            purpose,
            force,
            config_file,
            config_stdin,
            markdown,
            git,
        }),
        LoopAction::Guided {
            path,
            edit,
            novice,
            expert,
            ask,
        } => {
            // A flag is this run's business; `--ask` is the one that changes
            // what is remembered, by throwing the answer away so the question
            // comes back.
            if ask {
                loopsmith_wizard::preferences::forget();
            }
            let level = match (novice, expert) {
                (true, _) => Some(loopsmith_wizard::preferences::Level::Novice),
                (_, true) => Some(loopsmith_wizard::preferences::Level::Expert),
                _ => None,
            };
            crate::guided::execute(path, edit, level)
        }
        LoopAction::Validate { config, strict } => validate::execute(&config, strict),
        LoopAction::Plan { config } => plan::execute(&config),
        LoopAction::Convert {
            config,
            out,
            to_yaml,
        } => convert::execute(&config, out, to_yaml),
        LoopAction::Migrate {
            config,
            check,
            write,
        } => migrate::execute(&config, check, write),
        LoopAction::Permissions { config, write } => {
            permissions::execute(&config, write.as_deref())
        }
    }
}

/// `loopsmith run …` — one run of a loop.
fn run_noun(action: RunAction) -> Result<ExitCode, String> {
    match action {
        RunAction::Start {
            config,
            run_id,
            dry_run,
            no_acquire,
            verbose,
        } => run::execute(&config, run_id, dry_run, no_acquire, verbose),
        RunAction::Resume {
            config,
            run_id,
            verbose,
            answer,
        } => resume::execute(&config, run_id, verbose, answer),
        RunAction::Status { config, run_id } => status::execute(&config, &run_id),
        RunAction::Ledger {
            config,
            run_id,
            limit,
        } => ledger::execute(&config, &run_id, limit),
        RunAction::Gate {
            config,
            target,
            workdir,
        } => gate::execute(&config, &target, &workdir),
        RunAction::Watch {
            config,
            max_runs,
            check,
        } => watch::execute(&config, max_runs, check),
        RunAction::Schedule { config, install } => schedule::execute(&config, install),
        RunAction::Proposals { config, run_id } => proposals::execute(&config, &run_id),
        RunAction::Prune { config } => prune::execute(&config),
    }
}
