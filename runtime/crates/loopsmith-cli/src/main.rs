//! `loopsmith` — the control plane for self-evolving agent loops.

mod judgment;
mod permissions;
mod schedule;
mod run;
mod scaffold;
mod worktree;

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
        /// Do not acquire missing sub-agents; nodes run without them.
        #[arg(long)]
        no_acquire: bool,
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
    /// Stay resident and run the loop whenever a trigger fires. This is what
    /// makes a loop live for weeks rather than for one invocation.
    Watch {
        config: PathBuf,
        /// Stop after this many runs. Omit to run until interrupted.
        #[arg(long)]
        max_runs: Option<u32>,
        /// Report what would fire, then exit without running anything.
        #[arg(long)]
        check: bool,
    },
    /// Hand the schedule to the operating system so it survives a reboot.
    Schedule {
        config: PathBuf,
        /// Write the launchd agent or crontab line instead of printing it.
        #[arg(long)]
        install: bool,
    },
    /// Discover, install, and score sub-agents.
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Show what the loop wants changed about itself. It cannot apply these.
    Proposals { config: PathBuf, run_id: String },
    /// Remove the git worktrees this loop created.
    Prune { config: PathBuf },
    /// Serve the local MCP server on stdio.
    Mcp {
        #[arg(long, default_value = "state")]
        state: PathBuf,
    },
}

