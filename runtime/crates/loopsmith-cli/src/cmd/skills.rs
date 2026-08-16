//! `loopsmith skills` — discover, install, and score sub-agents.

use super::{config_dir, open_store};
use loopsmith_memory::Store;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn list(config: &Path, all: bool) -> Result<ExitCode, String> {
    let root = config_dir(config);
    let mut found = loopsmith_skills::list_installed(&root);
    if !all {
        // Default to what this loop actually owns; the global directory is
        // dozens of entries and drowns the signal.
        let home = std::env::var_os("HOME").map(PathBuf::from);
        found.retain(|s| {
            home.as_ref()
                .map(|h| !s.path.starts_with(h.join(".claude/skills")))
                .unwrap_or(true)
        });
    }
    if found.is_empty() {
        println!(
            "no loop-local skills in {}{}",
            root.display(),
            if all {
                ""
            } else {
                " (use --all to include ~/.claude/skills)"
            }
        );
        return Ok(ExitCode::SUCCESS);
    }
    for s in found {
        println!(
            "{:<28} {:<14} {}",
            s.name,
            if s.quarantined {
                "quarantined"
            } else {
                "promoted"
            },
            s.path.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub fn search(terms: &[String], min_stars: u64, limit: usize) -> Result<ExitCode, String> {
    if terms.is_empty() {
        return Err("give at least one search term".into());
    }
    let opts = loopsmith_skills::marketplace::SearchOptions {
        min_stars,
        limit,
        ..Default::default()
    };
    println!("claudemarketplaces.com:");
    match loopsmith_skills::search_marketplace(terms, &opts) {
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

pub fn acquire(config: &Path, name: &str) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let root = config_dir(config);
    let r = loopsmith_skills::acquire(name, "acquired on request", &cfg.skills, &root)
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

/// `loopsmith skills install <config>` — materialise section J on demand,
/// without starting a run.
pub fn install(config: &Path) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let root = config_dir(config);
    if cfg.default_skills.is_empty() {
        println!("this loop declares no `default_skills`");
        return Ok(ExitCode::SUCCESS);
    }
    let mut failed = 0;
    for spec in &cfg.default_skills {
        match loopsmith_skills::install_default(spec, &cfg.skills, &root) {
            Ok(r) => println!(
                "{:<28} {:<12} {}",
                r.name,
                spec.source.as_str(),
                r.path.display()
            ),
            Err(e) => {
                failed += 1;
                println!("{:<28} {:<12} FAILED: {e}", spec.name, spec.source.as_str());
            }
        }
    }
    println!(
        "\nQuarantined skills wait for a human. Read each SKILL.md before promoting —\na sub-agent runs with whatever your permission grant allowed."
    );
    Ok(if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

pub fn scores(config: &Path) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let store = open_store(config)?;
    let trials = store.skill_trials().map_err(|e| e.to_string())?;
    if trials.is_empty() {
        println!("no trials recorded yet — run the loop with skills attached to a node");
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "{:<28} {:>7} {:>9} {:>11}  source",
        "skill", "trials", "satisfied", "mean pass"
    );
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
