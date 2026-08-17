//! What a loop leaves behind when it actually worked.
//!
//! A run that meets its bar has produced something more valuable than the
//! deliverable: a configuration that is known to converge, and the evidence
//! that it did. `<name>-success/` packages both as a sub-agent skill, so the
//! next person with the same problem starts from a thing that worked rather
//! than from a blank template.
//!
//! The export is written **only** when [`loopsmith_gate`] certified overall
//! success. It is not written because a node said so, and there is no flag to
//! write it anyway — an export that could be produced by a confident model
//! would be a certificate that means nothing.

use loopsmith_core::LoopConfig;
use loopsmith_gate::TargetVerdict;
use loopsmith_memory::IterationSummary;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Write `<root>/<name>-success/`. Returns where it went.
pub fn export_success(
    cfg: &LoopConfig,
    root: &Path,
    verdicts: &BTreeMap<String, TargetVerdict>,
    summaries: &[IterationSummary],
    iterations: u32,
    config_file: &str,
) -> std::io::Result<PathBuf> {
    let dir = root.join(format!("{}-success", sanitize(&cfg.name)));
    std::fs::create_dir_all(&dir)?;

    std::fs::write(dir.join("SKILL.md"), skill_md(cfg, verdicts, iterations))?;
    std::fs::write(dir.join("EVIDENCE.md"), evidence_md(cfg, verdicts, summaries))?;

    // The config that converged, verbatim. Someone reusing this needs the
    // thing that worked, not a description of it.
    if let Ok(yaml) = serde_yaml::to_string(cfg) {
        std::fs::write(dir.join("loop.yaml"), yaml)?;
    }

    // Whatever the nodes produced.
    let out = root.join("out");
    if out.is_dir() {
        copy_tree(&out, &dir.join("out"))?;
    }

    std::fs::write(dir.join("run.sh"), rerun_script(config_file))?;
    std::fs::write(dir.join("run.cmd"), rerun_cmd(config_file))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.join("run.sh"),
            std::fs::Permissions::from_mode(0o755),
        )?;
    }

    Ok(dir)
}

fn skill_md(cfg: &LoopConfig, verdicts: &BTreeMap<String, TargetVerdict>, iterations: u32) -> String {
    let goals: Vec<&str> = cfg.goals.iter().map(|g| g.name.as_str()).collect();
    let checks: usize = verdicts.values().map(|v| v.total).sum();

    let mut s = format!(
        "---\nname: {}-success\ndescription: >\n  A loopsmith loop that reached its bar: {}.\n  \
         Reuse its config, its checks, and the evidence that it converged.\n---\n\n\
         # {}-success\n\n\
         This directory is what `{}` looked like when the gate certified it. It \
         converged in {iterations} iteration(s) across {checks} check(s).\n\n\
         **{}**\n\n",
        sanitize(&cfg.name),
        first_sentence(&cfg.description),
        sanitize(&cfg.name),
        cfg.name,
        if cfg.description.trim().is_empty() {
            "No description was set on this loop.".to_string()
        } else {
            cfg.description.trim().to_string()
        }
    );

    s.push_str("## What it proved\n\n");
    for (target, v) in verdicts {
        s.push_str(&format!(
            "- **{target}** — {} ({}/{} checks)\n",
            if v.satisfied {
                "satisfied"
            } else {
                "not satisfied"
            },
            v.passed,
            v.total
        ));
    }

    s.push_str("\n## How to reuse it\n\n```bash\n./run.sh\n```\n\n");
    s.push_str(
        "`loop.yaml` is the configuration that converged. Change the goals and \
         the information block to your problem, and keep the validations — they \
         are the part that made the result checkable.\n\n",
    );

    if !goals.is_empty() {
        s.push_str(&format!("Goals it carried: `{}`.\n\n", goals.join("`, `")));
    }

    s.push_str(
        "## What this is not\n\n\
         This is a record, not a guarantee. It says these checks passed on this \
         input, on the day it ran. Re-run the gate against your own evidence \
         before relying on it — the gate can and does take `done` back.\n",
    );
    s
}