#[derive(Subcommand)]
enum SkillsAction {
    /// Sub-agents already visible to this loop.
    List {
        config: PathBuf,
        /// Include the ~/.claude/skills directory, which is usually large.
        #[arg(long)]
        all: bool,
    },
    /// Search claudemarketplaces.com and the skills CLI.
    Search {
        /// Words to match against repo, description, categories, keywords.
        terms: Vec<String>,
        #[arg(long, default_value_t = 100)]
        min_stars: u64,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Install a skill into the loop's quarantine directory.
    Acquire {
        config: PathBuf,
        /// Skill name, or an `owner/repo@skill` spec.
        name: String,
    },
    /// Rank sub-agents by the gate outcomes that followed their use.
    Scores { config: PathBuf },
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
            no_acquire,
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
                    acquire_skills: !no_acquire,
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
                    acquire_skills: true,
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
            // A one-shot gate check has no judge run behind it, so subjective
            // checks correctly report that no judgment was recorded.
            let ev = run::collect_evidence(&workdir, Some(&workdir.join("metrics.json")), vec![]);
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

        Command::Watch {
            config,
            max_runs,
            check,
        } => {
            let cfg = loopsmith_core::load_validated(&config).map_err(|e| e.to_string())?;
            let root = config_dir(&config);
            let store = open_store(&config)?;

            if cfg.schedules.is_empty()
                || cfg.schedules.iter().all(|t| matches!(t, loopsmith_core::Trigger::Manual))
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
                // Goal state feeds the goal_satisfied trigger; read it fresh
                // so a run started elsewhere still counts.
                let satisfied: std::collections::BTreeMap<String, bool> = store
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

                    match run::execute(
                        &cfg,
                        &store,
                        &run::RunOptions {
                            run_id,
                            workdir: root.clone(),
                            dry_run: false,
                            resume: false,
                            acquire_skills: true,
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

        Command::Schedule { config, install } => {
            let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("loopsmith"));
            let abs = std::fs::canonicalize(&config).unwrap_or_else(|_| config.clone());
            let root = config_dir(&config);
            let logs = root.join("state");
            let label = schedule::default_label(&cfg.name);

            if cfg!(target_os = "macos") {
                let plist = schedule::launchd_plist(&label, &exe, &abs, &logs);
                let dest = schedule::launch_agents_dir()
                    .ok_or("HOME is not set, cannot locate LaunchAgents")?
                    .join(format!("{label}.plist"));
                if install {
                    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
                    std::fs::write(&dest, &plist).map_err(|e| e.to_string())?;
                    println!("wrote {}", dest.display());
                    // Loading it is a persistent, user-visible change to their
                    // machine, so it stays their call.
                    println!("\nEnable it with:\n  launchctl load -w {}", dest.display());
                } else {
                    println!("{plist}");
                    println!("# write it with: loopsmith schedule {} --install", config.display());
                }
            } else {
                let expr = cfg
                    .schedules
                    .iter()
                    .find_map(|t| match t {
                        loopsmith_core::Trigger::Cron { expr } => Some(expr.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "@reboot".into());
                println!("{}", schedule::crontab_line(&exe, &abs, &expr, &logs));
                println!("# add it with: crontab -e");
            }
            Ok(ExitCode::SUCCESS)
        }

        Command::Skills { action } => match action {
            SkillsAction::List { config, all } => {
                let root = config_dir(&config);
                let mut found = loopsmith_skills::list_installed(&root);
                if !all {
                    // Default to what this loop actually owns; the global
                    // directory is dozens of entries and drowns the signal.
                    let home = std::env::var_os("HOME").map(PathBuf::from);
                    found.retain(|s| {
                        home.as_ref()
                            .map(|h| !s.path.starts_with(h.join(".claude/skills")))
                            .unwrap_or(true)
                    });
                }
                let all_list = found;
                if all_list.is_empty() {
                    println!(
                        "no loop-local skills in {}{}",
                        root.display(),
                        if all { "" } else { " (use --all to include ~/.claude/skills)" }
                    );
                    return Ok(ExitCode::SUCCESS);
                }
                for s in all_list {
                    println!(
                        "{:<28} {:<14} {}",
                        s.name,
                        if s.quarantined { "quarantined" } else { "promoted" },
                        s.path.display()
                    );
                }
                Ok(ExitCode::SUCCESS)
            }

            SkillsAction::Search {
                terms,
                min_stars,
                limit,
            } => {
                if terms.is_empty() {
                    return Err("give at least one search term".into());
                }
                let opts = loopsmith_skills::marketplace::SearchOptions {
                    min_stars,
                    limit,
                    ..Default::default()
                };
                println!("claudemarketplaces.com:");
                match loopsmith_skills::search_marketplace(&terms, &opts) {
                    Ok(hits) if hits.is_empty() => {
                        println!("  no marketplace above {min_stars} stars matched")
                    }
                    Ok(hits) => {
                        for h in hits {
                            println!(
                                "  {:<44} {:>7} stars  {} plugin(s)",
                                h.repo,
                                h.star_count(),
                                h.plugin_count
                            );
                            if !h.description.is_empty() {
                                let d: String = h.description.chars().take(96).collect();
                                println!("      {d}");
                            }
                            println!("      claude plugin marketplace add {}", h.repo);
                        }
                    }
                    Err(e) => println!("  unavailable: {e}"),
                }

                println!("\nskills.sh (via npx skills find):");
                match loopsmith_skills::marketplace::search_skills_cli(
                    &terms.join(" "),
                    &std::env::current_dir().unwrap_or_default(),
                ) {
                    Ok(out) if out.trim().is_empty() => println!("  no results"),
                    Ok(out) => {
                        for line in out.lines().take(20) {
                            println!("  {line}");
                        }
                    }
                    Err(e) => println!("  unavailable: {e}"),
                }
                println!(
                    "\nNothing was installed. Acquire one with:\n  loopsmith skills acquire <config> <name>"
                );
                Ok(ExitCode::SUCCESS)
            }

            SkillsAction::Acquire { config, name } => {
                let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
                let root = config_dir(&config);
                let r = loopsmith_skills::acquire(&name, "acquired on request", &cfg.skills, &root)
                    .map_err(|e| e.to_string())?;
                println!(
                    "{} `{}` at {}",
                    match r.source {
                        loopsmith_skills::Source::Installed => "already installed:",
                        loopsmith_skills::Source::Marketplace => "installed:",
                        loopsmith_skills::Source::Generated => "generated:",
                    },
                    r.name,
                    r.path.display()
                );
                if r.quarantined {
                    println!(
                        "\nIt is quarantined. Read its SKILL.md before promoting — a sub-agent\nruns with whatever your permission grant allowed."
                    );
                }
                Ok(ExitCode::SUCCESS)
            }

            SkillsAction::Scores { config } => {
                let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
                let store = open_store(&config)?;
                let trials = store.skill_trials().map_err(|e| e.to_string())?;
                if trials.is_empty() {
                    println!("no trials recorded yet — run the loop with skills attached to a node");
                    return Ok(ExitCode::SUCCESS);
                }
                println!("{:<28} {:>7} {:>9} {:>11}  source", "skill", "trials", "satisfied", "mean pass");
                for s in loopsmith_memory::score_skills(&trials) {
                    println!(
                        "{:<28} {:>7} {:>8.0}% {:>10.2}  {}",
                        s.skill,
                        s.trials,
                        s.satisfaction_rate() * 100.0,
                        s.mean_pass_rate,
                        s.source
                    );
                }
                println!(
                    "\nFewer than {} trials is not evidence; one lucky run proves nothing.",
                    cfg.skills.min_trials
                );
                Ok(ExitCode::SUCCESS)
            }
        },

        Command::Proposals { config, run_id } => {
            let store = open_store(&config)?;
            let all = store.proposals(&run_id).map_err(|e| e.to_string())?;
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

        Command::Prune { config } => {
            let cfg = loopsmith_core::load(&config).map_err(|e| e.to_string())?;
            let root = config_dir(&config);
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
            "spend:       {} tokens{}, ${:.4}",
            out.tokens_used,
            if out.tokens_estimated {
                " (estimated: no provider reported usage)"
            } else {
                ""
            },
            out.cost_usd
        );
    }
    if out.proposals > 0 {
        println!(
            "proposals:   {} written — review them, the loop cannot apply them itself",
            out.proposals
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
