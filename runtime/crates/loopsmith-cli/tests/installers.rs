//! The installers agree with each other, because they read the same file.
//!
//! `install.sh`, `install.ps1` and `install.bat` are not built by cargo, so
//! nothing else in this workspace looks at them. Before 1.0 they each carried
//! their own copy of the repository URL, the install directory, the build
//! command and the next-steps lines, and the way that goes wrong is specific:
//! somebody moves the repository, fixes the two scripts for the OS they are
//! on, and the third keeps cloning the old address for a year.
//!
//! CI runs both installers on every OS, which proves each one works. This is
//! the other half — that there is nothing left in them to disagree about.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate is three deep in the repository")
        .to_path_buf()
}

/// One flat string value out of the manifest, read the way `install.sh` reads
/// it: by looking for the key, not by parsing JSON. A test that parsed it
/// properly would pass on a manifest the installer cannot read.
fn manifest_value(manifest: &str, key: &str) -> String {
    let needle = format!("\"{key}\"");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with(&needle))
        .unwrap_or_else(|| panic!("the manifest has no `{key}`"));
    let after = line.split_once(':').expect("a key line has a colon").1;
    after
        .trim()
        .trim_end_matches(',')
        .trim_matches('"')
        .to_string()
}


/// A script with its whole-line comments removed.
///
/// The check below is about what the code hard-codes, not about what the prose
/// mentions. `install.sh` has a comment explaining that a checkout beside the
/// script wins over "whatever main happens to be", and a test that treats that
/// sentence as a duplicated branch name is a test nobody can satisfy without
/// making the script worse.
fn code_of(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.starts_with('#') || t.starts_with("rem ") || t.starts_with("REM "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn nothing_an_installer_shares_is_written_down_twice() {
    let root = repo_root();
    let manifest = std::fs::read_to_string(root.join("installers/manifest.json"))
        .expect("installers/manifest.json");

    let scripts: Vec<(&str, String)> = ["install.sh", "install.ps1", "install.bat"]
        .iter()
        .map(|name| (*name, code_of(&root.join(name))))
        .collect();

    // The values worth checking are the ones that change together or not at
    // all. `binary` is excluded: it is the word `loopsmith`, which appears in
    // every log line and every comment, and demanding it appear nowhere would
    // be a test about prose.
    for key in [
        "repo_url",
        "branch",
        "install_dir_name",
        "unix_link_dir",
        "built_unix",
        "built_windows",
    ] {
        let value = manifest_value(&manifest, key);
        assert!(!value.is_empty(), "the manifest's `{key}` is empty");
        for (name, text) in &scripts {
            assert!(
                !text.contains(&value),
                "{name} hard-codes `{value}`, which is the manifest's `{key}` — \
                 read it from there instead, or the two will disagree"
            );
        }
    }
}

#[test]
fn every_installer_actually_reads_the_manifest() {
    // The check above passes trivially for a script that mentions none of
    // those values because it does nothing at all. This is the other side.
    let root = repo_root();
    for (name, needle) in [
        ("install.sh", "installers/manifest.sh"),
        ("install.ps1", "installers\\manifest.json"),
        // `install.bat` has nothing of its own to configure; it exists so the
        // install is one word rather than an execution-policy incantation, and
        // hands straight over to the script that does read the manifest.
        ("install.bat", "install.ps1"),
    ] {
        let text = std::fs::read_to_string(root.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(text.contains(needle), "{name} never mentions {needle}");
    }
}

#[test]
fn the_manifest_stays_flat_enough_for_a_shell_to_read() {
    // `install.sh` reads this with sed and awk, because the machine it runs on
    // has just been told it needs cargo and git and is in no position to also
    // need jq. That only works while every value is a string or a one-per-line
    // array of strings.
    let root = repo_root();
    let manifest = std::fs::read_to_string(root.join("installers/manifest.json")).expect("manifest");
    let parsed: serde_json::Value = serde_json::from_str(&manifest).expect("the manifest is JSON");
    let object = parsed.as_object().expect("the manifest is an object");

    for (key, value) in object {
        match value {
            serde_json::Value::String(_) => {}
            serde_json::Value::Array(items) => {
                for item in items {
                    assert!(
                        item.is_string(),
                        "`{key}` holds something that is not a string; the shell reader \
                         only handles strings"
                    );
                }
            }
            other => panic!("`{key}` is {other:?}; the shell reader handles strings and arrays of strings"),
        }
    }

    // An array written on one line reads fine and defeats the awk reader's
    // assumption, so the arrays that matter are checked for a real element.
    for key in ["build_args", "requires", "next_steps"] {
        let items = object[key].as_array().unwrap_or_else(|| panic!("`{key}` is an array"));
        assert!(!items.is_empty(), "`{key}` is empty");
    }
}
