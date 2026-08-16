//! `loopsmith ledger` — the append-only audit trail for a run.

use super::open_store;
use loopsmith_memory::Store;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, run_id: &str, limit: usize) -> Result<ExitCode, String> {
    let store = open_store(config)?;
    let entries = store.ledger(run_id).map_err(|e| e.to_string())?;
    let start = entries.len().saturating_sub(limit);
    for e in &entries[start..] {
        println!(
            "[it {:>3}] {:<20} {}{}",
            e.iteration,
            format!("{:?}", e.kind),
            e.node_id
                .as_ref()
                .map(|n| format!("{n}: "))
                .unwrap_or_default(),
            e.detail
        );
    }
    Ok(ExitCode::SUCCESS)
}
