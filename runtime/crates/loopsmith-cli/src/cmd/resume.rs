//! `loopsmith resume` — continue a run from its last checkpoint.

use super::config_dir;
use loopsmith_run::RunOptions;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(
    config: &Path,
    run_id: String,
    verbose: bool,
    answer: bool,
) -> Result<ExitCode, String> {
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
            answer_escalations: answer,
        },
    )?;
    Ok(super::run::exit_code(&out))
}
