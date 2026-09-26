//! The 1.0 noun-verb grammar, and the 0.3 spellings it did not break.
//!
//! The unit tests in `cli::alias` prove the argument rewrite. These prove the
//! whole binary honours it: that both spellings reach the same command, that
//! the old one says so once on stderr and the new one says nothing, and that
//! the two lines every generated launcher and crontab entry contains still
//! run the loop.

mod harness;

use harness::{examples_dir, LOOPSMITH};
use std::process::{Command, Output};

fn loopsmith(args: &[&str]) -> Output {
    Command::new(LOOPSMITH)
        .args(args)
        .output()
        .expect("the binary runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// A shipped example. Every one refuses validation with exactly one error, on
/// purpose, so these assert on the *report* rather than on the exit code.
fn example() -> String {
    examples_dir()
        .join("research-loop.yaml")
        .to_string_lossy()
        .to_string()
}

#[test]
fn both_spellings_of_a_moved_verb_do_the_same_thing() {
    let cfg = example();
    let new = loopsmith(&["loop", "validate", &cfg]);
    let old = loopsmith(&["validate", &cfg]);

    assert_eq!(new.status.code(), old.status.code(), "different exit codes");
    assert_eq!(stdout(&new), stdout(&old), "different reports");
    assert!(
        stdout(&new).contains("error(s)"),
        "the validator did not run: {}",
        stdout(&new)
    );
}

#[test]
fn only_the_old_spelling_says_it_is_old() {
    let cfg = example();
    assert!(
        !stderr(&loopsmith(&["loop", "validate", &cfg])).contains("goes away in 2.0"),
        "the 1.0 spelling printed a deprecation notice"
    );
    let note = stderr(&loopsmith(&["validate", &cfg]));
    assert!(
        note.contains("`loopsmith loop validate`") && note.contains("goes away in 2.0"),
        "the 0.3 spelling did not name its replacement: {note}"
    );
    assert_eq!(
        note.lines().filter(|l| l.starts_with("note:")).count(),
        1,
        "the notice is one line, not a banner: {note}"
    );
}

#[test]
fn the_two_lines_in_every_generated_launcher_still_run_the_loop() {
    // `run.sh` says `loopsmith run <config>` and `resume.sh` says
    // `loopsmith resume <config> <id>`. Both are in loop directories and
    // crontabs that this release must not break.
    let cfg = example();
    let run = loopsmith(&["run", &cfg, "--dry-run"]);
    assert!(
        stderr(&run).contains("run start"),
        "`run <config>` did not report its new spelling: {}",
        stderr(&run)
    );
    // The example refuses to run until its prerequisites are marked done, so
    // reaching that refusal is proof the command was routed, not misparsed.
    assert!(
        stderr(&run).contains("prerequisites") || stdout(&run).contains("prerequisites"),
        "`run <config>` did not reach the run command: {}{}",
        stdout(&run),
        stderr(&run)
    );

    let resume = loopsmith(&["resume", &cfg, "no-such-run"]);
    assert!(
        stderr(&resume).contains("`loopsmith run resume`"),
        "`resume` did not report its new spelling: {}",
        stderr(&resume)
    );
}

#[test]
fn a_run_verb_is_never_mistaken_for_a_config_path() {
    // `loopsmith run status <config> <id>` has to be the status command, not
    // an attempt to run a loop stored in a file called `status`.
    let out = loopsmith(&["run", "status", "no-such-config.yaml", "r-1"]);
    let all = format!("{}{}", stdout(&out), stderr(&out));
    assert!(
        !all.contains("run start"),
        "`run status` was rewritten as a 0.3 run: {all}"
    );
}

#[test]
fn the_nouns_are_the_top_level_surface() {
    let help = stdout(&loopsmith(&["--help"]));
    for noun in ["loop", "run", "memory", "skills", "doctor", "providers", "web", "mcp"] {
        assert!(help.contains(noun), "`{noun}` is missing from the help: {help}");
    }
    // The moved verbs are gone from the top level: they are the point of the
    // regroup, and leaving them listed would make the help longer, not shorter.
    for verb in ["validate", "resume", "ledger", "proposals"] {
        assert!(
            !help.contains(&format!("  {verb} ")),
            "`{verb}` is still listed at the top level: {help}"
        );
    }
}
