//! `loopsmith validate` — check a config against the A–H model.

use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, strict: bool) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let report = loopsmith_core::validate(&cfg);
    if report.issues.is_empty() {
        println!("ok: {} is valid", config.display());
        return Ok(ExitCode::SUCCESS);
    }
    print!("{}", report.render());
    let errors = report.errors().count();
    let warnings = report.warnings().count();
    println!("\n{errors} error(s), {warnings} warning(s)");
    if report.has_errors() || (strict && warnings > 0) {
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}
