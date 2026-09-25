//! `loopsmith watch` — stay resident and run whenever a trigger fires.
//!
//! This is what makes a loop live for weeks rather than for one invocation.

use super::{config_dir, config_file_name, open_store, report_outcome};
use loopsmith_run::RunOptions;
use loopsmith_run::schedule;
use loopsmith_memory::Store;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, max_runs: Option<u32>, check: bool) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load_validated(config).map_err(|e| e.to_string())?;
    let root = config_dir(config);
    let store = open_store(config)?;

    if cfg.execution.triggers.triggers.is_empty()
        || cfg
            .execution
            .triggers
            .triggers
            .iter()
            .all(|t| matches!(t.trigger, loopsmith_core::Trigger::Manual {}))
    {
        return Err(
            "this loop has no non-manual trigger, so `watch` would sleep forever. \n                     Add a cron, interval, file_change, or goal_satisfied trigger to `schedules`."
                .into(),
        );
    }

    let interval = schedule::poll_interval(&cfg.execution.triggers.triggers);
    println!(
        "watching `{}` — {} trigger(s), polling every {}s. Cron is evaluated in UTC.",
        cfg.name,
        cfg.execution.triggers.triggers.len(),
        interval.as_secs()
    );
    for t in &cfg.execution.triggers.triggers {
        println!("  {t:?}");
    }

    if check {
        println!("\n--check: no run performed");
        return Ok(ExitCode::SUCCESS);
    }

    // The success export is written into the loop directory whenever a run
    // meets its bar, so a `file_change` trigger on the root would see it and
    // start another run — which would write it again.
    let mut watcher = schedule::Watcher::ignoring(vec![format!("{}-success", cfg.name)]);
    watcher.prime(&cfg.execution.triggers.triggers, &root);
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

        let now = schedule::now_unix();
        let policy = &cfg.execution.triggers;
        let fired = watcher.poll(&policy.triggers, &root, now, &satisfied);

        // Several triggers can fire in one poll; they start one run between
        // them, at the shallowest depth any of them allows.
        let mut depth: Option<u32> = None;
        let mut why: Vec<String> = Vec::new();
        for f in &fired {
            match watcher.admit(policy, f, now) {
                schedule::Decision::Run { depth: d } => {
                    depth = Some(depth.map_or(d, |x| x.min(d)));
                    why.push(f.describe());
                }
                schedule::Decision::Duplicate { key } => println!(
                    "  skipped: {} — the same firing (key `{key}`) already ran inside the \
                     {}s dedup window",
                    f.describe(),
                    policy.dedup_window_seconds
                ),
                schedule::Decision::DepthCapped { depth: d } => println!(
                    "  refused: {} — it would be run {} in a chain this loop started itself, \
                     and `max_depth` is {}",
                    f.describe(),
                    d + 1,
                    policy.max_depth
                ),
            }
        }

        if let Some(depth) = depth {
            watcher.started(depth, now);
            let run_id = format!("run-{}", loopsmith_memory::now_ms());
            println!(
                "\n[{}] {} — starting {run_id}{}",
                runs + 1,
                why.join("; "),
                if depth > 0 {
                    format!(" (chain depth {depth} of {})", policy.max_depth)
                } else {
                    String::new()
                }
            );

            match loopsmith_run::execute(
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
                    answer_escalations: false,
                },
            ) {
                Ok(out) => report_outcome(&out),
                // A failed run must not kill the watcher; that is the
                // difference between a scheduler and a one-shot.
                Err(e) => eprintln!("run failed: {e}"),
            }

            watcher.finished(schedule::now_unix());
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