fn evidence_md(
    cfg: &LoopConfig,
    verdicts: &BTreeMap<String, TargetVerdict>,
    summaries: &[IterationSummary],
) -> String {
    let mut s = format!("# Evidence — {}\n\n", cfg.name);
    s.push_str(
        "Every ruling below was written by the deterministic gate, from artifacts \
         on disk, reported metrics, and judge verdicts whose provider differed \
         from the builder's. No node's own claim appears here.\n\n",
    );

    s.push_str("## Final rulings\n\n");
    for (target, v) in verdicts {
        s.push_str(&format!(
            "### {target}\n\n{}\n\n",
            if v.satisfied {
                format!("**SATISFIED** — {}", v.reason)
            } else {
                format!("**not satisfied** — {}", v.reason)
            }
        ));
        for c in &v.checks {
            s.push_str(&format!(
                "- [{}]{} `{}` — {}\n",
                if c.passed { "pass" } else { "FAIL" },
                if c.blocking { "" } else { " (advisory)" },
                c.name,
                c.evidence
            ));
        }
        s.push('\n');
    }

    if !summaries.is_empty() {
        s.push_str("## How it got there\n\n");
        for entry in summaries {
            s.push_str(&entry.render());
            s.push('\n');
        }
    }
    s
}

/// The export is the artifact most likely to be handed to someone else, so its
/// script assumes least: POSIX `sh`, no bash 4 syntax, and a `loopsmith` found
/// on `PATH` rather than pinned to a path that only existed on the machine that
/// produced the export.
fn rerun_script(config_file: &str) -> String {
    format!(
        "#!/bin/sh\n\
# Re-run the configuration that converged.\n\
#\n\
# POSIX sh on purpose: this package travels, and macOS ships bash 3.2.\n\
set -eu\n\
cd \"$(dirname \"$0\")\"\n\
\n\
if ! command -v loopsmith >/dev/null 2>&1; then\n\
  echo \"loopsmith is not on PATH\" >&2\n\
  echo \"This package is a config and its evidence; it needs the binary to run.\" >&2\n\
  exit 127\n\
fi\n\
exec loopsmith run \"{config_file}\" \"$@\"\n"
    )
}

/// The `cmd.exe` counterpart. An export is the artifact most likely to be handed
/// to someone else, and "someone else" is the case where the receiving machine
/// is least predictable — so it travels with both launchers, the same as a
/// scaffolded loop does.
///
/// CRLF, because `cmd.exe` needs it in a batch file.
/// One `exit /b`, on the last line, reached by every path — the same shape the
/// scaffolded launchers use, and for the same two reasons. `setlocal` saves the
/// errorlevel and the implicit `endlocal` restores it, so an early `exit /b 127`
/// reports 0; and `endlocal & exit /b` does not fix that inside a nested block.
fn rerun_cmd(config_file: &str) -> String {
    format!(
        "@echo off\r\n\
rem Re-run the configuration that converged.\r\n\
rem\r\n\
rem The POSIX `run.sh` beside this file does the same job on Unix.\r\n\
rem\r\n\
rem One exit, on the last line: an early `exit /b` under `setlocal` reports 0\r\n\
rem because the implicit `endlocal` restores the saved errorlevel.\r\n\
setlocal enabledelayedexpansion\r\n\
cd /d \"%~dp0\"\r\n\
set \"CODE=0\"\r\n\
\r\n\
where loopsmith >nul 2>&1\r\n\
if errorlevel 1 (\r\n\
  echo loopsmith is not on PATH 1>&2\r\n\
  echo This package is a config and its evidence; it needs the binary to run. 1>&2\r\n\
  set \"CODE=127\"\r\n\
  goto :loopsmith_done\r\n\
)\r\n\
loopsmith run \"{config_file}\" %*\r\n\
set \"CODE=!ERRORLEVEL!\"\r\n\
\r\n\
:loopsmith_done\r\n\
endlocal & exit /b %CODE%\r\n"
    )
}

/// First sentence of a description, for the skill frontmatter.
fn first_sentence(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return "an automated loop".into();
    }
    match t.find(". ") {
        Some(i) => t[..i].to_string(),
        None => t.trim_end_matches('.').to_string(),
    }
}

