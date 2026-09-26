//! `loopsmith guided` driven through the real binary.
//!
//! The wizard reads a TTY when it has one and a plain answer stream when it does
//! not — and a piped stdin is exactly the second case. So these tests script the
//! whole conversation as lines of stdin and assert on what lands on disk: the
//! config the wizard wrote, run through the same validator a human would use.
//!
//! Two conventions keep the scripts readable and stable:
//!
//! - Answers use option *values* (`__byok__`, `file_exists`, `done`) rather
//!   than menu numbers, because the wizard accepts either and a value does not
//!   move when the catalog gains an entry. That is the same label/number
//!   fallback the interactive user relies on, exercised here on purpose.
//! - Every run passes `--novice`. Without it the wizard would consult the
//!   remembered level in `~/.loopsmith`, so a developer who once picked the
//!   expert path would watch these tests try to open their editor.
//!
//! An empty line means "take the default", which is how the question order
//! below stays legible: the interesting answers are the ones that are spelled
//! out.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const LOOPSMITH: &str = env!("CARGO_BIN_EXE_loopsmith");

fn scratch(tag: &str) -> std::path::PathBuf {
    loopsmith_util::testing::temp_dir(tag)
}

/// Run `loopsmith guided --novice <args>` with `lines` fed on stdin, one answer
/// per line.
fn guided(args: &[&str], lines: &[&str], cwd: &Path) -> Output {
    let script = lines.join("\n") + "\n";
    let mut child = Command::new(LOOPSMITH)
        .arg("guided")
        .arg("--novice")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(script.as_bytes())
        .expect("the script is written");
    child.wait_with_output().expect("the binary exits")
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The eleven opt-in sections, all declined: graph, background, prerequisites,
/// success, triggers, limits, phases, default skills, memory, alerts,
/// evolution. Each is one yes/no that defaults to no.
const OPTIONAL_SECTIONS: [&str; 11] = [""; 11];

/// The answers that build the smallest complete loop: one BYOK provider, one
/// goal, one `file_exists` check, a cost ceiling, and no optional sections.
fn minimal_script() -> Vec<&'static str> {
    let mut script = vec![
        // identity: name, description, version (blank takes 0.1.0), environment
        "acceptance-loop",
        "check that the artifact is produced",
        "",
        "",
        // providers: add one by hand, then finish
        "add",
        "__byok__",
        "p1",
        "echo",
        "",
        "done",
        // goals: add one, then finish
        "add",
        "g1",
        "produce the artifact correctly",
        "",
        "",
        "done",
        // checks: one file_exists check against that goal, then finish
        "add",
        "g1",
        "v1",
        "objective",
        "the output file exists",
        "file_exists",
        "out.txt",
        "",
        "",
        "done",
        // stop gates: keep every default but set a cost ceiling
        "",
        "",
        "",
        "",
        "5",
        "",
        "",
    ];
    script.extend_from_slice(&OPTIONAL_SECTIONS);
    // review: write despite warnings; markdown; no git; do not run
    script.extend_from_slice(&["", "md", "", ""]);
    script
}

#[test]
fn builds_a_valid_loop_from_a_scripted_conversation() {
    let dir = scratch("guided-minimal");
    let out = guided(&["."], &minimal_script(), &dir);
    assert!(out.status.success(), "wizard failed:\n{}", combined(&out));

    let config = dir.join("acceptance-loop").join("loop.md");
    assert!(config.is_file(), "config not written:\n{}", combined(&out));

    let text = std::fs::read_to_string(&config).unwrap();
    assert!(text.contains("acceptance-loop"), "name missing: {text}");
    assert!(text.contains("file_exists"), "detector missing: {text}");
    assert!(text.contains("max_cost_usd"), "cost ceiling missing: {text}");
    // The version question was answered with a bare Enter. A required field
    // whose preset does not reach the prompt would have blocked the whole run.
    assert!(text.contains("version: 0.1.0"), "preset not taken: {text}");

    // The real gate: the wizard's output must pass the same validator the CLI
    // runs on a hand-written file.
    let v = Command::new(LOOPSMITH)
        .args(["validate", config.to_str().unwrap()])
        .output()
        .expect("validate runs");
    assert!(
        v.status.success(),
        "generated config does not validate:\n{}",
        combined(&v)
    );
}

