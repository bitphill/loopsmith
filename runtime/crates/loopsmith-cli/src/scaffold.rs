//! `loopsmith new --path <dir>` — materialise a purpose-specific loop.
//!
//! The `--path` argument is mandatory by design. A loop is a durable thing
//! with state, a ledger, and a schedule; leaving its home directory implicit
//! is how you end up with three half-finished loops writing into each other's
//! sled trees.

use loopsmith_core::{
    AcquisitionSource, Concurrency, ConstraintSet, Constraints, Detector, Goal, GraphSpec,
    InfoItem, LoopConfig, Mode, NodeSpec, ProviderRouting, Role, SkillPolicy, StopGates,
    SuccessScenario, Tier, Trigger, Validation, WorkItem, OVERALL,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Turn the new loop directory into a git repository.
///
/// This is what makes `isolated: true` actually isolate. A worktree is a second
/// checkout of the same repository, so without a repository there is nothing to
/// check out a second time and every node falls back to sharing one directory —
/// which is fine for a single builder and silently destructive for two.
///
/// The initial commit is not optional. `git worktree add` resolves a start
/// point, and a repository with no HEAD has none, so an uncommitted repo fails
/// exactly as unhelpfully as no repo at all.
///
/// Identity is passed inline with `-c` rather than written to the repo's
/// config: a machine with no `user.email` set would otherwise fail the commit,
/// and silently editing someone's git identity to scaffold a loop is not a
/// trade anyone agreed to.
pub fn init_git(root: &Path) -> Result<(), String> {
    if loopsmith_util::which("git").is_none() {
        return Err("git is not on PATH".into());
    }
    let run = |args: &[&str]| -> Result<(), String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|e| format!("git {}: {e}", args[0]))?;
        if out.status.success() {
            return Ok(());
        }
        Err(format!(
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    };

    // Already a repository — someone scaffolded into an existing checkout, and
    // re-initialising it would be a surprise rather than a service.
    if root.join(".git").exists() {
        return Ok(());
    }

    run(&["init", "-q"])?;
    run(&[
        "-c", "user.email=loopsmith@localhost",
        "-c", "user.name=loopsmith",
        "add", "-A",
    ])?;
    run(&[
        "-c", "user.email=loopsmith@localhost",
        "-c", "user.name=loopsmith",
        "commit", "-qm", "loopsmith: initial scaffold",
    ])?;
    Ok(())
}

#[derive(Debug)]
pub struct NewLoopArgs {
    pub path: PathBuf,
    pub name: String,
    pub purpose: String,
    pub force: bool,
    /// A complete config supplied by the caller, instead of the starter.
    pub config: Option<ProvidedConfig>,
    /// Initialise a git repository in the new directory, so `isolated: true`
    /// nodes can have a worktree each.
    pub git: bool,
}

/// Config text handed in whole, from a file or from stdin.
#[derive(Debug, Clone)]
pub struct ProvidedConfig {
    pub text: String,
    pub markdown: bool,
}

/// Files written into the new loop directory.
#[derive(Debug)]
pub struct Scaffold {
    pub written: Vec<PathBuf>,
    /// `loop.yaml` or `loop.md`, whichever was written.
    pub config_file: String,
    /// `Some(Ok(()))` when a repository was initialised, `Some(Err(why))` when
    /// it was asked for and could not be, `None` when it was not asked for.
    /// Never fatal: a loop without a repository still runs.
    pub git: Option<Result<(), String>>,
}

pub fn starter_config(name: &str, purpose: &str) -> LoopConfig {
    let mut cascade: BTreeMap<String, Vec<String>> = BTreeMap::new();
    cascade.insert(
        "cheap".into(),
        vec!["ollama".into(), "claude".into()],
    );
    cascade.insert("standard".into(), vec!["claude".into(), "gemini".into()]);
    cascade.insert("strong".into(), vec!["openai".into(), "claude".into()]);

    LoopConfig {
        name: name.to_string(),
        version: "0.1.0".into(),
        description: purpose.to_string(),

        information: vec![InfoItem {
            key: "purpose".into(),
            value: purpose.to_string(),
            note: Some("Replace with the durable facts every node should know.".into()),
        }],

        pre_execution: vec![
            WorkItem {
                step: "Run this task manually end to end at least once".into(),
                done: false,
                evidence: Some("Link the transcript or notes here.".into()),
            },
            WorkItem {
                step: "Write down what 'done' means in checkable terms".into(),
                done: false,
                evidence: None,
            },
        ],

        goals: vec![Goal {
            name: "primary".into(),
            description: format!("Deliver the {purpose} outcome to the stated bar."),
            depends_on: vec![],
            priority: Some(1),
        }],

        validations: vec![
            Validation {
                target: "primary".into(),
                name: "artifact-exists".into(),
                mode: Mode::Objective,
                statement: "The deliverable exists and is non-empty.".into(),
                detector: Detector::FileExists {
                    path: "out/result.md".into(),
                    non_empty: true,
                },
                blocking: true,
            },
            Validation {
                target: OVERALL.into(),
                name: "checks-pass".into(),
                mode: Mode::Objective,
                statement: "The project's own check command exits clean.".into(),
                detector: Detector::Script {
                    command: "true".into(),
                    args: vec![],
                    expect_exit: Some(0),
                },
                blocking: true,
            },
        ],

        success: vec![SuccessScenario {
            target: OVERALL.into(),
            name: "all-blocking-pass".into(),
            mode: Mode::Percentage,
            statement: "Every blocking validation passes.".into(),
            threshold: Some(1.0),
        }],

        stop_gates: StopGates {
            max_iterations: 8,
            max_revisions_per_node: 3,
            max_wall_clock_seconds: Some(3600),
            max_tokens: Some(2_000_000),
            max_cost_usd: Some(5.0),
            no_progress_iterations: 3,
            no_progress_iterations_randomness: Some(2),
            stop_on_overall_success: true,
        },

        schedules: vec![Trigger::Manual],

        execution_guidelines: Default::default(),

        // Section J is empty in the starter: a fresh loop should not reach the
        // network on its first run to fetch something nobody asked for.
        default_skills: vec![],

        constraints: Constraints {
            global: ConstraintSet {
                rules: ConstraintSet::frozen_git_rules(),
                forbidden_paths: vec![".git/".into(), "node_modules/".into()],
                forbidden_commands: vec!["rm -rf".into(), "git push".into()],
                max_tokens: None,
                max_seconds: Some(900),
                human_checkpoint: vec![
                    "publishing anything".into(),
                    "sending a message".into(),
                    "deleting data".into(),
                ],
            },
            per_node: BTreeMap::new(),
        },

        graph: GraphSpec {
            nodes: vec![
                NodeSpec {
                    id: "build".into(),
                    role: Role::Builder,
                    instruction: "Produce the deliverable described in the goal. State any assumption you had to make.".into(),
                    depends_on: vec![],
                    goals: vec!["primary".into()],
                    tier: Tier::Standard,
                    provider: None,
                    stage: None,
                    skills: vec![],
                    weight: 3.0,
                    isolated: true,
                },
                NodeSpec {
                    id: "judge".into(),
                    role: Role::Judge,
                    instruction: "Check the builder's output against the named standard and the original brief. Report per-check pass or fail with evidence.".into(),
                    depends_on: vec!["build".into()],
                    goals: vec!["primary".into()],
                    tier: Tier::Strong,
                    provider: None,
                    stage: None,
                    skills: vec![],
                    weight: 1.0,
                    isolated: false,
                },
            ],
            concurrency: Concurrency::Auto {
                cap: 16,
                min_marginal_gain: 0.05,
            },
        },

        providers: ProviderRouting {
            providers: loopsmith_provider::starter_providers(),
            cascade,
            enforce_judge_independence: true,
        },

        skills: SkillPolicy {
            acquisition_order: vec![
                AcquisitionSource::Installed,
                AcquisitionSource::Marketplace,
                AcquisitionSource::Generate,
            ],
            quarantine_dir: "generated-skills".into(),
            min_marketplace_stars: 100,
            require_human_promotion: true,
            explore: false,
            explore_candidates: vec![],
            min_trials: 3,
        },

        context: Default::default(),
    }
}

const GITIGNORE: &str = "\
# loopsmith run state — regenerable, machine-local, and often large
state/
logs/
out/
*.log

# Quarantined sub-agents wait for human promotion; they are not source
generated-skills/
";

// The harness templates are compiled in rather than read from the install
// directory. That is what makes a new loop self-contained and makes it
// impossible for `loopsmith new` to depend on — or touch — the loopsmith
// checkout it was launched from.
//
// They live inside this crate rather than in the repository's `config/`, and
// that is a packaging constraint rather than a preference: a published crate
// tarball holds only its own directory, so an `include_str!` reaching above the
// crate root compiles here and fails `cargo package --verify`.
const MCP_TEMPLATE: &str = include_str!("../templates/mcp.template.json");
const PERMISSIONS_TEMPLATE: &str = include_str!("../templates/permissions.template.json");
const COMPAT_TEMPLATE: &str = include_str!("../templates/compat.template.sh");
const MARKETPLACES: &str = include_str!("../templates/marketplaces.json");

fn readme(name: &str, purpose: &str, config_file: &str) -> String {
    format!(
        "# {name}\n\n\
A loopsmith loop.\n\n\
**Purpose:** {purpose}\n\n\
## Run it\n\n\
```bash\n\
./run.sh          # macOS, Linux, BSD, Git Bash, WSL\n\
run.cmd           # Windows cmd.exe or PowerShell\n\
```\n\n\
That is `loopsmith run {config_file}` with this directory's absolute paths \
already filled in. If the loop stops before it is done, `./resume.sh <run-id>` \
(or `resume.cmd <run-id>`) picks up from the last checkpoint — the run id is \
printed at the end of every run and appears in `logs/`.\n\n\
Both launchers are written on every platform, so this directory keeps working \
after it moves to a different kind of machine.\n\n\
The long way, when you want to see each step:\n\n\
```bash\n\
loopsmith validate {config_file}   # the A-J model must be complete\n\
loopsmith plan     {config_file}   # waves, critical path, predicted speedup\n\
loopsmith run      {config_file}\n\
```\n\n\
## Before the first run\n\n\
`pre_execution` in `{config_file}` is deliberately unfinished. Run the task by \
hand once, record what you learned, and set each step to `done: true`. \
Validation fails until you do, because automating a process you cannot \
describe produces fast, confident garbage.\n\n\
## Secrets\n\n\
Providers name the environment variables they need under `requires_env`. \
loopsmith checks that those variables **exist** and never reads their values, \
so a key never reaches a prompt, a log, or the ledger. Export them in your \
shell:\n\n\
```bash\n\
export OPENAI_API_KEY=...   # in your shell, not in this repo\n\
```\n\n\
Never paste a key into a chat window, a config file, or an issue. If one is \
ever pasted somewhere it should not be, rotate it rather than deleting the \
message.\n\n\
## Layout\n\n\
| Path | What it is |\n\
|---|---|\n\
| `{config_file}` | The A-J config: goals, validations, success, stop gates, schedules, constraints, phases, default skills |\n\
| `run.sh` / `resume.sh` | This loop's exact commands, with absolute paths (POSIX `sh`) |\n\
| `run.cmd` / `resume.cmd` | The same two commands for `cmd.exe` |\n\
| `scripts/compat.sh` | Source this in a detector: `sed_i`, `stat_size`, `readlink_f`, `sha256`, `require`, `need_bash` |\n\
| `.mcp.json` | MCP server definition, so an agent can read this loop's memory |\n\
| `.claude/settings.local.json` | Permission grant this config needs |\n\
| `marketplaces.json` | Sub-agent index sources |\n\
| `state/` | sled memory: episodes, goal state, ledger, checkpoints, summaries |\n\
| `logs/` | Plain-text run logs, one per run |\n\
| `out/` | Deliverables the nodes produce |\n\
| `proposals/` | Changes the loop wants to make to itself — review these |\n\
| `generated-skills/` | Auto-created sub-agents awaiting promotion |\n"
    )
}

/// Refuse a `--path` that would put a loop somewhere it must not go.
///
/// The load-bearing one is the install-directory check. A purpose-specific loop
/// writes state, clones sub-agents, and lets nodes edit files; pointing it at
/// the loopsmith checkout would let a loop modify the thing that runs it.
pub fn guard_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("--path is empty; a loop needs a directory of its own".into());
    }

    let target = absolutize(path);

    if target.parent().is_none() {
        return Err(format!(
            "{} is a filesystem root; give the loop a directory of its own",
            target.display()
        ));
    }
    if let Some(home) = loopsmith_util::platform::home_dir() {
        if target == absolutize(&home) {
            return Err("--path is your home directory; give the loop a subdirectory".into());
        }
    }
    if let Some(install) = install_root() {
        if target == install || target.starts_with(&install) {
            return Err(format!(
                "{} is inside the loopsmith installation at {}.\n\
                 A loop edits files, installs sub-agents, and writes state — it must not be \
                 pointed at the tool that runs it.\n\
                 Pick a directory outside it, for example: --path ~/loops/{}",
                target.display(),
                install.display(),
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("my-loop")
            ));
        }
    }
    Ok(())
}

