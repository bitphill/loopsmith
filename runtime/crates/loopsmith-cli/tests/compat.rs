//! Portability: what a generated loop assumes about the machine it lands on.
//!
//! A loop directory outlives the checkout that produced it and gets copied to
//! build boxes, containers, and colleagues' laptops. Three differences break it
//! every time, and all three are invisible on the machine that wrote it:
//!
//! - **bash 3.2.** macOS still ships it, because 4.0 changed licence.
//! - **BSD versus GNU `sed`, `stat`, and `readlink`.**
//! - **Whichever scheduler is installed**, which is not implied by the OS.
//!
//! These tests run the generated scripts rather than reading them.

mod harness;

use harness::{Fixture, LOOPSMITH};
use std::path::Path;
use std::process::Command;

fn scratch(tag: &str) -> std::path::PathBuf {
    loopsmith_util::testing::temp_dir(tag)
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Scaffold a loop into a scratch directory. `loopsmith new` refuses a path
/// inside the loopsmith checkout, which is why this is never done in-tree.
fn new_loop(tag: &str) -> std::path::PathBuf {
    let dir = scratch(tag);
    let out = Command::new(LOOPSMITH)
        .args(["new", "--path", ".", "--name", "portable", "--force"])
        .current_dir(&dir)
        .output()
        .expect("the binary runs");
    assert!(out.status.success(), "{}", combined(&out));
    dir
}

// ---------------------------------------------------------------------------
// compat.sh
// ---------------------------------------------------------------------------

/// Every new loop gets the compatibility helpers its detectors will need.
#[test]
fn a_new_loop_ships_the_compatibility_helpers() {
    let dir = new_loop("compat-shipped");
    let compat = dir.join("scripts/compat.sh");
    assert!(compat.is_file(), "scripts/compat.sh must travel with the loop");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&compat).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "compat.sh must be executable");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Sourcing it must work under POSIX `sh`, not only under whatever shell the
/// author happened to have. Every helper is exercised, because a helper that is
/// never called is a helper nobody has checked.
#[test]
fn the_compatibility_helpers_work_under_posix_sh() {
    let dir = new_loop("compat-helpers");
    std::fs::write(dir.join("sample.txt"), "alpha\nbeta\n").unwrap();

    let script = "\
. ./scripts/compat.sh
compat_report
sed_i 's/alpha/ALPHA/' sample.txt
cat sample.txt
echo \"size=$(stat_size sample.txt)\"
echo \"mtime=$(stat_mtime sample.txt)\"
echo \"abs=$(readlink_f sample.txt)\"
echo \"sha=$(sha256 sample.txt)\"
require sh
echo require-ok
";
    let out = Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(&dir)
        .output()
        .expect("sh runs");
    let text = combined(&out);
    assert!(out.status.success(), "{text}");

    assert!(text.contains("os="), "compat_report must say what it found: {text}");
    assert!(
        text.contains("ALPHA"),
        "sed_i must edit in place on this userland: {text}"
    );
    assert!(text.contains("size=11"), "{text}");
    assert!(text.contains("mtime="), "{text}");
    // 64 hex characters, whichever sha tool was found.
    let sha = text
        .lines()
        .find_map(|l| l.strip_prefix("sha="))
        .expect("sha256 produced a line");
    assert_eq!(sha.len(), 64, "not a sha256 digest: {sha}");
    assert!(text.contains("require-ok"), "{text}");

    // The in-place edit must leave no backup litter, which is what a wrong
    // `sed -i` spelling produces on BSD.
    let strays: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sample.txt") && n != "sample.txt")
        .collect();
    assert!(strays.is_empty(), "sed left backups behind: {strays:?}");

    let _ = std::fs::remove_dir_all(dir);
}

/// `need_bash` must exit 2, not 1.
///
/// A detector's exit code is its verdict. "This machine cannot run the check"
/// is a different fact from "the check failed", and a gate that cannot tell
/// them apart reports missing tooling as unfinished work.
#[test]
fn need_bash_exits_two_so_a_missing_tool_is_not_read_as_a_failed_check() {
    let dir = new_loop("compat-needbash");
    // 99 is not a bash version anyone has, so this asks for the impossible on
    // every machine and the assertion holds wherever the suite runs.
    let out = Command::new("sh")
        .arg("-c")
        .arg(". ./scripts/compat.sh; need_bash 99; echo unreachable")
        .current_dir(&dir)
        .output()
        .expect("sh runs");
    assert_eq!(out.status.code(), Some(2), "{}", combined(&out));
    let text = combined(&out);
    assert!(!text.contains("unreachable"), "it must not continue: {text}");
    assert!(
        text.contains("needs bash 99"),
        "the message must name what it wanted: {text}"
    );

    // A version that exists everywhere must not trip it.
    let ok = Command::new("sh")
        .arg("-c")
        .arg(". ./scripts/compat.sh; need_bash 1; echo fine")
        .current_dir(&dir)
        .output()
        .expect("sh runs");
    assert!(combined(&ok).contains("fine"), "{}", combined(&ok));

    let _ = std::fs::remove_dir_all(dir);
}

