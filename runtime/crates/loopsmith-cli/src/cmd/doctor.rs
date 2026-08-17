//! `loopsmith doctor` — what this machine is, and what that stops you doing.
//!
//! Most of what goes wrong with a loop on a new machine is environmental and
//! reported far from its cause: a detector written against GNU `sed` edits a
//! file called `-e` on a Mac, a script using `${x,,}` dies with a syntax error
//! against bash 3.2, `schedule` prints a crontab line to a host with no cron.
//!
//! Each of those is one probe away from being obvious. This runs the probes and
//! says what follows from them.

use loopsmith_util::platform::{Platform, Userland};
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: Option<&Path>) -> Result<ExitCode, String> {
    let p = Platform::detect();

    println!("platform");
    println!("  os            {}", p.os.as_str());
    // The empty argument is rendered as `''` rather than joined away: it *is*
    // the difference between the two userlands, and a report that hides it
    // says the two are the same.
    let sed_flags: Vec<&str> = p
        .userland
        .sed_in_place()
        .iter()
        .map(|a| if a.is_empty() { "''" } else { *a })
        .collect();
    println!(
        "  userland      {}  (in place: `sed {}`)",
        p.userland.as_str(),
        sed_flags.join(" ")
    );
    match &p.bash {
        Some(b) => println!(
            "  bash          {}.{}{}",
            b.major,
            b.minor,
            if b.is_modern() {
                ""
            } else {
                "  — predates 4.0"
            }
        ),
        None => println!("  bash          not on PATH"),
    }
    match &p.sh_bash {
        Some(b) => println!("  /bin/sh       bash {}.{} in posix mode", b.major, b.minor),
        None => println!("  /bin/sh       not bash (dash, ash, or similar)"),
    }
    println!(
        "  scheduler     {}",
        p.scheduler().unwrap_or("none installed")
    );

    println!("\ntools");
    for tool in ["git", "sh", "sed", "awk", "curl"] {
        report_tool(tool);
    }

    let mut notes: Vec<String> = Vec::new();
    if let Some(note) = p.portability_note() {
        notes.push(note);
    }
    if p.userland == Userland::Bsd {
        notes.push(
            "BSD userland: `sed -i` requires a backup suffix and `stat` takes `-f`, not `-c`. \
             Source `scripts/compat.sh` in a detector rather than branching by hand."
                .into(),
        );
    }
    if p.userland == Userland::Unknown {
        notes.push(
            "could not tell GNU from BSD userland; `scripts/compat.sh` assumes BSD, which \
             fails loudly on GNU rather than quietly doing the wrong thing"
                .into(),
        );
    }
    if p.scheduler().is_none() {
        notes.push(
            "no scheduler installed, so `loopsmith schedule` has nothing to hand the loop to. \
             `loopsmith watch` still works under any process supervisor."
                .into(),
        );
    }
    if loopsmith_util::which("git").is_none() {
        notes.push(
            "git is not on PATH: `isolated: true` nodes will run in the shared working \
             directory and say so, and section J `github` sub-agents cannot be fetched"
                .into(),
        );
    }

    if let Some(path) = config {
        notes.extend(config_notes(path));
    }

    if notes.is_empty() {
        println!("\nNothing here will get in your way.");
    } else {
        println!("\nworth knowing");
        for n in &notes {
            println!("  - {n}");
        }
    }

    // Advisory by design. `doctor` reporting a constraint is not the same as
    // the machine being unusable, and a non-zero exit here would fail a CI step
    // that was working perfectly well.
    Ok(ExitCode::SUCCESS)
}

fn report_tool(name: &str) {
    match loopsmith_util::which(name) {
        Some(p) => println!("  {name:<13} {}", p.display()),
        None => println!("  {name:<13} not on PATH"),
    }
}

/// What this particular config needs that the machine may not have.
fn config_notes(path: &Path) -> Vec<String> {
    let Ok(cfg) = loopsmith_core::load(path) else {
        return vec![format!(
            "{} could not be loaded, so nothing config-specific was checked",
            path.display()
        )];
    };
    let root = super::config_dir(path);
    let mut out = Vec::new();

    for v in &cfg.validations {
        let loopsmith_core::Detector::Script { command, .. } = &v.detector else {
            continue;
        };
        // A detector runs with no shell: `command` is argv[0], so a relative
        // path is resolved against the loop directory and must be executable
        // there. Reporting this beats discovering it as a detector error on
        // the first gate evaluation.
        let candidate = if command.contains('/') {
            root.join(command)
        } else {
            match loopsmith_util::which(command) {
                Some(p) => p,
                None => {
                    out.push(format!(
                        "`{}` names detector command `{command}`, which is not on PATH",
                        v.name
                    ));
                    continue;
                }
            }
        };
        if !candidate.exists() {
            out.push(format!(
                "`{}` names detector `{command}`, which does not exist at {}",
                v.name,
                candidate.display()
            ));
        } else if !loopsmith_util::is_executable(&candidate) {
            out.push(format!(
                "detector `{command}` exists but is not executable; `chmod +x {}`",
                candidate.display()
            ));
        }
    }

    if !cfg.default_skills.is_empty() && loopsmith_util::which("git").is_none() {
        out.push(format!(
            "{} section J sub-agent(s) declared, and git is not on PATH to fetch them",
            cfg.default_skills.len()
        ));
    }
    out
}
