//! `loopsmith watch` — stay resident and run whenever a trigger fires.
//!
//! This is what makes a loop live for weeks rather than for one invocation.

use super::{config_dir, config_file_name, open_store, report_outcome};
use crate::run::RunOptions;
use crate::schedule;
use loopsmith_memory::Store;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, max_runs: Option<u32>, check: bool) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load_validated(config).map_err(|e| e.to_string())?;
    let root = config_dir(config);
    let store = open_store(config)?;

    if cfg.schedules.is_empty()
        || cfg
            .schedules
            .iter()
            .all(|t| matches!(t, loopsmith_core::Trigger::Manual))
    {
        return Err(
            "this loop has no non-manual trigger, so `watch` would sleep forever. \n                     Add a cron, interval, file_change, or goal_satisfied trigger to `schedules`."
                .into(),
        );
    }

    let interval = schedule::poll_interval(&cfg.schedules);
    println!(
        "watching `{}` — {} trigger(s), polling every {}s. Cron is evaluated in UTC.",
        cfg.name,
        cfg.schedules.len(),
        interval.as_secs()
    );
    for t in &cfg.schedules {
        println!("  {t:?}");
    }

    if check {
        println!("\n--check: no run performed");
        return Ok(ExitCode::SUCCESS);
    }

    let mut watcher = schedule::Watcher::new();
    watcher.prime(&cfg.schedules, &root);
    let mut runs = 0u32;

    loop {
        // Goal state feeds the goal_satisfied trigger; read it fresh so a run
        // started elsewhere still counts.
        let satisfied: BTreeMap<String, bool> = store
            .runs()
            .unwrap_or_default()
            .last()
            .and_then(|r| store.goal_states(r).ok())
            .map(|m| m.into_iter().map(|(k, v)| (k, v.satisfied)).collect())
            .unwrap_or_default();

        let fired = watcher.poll(&cfg.schedules, &root, schedule::now_unix(), &satisfied);
        if !fired.is_empty() {
            let why: Vec<String> = fired.iter().map(|f| f.describe()).collect();
            let run_id = format!("run-{}", loopsmith_memory::now_ms());
            println!("\n[{}] {} — starting {run_id}", runs + 1, why.join("; "));

            match crate::run::execute(
                &cfg,
                &store,
                &RunOptions {
                    run_id,
                    workdir: root.clone(),
                    dry_run: false,
                    resume: false,
                    acquire_skills: true,
                    // A resident watcher already prints to the terminal; the
                    // per-run log file is where the detail belongs.
                    verbose: false,
                    config_file: config_file_name(config),
                },
            ) {
                Ok(out) => report_outcome(&out),
                // A failed run must not kill the watcher; that is the
                // difference between a scheduler and a one-shot.
                Err(e) => eprintln!("run failed: {e}"),
            }

            runs += 1;
            if let Some(limit) = max_runs {
                if runs >= limit {
                    println!("\nreached --max-runs {limit}; exiting");
                    return Ok(ExitCode::SUCCESS);
                }
            }
        }
        std::thread::sleep(interval);
    }
}
