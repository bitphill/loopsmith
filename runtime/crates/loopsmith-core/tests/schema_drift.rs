//! `config/loop.schema.json` is 800+ lines of contract that nothing executes.
//!
//! It is the file authors read, the file the docs point at, and — until this
//! test — the file nothing checked. It had already drifted:
//! `max_revisions_per_node` was declared in the schema, defaulted in Rust,
//! documented in two markdown files, present in both examples and the
//! scaffold, and read by zero lines of runtime code.
//!
//! These tests make the two artifacts hold each other honest in both
//! directions. Adding a config field now means touching three things — the
//! schema, the Rust struct, and the fixture below — and forgetting any one of
//! them fails here rather than silently at someone's 3am unattended run.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../config/loop.schema.json")
        .canonicalize()
        .expect("config/loop.schema.json is reachable from the crate")
}

fn schema() -> Value {
    let text = std::fs::read_to_string(schema_path()).expect("schema is readable");
    serde_json::from_str(&text).expect("schema is valid JSON")
}

/// Every name the schema declares as an object property, at any depth,
/// including inside `$defs`.
fn schema_property_names(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            if let Some(Value::Object(props)) = m.get("properties") {
                out.extend(props.keys().cloned());
            }
            for sub in m.values() {
                schema_property_names(sub, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| schema_property_names(x, out)),
        _ => {}
    }
}

/// Fields whose *keys* are user-chosen data rather than part of the grammar.
/// `per_node` is keyed by node id, `cascade` by tier name, `metrics` by metric
/// name. Collecting those keys would compare a user's node id against the
/// schema and fail for no reason.
const DYNAMIC_KEY_FIELDS: &[&str] = &["per_node", "cascade"];

/// Every key name in a serialized config, skipping the dynamic-key maps above.
fn value_key_names(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, sub) in m {
                out.insert(k.clone());
                if DYNAMIC_KEY_FIELDS.contains(&k.as_str()) {
                    // Skip the map's own keys, but still inspect its values so
                    // the shape underneath is checked.
                    if let Value::Object(inner) = sub {
                        inner.values().for_each(|x| value_key_names(x, out));
                    }
                    continue;
                }
                value_key_names(sub, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| value_key_names(x, out)),
        _ => {}
    }
}

/// A config that sets every field the schema declares. When you add a section,
/// add it here too — that is the whole point of the fixture.
const FULL_COVERAGE: &str = r#"
name: coverage
version: 0.2.0
description: a config exercising every documented field

information:
  - key: repo
    value: /tmp/example
    note: why this matters

pre_execution:
  - step: did it by hand once
    done: true
    evidence: transcript.md

goals:
  - name: g1
    description: a sufficiently long goal description
    depends_on: []
    priority: 1

validations:
  - target: g1
    name: v-script
    mode: objective
    statement: the suite passes
    blocking: true
    detector: { type: script, command: "true", args: ["-x"], expect_exit: 0 }
  - target: g1
    name: v-file
    mode: objective
    statement: the report exists
    detector: { type: file_exists, path: out/report.md, non_empty: true }
  - target: g1
    name: v-regex
    mode: objective
    statement: the artifact names a source
    detector: { type: regex_match, artifact: report, pattern: "https?://" }
  - target: g1
    name: v-threshold
    mode: percentage
    statement: coverage holds
    detector: { type: threshold, metric: coverage, op: gte, value: 0.8 }
  - target: overall
    name: v-judge
    mode: subjective
    statement: it reads well
    blocking: false
    detector: { type: judge, standard: the house style guide, min_score: 7 }

success:
  - target: overall
    name: done
    mode: percentage
    statement: most blocking checks pass
    threshold: 0.9

stop_gates:
  max_iterations: 12
  max_revisions_per_node: 3
  max_wall_clock_seconds: 7200
  max_tokens: 4000000
  max_cost_usd: 10.0
  no_progress_iterations: 3
  no_progress_iterations_randomness: 2
  stop_on_overall_success: true

schedules:
  - { type: cron, expr: "0 2 * * *" }
  - { type: interval, seconds: 900 }
  - { type: file_change, path: src/ }
  - { type: goal_satisfied, goal: g1 }
  - { type: manual }

constraints:
  global:
    rules: ["never git stash"]
    forbidden_paths: ["tests/"]
    forbidden_commands: ["git push"]
    max_tokens: 200000
    max_seconds: 900
    human_checkpoint: ["deleting anything"]
  per_node:
    worker:
      rules: ["stay in your worktree"]
      forbidden_paths: []
      forbidden_commands: []
      max_tokens: 100000
      max_seconds: 600
      human_checkpoint: []

