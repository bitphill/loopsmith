//! `loopsmith` — the control plane for self-evolving agent loops.

mod permissions;
mod run;
mod scaffold;

use clap::{Parser, Subcommand};
use loopsmith_memory::Store;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "loopsmith",
    version,
    about = "Plan, run, and gate self-evolving agent loops",
    long_about = "loopsmith owns the parts of an agent loop that must not be a matter of \
opinion: the dependency graph, the persistent ledger, and the gate that decides whether a \
goal is actually satisfied. Models supply judgment; this binary supplies the truth."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new purpose-specific loop at a path.
    New {
        /// Directory for the new loop. Required: a loop owns durable state and
        /// needs a home of its own.
        #[arg(short = 'p', long = "path", value_name = "DIR")]
        path: PathBuf,
        /// Loop name. Defaults to the directory name.
        #[arg(short, long)]
        name: Option<String>,
        /// One line on what this loop is for.
        #[arg(long, default_value = "a loopsmith loop")]
        purpose: String,
        /// Write into a non-empty directory.
        #[arg(long)]
        force: bool,
    },
    /// Check a config against the A–H model.
    Validate {
        config: PathBuf,
        /// Treat warnings as errors.
        #[arg(long)]
        strict: bool,
    },
    /// Show waves, critical path, and predicted speedup without running.
    Plan { config: PathBuf },
    /// Run the loop.
    Run {
        config: PathBuf,
        #[arg(long)]
        run_id: Option<String>,
        /// Plan and log without invoking any provider.
        #[arg(long)]
        dry_run: bool,
    },
    /// Continue a run from its last checkpoint.
    Resume {
        config: PathBuf,
        run_id: String,
    },
    /// Current gate rulings for a run.
    Status {
        config: PathBuf,
        run_id: String,
    },
    /// Print the append-only ledger for a run.
    Ledger {
        config: PathBuf,
        run_id: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Evaluate the gate once against the current working tree.
    Gate {
        config: PathBuf,
        /// Goal name, or `overall`.
        #[arg(long, default_value = "overall")]
        target: String,
        #[arg(long, default_value = ".")]
        workdir: PathBuf,
    },
    /// Report which providers are usable right now.
    Providers { config: PathBuf },
    /// Print the consolidated permission grant this config needs.
    Permissions {
        config: PathBuf,
        /// Merge into .claude/settings.local.json instead of printing.
        #[arg(long)]
        write: Option<PathBuf>,
    },
    /// Serve the local MCP server on stdio.
    Mcp {
        #[arg(long, default_value = "state")]
        state: PathBuf,
    },
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Directory holding the config. `Path::parent()` yields `Some("")` for a
/// bare filename like `loop.yaml`, and an empty path is not a usable working
/// directory — spawning into it fails with ENOENT. Normalise to `.`.
fn config_dir(config: &Path) -> PathBuf {
    match config.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn open_store(config: &Path) -> Result<loopsmith_memory::SledStore, String> {
    loopsmith_memory::open(config_dir(config).join("state")).map_err(|e| e.to_string())
}

fn real_main() -> Result<ExitCode, String> {
    let cli = Cli::parse();

    match cli.command {
        Command::New {
            path,
            name,
            purpose,
            force,
        } => {
            let name = name.unwrap_or_else(|| scaffold::name_from_path(&path));
            let s = scaffold::scaffold(&scaffold::NewLoopArgs {
                path: path.clone(),
                name: name.clone(),
                purpose,
                force,
            })
            .map_err(|e| e.to_string())?;
            println!("Created loop `{name}` at {}", path.display());
            for f in &s.written {
                println!("  {}", f.display());
            }
            println!(
                "\nNext: finish `pre_execution` in {}/loop.yaml, then run\n  loopsmith validate {}/loop.yaml",
                path.display(),
                path.display()
            );
            Ok(ExitCode::SUCCESS)
        }

        Command::Validate { config, strict } => {
            let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
            let report = loopsmith_core::validate(&cfg);
            if report.issues.is_empty() {
                println!("ok: {} is valid", config.display());
                return Ok(ExitCode::SUCCESS);
            }
            print!("{}", report.render());
            let errors = report.errors().count();
            let warnings = report.warnings().count();
            println!("\n{errors} error(s), {warnings} warning(s)");
            if report.has_errors() || (strict && warnings > 0) {
                Ok(ExitCode::FAILURE)
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }

        Command::Plan { config } => {
            let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
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

        Command::Run {
            config,
            run_id,
            dry_run,
        } => {
            let cfg = loopsmith_core::load_validated(&config).map_err(|e| e.to_string())?;
            let store = open_store(&config)?;
            let workdir = config_dir(&config);
            let run_id = run_id.unwrap_or_else(|| format!("run-{}", loopsmith_memory::now_ms()));
            let out = run::execute(
                &cfg,
                &store,
                &run::RunOptions {
                    run_id,
                    workdir,
                    dry_run,
                    resume: false,
                },
            )?;
            report_outcome(&out);
            Ok(if out.stop.is_success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }

        Command::Resume { config, run_id } => {
            let cfg = loopsmith_core::load_validated(&config).map_err(|e| e.to_string())?;
            let store = open_store(&config)?;
            let workdir = config_dir(&config);
            let out = run::execute(
                &cfg,
                &store,
                &run::RunOptions {
                    run_id,
                    workdir,
                    dry_run: false,
                    resume: true,
                },
            )?;
            report_outcome(&out);
            Ok(if out.stop.is_success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }

        Command::Status { config, run_id } => {
            let store = open_store(&config)?;
            let states = store.goal_states(&run_id).map_err(|e| e.to_string())?;
            if states.is_empty() {
                println!("no rulings recorded for run `{run_id}`");
                return Ok(ExitCode::SUCCESS);
            }
            for (target, st) in &states {
                println!(
                    "{:<20} {:<14} {}/{} checks  (iteration {})\n  {}",
                    target,
                    if st.satisfied { "SATISFIED" } else { "not satisfied" },
                    st.passed,
                    st.total,
                    st.iteration,
                    st.reason
                );
            }
            if let Some(cp) = store.checkpoint(&run_id).map_err(|e| e.to_string())? {
                println!("\ncheckpoint: iteration {}", cp.iteration);
            }
            Ok(ExitCode::SUCCESS)
        }

        Command::Ledger {
            config,
            run_id,
            limit,
        } => {
            let store = open_store(&config)?;
            let entries = store.ledger(&run_id).map_err(|e| e.to_string())?;
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

        Command::Gate {
            config,
            target,
            workdir,
        } => {
            let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
            let ev = run::collect_evidence(&workdir, Some(&workdir.join("metrics.json")));
            let v = loopsmith_gate::evaluate(&cfg, &target, &ev);
            println!("{}: {}", v.target, if v.satisfied { "SATISFIED" } else { "NOT SATISFIED" });
            println!("{}\n", v.reason);
            for c in &v.checks {
                println!(
                    "  [{}]{} {} — {}",
                    if c.passed { "pass" } else { "FAIL" },
                    if c.blocking { "" } else { " (advisory)" },
                    c.name,
                    c.evidence
                );
            }
            Ok(if v.satisfied {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }

        Command::Providers { config } => {
            let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
            if cfg.providers.providers.is_empty() {
                println!("no providers declared");
                return Ok(ExitCode::SUCCESS);
            }
            for p in &cfg.providers.providers {
                let av = loopsmith_provider::availability(p);
                println!(
                    "{:<12} {:<12} {}",
                    p.id,
                    if av.ok() { "available" } else { "unavailable" },
                    if av.ok() {
                        p.command.clone()
                    } else {
                        av.why_not()
                    }
                );
            }
            Ok(ExitCode::SUCCESS)
        }

        Command::Permissions { config, write } => {
            let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
            let grant = permissions::required(&cfg);
            match write {
                Some(path) => {
                    let merged = permissions::merge_into(&path, &grant).map_err(|e| e.to_string())?;
                    println!("wrote {} permission rule(s) to {}", grant.len(), path.display());
                    println!("{merged}");
                }
                None => {
                    println!("{}", permissions::render(&grant));
                }
            }
            Ok(ExitCode::SUCCESS)
        }

        Command::Mcp { state } => {
            let store = loopsmith_memory::open(state).map_err(|e| e.to_string())?;
            let server = loopsmith_mcp::Server::new(store);
            let stdin = std::io::stdin();
            server
                .serve(stdin.lock(), std::io::stdout())
                .map_err(|e| e.to_string())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn report_outcome(out: &run::RunOutcome) {
    println!("\nrun {} finished after {} iteration(s)", out.run_id, out.iterations);
    println!("stop reason: {}", out.stop.describe());
    if out.tokens_used > 0 || out.cost_usd > 0.0 {
        println!(
            "spend:       {} tokens, ${:.4}",
            out.tokens_used, out.cost_usd
        );
    }
    for (target, v) in &out.verdicts {
        println!(
            "  {:<20} {:<14} {}/{}",
            target,
            if v.satisfied { "SATISFIED" } else { "not satisfied" },
            v.passed,
            v.total
        );
    }
    if !out.stop.is_success() {
        println!(
            "\nThis run did not meet the bar. The ledger holds what was tried:\n  loopsmith ledger <config> {}",
            out.run_id
        );
    }
}