/// Best-effort absolute path that does not require the path to exist.
fn absolutize(p: &Path) -> PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
    if p.is_absolute() {
        return p.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(p))
        .unwrap_or_else(|_| p.to_path_buf())
}

/// Where loopsmith itself lives, if it can be found.
///
/// Identified by the two files only a loopsmith checkout has together. Walking
/// up from the running binary covers `cargo run`, an installed `target/release`
/// binary, and a symlink into the repo.
pub fn install_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.canonicalize().unwrap_or(exe);
    while dir.pop() {
        if dir.join("config/loop.schema.json").is_file() && dir.join("runtime/Cargo.toml").is_file()
        {
            return Some(dir);
        }
    }
    None
}

/// The absolute path of the running binary, for the generated scripts. Falls
/// back to the bare name so the scripts still work when it is on `PATH`.
///
/// **Deliberately not canonicalized when it is already absolute.** Package
/// managers install behind a stable symlink into a versioned directory —
/// Homebrew's `/usr/local/bin/loopsmith` points at
/// `/usr/local/Cellar/loopsmith/0.1.2/bin/loopsmith`. Resolving the symlink pins
/// the version number, so every loop created before an upgrade has a dead path in
/// its launcher the moment that Cellar directory is replaced. The symlink is the
/// durable answer and the one to write down.
///
/// A relative `current_exe` is still canonicalized: a launcher pins an absolute
/// path because cron, launchd, and Task Scheduler do not inherit a shell's
/// working directory any more than they inherit its `PATH`.
///
/// The fallback in the generated script covers the rest — if the pin ever goes
/// stale, the launcher finds `loopsmith` on `PATH` and says so.
fn binary_path() -> String {
    let Ok(exe) = std::env::current_exe() else {
        return "loopsmith".into();
    };
    if exe.is_absolute() {
        return exe.display().to_string();
    }
    exe.canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "loopsmith".into())
}

