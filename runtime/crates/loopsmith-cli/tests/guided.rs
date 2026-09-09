//! `loopsmith guided` driven through the real binary.
//!
//! The wizard reads a TTY when it has one and a plain answer stream when it does
//! not — and a piped stdin is exactly the second case. So these tests script the
//! whole conversation as lines of stdin and assert on what lands on disk: the
//! config the wizard wrote, run through the same validator a human would use.
//!
//! Answers use option *values* (`__byok__`, `file_exists`, `done`) rather than
//! menu numbers, because the wizard accepts either and a value does not move
//! when the catalog gains an entry. That is the same label/number fallback the
//! interactive user relies on, exercised here on purpose.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const LOOPSMITH: &str = env!("CARGO_BIN_EXE_loopsmith");

fn scratch(tag: &str) -> std::path::PathBuf {
    loopsmith_util::testing::temp_dir(tag)
}

/// Run `loopsmith guided <args>` with `lines` fed on stdin, one answer per line.
fn guided(args: &[&str], lines: &[&str], cwd: &Path) -> Output {
    let script = lines.join("\n") + "\n";
    let mut child = Command::new(LOOPSMITH)
        .arg("guided")
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

/// The answers that build the smallest complete loop: one BYOK provider, one
/// goal, one file_exists validation, a cost ceiling, and no advanced sections.
fn minimal_script() -> Vec<&'static str> {
    vec![
        // identity: name, description, version
        "acceptance-loop", "", "",
        // providers: add a BYOK provider, then finish
        "add", "__byok__", "p1", "byok", "echo", "", "", "", "", "", "done",
        // goals: add one, then finish
        "add", "g1", "produce the artifact correctly", "", "", "done",
        // validations: add one file_exists check, then finish
        "add", "g1", "v1", "objective", "the output file exists",
        "file_exists", "out.txt", "", "", "done",
        // stop gates: keep defaults but set a cost ceiling
        "", "", "", "", "5", "", "",
        // nine advanced sections, all declined
        "", "", "", "", "", "", "", "", "",
        // review: write despite warnings; markdown; git no; run no
        "", "md", "", "",
    ]
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

    // The real gate: the wizard's output must pass the same validator the CLI
    // runs on a hand-written file.
    let v = Command::new(LOOPSMITH)
        .args(["validate", config.to_str().unwrap()])
        .output()
        .expect("validate runs");
    assert!(v.status.success(), "generated config does not validate:\n{}", combined(&v));
}

#[test]
fn a_yaml_choice_writes_yaml() {
    let dir = scratch("guided-yaml");
    let mut script = minimal_script();
    // The format answer is the second-to-last-but-one entry; rebuild the tail
    // explicitly rather than index into it.
    let format_pos = script.iter().position(|s| *s == "md").unwrap();
    script[format_pos] = "yaml";
    let out = guided(&["."], &script, &dir);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(dir.join("acceptance-loop").join("loop.yaml").is_file(), "yaml not written");
}

#[test]
fn back_returns_to_the_previous_field() {
    // Answer the description, then `:back` to the name to correct it. The final
    // name must be the corrected one, proving `:back` re-asked an earlier field
    // rather than starting the section over or losing the later answer.
    let dir = scratch("guided-back");
    let mut script = vec!["wrong-name", ":back", "right-name", "", ""];
    script.extend_from_slice(&minimal_script()[3..]);
    let out = guided(&["."], &script, &dir);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(dir.join("right-name").join("loop.md").is_file(), "corrected name not used:\n{}", combined(&out));
    assert!(!dir.join("wrong-name").exists(), "the discarded name was written");
}

#[test]
fn quitting_writes_nothing() {
    let dir = scratch("guided-quit");
    let out = guided(&["."], &["some-loop", "", "", ":quit"], &dir);
    // A clean exit, and no loop directory created.
    assert!(out.status.success(), "{}", combined(&out));
    assert!(!dir.join("some-loop").exists(), "a quit run must write nothing");
}

#[test]
fn edit_loads_an_existing_config_and_rewrites_it() {
    let dir = scratch("guided-edit");
    // Start from a known-good config on disk.
    let seed = dir.join("loop.yaml");
    std::fs::write(
        &seed,
        "name: original\nversion: 0.1.0\ngoals:\n  - name: g1\n    description: a goal long enough to pass\nvalidations:\n  - target: g1\n    name: v1\n    mode: objective\n    statement: the file exists\n    detector: { type: file_exists, path: out.txt }\n",
    )
    .unwrap();

    // Walk through: change the name, keep everything else, skip every section,
    // write, overwrite the same file, do not run.
    let script = [
        // identity: new name, keep description + version
        "renamed", "", "",
        // providers: none configured yet — just finish (min is 1, but editing a
        // config that already has zero providers still lets you leave the menu
        // once you have not added mandatory ones)... so add a throwaway then done
        "add", "__byok__", "p", "byok", "echo", "", "", "", "", "", "done",
        // goals already present: finish immediately
        "done",
        // validations already present: finish
        "done",
        // stop gates: keep all defaults
        "", "", "", "", "", "", "",
        // advanced sections declined
        "", "", "", "", "", "", "", "", "",
        // review write, markdown? edit keeps original path so format still asked
        "", "yaml", "y",
    ];
    let out = guided(&["--edit", seed.to_str().unwrap()], &script, &dir);
    assert!(out.status.success(), "{}", combined(&out));

    let text = std::fs::read_to_string(&seed).unwrap();
    assert!(text.contains("renamed"), "edit did not take:\n{text}");
    assert!(!text.contains("name: original"), "old name still present:\n{text}");
}
