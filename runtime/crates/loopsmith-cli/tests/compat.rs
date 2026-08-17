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

/// Everything from the first comment marker on each line, dropped.
///
/// Crude, and sufficient here: none of the generated scripts puts a `#` inside a
/// quoted string, and the alternative is a shell lexer in a test.
///
/// The `rem ` case is the same trap in the other dialect. These files document
/// the constructs they must not use, so a check that reads prose flags its own
/// explanation — `rem POSIX sh, so no [[ … ]]` is a comment, not a bashism.
fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|l| l.split_once('#').map_or(l, |(before, _)| before))
        .map(|l| {
            let trimmed = l.trim_start();
            if trimmed.len() >= 4 && trimmed[..4].eq_ignore_ascii_case("rem ") {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A `.cmd` launcher travels beside every `.sh` one, on every host, and
/// `cmd.exe` requires CRLF in a batch file. With LF only, the trailing newline
/// becomes part of the last token on the line, so `exit /b 2` turns into an
/// unknown command with no useful message attached.
///
/// This machine cannot execute a `.cmd`, so what is checkable here is its shape.
/// The CI matrix runs the Windows leg that actually invokes it.
#[test]
fn the_generated_cmd_launchers_are_crlf_and_shaped_for_cmd_exe() {
    let dir = new_loop("compat-cmd");
    for name in ["run.cmd", "resume.cmd"] {
        let raw = std::fs::read(dir.join(name)).expect("the cmd launcher travels too");
        let text = String::from_utf8(raw).expect("utf-8");

        assert!(
            text.starts_with("@echo off\r\n"),
            "{name} must start with `@echo off` and CRLF: {:?}",
            &text[..text.len().min(20)]
        );
        let lone_lf = text
            .as_bytes()
            .windows(2)
            .filter(|w| w[1] == b'\n' && w[0] != b'\r')
            .count();
        assert_eq!(lone_lf, 0, "{name} has {lone_lf} LF-only line ending(s)");

        // Absolute, because Task Scheduler does not inherit an interactive
        // shell's PATH, and a `where` fallback for when the binary has moved.
        assert!(
            text.contains("set \"LOOPSMITH="),
            "{name} must pin the binary: {text}"
        );
        assert!(
            text.contains("where loopsmith"),
            "{name} must fall back to PATH: {text}"
        );
        // Exactly one exit, on the last line, reached by every path.
        //
        // Two separate cmd.exe traps live here and Windows CI walked into both.
        // `setlocal` saves the errorlevel and the implicit `endlocal` restores
        // it, so an early `exit /b 127` reports 0 — a loop whose binary had moved
        // printed its diagnostic and then exited successfully. Writing
        // `endlocal & exit /b 127` fixes that on a top-level line but *not*
        // inside a nested `if ( … )` block, which is where the broken one was.
        // A single exit point needs no reasoning about block parsing.
        let code_lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with("rem "))
            .collect();

        let exits: Vec<&&str> = code_lines
            .iter()
            .filter(|l| l.contains("exit /b"))
            .collect();
        assert_eq!(
            exits.len(),
            1,
            "{name} must have exactly one exit, got {exits:?}"
        );
        assert_eq!(
            *exits[0], "endlocal & exit /b %CODE%",
            "{name}'s single exit must carry the captured code: {text}"
        );
        assert!(
            code_lines.contains(&":loopsmith_done"),
            "{name} needs the label its early paths jump to: {text}"
        );

        // Delayed expansion, because a parenthesised block is parsed before it
        // runs: `%ERRORLEVEL%` inside one expands to the value from *before* the
        // block, which is the same class of bug one level down.
        assert!(
            code_lines.contains(&"setlocal enabledelayedexpansion"),
            "{name} must enable delayed expansion: {text}"
        );
        assert!(
            !text.contains("set \"CODE=%ERRORLEVEL%\""),
            "{name} captures the exit code with parse-time expansion, which reads \
             the value from before the command ran; use !ERRORLEVEL!: {text}"
        );
        assert!(
            text.contains("cd /d \"%~dp0\""),
            "{name} must run from its own directory: {text}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// The `format!`-with-`\`-line-continuation trap, pinned.
///
/// A `\` continuation inside a non-raw `format!` eats the leading whitespace of
/// the next line, and the generated `run.sh` reached disk flat and unindented
/// because of it. Nothing about that fails a build or a parse, so the only way
/// it stays fixed is a test that reads the layout back.
#[test]
fn the_generated_scripts_keep_the_indentation_they_were_written_with() {
    let dir = new_loop("compat-layout");

    let run = std::fs::read_to_string(dir.join("run.sh")).unwrap();
    // The `if [ ! -x … ]` block in the header nests two levels deep. Flattened
    // output still parses and still runs; it just becomes unreadable, which is
    // exactly why this needs asserting rather than eyeballing.
    assert!(
        run.contains("\n  if command -v loopsmith"),
        "the header lost its 2-space nesting:\n{run}"
    );
    assert!(
        run.contains("\n    LOOPSMITH=$(command -v loopsmith)"),
        "the header lost its 4-space nesting:\n{run}"
    );
    assert!(
        run.contains("\n  else\n"),
        "the `else` should sit at the outer block's indent:\n{run}"
    );

    let resume = std::fs::read_to_string(dir.join("resume.sh")).unwrap();
    assert!(
        resume.contains("\n  echo \"usage: ./resume.sh <run-id>\" >&2"),
        "resume.sh lost the indentation of its usage block:\n{resume}"
    );

    let cmd = std::fs::read_to_string(dir.join("resume.cmd")).unwrap();
    assert!(
        cmd.contains("\r\n  echo usage: resume.cmd"),
        "resume.cmd lost the indentation of its usage block:\n{cmd}"
    );

    let _ = std::fs::remove_dir_all(dir);
}

/// The launcher this host can actually execute, as a runnable `Command`.
///
/// Windows cannot run a `#!` script and no POSIX shell will run a `.cmd`, which is
/// exactly why both are generated. A test that hardcodes `./run.sh` therefore
/// tests nothing on Windows — and gating it out with `#[cfg(unix)]` would leave
/// the `.cmd` launcher unexercised on the only platform that runs it. Picking the
/// right one keeps a single test meaningful on both.
fn launcher(dir: &std::path::Path, stem: &str) -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/c").arg(format!("{stem}.cmd")).current_dir(dir);
        c
    } else {
        let mut c = Command::new(format!("./{stem}.sh"));
        c.current_dir(dir);
        c
    }
}

/// The name of the launcher this host runs, for messages and for rewriting.
fn launcher_file(stem: &str) -> String {
    format!("{stem}.{}", if cfg!(windows) { "cmd" } else { "sh" })
}

/// A `PATH` containing only `dir`, plus whatever the platform needs to function.
///
/// On unix that is just `dir`: the `.sh` launcher looks a command up with
/// `command -v`, a shell builtin that needs nothing on `PATH` to work.
///
/// On Windows the equivalent is `where.exe`, which lives in `System32` — so a
/// `PATH` of only `dir` removes the launcher's ability to search at all, and the
/// test stops measuring the fallback and starts measuring whether `where` exists.
/// `cmd.exe` itself is in the same directory. A machine without `System32` on
/// `PATH` is broken in a way no launcher should try to survive.
fn with_system_path(dir: impl AsRef<Path>) -> std::ffi::OsString {
    let dir = dir.as_ref().as_os_str().to_os_string();
    if !cfg!(windows) {
        return dir;
    }
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let mut path = dir;
    path.push(";");
    path.push(&root);
    path.push(r"\System32");
    path
}

/// `resume` with no argument must explain itself and exit 2 rather than invoking
/// the binary with an empty run id.
#[test]
fn resume_without_a_run_id_explains_itself() {
    let dir = new_loop("compat-resume");
    let out = launcher(&dir, "resume")
        .output()
        .expect("the launcher runs");
    assert_eq!(out.status.code(), Some(2), "{}", combined(&out));
    // Each dialect spells its own usage line; both must name the argument.
    assert!(
        combined(&out).contains("usage: ./resume.sh") || combined(&out).contains("usage: resume.cmd"),
        "{}",
        combined(&out)
    );
    assert!(combined(&out).contains("run-id"), "{}", combined(&out));
    let _ = std::fs::remove_dir_all(dir);
}

/// A loop directory outlives the binary it was made against. The scripts pin an
/// absolute path — cron and launchd do not inherit a login shell's `PATH` — but
/// must fall back to `PATH` rather than failing with a bare "not found".
#[test]
fn a_generated_script_falls_back_to_path_when_the_pinned_binary_has_moved() {
    let dir = new_loop("compat-moved");
    let file = launcher_file("run");
    let script = std::fs::read_to_string(dir.join(&file)).unwrap();
    assert!(
        script.contains(LOOPSMITH),
        "{file} should pin the absolute binary: {script}"
    );

    // Repoint it at somewhere that does not exist, and give it no PATH either.
    // The bogus path is spelled for this platform: on Windows an absolute path
    // starts with a drive letter, and `if not exist` on a malformed one is not
    // the same check.
    let missing = if cfg!(windows) {
        r"C:\nonexistent\loopsmith.exe"
    } else {
        "/nonexistent/loopsmith"
    };
    let empty_path = with_system_path("nonexistent");
    let rewritten = script.replace(LOOPSMITH, missing);
    std::fs::write(dir.join(&file), &rewritten).unwrap();

    let out = launcher(&dir, "run")
        .env("PATH", empty_path)
        .output()
        .expect("the launcher runs");
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
    let found = launcher(&dir, "run")
        .args(["--dry-run"])
        .env("PATH", with_system_path(bin_dir))
        .output()
        .expect("the launcher runs");
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
///
/// Unix only, and not merely because `set_permissions` needs `PermissionsExt`:
/// there is no executable bit to clear off unix, so `loopsmith_util::is_executable`
/// degrades to a file check and `doctor` is *right* not to report anything. Gating
/// the whole test says that, where a `#[cfg]` around just the chmod would have
/// left a test that asserts the wrong thing on Windows.
#[cfg(unix)]
#[test]
fn doctor_reports_a_detector_that_is_not_executable() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::example("traffic-loop", "doctor-chmod").stub_scripts(harness::Stubs::Pass);
    let stub = f.dir.join("scripts/check-venues.sh");
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o644)).unwrap();

    let text = combined(&f.run(&["doctor", "loop.yaml"]));
    assert!(
        text.contains("not executable") && text.contains("chmod +x"),
        "the fix must be in the message:\n{text}"
    );
    f.cleanup();
}