/// The header every generated script carries.
///
/// `#!/bin/sh`, not `#!/bin/bash` and not `#!/usr/bin/env bash`: macOS still
/// ships bash 3.2.57 because 4.0 changed licence, so a script written against
/// bash 4 syntax fails with a syntax error on the most common developer machine
/// there is. Everything loopsmith generates is POSIX, which runs identically
/// under dash, bash 3.2, bash 5, and busybox ash.
///
/// The binary is missing more often than anything else goes wrong — a loop
/// directory outlives the checkout it was made from, and cron and launchd do
/// not inherit a login shell's `PATH` — so that check comes first and says what
/// to do about it.
fn script_header(binary: &str, purpose: &str) -> String {
    // A raw string, not an escaped one with `\` line continuations: those eat
    // the leading whitespace of the next line, so the script that reached disk
    // came out flat and unreadable.
    format!(
        r#"#!/bin/sh
# {purpose}
#
# POSIX sh on purpose: macOS ships bash 3.2, so anything needing bash 4 syntax
# would fail there. Paths are absolute because cron and launchd do not inherit
# your shell's PATH.
set -eu
cd "$(dirname "$0")"

LOOPSMITH="{binary}"
if [ ! -x "$LOOPSMITH" ]; then
  if command -v loopsmith >/dev/null 2>&1; then
    LOOPSMITH=$(command -v loopsmith)
  else
    echo "loopsmith is not at $LOOPSMITH and not on PATH" >&2
    echo "This loop was created against a binary that has since moved." >&2
    echo "Re-point it by editing this script, or put loopsmith on PATH." >&2
    exit 127
  fi
fi
"#
    )
}

