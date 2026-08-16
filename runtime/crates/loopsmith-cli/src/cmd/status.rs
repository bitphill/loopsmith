//! `loopsmith status` — the gate's current rulings for a run.

use super::open_store;
use loopsmith_memory::Store;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, run_id: &str) -> Result<ExitCode, String> {
    let store = open_store(config)?;
    let states = store.goal_states(run_id).map_err(|e| e.to_string())?;
    if states.is_empty() {
        println!("no rulings recorded for run `{run_id}`");
        return Ok(ExitCode::SUCCESS);
    }
    for (target, st) in &states {
        println!(
            "{:<20} {:<14} {}/{} checks  (iteration {})\n  {}",
            target,
            if st.satisfied {
                "SATISFIED"
            } else {
                "not satisfied"
            },
            st.passed,
            st.total,
            st.iteration,
            st.reason
        );
    }
    if let Some(cp) = store.checkpoint(run_id).map_err(|e| e.to_string())? {
        println!("\ncheckpoint: iteration {}", cp.iteration);
    }
    Ok(ExitCode::SUCCESS)
}