/// Loop names reach the filesystem here, and a name with a separator in it
/// would put the export somewhere nobody is looking.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "loop".into()
    } else {
        cleaned
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_gate::CheckResult;

    fn cfg() -> LoopConfig {
        loopsmith_core::parse_str(
            r#"
name: demo/loop
description: Ship the thing. Then prove it shipped.
goals:
  - name: g1
    description: a sufficiently long goal description
validations:
  - target: g1
    name: v
    mode: objective
    statement: the file exists
    detector: { type: file_exists, path: out/result.md }
"#,
            "test",
        )
        .unwrap()
    }

    fn verdicts() -> BTreeMap<String, TargetVerdict> {
        let mut m = BTreeMap::new();
        m.insert(
            "overall".to_string(),
            TargetVerdict {
                target: "overall".into(),
                satisfied: true,
                checks: vec![CheckResult {
                    name: "v".into(),
                    text: "the file exists".into(),
                    passed: true,
                    blocking: true,
                    evidence: "out/result.md exists and is non-empty".into(),
                }],
                passed: 1,
                failed: 0,
                total: 1,
                reason: "all 1 blocking checks passed".into(),
            },
        );
        m
    }

    #[test]
    fn the_export_carries_the_config_the_evidence_and_the_artifacts() {
        let root = loopsmith_util::testing::temp_dir("export");
        std::fs::create_dir_all(root.join("out")).unwrap();
        std::fs::write(root.join("out/result.md"), "the deliverable").unwrap();

        let dir = export_success(&cfg(), &root, &verdicts(), &[], 3, "loop.yaml").unwrap();

        // The name is sanitised: `demo/loop` must not escape into a subdirectory.
        assert!(dir.ends_with("demo-loop-success"), "got {}", dir.display());
        // `run.cmd` alongside `run.sh`: an export is the artifact most likely to
        // be handed to someone else, which is exactly when you cannot predict
        // what kind of machine will open it.
        for f in [
            "SKILL.md",
            "EVIDENCE.md",
            "loop.yaml",
            "run.sh",
            "run.cmd",
            "out/result.md",
        ] {
            assert!(dir.join(f).is_file(), "{f} missing from the export");
        }

        let cmd = std::fs::read_to_string(dir.join("run.cmd")).unwrap();
        assert!(cmd.starts_with("@echo off\r\n"), "run.cmd must be CRLF: {cmd:?}");
        assert!(
            !cmd.contains("set \"LOOPSMITH="),
            "the export pins nothing, on either platform: {cmd}"
        );
        assert!(cmd.contains("where loopsmith"), "{cmd}");

        let skill = std::fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(skill.starts_with("---\nname: demo-loop-success"), "got: {skill}");
        assert!(skill.contains("converged in 3 iteration(s)"), "got: {skill}");
        // It must not oversell itself.
        assert!(skill.contains("a record, not a guarantee"), "got: {skill}");

        let evidence = std::fs::read_to_string(dir.join("EVIDENCE.md")).unwrap();
        assert!(evidence.contains("out/result.md exists and is non-empty"));
        assert!(
            evidence.contains("No node's own claim appears here"),
            "the evidence file must say where its facts came from"
        );

        assert_eq!(
            std::fs::read_to_string(dir.join("out/result.md")).unwrap(),
            "the deliverable"
        );

        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn the_iteration_history_is_included_when_there_is_one() {
        let root = loopsmith_util::testing::temp_dir("export-history");
        let summaries = vec![IterationSummary {
            run_id: "r".into(),
            iteration: 1,
            headline: "1 node(s) ran (0 failed); 1/1 target(s) satisfied.".into(),
            facts: vec!["Ran: `build` via `echoer`".into()],
            narrative: None,
            created_ms: 0,
        }];

        let dir = export_success(&cfg(), &root, &verdicts(), &summaries, 1, "loop.yaml").unwrap();
        let evidence = std::fs::read_to_string(dir.join("EVIDENCE.md")).unwrap();
        assert!(evidence.contains("How it got there"));
        assert!(evidence.contains("Ran: `build` via `echoer`"));

        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn a_name_with_separators_cannot_escape_the_loop_directory() {
        assert_eq!(sanitize("../../etc"), "------etc");
        assert_eq!(sanitize("clean-name_1"), "clean-name_1");
        assert_eq!(sanitize(""), "loop");
    }
}
