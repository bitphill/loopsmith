//! `loopsmith resume` — continue a run from its last checkpoint.

use super::config_dir;
use crate::run::RunOptions;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, run_id: String, verbose: bool) -> Result<ExitCode, String> {
    let out = super::run::start(
        config,
        RunOptions {
            run_id,
            workdir: config_dir(config),
            dry_run: false,
            resume: true,
            acquire_skills: true,
            verbose,
            config_file: super::config_file_name(config),
        },
    )?;
    Ok(super::run::exit_code(&out))
}
