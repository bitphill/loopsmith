//! A `.md` config and a `.yaml` config must mean the same thing.
//!
//! The renderer and the parser are inverses, so the strongest test available is
//! the round trip: take the real example configs, render them to markdown, read
//! them back, and require the resulting `LoopConfig` to match field for field.
//!
//! Trailing whitespace inside a value is the one documented exception — a
//! markdown bullet ends where the line ends — so both sides are compared after
//! trimming it.

use serde_json::Value;
use std::path::PathBuf;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../config/examples")
        .canonicalize()
        .expect("config/examples is reachable from the crate")
}

/// Compare configs by their serialized form, with trailing whitespace in every
/// string trimmed. YAML folded scalars (`>`) end in a newline that a bullet
/// cannot carry, and that is the only difference markdown may introduce.
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

fn roundtrip(yaml_path: &PathBuf) {
    let from_yaml = loopsmith_core::load(yaml_path)
        .unwrap_or_else(|e| panic!("{} should load: {e}", yaml_path.display()));

    let markdown = loopsmith_core::render_md(&from_yaml);
    let from_md = loopsmith_core::parse_md(&markdown, "rendered").unwrap_or_else(|e| {
        panic!(
            "rendered markdown for {} should parse back: {e}\n\n--- rendered ---\n{markdown}",
            yaml_path.display()
        )
    });

    assert_eq!(
        normalized(&from_yaml),
        normalized(&from_md),
        "round trip changed {}\n\n--- rendered ---\n{markdown}",
        yaml_path.display()
    );
}

#[test]
fn every_example_config_survives_a_markdown_round_trip() {
    let dir = examples_dir();
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("examples dir is readable") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        roundtrip(&path);
        checked += 1;
    }
    assert!(checked >= 2, "expected at least the two shipped examples");
}

#[test]
fn every_shipped_md_example_means_the_same_as_its_yaml_twin() {
    // The `.md` examples are generated from the `.yaml` ones. This is what
    // stops them drifting once someone edits one by hand.
    let dir = examples_dir();
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("examples dir is readable") {
        let md = entry.expect("readable entry").path();
        if md.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let yaml = md.with_extension("yaml");
        assert!(
            yaml.is_file(),
            "{} has no .yaml twin; every example ships as a pair",
            md.display()
        );

        let from_md = loopsmith_core::load(&md)
            .unwrap_or_else(|e| panic!("{} should load: {e}", md.display()));
        let from_yaml = loopsmith_core::load(&yaml)
            .unwrap_or_else(|e| panic!("{} should load: {e}", yaml.display()));

        assert_eq!(
            normalized(&from_yaml),
            normalized(&from_md),
            "{} and its .yaml twin describe different loops",
            md.display()
        );
        checked += 1;
    }
    assert!(checked >= 2, "expected at least two example pairs");
}

#[test]
fn a_markdown_config_parses_into_the_same_model_as_its_yaml_twin() {
    let yaml = r#"
name: twin
description: two spellings of one loop
goals:
  - name: g1
    description: a sufficiently long goal description
    priority: 2
validations:
  - target: g1
    name: v1
    mode: objective
    statement: the file exists
    detector: { type: file_exists, path: out/report.md, non_empty: true }
stop_gates:
  max_iterations: 4
  max_cost_usd: 2.5
graph:
  nodes:
    - id: build
      role: builder
      instruction: produce the report described by the goal
      goals: [g1]
      isolated: true
  concurrency:
    mode: fixed
    max_parallel: 2
"#;

    let markdown = r#"
# twin

- description: two spellings of one loop

Anything at the left margin is documentation. This paragraph is ignored, and so
is the table below.

| this | is |
|------|----|
| also | prose |

## C. Goals

### g1
- description: a sufficiently long goal description
- priority: 2

## D. Validations

### v1
- target: g1
- mode: objective
- statement: the file exists
- detector:
  - type: file_exists
  - path: out/report.md
  - non_empty: true

## F. Stop gates
- max_iterations: 4
- max_cost_usd: 2.5

## Graph
- concurrency:
  - mode: fixed
  - max_parallel: 2

### build
- role: builder
- instruction: produce the report described by the goal
- goals: [g1]
- isolated: true
"#;

    let a = loopsmith_core::parse_str(yaml, "yaml").expect("yaml parses");
    let b = loopsmith_core::parse_md(markdown, "md").expect("markdown parses");
    assert_eq!(normalized(&a), normalized(&b));
}

#[test]
fn prose_and_fenced_blocks_are_documentation_not_config() {
    let markdown = r#"
# quiet

## C. Goals

Here is why this goal exists. It mentions `name: not-a-goal` in prose, which
must not become a field.

```yaml
name: definitely-not-the-loop-name
goals:
  - name: fake
```

### real
- description: a sufficiently long goal description

## D. Validations

### v
- target: real
- mode: objective
- statement: it exists
- detector:
  - type: file_exists
  - path: out.txt
"#;
    let cfg = loopsmith_core::parse_md(markdown, "md").expect("parses");
    assert_eq!(cfg.name, "quiet", "the fenced block must not rename the loop");
    assert_eq!(cfg.goals.len(), 1, "prose must not add a goal");
    assert_eq!(cfg.goals[0].name, "real");
}

#[test]
fn a_misspelled_field_is_refused_in_markdown_too() {
    // `deny_unknown_fields` applies on the markdown path because the markdown
    // path ends in the same `Deserialize` impl.
    let markdown = r#"
# typo

## C. Goals

### g1
- descriptoin: a sufficiently long goal description

## D. Validations

### v
- target: g1
- mode: objective
- statement: it exists
- detector:
  - type: file_exists
  - path: out.txt
"#;
    let err = loopsmith_core::parse_md(markdown, "md").expect_err("must be refused");
    assert!(err.to_string().contains("descriptoin"), "got: {err}");
}

#[test]
fn a_multi_line_instruction_survives() {
    let markdown = r#"
# multiline

## C. Goals

### g1
- description: a sufficiently long goal description

## D. Validations

### v
- target: g1
- mode: objective
- statement: it exists
- detector:
  - type: file_exists
  - path: out.txt

## Graph

### build
- role: builder
- instruction: Read the goal, then produce the report.
    Cite every claim with a URL or a file and line.
    Do not edit anything under tests/.
- goals: [g1]
"#;
    let cfg = loopsmith_core::parse_md(markdown, "md").expect("parses");
    let instruction = &cfg.graph.nodes[0].instruction;
    assert!(instruction.contains("Read the goal"), "got: {instruction}");
    assert!(instruction.contains("Cite every claim"), "got: {instruction}");
    assert!(
        instruction.contains("tests/"),
        "the third line must survive too: {instruction}"
    );
    assert_eq!(instruction.lines().count(), 3);
}
