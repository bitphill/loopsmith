//! `loopsmith providers` — which providers are usable right now.

use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    if cfg.providers.providers.is_empty() {
        println!("no providers declared");
        return Ok(ExitCode::SUCCESS);
    }
    for p in &cfg.providers.providers {
        let av = loopsmith_provider::availability(p);
        println!(
            "{:<12} {:<12} {}",
            p.id,
            if av.ok() { "available" } else { "unavailable" },
            if av.ok() {
                p.command.clone()
            } else {
                av.why_not()
            }
        );
    }
    Ok(ExitCode::SUCCESS)
}