default_skills:
  - name: agent-reach
    source: github
    url: https://github.com/Panniantong/agent-reach
    init_command: npm install
    note: why this loop needs it

execution_guidelines:
  items:
    - name: gather
      guideline: Collect what the goal needs. Write nothing yet.
      note: for the human reading this config
    - name: deliver
      guideline: Produce the deliverable from what gather collected.
  dependency:
    - gather -> deliver

graph:
  nodes:
    - id: worker
      role: builder
      instruction: do the work described in the goal
      depends_on: []
      goals: [g1]
      tier: standard
      provider: openai
      stage: deliver
      skills: [helper]
      weight: 2.0
      isolated: true
  concurrency:
    mode: auto
    cap: 16
    min_marginal_gain: 0.05

providers:
  providers:
    - id: openai
      kind: openai
      tiers: [strong]
      command: curl
      args: ["-s"]
      model: gpt-x
      requires_env: [OPENAI_API_KEY]
      timeout_seconds: 300
      prompt_on_stdin: true
      usage_regex: '"total_tokens"\s*:\s*(\d+)'
      cost_per_1k_tokens: 0.0006
  cascade:
    strong: [openai]
  enforce_judge_independence: true

context:
  carry_summaries: 2
  summary_provider: openai
  max_summary_chars: 1200

skills:
  acquisition_order: [installed, marketplace, generate]
  quarantine_dir: generated-skills
  min_marketplace_stars: 100
  require_human_promotion: true
  explore: false
  explore_candidates: [some-candidate]
  min_trials: 3
"#;

/// Some fields only exist on one variant of a tagged enum, and a single config
/// can pick only one variant. These cover the alternates the main fixture
/// cannot reach.
const VARIANTS: &[&str] = &[
    // `max_parallel` lives only on `concurrency.mode: fixed`.
    r#"
name: v-fixed
goals: [{ name: g1, description: a sufficiently long goal description }]
validations:
  - target: g1
    name: v
    mode: objective
    statement: it works
    detector: { type: file_exists, path: out.txt }
graph:
  concurrency: { mode: fixed, max_parallel: 4 }
"#,
    r#"
name: v-sequential
goals: [{ name: g1, description: a sufficiently long goal description }]
validations:
  - target: g1
    name: v
    mode: objective
    statement: it works
    detector: { type: file_exists, path: out.txt }
graph:
  concurrency: { mode: sequential }
"#,
];

/// Every key name reachable from the main fixture plus every variant fixture.
fn all_rust_field_names() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, text) in std::iter::once(&FULL_COVERAGE).chain(VARIANTS.iter()).enumerate() {
        let cfg = loopsmith_core::parse_str(text, "fixture")
            .unwrap_or_else(|e| panic!("fixture {i} must parse:\n{e}"));
        let serialized = serde_json::to_value(&cfg).expect("config serializes");
        value_key_names(&serialized, &mut out);
    }
    out
}

#[test]
fn the_full_coverage_fixture_parses() {
    // `deny_unknown_fields` means this fails the moment the schema documents a
    // field the Rust model does not have.
    let cfg = loopsmith_core::parse_str(FULL_COVERAGE, "fixture")
        .unwrap_or_else(|e| panic!("full-coverage fixture must parse:\n{e}"));
    assert_eq!(cfg.name, "coverage");
    assert_eq!(cfg.graph.nodes.len(), 1);
    assert_eq!(cfg.schedules.len(), 5);
}

#[test]
fn every_schema_property_is_reachable_from_the_rust_model() {
    let in_rust = all_rust_field_names();
    let mut in_schema = BTreeSet::new();
    schema_property_names(&schema(), &mut in_schema);

    let missing: Vec<&String> = in_schema.difference(&in_rust).collect();
    assert!(
        missing.is_empty(),
        "these fields are documented in config/loop.schema.json but never appear \
         in a serialized LoopConfig — either implement them or delete them from \
         the schema: {missing:?}"
    );
}

#[test]
fn every_rust_field_is_documented_in_the_schema() {
    let in_rust = all_rust_field_names();
    let mut in_schema = BTreeSet::new();
    schema_property_names(&schema(), &mut in_schema);

    let undocumented: Vec<&String> = in_rust.difference(&in_schema).collect();
    assert!(
        undocumented.is_empty(),
        "these fields exist in the Rust model but are absent from \
         config/loop.schema.json, so nobody authoring a config can discover \
         them: {undocumented:?}"
    );
}