fn run_script(binary: &str, config_file: &str) -> String {
    format!(
        "{}\nexec \"$LOOPSMITH\" run \"{config_file}\" \"$@\"\n",
        script_header(
            binary,
            "Generated by `loopsmith new`. One supervised pass of the loop."
        )
    )
}

fn resume_script(binary: &str, config_file: &str) -> String {
    format!(
        r#"{}
if [ $# -eq 0 ]; then
  echo "usage: ./resume.sh <run-id>" >&2
  echo "The run id is printed at the end of every run and names the file in logs/." >&2
  echo "recent runs:" >&2
  # `ls -1t` and `sed` behave the same on either userland here. The flag that
  # differs between GNU and BSD is `-i`, which this does not use.
  ls -1t logs/ 2>/dev/null | head -5 | sed -e 's/\.log$//' -e 's/^/  /' >&2
  exit 2
fi
exec "$LOOPSMITH" resume "{config_file}" "$1"
"#,
        script_header(
            binary,
            "Generated by `loopsmith new`. Usage: ./resume.sh <run-id>"
        )
    )
}

/// The header every generated `.cmd` launcher carries.
///
/// `cmd.exe` is not a POSIX shell and none of the `.sh` scripts run under it, so
/// Windows needs its own launcher rather than a shebang tweak. Both flavours are
/// written on every platform, unconditionally: the premise of the whole
/// portability design is that a loop directory outlives the machine that made
/// it, and a loop created on a Mac then copied to a Windows box has to start
/// there without being regenerated.
///
/// `%~dp0` is the script's own directory with a trailing backslash, which is why
/// it is not quoted with a separate separator.
///
/// **One exit point, at the very end.** Getting an exit code out of a batch file
/// under `setlocal` is a minefield and the launcher walked into two of them:
///
/// - The implicit `endlocal` at the end of a batch file **restores the errorlevel
///   saved by `setlocal`**, so a bare `exit /b 127` reports 0. A loop whose binary
///   had moved printed its diagnostic and then exited successfully.
/// - `endlocal & exit /b 127` fixes that on a top-level line, but not inside a
///   nested `if ( … )` block, which is where the failing one lived.
///
/// So there is exactly one `exit /b` in a generated launcher, on the last line,
/// reached by every path. Each path sets `CODE` and falls through to it, which
/// needs no reasoning about block parsing at all.
///
/// `enabledelayedexpansion` and `!ERRORLEVEL!` because a parenthesised block is
/// parsed as a unit before it runs — `%ERRORLEVEL%` inside one expands to the
/// value from *before* the block, which is the same class of bug one level down.
fn cmd_header(binary: &str, purpose: &str) -> String {
    format!(
        r#"@echo off
rem {purpose}
rem
rem Paths are absolute because Task Scheduler does not inherit an interactive
rem shell's PATH. The POSIX `.sh` sibling of this file does the same job under
rem Git Bash, WSL, macOS, and Linux.
rem
rem There is exactly one `exit /b`, on the last line. `setlocal` saves the
rem errorlevel and the implicit `endlocal` restores it, so an early `exit /b`
rem reports 0 no matter what code it was given -- and `endlocal & exit /b` does
rem not help inside a nested block. Every path sets CODE and falls through.
setlocal enabledelayedexpansion
cd /d "%~dp0"
set "CODE=0"

set "LOOPSMITH={binary}"
if not exist "%LOOPSMITH%" (
  for /f "delims=" %%i in ('where loopsmith 2^>nul') do set "LOOPSMITH=%%i"
)
"#
    )
}

/// The single exit line every launcher ends with.
///
/// `%CODE%` is expanded while this line is parsed, which happens before
/// `endlocal` runs, so the value survives the scope teardown.
const CMD_FOOTER: &str = "\r\n:loopsmith_done\r\nendlocal & exit /b %CODE%\r\n";

/// The "binary is gone" branch, shared by both launchers.
const CMD_NO_BINARY: &str = r#"if not exist "%LOOPSMITH%" (
  echo loopsmith is not at %LOOPSMITH% and not on PATH 1>&2
  echo This loop was created against a binary that has since moved. 1>&2
  echo Re-point it by editing this script, or put loopsmith on PATH. 1>&2
  set "CODE=127"
  goto :loopsmith_done
)
"#;