/// `require` reports a missing command as a tooling problem, with the same
/// exit code and for the same reason.
#[test]
fn require_names_the_command_that_is_missing() {
    let dir = new_loop("compat-require");
    let out = Command::new("sh")
        .arg("-c")
        .arg(". ./scripts/compat.sh; require definitely-not-installed-xyz")
        .current_dir(&dir)
        .output()
        .expect("sh runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        combined(&out).contains("definitely-not-installed-xyz"),
        "{}",
        combined(&out)
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// The generated scripts
// ---------------------------------------------------------------------------

/// `run.sh` and `resume.sh` must be POSIX. `sh -n` parses without executing,
/// which is exactly the check that would have caught bash-4 syntax before it
/// reached a Mac.
#[test]
fn the_generated_scripts_parse_under_posix_sh() {
    let dir = new_loop("compat-parse");
    for name in ["run.sh", "resume.sh", "scripts/compat.sh"] {
        let out = Command::new("sh")
            .args(["-n", name])
            .current_dir(&dir)
            .output()
            .expect("sh runs");
        assert!(
            out.status.success(),
            "{name} is not valid POSIX sh: {}",
            combined(&out)
        );
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        assert!(
            text.starts_with("#!/bin/sh"),
            "{name} must not ask for bash: {}",
            text.lines().next().unwrap_or("")
        );
        // Scan the code, not the prose: these files document the very
        // constructs they must not use, and a check that reads comments would
        // flag its own explanation.
        let code = strip_comments(&text);
        for bashism in ["[[", "declare -A", "mapfile", "readarray", "${!", "&>>"] {
            assert!(
                !code.contains(bashism),
                "{name} uses `{bashism}`, which bash 3.2 or dash will reject"
            );
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Everything from the first `#` on each line, dropped.
///
/// Crude, and sufficient here: none of the generated scripts puts a `#` inside
/// a quoted string, and the alternative is a shell lexer in a test.
fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|l| l.split_once('#').map_or(l, |(before, _)| before))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `resume.sh` with no argument must explain itself and exit 2 rather than
/// invoking the binary with an empty run id.
#[test]
fn resume_without_a_run_id_explains_itself() {
    let dir = new_loop("compat-resume");
    let out = Command::new("./resume.sh")
        .current_dir(&dir)
        .output()
        .expect("the script runs");
    assert_eq!(out.status.code(), Some(2), "{}", combined(&out));
    assert!(combined(&out).contains("usage: ./resume.sh"), "{}", combined(&out));
    let _ = std::fs::remove_dir_all(dir);
}

/// A loop directory outlives the binary it was made against. The scripts pin an
/// absolute path — cron and launchd do not inherit a login shell's `PATH` — but
/// must fall back to `PATH` rather than failing with a bare "not found".
#[test]
fn a_generated_script_falls_back_to_path_when_the_pinned_binary_has_moved() {
    let dir = new_loop("compat-moved");
    let script = std::fs::read_to_string(dir.join("run.sh")).unwrap();
    assert!(
        script.contains(LOOPSMITH),
        "run.sh should pin the absolute binary: {script}"
    );

    // Repoint it at somewhere that does not exist, and give it no PATH either.
    let rewritten = script.replace(LOOPSMITH, "/nonexistent/loopsmith");
    std::fs::write(dir.join("run.sh"), &rewritten).unwrap();

    let out = Command::new("./run.sh")
        .current_dir(&dir)
        .env("PATH", "/nonexistent")
        .output()
        .expect("the script runs");
    assert_eq!(
        out.status.code(),
        Some(127),
        "a missing binary is 127, not a silent failure: {}",
        combined(&out)
    );
    let text = combined(&out);
    assert!(text.contains("not on PATH"), "{text}");
    assert!(
        text.contains("Re-point it"),
        "the message must say what to do: {text}"
    );

    // With the real binary on PATH, the fallback finds it.
    let bin_dir = Path::new(LOOPSMITH).parent().unwrap();
    let found = Command::new("./run.sh")
        .args(["--dry-run"])
        .current_dir(&dir)
        .env("PATH", bin_dir)
        .output()
        .expect("the script runs");
    assert_ne!(
        found.status.code(),
        Some(127),
        "the PATH fallback did not fire: {}",
        combined(&found)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A success export travels further than anything else a loop produces, so its
/// script must assume least of all.
#[test]
fn the_export_script_is_posix_and_does_not_pin_a_binary() {
    let f = Fixture::from_yaml(
        r#"
name: exported
goals:
  - name: g1
    description: a goal description long enough to satisfy the validator
pre_execution:
  - step: done by hand
    done: true
validations:
  - target: g1
    name: v1
    mode: objective
    statement: always true
    detector: { type: script, command: "true" }
  - target: overall
    name: ov
    mode: objective
    statement: always true
    detector: { type: script, command: "true" }
graph:
  nodes:
    - id: build
      role: builder
      instruction: produce the thing the goal describes
      goals: [g1]
providers:
  providers:
    - id: p
      kind: byok
      command: echo
      args: ["ok"]
  cascade:
    standard: [p]
"#,
        "compat-export",
    );
    f.run_loop("exp", &[]);

    let script = f.export_dir().join("run.sh");
    assert!(script.is_file(), "the run should have exported");

    let out = Command::new("sh")
        .args(["-n", "run.sh"])
        .current_dir(f.export_dir())
        .output()
        .expect("sh runs");
    assert!(out.status.success(), "{}", combined(&out));

    let text = std::fs::read_to_string(&script).unwrap();
    assert!(text.starts_with("#!/bin/sh"), "{text}");
    assert!(
        !text.contains(LOOPSMITH),
        "an export must not pin a path from the machine that made it: {text}"
    );
    assert!(
        text.contains("not on PATH"),
        "and must say so when the binary is absent: {text}"
    );
    f.cleanup();
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

/// `doctor` must report the three facts that change what a script may assume,
/// and must be advisory: reporting a constraint is not the machine being
/// unusable, and a non-zero exit would fail a CI step that was working.
#[test]
fn doctor_reports_the_platform_and_stays_advisory() {
    let dir = scratch("doctor");
    let out = Command::new(LOOPSMITH)
        .arg("doctor")
        .current_dir(&dir)
        .output()
        .expect("the binary runs");
    assert!(out.status.success(), "doctor must not fail a build");

    let text = combined(&out);
    for expected in ["os", "userland", "bash", "scheduler", "git"] {
        assert!(text.contains(expected), "doctor omitted `{expected}`:\n{text}");
    }

    // The in-place spelling is the single most common portability mistake, so
    // it is quoted rather than described.
    assert!(
        text.contains("sed -i"),
        "doctor should show the in-place spelling for this userland:\n{text}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Pointed at a config, `doctor` also reports what that config needs and this
/// machine has not got — chiefly detector scripts that do not exist, which
/// would otherwise surface as a detector error at the first gate evaluation.
#[test]
fn doctor_reports_detectors_the_machine_cannot_run() {
    let f = Fixture::example("traffic-loop", "doctor-config");
    let out = f.run(&["doctor", "loop.yaml"]);
    let text = combined(&out);
    assert!(out.status.success(), "{text}");
    assert!(
        text.contains("check-venues.sh"),
        "a missing detector must be named:\n{text}"
    );
    assert!(
        text.contains("does not exist"),
        "and the reason given:\n{text}"
    );

    // Generate the stubs and it should stop complaining about them.
    let f = f.stub_scripts(harness::Stubs::Pass);
    let after = combined(&f.run(&["doctor", "loop.yaml"]));
    assert!(
        !after.contains("does not exist"),
        "the stubs exist now:\n{after}"
    );
    f.cleanup();
}

/// A detector that exists but is not executable is the other half of the same
/// mistake, and reads as a detector error rather than as a permissions problem.
#[test]
fn doctor_reports_a_detector_that_is_not_executable() {
    let f = Fixture::example("traffic-loop", "doctor-chmod").stub_scripts(harness::Stubs::Pass);
    let stub = f.dir.join("scripts/check-venues.sh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let text = combined(&f.run(&["doctor", "loop.yaml"]));
    assert!(
        text.contains("not executable") && text.contains("chmod +x"),
        "the fix must be in the message:\n{text}"
    );
    f.cleanup();
}
