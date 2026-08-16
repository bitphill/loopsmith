//! `loopsmith proposals` — what the loop wants changed about itself.

use super::open_store;
use loopsmith_memory::Store;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, run_id: &str) -> Result<ExitCode, String> {
    let store = open_store(config)?;
    let all = store.proposals(run_id).map_err(|e| e.to_string())?;
    if all.is_empty() {
        println!("no proposals for run `{run_id}`");
        return Ok(ExitCode::SUCCESS);
    }
    for p in all {
        println!("[{:?}] {} (iteration {})", p.kind, p.subject, p.iteration);
        println!("  {}", p.rationale);
        if let Some(patch) = &p.patch {
            println!("  suggested: {patch}");
        }
    }
    println!("\nApply these by editing the config yourself. The loop cannot.");
    Ok(ExitCode::SUCCESS)
}
