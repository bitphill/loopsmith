//! `loopsmith permissions` — the consolidated grant this config needs.

use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, write: Option<&Path>) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let grant = crate::permissions::required(&cfg);
    match write {
        Some(path) => {
            let merged =
                crate::permissions::merge_into(path, &grant).map_err(|e| e.to_string())?;
            println!(
                "wrote {} permission rule(s) to {}",
                grant.len(),
                path.display()
            );
            println!("{merged}");
        }
        None => println!("{}", crate::permissions::render(&grant)),
    }
    Ok(ExitCode::SUCCESS)
}
