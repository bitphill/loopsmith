//! `loopsmith prune` — remove the git worktrees this loop created.

use super::config_dir;
use crate::worktree;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let root = config_dir(config);
    let mut removed = 0;
    for node in cfg.graph.nodes.iter().filter(|n| n.isolated) {
        let iso = worktree::create(&root, &node.id, "prune");
        if matches!(iso, worktree::Isolation::Worktree { .. }) {
            worktree::remove(&root, &iso);
            println!("removed worktree for `{}`", node.id);
            removed += 1;
        }
    }
    if removed == 0 {
        println!("no worktrees to remove");
    }
    Ok(ExitCode::SUCCESS)
}
