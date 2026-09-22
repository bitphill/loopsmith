//! `loopsmith status` — where a run is, and the gate's current rulings for it.

use super::open_store;
use loopsmith_memory::Store;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, run_id: &str) -> Result<ExitCode, String> {
    let store = open_store(config)?;
    let states = store.goal_states(run_id).map_err(|e| e.to_string())?;
    // A run can stop before the gate ever rules — an entry gate, an
    // unschedulable graph — and its state is exactly what the reader wants
    // then, so this does not return early.
    if states.is_empty() {
        println!("no rulings recorded for run `{run_id}`");
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
        println!("state:      {}", describe_state(&cp));
        for e in &cp.escalations {
            println!("  waiting on a human: {}", e.question);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The lifecycle line. A checkpoint from before runs had states is a run that
/// closed — the old engine had no other way to stop — so it says that rather
/// than "unknown".
fn describe_state(cp: &loopsmith_memory::Checkpoint) -> String {
    use loopsmith_run::RunState;
    let state = cp.state.as_deref().and_then(RunState::parse);
    let outcome = cp.outcome.as_deref().and_then(RunState::parse);
    match (state, outcome) {
        (Some(RunState::Closed), Some(outcome)) => format!("closed ({outcome})"),
        (Some(state), _) => state.to_string(),
        (None, _) => RunState::Closed.to_string(),
    }
}