fn run_cmd(binary: &str, config_file: &str) -> String {
    format!(
        "{header}{no_binary}\"%LOOPSMITH%\" run \"{config_file}\" %*\r\nset \"CODE=!ERRORLEVEL!\"{CMD_FOOTER}",
        header = cmd_header(
            binary,
            "Generated by `loopsmith new`. One supervised pass of the loop."
        ),
        no_binary = CMD_NO_BINARY,
    )
}

fn resume_cmd(binary: &str, config_file: &str) -> String {
    format!(
        r#"{header}{no_binary}if "%~1"=="" (
  echo usage: resume.cmd ^<run-id^> 1>&2
  echo The run id is printed at the end of every run and names the file in logs\. 1>&2
  echo recent runs: 1>&2
  for /f "delims=" %%f in ('dir /b /o-d logs\*.log 2^>nul') do @echo   %%~nf 1>&2
  set "CODE=2"
  goto :loopsmith_done
)
"%LOOPSMITH%" resume "{config_file}" "%~1"
set "CODE=!ERRORLEVEL!"{CMD_FOOTER}"#,
        header = cmd_header(
            binary,
            "Generated by `loopsmith new`. Usage: resume.cmd <run-id>"
        ),
        no_binary = CMD_NO_BINARY,
    )
}

/// `cmd.exe` requires CRLF line endings in a batch file. With LF only, older
/// `cmd.exe` reads the trailing `\n` as part of the last token on the line,
/// which turns `exit /b 2` into an unknown command and a label into a
/// not-found error — a failure mode with no useful message attached.
fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

