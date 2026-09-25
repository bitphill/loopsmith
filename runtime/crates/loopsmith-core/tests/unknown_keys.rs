//! A key the model does not know is refused, not dropped.
//!
//! Every plain struct in `config/` carries `#[serde(deny_unknown_fields)]`,
//! and `TriggerSpec`'s doc comment explains what that guard is worth: a
//! misspelled `idempotency_kye` that parses leaves a trigger with no dedup and
//! no warning. The seven internally tagged enums did not have it, and they are
//! where the most typo-prone parts of a config live — the detector on every
//! check, the isolation on every node, the action for every failure class.
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

/// Where the guard stops: a variant with no fields of its own.
///
/// `deny_unknown_fields` on an internally tagged enum reaches its **struct**
/// variants and not its unit ones. `isolation: { mode: worktree, network: true }`
/// and `transient_error: { action: stop, max_attempts: 3 }` are both still
/// accepted, and in both the extra key does nothing while reading as though it
/// does something. The second is the likelier mistake — it is what a retry
/// policy looks like after someone changes `action` and leaves the rest.
///
/// This records the limit rather than hiding it. Closing it means giving each
/// unit variant an empty struct body (`Worktree {}`), which turns it into a
/// struct variant that the guard does reach — about sixty match sites across
/// the workspace, plus hand-written `Default` impls for `Isolation` and `Join`,
/// whose derived ones need a unit variant. Worth doing deliberately, not as a
/// side effect of this change.
///
/// When it is done, this test fails, and that is the signal to replace it with
/// the case above.
#[test]
fn a_variant_with_no_fields_of_its_own_is_still_not_guarded() {
    let bad = config("{ISOLATION}", "{ mode: worktree, network: true }");
    assert!(
        loopsmith_core::parse_str(&bad, "bad").is_ok(),
        "a unit variant now refuses an unknown key — good; fold this case into \
         `a_misspelled_key_inside_a_tagged_section_is_refused_rather_than_dropped` \
         and delete this test"
    );

    let bad = config("{RECOVERY}", "{ action: stop, max_attempts: 3 }");
    assert!(loopsmith_core::parse_str(&bad, "bad").is_ok());
}
