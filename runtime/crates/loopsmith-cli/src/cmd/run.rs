//! `loopsmith run` — one supervised pass of the loop.

use super::{config_dir, config_file_name, open_store, report_outcome};
use crate::run::{RunOptions, RunOutcome};
use std::path::Path;
use std::process::ExitCode;

pub fn execute(
    config: &Path,
    run_id: Option<String>,
    dry_run: bool,
    no_acquire: bool,
    verbose: bool,
) -> Result<ExitCode, String> {
    let run_id = run_id.unwrap_or_else(|| format!("run-{}", loopsmith_memory::now_ms()));
    let out = start(
        config,
        RunOptions {
            run_id,
            workdir: config_dir(config),
            dry_run,
            resume: false,
            acquire_skills: !no_acquire,
            verbose,
            config_file: config_file_name(config),
        },
    )?;
    Ok(exit_code(&out))
}

/// Load, validate, open the store, and execute. Shared with `resume`, which is
/// the same operation with a different starting checkpoint.
pub fn start(config: &Path, opts: RunOptions) -> Result<RunOutcome, String> {
    let cfg = loopsmith_core::load_validated(config).map_err(|e| e.to_string())?;
    let store = open_store(config)?;
    let out = crate::run::execute(&cfg, &store, &opts)?;
    report_outcome(&out);
    Ok(out)
}

/// A run that did not meet its bar exits non-zero, so a scheduler or CI step
/// notices without having to parse the output.
pub fn exit_code(out: &RunOutcome) -> ExitCode {
    if out.stop.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