pub fn scaffold(args: &NewLoopArgs) -> std::io::Result<Scaffold> {
    guard_path(&args.path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    let root = &args.path;
    if root.exists() && !args.force {
        let non_empty = std::fs::read_dir(root)?.next().is_some();
        if non_empty {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "{} already exists and is not empty; pass --force to write into it anyway",
                    root.display()
                ),
            ));
        }
    }

    let mut written = Vec::new();
    // Joined component by component rather than written as one `.claude/skills`
    // literal: a forward slash inside a `join` is a path on Unix and a filename
    // component that merely happens to work on Windows, and the two stop
    // agreeing the moment one of these names is passed anywhere but `create_dir_all`.
    for sub in [
        Path::new("state").to_path_buf(),
        Path::new("logs").to_path_buf(),
        Path::new("out").to_path_buf(),
        Path::new("proposals").to_path_buf(),
        Path::new("generated-skills").to_path_buf(),
        Path::new(".claude").join("skills"),
    ] {
        std::fs::create_dir_all(root.join(sub))?;
    }

    // A config supplied by the caller is parsed before it is written: a new
    // loop directory holding an unparseable config is worse than no directory.
    let (config_file, config_text, cfg) = match &args.config {
        Some(provided) => {
            let name = if provided.markdown {
                "loop.md"
            } else {
                "loop.yaml"
            };
            let parsed = if provided.markdown {
                loopsmith_core::parse_md(&provided.text, name)
            } else {
                loopsmith_core::parse_str(&provided.text, name)
            }
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
            (name, provided.text.clone(), parsed)
        }
        None => {
            let cfg = starter_config(&args.name, &args.purpose);
            let yaml = serde_yaml::to_string(&cfg).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
            })?;
            ("loop.yaml", yaml, cfg)
        }
    };

    write_file(root.join(config_file), &config_text, &mut written)?;
    write_file(root.join(".gitignore"), GITIGNORE, &mut written)?;
    write_file(
        root.join("README.md"),
        &readme(&args.name, &args.purpose, config_file),
        &mut written,
    )?;
    write_file(root.join("proposals/.gitkeep"), "", &mut written)?;

    // --- the harness ------------------------------------------------------
    // Everything the base tool can reach, the new loop can reach: its own MCP
    // server definition, its own permission grant, its own sub-agent index, and
    // its own skills directory.
    write_file(root.join(".mcp.json"), MCP_TEMPLATE, &mut written)?;
    write_file(root.join("marketplaces.json"), MARKETPLACES, &mut written)?;
    write_file(
        root.join("permissions.template.json"),
        PERMISSIONS_TEMPLATE,
        &mut written,
    )?;

    let grant = crate::permissions::required(&cfg);
    let settings = crate::permissions::merge_into(&root.join(".claude/settings.local.json"), &grant)
        .unwrap_or_else(|_| crate::permissions::render(&grant));
    write_file(
        root.join(".claude/settings.local.json"),
        &settings,
        &mut written,
    )?;

    let binary = binary_path();
    write_script(
        root.join("run.sh"),
        &run_script(&binary, config_file),
        &mut written,
    )?;
    write_script(
        root.join("resume.sh"),
        &resume_script(&binary, config_file),
        &mut written,
    )?;
    // Both flavours, on every host. A loop directory outlives the machine that
    // made it — that is the whole reason `compat.sh` probes on arrival instead
    // of being written out with this machine's answers baked in — so a loop
    // scaffolded on a Mac has to be startable on Windows without regenerating
    // it. `cmd.exe` cannot run a `#!` script and no POSIX shell will run a
    // `.cmd`, so the pair is the only arrangement that survives the copy.
    write_file(
        root.join("run.cmd"),
        &crlf(&run_cmd(&binary, config_file)),
        &mut written,
    )?;
    write_file(
        root.join("resume.cmd"),
        &crlf(&resume_cmd(&binary, config_file)),
        &mut written,
    )?;
    // Detector scripts are the part of a loop most likely to be written on one
    // machine and run on another, and the differences that break them —
    // `sed -i`, `stat`, `readlink -f`, and a bash from 2007 — are the same
    // three every time. This is shipped rather than generated so it detects on
    // arrival: a baked-in answer would be wrong the moment the directory moved.
    write_script(
        root.join("scripts/compat.sh"),
        COMPAT_TEMPLATE,
        &mut written,
    )?;

    // Last, so the initial commit captures the whole scaffold rather than
    // whatever half of it existed when git was called.
    let git = args.git.then(|| init_git(root));

    Ok(Scaffold {
        written,
        config_file: config_file.to_string(),
        git,
    })
}

fn write_script(path: PathBuf, contents: &str, written: &mut Vec<PathBuf>) -> std::io::Result<()> {
    write_file(path.clone(), contents, written)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn write_file(path: PathBuf, contents: &str, written: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, contents)?;
    written.push(path);
    Ok(())
}