#[test]
fn a_yaml_choice_writes_yaml() {
    let dir = scratch("guided-yaml");
    let mut script = minimal_script();
    // The grammar answer is the only "md" in the script; find it rather than
    // indexing, so adding a question above does not silently move it.
    let grammar = script.iter().position(|s| *s == "md").unwrap();
    script[grammar] = "yaml";
    let out = guided(&["."], &script, &dir);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        dir.join("acceptance-loop").join("loop.yaml").is_file(),
        "yaml not written:\n{}",
        combined(&out)
    );
}

#[test]
fn back_returns_to_the_previous_field() {
    // Answer the name, then `:back` from the description to correct it. The
    // final name must be the corrected one, proving `:back` re-asked an earlier
    // field rather than starting the section over or losing the later answers.
    let dir = scratch("guided-back");
    let mut script = vec!["wrong-name", ":back", "right-name", "", "", ""];
    script.extend_from_slice(&minimal_script()[4..]);
    let out = guided(&["."], &script, &dir);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        dir.join("right-name").join("loop.md").is_file(),
        "corrected name not used:\n{}",
        combined(&out)
    );
    assert!(!dir.join("wrong-name").exists(), "the discarded name was written");
}

#[test]
fn quitting_writes_nothing() {
    let dir = scratch("guided-quit");
    let out = guided(&["."], &["some-loop", "", "", "", ":quit"], &dir);
    // A clean exit, and no loop directory created. The draft offer only comes
    // up at a terminal, so a piped `:quit` leaves nothing at all behind.
    assert!(out.status.success(), "{}", combined(&out));
    assert!(!dir.join("some-loop").exists(), "a quit run must write nothing");
    assert!(
        !dir.join("loop.draft.yaml").exists(),
        "a piped quit must not write a draft"
    );
}

#[test]
fn edit_rewrites_the_file_and_keeps_what_it_never_asked_about() {
    let dir = scratch("guided-edit");
    // Start from a known-good config carrying one section the wizard has no
    // question for: `safety.protected`.
    let seed = dir.join("loop.yaml");
    std::fs::write(
        &seed,
        "name: original\n\
         version: 0.1.0\n\
         goals:\n  - name: g1\n    description: a goal long enough to pass\n\
         validations:\n  - target: g1\n    name: v1\n    mode: objective\n\
         \x20   statement: the file exists\n\
         \x20   detector: { type: file_exists, path: out.txt }\n\
         safety:\n  protected:\n    components: [gates, credentials]\n",
    )
    .unwrap();

    // Rename, add the provider the seed lacks, keep the goal and check it
    // already has, keep every stop gate, decline every optional section, write.
    let mut script = vec![
        // identity
        "renamed", "", "", "",
        // providers: the seed declares none, and a loop needs one
        "add", "__byok__", "p", "echo", "", "done",
        // goals and checks are already there — finish both straight away
        "done", "done",
        // stop gates, all defaults
        "", "", "", "", "", "", "",
    ];
    script.extend_from_slice(&OPTIONAL_SECTIONS);
    // review: write despite warnings. No grammar question on an edit — the
    // file's own extension settles it.
    script.push("");

    let out = guided(&["--edit", seed.to_str().unwrap()], &script, &dir);
    assert!(out.status.success(), "{}", combined(&out));

    let text = std::fs::read_to_string(&seed).unwrap();
    assert!(text.contains("renamed"), "edit did not take:\n{text}");
    assert!(!text.contains("name: original"), "old name still present:\n{text}");
    // A `.yaml` file stays YAML: nothing asked, and Markdown here would leave a
    // file the loader refuses to read back.
    assert!(
        text.starts_with("name:"),
        "an edited .yaml was not written as YAML:\n{text}"
    );
    // The protected list was never a question, so it must survive untouched.
    assert!(
        text.contains("credentials"),
        "an unasked section was dropped by the edit:\n{text}"
    );

    let v = Command::new(LOOPSMITH)
        .args(["validate", seed.to_str().unwrap()])
        .output()
        .expect("validate runs");
    assert!(
        v.status.success(),
        "the edited config does not validate:\n{}",
        combined(&v)
    );
}
