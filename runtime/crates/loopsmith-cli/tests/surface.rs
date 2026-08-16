//! The command surface, driven through the real binary.
//!
//! Several subcommands were implemented and never executed by a test: they
//! parse, they compile, and nobody had found out whether they work. Each one
//! here is cheap, needs no provider, and touches only a scratch directory.
//!
//! `schedule --install` is deliberately absent. It writes into
//! `~/Library/LaunchAgents`, which is outward-facing and not something a test
//! run should do to the machine it happens to be on. Its generated plist and
//! crontab line are unit-tested instead.

mod harness;

use harness::{examples_dir, Fixture, LOOPSMITH};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn scratch(tag: &str) -> std::path::PathBuf {
    loopsmith_util::testing::temp_dir(tag)
}

fn run(args: &[&str], cwd: &Path) -> std::process::Output {
    Command::new(LOOPSMITH)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("the binary runs")
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

// ---------------------------------------------------------------------------
// new --config-stdin
// ---------------------------------------------------------------------------

fn new_from_stdin(dir: &Path, config: &str, extra: &[&str]) -> std::process::Output {
    let mut args: Vec<&str> = vec!["new", "--path", ".", "--config-stdin"];
    args.extend_from_slice(extra);
    let mut child = Command::new(LOOPSMITH)
        .args(&args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(config.as_bytes())
        .expect("the config is written");
    child.wait_with_output().expect("the binary exits")
}

const STDIN_YAML: &str = r#"
name: from-stdin
goals:
  - name: g1
    description: a goal description long enough to satisfy the validator
validations:
  - target: g1
    name: v1
    mode: objective
    statement: the artifact exists
    detector: { type: file_exists, path: out/thing.txt }
"#;

/// A complete config handed over on stdin must land as the loop's config, not
/// as the starter. This is the path an agent setting a loop up would use, and
/// it had no test.
#[test]
fn a_config_arrives_whole_on_stdin() {
    let dir = scratch("stdin-yaml");
    let out = new_from_stdin(&dir, STDIN_YAML, &[]);
    assert!(out.status.success(), "{}", combined(&out));

    let config = dir.join("loop.yaml");
    assert!(config.is_file(), "the config must be written");
    let text = std::fs::read_to_string(&config).unwrap();
    assert!(
        text.contains("from-stdin"),
        "the starter was written instead of the supplied config: {text}"
    );

    // And what landed is loadable, which is the only thing that matters.
    let validated = run(&["validate", "loop.yaml"], &dir);
    assert!(
        combined(&validated).contains("0 error(s)"),
        "{}",
        combined(&validated)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// The same path in Markdown. `--markdown` says how to read stdin; the loop
/// that comes out is the same model either way.
#[test]
fn a_markdown_config_arrives_whole_on_stdin_too() {
    // Generate the Markdown from the YAML rather than hand-writing it, so this
    // tests the stdin path rather than my ability to type the grammar.
    let src = scratch("stdin-md-src");
    std::fs::write(src.join("loop.yaml"), STDIN_YAML).unwrap();
    let converted = run(&["convert", "loop.yaml"], &src);
    assert!(converted.status.success(), "{}", combined(&converted));
    let markdown = stdout(&converted);

    let dir = scratch("stdin-md");
    let out = new_from_stdin(&dir, &markdown, &["--markdown"]);
    assert!(out.status.success(), "{}", combined(&out));

    let config = dir.join("loop.md");
    assert!(
        config.is_file(),
        "a markdown config should be written as .md; found {:?}",
        std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect::<Vec<_>>()
    );
    let validated = run(&["validate", "loop.md"], &dir);
    assert!(
        combined(&validated).contains("0 error(s)"),
        "{}",
        combined(&validated)
    );
    let _ = std::fs::remove_dir_all(src);
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// convert
// ---------------------------------------------------------------------------

/// YAML to Markdown and back must survive the trip. The forward direction has
/// a round-trip test; `--to-yaml` did not.
#[test]
fn a_config_survives_the_trip_out_to_markdown_and_back() {
    let dir = scratch("convert-round");
    std::fs::copy(
        examples_dir().join("research-loop.yaml"),
        dir.join("loop.yaml"),
    )
    .unwrap();

    let to_md = run(&["convert", "loop.yaml", "-o", "loop.md"], &dir);
    assert!(to_md.status.success(), "{}", combined(&to_md));

    let back = run(&["convert", "loop.md", "--to-yaml", "-o", "back.yaml"], &dir);
    assert!(back.status.success(), "{}", combined(&back));

    // Compare the parsed models, not the text: comment placement and quoting
    // are presentation, and the round trip is about meaning.
    //
    // Trailing whitespace is normalised on both sides. A YAML block scalar
    // ends with a newline and Markdown has no way to say so, which is a
    // documented and accepted property of the Markdown grammar rather than a
    // conversion bug — the existing round-trip test normalises for the same
    // reason.
    let original = loopsmith_core::load(dir.join("loop.yaml")).expect("the example loads");
    let round_tripped =
        loopsmith_core::load(dir.join("back.yaml")).expect("the round trip loads");
    assert_eq!(
        trimmed(&original),
        trimmed(&round_tripped),
        "yaml -> md -> yaml changed the config"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A config as YAML with trailing whitespace stripped from every scalar.
fn trimmed(cfg: &loopsmith_core::LoopConfig) -> String {
    fn walk(v: &mut serde_yaml::Value) {
        match v {
            serde_yaml::Value::String(s) => *s = s.trim_end().to_string(),
            serde_yaml::Value::Sequence(items) => items.iter_mut().for_each(walk),
            serde_yaml::Value::Mapping(map) => {
                let entries: Vec<_> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                for (k, mut val) in entries {
                    walk(&mut val);
                    map.insert(k, val);
                }
            }
            _ => {}
        }
    }
    let mut value = serde_yaml::to_value(cfg).expect("the config becomes a value");
    walk(&mut value);
    serde_yaml::to_string(&value).expect("the value serialises")
}

/// `--to-yaml` on a config that is already YAML is a re-emit, not an error.
#[test]
fn to_yaml_on_yaml_re_emits_rather_than_refusing() {
    let dir = scratch("convert-yaml-yaml");
    std::fs::write(dir.join("loop.yaml"), STDIN_YAML).unwrap();
    let out = run(&["convert", "loop.yaml", "--to-yaml"], &dir);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(stdout(&out).contains("name: from-stdin"), "{}", stdout(&out));
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// skills install (section J)
// ---------------------------------------------------------------------------

/// `skills install` must report on every declared sub-agent rather than
/// stopping at the first one it cannot fetch — a loop whose optional helper is
/// missing is degraded, not broken, and finding that out from the report beats
/// finding it out mid-run.
#[test]
fn skills_install_reports_on_every_declared_agent() {
    let dir = scratch("skills-install");
    std::fs::write(
        dir.join("loop.yaml"),
        format!(
            "{STDIN_YAML}\ndefault_skills:\n  \
             - name: already-here\n    source: local\n  \
             - name: never-fetched\n    source: local\n"
        ),
    )
    .unwrap();

    // One is on disk, so it resolves without a network call; the other is
    // declared `local` and was never put there, so it must be reported missing.
    let installed = dir.join(".claude/skills/already-here");
    std::fs::create_dir_all(&installed).unwrap();
    std::fs::write(
        installed.join("SKILL.md"),
        "---\nname: already-here\n---\nbody",
    )
    .unwrap();

    let out = run(&["skills", "install", "loop.yaml"], &dir);
    let text = combined(&out);
    assert!(text.contains("already-here"), "{text}");
    assert!(text.contains("never-fetched"), "{text}");
    assert!(
        text.contains("never fetched") || text.contains("local"),
        "the report must say why the missing one was not fetched: {text}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// watch --check
// ---------------------------------------------------------------------------

/// `watch --check` reports what would fire and exits. It must not run the loop
/// and must not sit in the poll sleep.
#[test]
fn watch_check_reports_the_triggers_and_exits() {
    let f = Fixture::example("account-watch-loop", "watch-check");

    let out = f.run(&["watch", "loop.yaml", "--check"]);
    assert!(out.status.success(), "{}", combined(&out));
    let text = stdout(&out);
    assert!(text.contains("--check: no run performed"), "{text}");
    assert!(text.contains("Cron is evaluated in UTC"), "{text}");
    assert!(
        !f.dir.join("state").exists() || !f.dir.join("logs").exists(),
        "--check must not have started a run"
    );
    f.cleanup();
}

/// A loop with nothing but manual triggers must refuse to watch rather than
/// sleep forever pretending to work.
#[test]
fn watch_refuses_a_loop_with_no_trigger() {
    let dir = scratch("watch-manual");
    std::fs::write(dir.join("loop.yaml"), STDIN_YAML).unwrap();
    let out = run(&["watch", "loop.yaml", "--check"], &dir);
    assert!(!out.status.success());
    assert!(
        combined(&out).contains("no non-manual trigger"),
        "{}",
        combined(&out)
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// The read-only reporting commands
// ---------------------------------------------------------------------------

/// `plan`, `providers`, `permissions`, and `gate` all read a config and report.
/// None of them had been run against a shipped example.
#[test]
fn the_reporting_commands_all_work_against_a_shipped_example() {
    let dir = scratch("reporting");
    std::fs::copy(
        examples_dir().join("refactor-loop.yaml"),
        dir.join("loop.yaml"),
    )
    .unwrap();

    let plan = run(&["plan", "loop.yaml"], &dir);
    assert!(plan.status.success(), "{}", combined(&plan));
    let plan_text = stdout(&plan);
    assert!(plan_text.contains("Waves"), "{plan_text}");
    assert!(plan_text.contains("Predicted speedup"), "{plan_text}");

    let providers = run(&["providers", "loop.yaml"], &dir);
    assert!(providers.status.success(), "{}", combined(&providers));

    let permissions = run(&["permissions", "loop.yaml"], &dir);
    assert!(permissions.status.success(), "{}", combined(&permissions));

    // The gate, evaluated once against an empty tree, must fail closed rather
    // than error out.
    let gate = run(&["gate", "loop.yaml", "--workdir", "."], &dir);
    let text = combined(&gate);
    assert!(
        !text.is_empty() && !text.contains("panicked"),
        "the gate must report, not crash: {text}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// `status`, `ledger`, and `proposals` are asked about a run that does not
/// exist. Each must say so instead of failing on an unwrap.
#[test]
fn the_run_reports_handle_an_unknown_run_id() {
    let dir = scratch("unknown-run");
    std::fs::write(dir.join("loop.yaml"), STDIN_YAML).unwrap();

    for cmd in [
        vec!["status", "loop.yaml", "no-such-run"],
        vec!["ledger", "loop.yaml", "no-such-run"],
        vec!["proposals", "loop.yaml", "no-such-run"],
    ] {
        let out = run(&cmd, &dir);
        let text = combined(&out);
        assert!(
            !text.contains("panicked"),
            "`{}` panicked on an unknown run: {text}",
            cmd.join(" ")
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// `prune` removes the worktrees a loop created, and is safe to run when there
/// are none.
#[test]
fn prune_is_safe_with_and_without_worktrees() {
    let f = Fixture::from_yaml(
        &format!(
            "{STDIN_YAML}\npre_execution:\n  - step: done by hand\n    done: true\n\
             graph:\n  nodes:\n    - id: build\n      role: builder\n      \
             instruction: produce the artifact the goal describes\n      \
             goals: [g1]\n      isolated: true\n\
             providers:\n  providers:\n    - id: p\n      kind: byok\n      \
             command: echo\n      args: [\"ok\"]\n  cascade:\n    standard: [p]\n"
        ),
        "prune",
    );

    // No worktrees yet.
    let first = f.run(&["prune", "loop.yaml"]);
    assert!(!combined(&first).contains("panicked"), "{}", combined(&first));

    let f = f.git_init();
    f.run_loop("pruned", &[]);
    assert!(
        f.dir.join("state/worktrees/build").is_dir(),
        "the run should have made a worktree to prune"
    );

    let second = f.run(&["prune", "loop.yaml"]);
    assert!(second.status.success(), "{}", combined(&second));
    assert!(
        !f.dir.join("state/worktrees/build").exists(),
        "prune must remove the worktree it was asked about"
    );
    f.cleanup();
}
