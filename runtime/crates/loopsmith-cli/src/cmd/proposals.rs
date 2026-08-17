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
    let now = loopsmith_util::now_ms();
    let mut stale = 0usize;
    for p in all {
        let expired = p.is_expired(now);
        if expired {
            stale += 1;
        }
        println!(
            "[{:?}] {} (iteration {}, {}{})",
            p.kind,
            p.subject,
            p.iteration,
            age(now, p.created_ms),
            if expired { ", stale" } else { "" }
        );
        println!("  {}", p.rationale);
        if let Some(patch) = &p.patch {
            println!("  suggested: {patch}");
        }
    }
    println!("\nApply these by editing the config yourself. The loop cannot.");
    if stale > 0 {
        // Reported, never deleted. A proposal is a record of what the loop
        // wanted at a moment, and the moment is what went stale — the record is
        // still the only account of why the loop asked.
        println!(
            "{stale} of these are stale: they describe a graph or a skill set that has \
             probably been edited since. Read them as history, not as a to-do list."
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Coarse age, because the exact millisecond a proposal was written has never
/// been the question a reviewer is asking.
fn age(now_ms: u64, created_ms: u64) -> String {
    let secs = now_ms.saturating_sub(created_ms) / 1000;
    match secs {
        s if s < 90 => "just now".into(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 172_800 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}