/// Derive a loop name from a path when the caller does not supply one.
pub fn name_from_path(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("loop")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    use loopsmith_util::testing::temp_path as tmp;

    fn args(path: &Path) -> NewLoopArgs {
        NewLoopArgs {
            // Off in the shared helper: most scaffold tests are about which
            // files land, and shelling out to git in each of them would make
            // the suite slower and dependent on a git install for no gain.
            git: false,
            path: path.to_path_buf(),
            name: "demo".into(),
            purpose: "a demo loop".into(),
            force: false,
            config: None,
        }
    }

    /// (path, len, mtime) for everything under a directory, so a test can prove
    /// nothing changed.
    fn fingerprint(dir: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                let Ok(m) = e.metadata() else { continue };
                if m.is_dir() {
                    stack.push(p);
                } else {
                    out.push((p, m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH)));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn an_empty_path_is_refused() {
        let err = guard_path(Path::new("")).unwrap_err();
        assert!(err.contains("--path is empty"), "got: {err}");
    }

    #[test]
    fn a_filesystem_root_is_refused() {
        let err = guard_path(Path::new("/")).unwrap_err();
        assert!(err.contains("filesystem root"), "got: {err}");
    }

    #[test]
    fn a_loop_cannot_be_created_inside_the_loopsmith_installation() {
        // A loop edits files, clones sub-agents, and writes state. Pointing one
        // at the checkout that runs it would let a loop modify its own runtime.
        let Some(install) = install_root() else {
            // Installed outside a checkout; there is nothing to protect.
            return;
        };

        for candidate in [
            install.join("my-loop"),
            install.join("config/examples/my-loop"),
            install.clone(),
        ] {
            let err = match guard_path(&candidate) {
                Err(e) => e,
                Ok(()) => panic!("{} should have been refused", candidate.display()),
            };
            assert!(
                err.contains("inside the loopsmith installation"),
                "for {}: {err}",
                candidate.display()
            );
            assert!(
                err.contains("--path ~/loops/"),
                "the refusal must suggest somewhere that works: {err}"
            );
        }
    }

    #[test]
    fn creating_a_loop_leaves_the_loopsmith_installation_untouched() {
        let Some(install) = install_root() else {
            return;
        };
        // `config/` and `skills/` are what a misbehaving scaffold would write
        // into; `target/` and `.git/` churn for unrelated reasons.
        let before = (
            fingerprint(&install.join("config")),
            fingerprint(&install.join("skills")),
        );

        let root = tmp("isolation");
        scaffold(&args(&root)).expect("scaffold succeeds outside the install root");

        let after = (
            fingerprint(&install.join("config")),
            fingerprint(&install.join("skills")),
        );
        assert_eq!(before.0, after.0, "config/ must not change");
        assert_eq!(before.1, after.1, "skills/ must not change");

        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn the_harness_travels_with_the_new_loop() {
        // Requirement: whatever the base tool can reach, the new loop can reach.
        let root = tmp("harness");
        scaffold(&args(&root)).unwrap();

        for expected in [
            "loop.yaml",
            ".mcp.json",
            "marketplaces.json",
            "permissions.template.json",
            ".claude/settings.local.json",
            "run.sh",
            "resume.sh",
            // Both flavours, on every host — a loop directory outlives the
            // machine that made it, so the Windows launchers travel from a Mac.
            "run.cmd",
            "resume.cmd",
            "scripts/compat.sh",
            "README.md",
            ".gitignore",
        ] {
            assert!(
                root.join(expected).is_file(),
                "{expected} is missing from the new loop"
            );
        }
        for dir in ["state", "logs", "out", "proposals", "generated-skills", ".claude/skills"] {
            assert!(root.join(dir).is_dir(), "{dir}/ is missing");
        }

        // The permission grant is real, not a copied placeholder.
        let settings = std::fs::read_to_string(root.join(".claude/settings.local.json")).unwrap();
        assert!(
            settings.contains("permissions"),
            "the grant should be materialised: {settings}"
        );

        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn the_generated_scripts_pin_an_absolute_binary_and_are_executable() {
        let root = tmp("scripts");
        scaffold(&args(&root)).unwrap();

        let run = std::fs::read_to_string(root.join("run.sh")).unwrap();
        assert!(run.contains("cd \"$(dirname \"$0\")\""), "got: {run}");
        assert!(run.contains(" run \"loop.yaml\""), "got: {run}");
        // Absolute, because cron and launchd do not inherit a shell PATH.
        let binary = binary_path();
        assert!(run.contains(&binary), "run.sh should pin {binary}: {run}");

        let resume = std::fs::read_to_string(root.join("resume.sh")).unwrap();
        assert!(resume.contains("usage: ./resume.sh <run-id>"), "got: {resume}");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.join("run.sh")).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "run.sh must be executable");
        }

        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn a_supplied_config_is_used_instead_of_the_starter() {
        let root = tmp("supplied");
        let mut a = args(&root);
        a.config = Some(ProvidedConfig {
            markdown: false,
            text: "name: handed-over\ngoals:\n  - name: g1\n    description: a sufficiently long goal description\nvalidations:\n  - target: g1\n    name: v\n    mode: objective\n    statement: it exists\n    detector: { type: file_exists, path: out.txt }\n".into(),
        });
        let s = scaffold(&a).unwrap();

        assert_eq!(s.config_file, "loop.yaml");
        let written = std::fs::read_to_string(root.join("loop.yaml")).unwrap();
        assert!(written.contains("handed-over"), "got: {written}");
        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn a_supplied_markdown_config_is_written_as_md() {
        let root = tmp("supplied-md");
        let mut a = args(&root);
        a.config = Some(ProvidedConfig {
            markdown: true,
            text: "# md-loop\n\n## C. Goals\n\n### g1\n- description: a sufficiently long goal description\n\n## D. Validations\n\n### v\n- target: g1\n- mode: objective\n- statement: it exists\n- detector:\n  - type: file_exists\n  - path: out.txt\n".into(),
        });
        let s = scaffold(&a).unwrap();

        assert_eq!(s.config_file, "loop.md");
        assert!(root.join("loop.md").is_file());
        assert!(!root.join("loop.yaml").exists());
        // run.sh must point at the config that actually exists.
        let run = std::fs::read_to_string(root.join("run.sh")).unwrap();
        assert!(run.contains(" run \"loop.md\""), "got: {run}");
        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn an_unparseable_supplied_config_writes_no_directory_contents() {
        // A loop directory holding a config that cannot be read is worse than
        // no directory at all.
        let root = tmp("bad-config");
        let mut a = args(&root);
        a.config = Some(ProvidedConfig {
            markdown: false,
            text: "name: broken\ngoals: this is not a list\n".into(),
        });
        assert!(scaffold(&a).is_err());
        assert!(
            !root.join("loop.yaml").exists(),
            "a rejected config must not be written"
        );
        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn the_starter_config_validates_once_pre_execution_is_marked_done() {
        let mut cfg = starter_config("demo", "a demo loop");
        // As shipped it must NOT validate: the manual run has not happened.
        assert!(loopsmith_core::validate(&cfg).has_errors());
        for w in &mut cfg.pre_execution {
            w.done = true;
        }
        let r = loopsmith_core::validate(&cfg);
        assert!(!r.has_errors(), "unexpected errors:\n{}", r.render());
    }

    #[test]
    fn asking_for_git_produces_a_repo_a_worktree_can_actually_resolve_against() {
        // The point is not that `.git` exists. `git worktree add` resolves a
        // start point, so a repository with no commit fails exactly as
        // unhelpfully as no repository at all — which is the whole reason
        // `init_git` commits rather than just initialising.
        if loopsmith_util::which("git").is_none() {
            return;
        }
        let dir = tmp("scaffold-git");
        let mut a = args(&dir);
        a.git = true;
        let s = scaffold(&a).expect("scaffold succeeds");

        assert!(matches!(s.git, Some(Ok(()))), "git init reported: {:?}", s.git);
        assert!(dir.join(".git").exists(), "no repository was created");

        let wt = dir.join("state").join("worktrees").join("probe");
        let out = std::process::Command::new("git")
            .args(["worktree", "add", "-B", "loopsmith/probe"])
            .arg(&wt)
            .current_dir(&dir)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "a worktree must resolve against the fresh repo: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn not_asking_for_git_leaves_no_repository_and_says_nothing() {
        let dir = tmp("scaffold-nogit");
        let s = scaffold(&args(&dir)).expect("scaffold succeeds");
        assert!(s.git.is_none(), "nothing was asked for, nothing is reported");
        assert!(!dir.join(".git").exists());
    }

    #[test]
    fn init_git_on_an_existing_repository_leaves_it_alone() {
        // Scaffolding into a checkout someone already had is a plausible
        // thing to do; re-initialising it would be a surprise, not a service.
        if loopsmith_util::which("git").is_none() {
            return;
        }
        let dir = tmp("scaffold-existing-git");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git").args(["init", "-q"]).current_dir(&dir).output().unwrap();
        std::fs::write(dir.join("mine.txt"), "keep me").unwrap();

        assert!(init_git(&dir).is_ok());
        assert_eq!(
            std::fs::read_to_string(dir.join("mine.txt")).unwrap(),
            "keep me",
            "an existing working tree must not be touched"
        );
    }

    #[test]
    fn scaffold_writes_the_expected_files() {
        let root = tmp("writes");
        let s = scaffold(&NewLoopArgs {
            git: false,
            path: root.clone(),
            name: "demo".into(),
            purpose: "a demo loop".into(),
            force: false,
            config: None,
        })
        .unwrap();
        assert!(root.join("loop.yaml").exists());
        assert!(root.join(".gitignore").exists());
        assert!(root.join("README.md").exists());
        assert!(root.join("state").is_dir());
        assert!(root.join("proposals").is_dir());
        assert!(s.written.len() >= 4);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_written_config_round_trips_through_the_parser() {
        let root = tmp("roundtrip");
        scaffold(&NewLoopArgs {
            git: false,
            path: root.clone(),
            name: "demo".into(),
            purpose: "a demo loop".into(),
            force: false,
            config: None,
        })
        .unwrap();
        let cfg = loopsmith_core::load(root.join("loop.yaml")).expect("reloads");
        assert_eq!(cfg.name, "demo");
        assert_eq!(cfg.graph.nodes.len(), 2);
        assert!(cfg.providers.enforce_judge_independence);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn scaffolding_into_a_non_empty_directory_is_refused_without_force() {
        let root = tmp("occupied");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("something.txt"), b"hi").unwrap();
        let err = scaffold(&NewLoopArgs {
            git: false,
            path: root.clone(),
            name: "demo".into(),
            purpose: "p".into(),
            force: false,
            config: None,
        })
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn force_overrides_the_non_empty_check() {
        let root = tmp("forced");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("something.txt"), b"hi").unwrap();
        assert!(scaffold(&NewLoopArgs {
            git: false,
            path: root.clone(),
            name: "demo".into(),
            purpose: "p".into(),
            force: true,
            config: None,
        })
        .is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn name_falls_back_to_the_directory_name() {
        assert_eq!(name_from_path(Path::new("/tmp/my-loop")), "my-loop");
    }
}
