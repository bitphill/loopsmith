//! The 0.3 examples still mean what their 1.0 twins mean.
//!
//! Every shipped example moved to the 1.0 shape at 1.0, which is right — they
//! are the most-copied files in this repository and they should teach the
//! shape the tool has. But that removed the only end-to-end evidence that the
//! relocation table works on a real config rather than on a fixture: until
//! then, CI loaded thirteen 0.3 files on every run and would have noticed the
//! moment one of them stopped loading.
//!
//! `config/examples/legacy/` is those thirteen files, frozen. They are not
//! documentation and nothing links to them; they exist so that this test can
//! load each one, load the 1.0 file of the same name beside it, and require
//! the two to produce the same `LoopConfig`.
//!
//! That is a stronger statement than "the legacy file still parses". It says
//! the relocation table puts every key exactly where the 1.0 file puts it by
//! hand — which is the property `loopsmith loop migrate` promises, checked
//! against thirteen configs somebody actually wrote.

use serde_json::Value;
use std::path::PathBuf;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../config/examples")
        .canonicalize()
        .expect("config/examples is reachable from the crate")
}

/// Serialized, with trailing whitespace trimmed out of every string: a YAML
/// folded scalar ends in a newline that the same text written as a plain
/// scalar does not carry, and that difference is not a difference in meaning.
fn normalized(cfg: &loopsmith_core::LoopConfig) -> Value {
    fn trim(v: &mut Value) {
        match v {
            Value::String(s) => *s = s.trim_end().to_string(),
            Value::Array(a) => a.iter_mut().for_each(trim),
            Value::Object(m) => m.values_mut().for_each(trim),
            _ => {}
        }
    }
    let mut v = serde_json::to_value(cfg).expect("config serializes");
    trim(&mut v);
    v
}

#[test]
fn every_legacy_example_migrates_to_the_one_beside_it() {
    let dir = examples_dir();
    let legacy = dir.join("legacy");
    assert!(legacy.is_dir(), "config/examples/legacy is the 0.3 corpus");

    let mut checked = 0;
    for entry in std::fs::read_dir(&legacy).expect("the legacy corpus is readable") {
        let old = entry.expect("readable entry").path();
        if old.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let name = old.file_name().expect("a file has a name");
        let current = dir.join(name);
        assert!(
            current.is_file(),
            "{} has no 1.0 twin; a corpus entry with nothing to compare against \
             proves only that it parses",
            old.display()
        );

        let from_legacy = loopsmith_core::load(&old)
            .unwrap_or_else(|e| panic!("{} should still load: {e}", old.display()));
        let from_current = loopsmith_core::load(&current)
            .unwrap_or_else(|e| panic!("{} should load: {e}", current.display()));

        assert_eq!(
            normalized(&from_legacy),
            normalized(&from_current),
            "the relocation table does not put {} where its 1.0 twin puts it",
            old.display()
        );
        checked += 1;
    }
    assert!(checked >= 13, "only {checked} legacy configs checked");
}

#[test]
fn the_legacy_corpus_is_actually_legacy() {
    // A corpus entry that has quietly been migrated proves nothing: it would
    // pass the test above without the relocation table being involved at all.
    let legacy = examples_dir().join("legacy");
    let mut checked = 0;
    for entry in std::fs::read_dir(&legacy).expect("the legacy corpus is readable") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("readable");
        let doc: serde_yaml::Value = serde_yaml::from_str(&text).expect("still YAML");
        let (_, moved) = loopsmith_core::config::legacy::migrate(&doc);
        assert!(
            !moved.is_empty(),
            "{} no longer uses any 0.3 key, so it is not a migration corpus entry",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 13, "only {checked} legacy configs checked");
}

#[test]
fn no_shipped_example_still_uses_a_03_key() {
    // The other half. The examples outside `legacy/` are what people copy,
    // and one that still needs the relocation table would print a deprecation
    // notice to someone who has only just installed the tool.
    let dir = examples_dir();
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("examples dir is readable") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("readable");
        let doc: serde_yaml::Value = serde_yaml::from_str(&text).expect("valid YAML");
        let (_, moved) = loopsmith_core::config::legacy::migrate(&doc);
        assert!(
            moved.is_empty(),
            "{} still uses 0.3 key(s): {}",
            path.display(),
            moved
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        checked += 1;
    }
    assert!(checked >= 15, "only {checked} shipped examples checked");
}
