//! `loopsmith gate` — evaluate the gate once against the working tree.

use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, target: &str, workdir: &Path) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    // A one-shot gate check has no judge run behind it, so subjective checks
    // correctly report that no judgment was recorded.
    let ev = crate::run::collect_evidence(workdir, Some(&workdir.join("metrics.json")), vec![]);
    let v = loopsmith_gate::evaluate(&cfg, target, &ev);
    println!(
        "{}: {}",
        v.target,
        if v.satisfied {
            "SATISFIED"
        } else {
            "NOT SATISFIED"
        }
    );
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
