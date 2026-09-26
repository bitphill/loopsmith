//! A key the model does not know is refused, not dropped.
//!
//! Every plain struct in `config/` carries `#[serde(deny_unknown_fields)]`,
//! and `TriggerSpec`'s doc comment explains what that guard is worth: a
//! misspelled `idempotency_kye` that parses leaves a trigger with no dedup and
//! no warning. The seven internally tagged enums did not have it, and they are
//! where the most typo-prone parts of a config live — the detector on every
//! check, the isolation on every node, the action for every failure class.
//!
//! On an internally tagged enum the guard reaches struct variants and not unit
//! ones, so every variant that had no fields of its own was given an empty
//! body — `Worktree {}` rather than `Worktree`. That changes nothing a config
//! can see: an internally tagged unit variant was always written as a map
//! (`{ mode: worktree }`), and it still serializes back to exactly that. The
//! table below therefore carries a case per enum for the fields-carrying
//! variants and a case per enum for the empty ones, because the two are guarded
//! by different halves of the same attribute.
//!
//! The failure this prevents is specific and silent. `detector: { type: judge,
//! standard: …, node: review }` parsed, validated with no errors, ran, and
//! then lost `node` the next time anything wrote the file back out — so the
//! author's config on disk quietly stopped saying what they wrote. It was
//! found by writing exactly that line into `LOOP-TEMPLATE.md` while documenting
//! a field that does not exist.
//!
//! Everything here goes through [`loopsmith_core::parse_str`] rather than
//! `serde_yaml::from_str::<LoopConfig>`, because the loader is the only
//! supported way in and a test that skipped it would be testing a path nobody
//! uses.

/// A config with one node and one check, with `{DETECTOR}` and `{NODE_EXTRA}`
/// left for each case to fill in.
///
/// Deliberately minimal: this is about deserialization, and a config that
/// tripped a *validation* rule would fail for the wrong reason and pass this
/// test while the guard was missing.
const BASE: &str = r#"
name: keys
intent:
  goals:
    - name: g
      description: a sufficiently long goal description
safety:
  checks:
    - target: g
      name: c
      mode: objective
      statement: something checkable happens
      detector: {DETECTOR}
  recovery:
    transient_error: {RECOVERY}
execution:
  graph:
    nodes:
      - id: a
        role: builder
        instruction: do the thing properly
        goals: [g]
        isolation: {ISOLATION}
    join: {JOIN}
    concurrency: {CONCURRENCY}
  memory:
    namespaces:
      semantic:
        promotion: {PROMOTION}
  triggers:
    triggers:
      - on: {TRIGGER}
"#;

/// The base with every slot filled in with something valid.
fn config(slot: &str, value: &str) -> String {
    let defaults = [
        ("{DETECTOR}", "{ type: file_exists, path: out/x.md }"),
        ("{RECOVERY}", "{ action: retry, max_attempts: 2 }"),
        ("{ISOLATION}", "{ mode: worktree }"),
        ("{JOIN}", "{ strategy: quorum, count: 2 }"),
        ("{CONCURRENCY}", "{ mode: fixed, max_parallel: 2 }"),
        ("{PROMOTION}", "{ rule: repeated_validation, times: 2 }"),
        ("{TRIGGER}", "{ type: interval, seconds: 60 }"),
    ];
    let mut out = BASE.to_string();
    for (name, default) in defaults {
        out = out.replace(name, if name == slot { value } else { default });
    }
    out
}

/// Each tagged enum, with a valid body and the same body plus one key that is
/// not a field of it.
///
/// The names are the ones a person would actually get wrong: `node` on a judge
/// (it reads as though a judge names its judging node), `netwrok` on an
/// isolation, `jitter` on an interval trigger.
const CASES: &[(&str, &str, &str, &str)] = &[
    (
        "Detector",
        "{DETECTOR}",
        "{ type: judge, standard: docs/style.md }",
        "{ type: judge, standard: docs/style.md, node: review }",
    ),
    (
        "RecoveryAction",
        "{RECOVERY}",
        "{ action: retry, max_attempts: 2 }",
        "{ action: retry, max_attempts: 2, jitter: true }",
    ),
    (
        "Isolation",
        "{ISOLATION}",
        "{ mode: container, network: true }",
        "{ mode: container, netwrok: true }",
    ),
    (
        "Join",
        "{JOIN}",
        "{ strategy: quorum, count: 2 }",
        "{ strategy: quorum, count: 2, timeout_seconds: 30 }",
    ),
    (
        "Concurrency",
        "{CONCURRENCY}",
        "{ mode: auto, cap: 4 }",
        "{ mode: auto, cap: 4, max_parallel: 4 }",
    ),
    (
        "Promotion",
        "{PROMOTION}",
        "{ rule: repeated_validation, times: 2 }",
        "{ rule: repeated_validation, times: 2, within_days: 7 }",
    ),
    (
        "Trigger",
        "{TRIGGER}",
        "{ type: interval, seconds: 60 }",
        "{ type: interval, seconds: 60, jitter: 5 }",
    ),
    // From here down the tag names a variant with no fields at all. The bad
    // form in each is the same mistake: the tag was changed and the previous
    // variant's body was left sitting underneath it, reading as though it still
    // means something.
    (
        "Isolation, no fields",
        "{ISOLATION}",
        "{ mode: worktree }",
        "{ mode: worktree, network: true }",
    ),
    (
        "Join, no fields",
        "{JOIN}",
        "{ strategy: first_success }",
        "{ strategy: first_success, count: 2 }",
    ),
    (
        "Concurrency, no fields",
        "{CONCURRENCY}",
        "{ mode: sequential }",
        "{ mode: sequential, max_parallel: 2 }",
    ),
    (
        "Promotion, no fields",
        "{PROMOTION}",
        "{ rule: never }",
        "{ rule: never, times: 2 }",
    ),
    (
        "Trigger, no fields",
        "{TRIGGER}",
        "{ type: manual }",
        "{ type: manual, seconds: 60 }",
    ),
    (
        "RecoveryAction, no fields",
        "{RECOVERY}",
        "{ action: stop }",
        "{ action: stop, max_attempts: 3 }",
    ),
];

#[test]
fn a_misspelled_key_inside_a_tagged_section_is_refused_rather_than_dropped() {
    for (what, slot, good, bad) in CASES {
        loopsmith_core::parse_str(&config(slot, good), "good").unwrap_or_else(|e| {
            panic!("{what}: the valid form stopped parsing, so this case proves nothing: {e}")
        });

        let err = match loopsmith_core::parse_str(&config(slot, bad), "bad") {
            Err(e) => e.to_string(),
            Ok(_) => panic!("{what}: `{bad}` was accepted; the unknown key is being dropped"),
        };
        // The message has to name the key, or the author is told their config
        // is wrong and not which word is wrong.
        let key = bad
            .rsplit(',')
            .next()
            .and_then(|last| last.split(':').next())
            .map(str::trim)
            .expect("each bad case ends with the offending key");
        assert!(
            err.contains(key),
            "{what}: the refusal does not name `{key}`: {err}"
        );
    }
}
