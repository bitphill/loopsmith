//! `loopsmith plan` — waves, critical path, and predicted speedup.

use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let plan = loopsmith_graph::plan(&cfg.graph).map_err(|e| e.to_string())?;
    println!("loop: {}", cfg.name);
    println!("\nWaves ({} total):", plan.waves.len());
    for w in &plan.waves {
        println!("  {:>2}. {}", w.index + 1, w.nodes.join(", "));
    }
    println!(
        "\nCritical path ({:.1} cost): {}",
        plan.critical_path_cost,
        plan.critical_path.join(" -> ")
    );
    println!("Total work cost:     {:.1}", plan.total_cost);
    println!("Parallel fraction p: {:.3}", plan.parallel_fraction);
    println!("Concurrency chosen:  {}", plan.concurrency);
    println!(
        "Predicted speedup:   {:.2}x  (ceiling {:.2}x at infinite workers)",
        plan.predicted_speedup, plan.speedup_ceiling
    );

    let risky = loopsmith_graph::unisolated_parallel_writers(&cfg.graph, &plan.waves);
    if !risky.is_empty() {
        println!(
            "\nwarning: builder nodes may run in parallel without worktree isolation: {}",
            risky.join(", ")
        );
    }
    Ok(ExitCode::SUCCESS)
}
