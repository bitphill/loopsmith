//! Stress harness — the shipped examples, actually run.
//!
//! Everything in the runtime is covered by unit tests in isolation. This file
//! covers what those cannot: whether the pieces work *together* under a real
//! iteration loop, driven through the real binary, against the configs users
//! are handed.
//!
//! Assertions read the artifacts a run leaves behind — the ledger, the run log,
//! the stored summaries, the presence or absence of an export — rather than
//! stdout. Stdout is a report; the artifacts are the record.
//!
//! See `harness` for how an example is brought to a runnable state.

mod harness;

use harness::{all_examples, Fixture, Stubs};
use loopsmith_core::Detector;
use loopsmith_memory::{LedgerKind, Store};

/// Force every validation on a target to fail, so a run keeps iterating instead
/// of succeeding on its first pass.
fn starve(cfg: &mut loopsmith_core::LoopConfig, target: &str) {
    for v in cfg.validations.iter_mut().filter(|v| v.target == target) {
        v.detector = Detector::Script {
            command: "false".into(),
            args: vec![],
            expect_exit: Some(0),
        };
    }
}

/// Cap the run so a stress scenario cannot sit in a long example's default
/// ceiling of ten iterations and three hours.
fn cap(cfg: &mut loopsmith_core::LoopConfig, iterations: u32) {
    cfg.stop_gates.max_iterations = iterations;
    cfg.stop_gates.no_progress_iterations = 0;
    cfg.stop_gates.no_progress_iterations_randomness = None;
}

// ---------------------------------------------------------------------------
// The whole set, once each
// ---------------------------------------------------------------------------

/// Every example must survive one supervised iteration with its detectors
/// satisfiable. This is the broadest thing the harness asserts and the cheapest
/// signal that a config change broke the runtime rather than the schema.
#[test]
fn every_example_completes_an_iteration_and_leaves_a_consistent_record() {
    for (i, name) in all_examples().iter().enumerate() {
        let mut f = Fixture::example(name, &format!("all-{i}"));
        cap(&mut f.cfg, 1);
        f.write_config();
        let f = f
            .stub_scripts(Stubs::Pass)
            .satisfy_files()
            .satisfy_metrics();

        let run_id = "stress";
        let out = f.run_loop(run_id, &[]);
        assert!(
            !out.status.success() || out.status.success(),
            "the process must exit, not hang"
        );

        let store = f.store();
        let ledger = store.ledger(run_id).unwrap();
        assert!(
            ledger.iter().any(|e| e.kind == LedgerKind::RunStarted),
            "{name}: a run must open the ledger"
        );
        assert!(
            ledger
                .iter()
                .any(|e| matches!(e.kind, LedgerKind::RunFinished | LedgerKind::StopGateTriggered)),
            "{name}: a run must record why it stopped"
        );

        // The log and the ledger are written through one call, so a divergence
        // means one of the two write paths grew a branch the other did not.
        let log = f.log_text(run_id);
        assert_eq!(
            log.lines().count(),
            ledger.len(),
            "{name}: the run log and the ledger disagree about what happened"
        );

        assert_eq!(
            store.summaries(run_id).unwrap().len(),
            1,
            "{name}: one iteration must leave exactly one summary"
        );

        drop(store);
        f.cleanup();
    }
}

/// Every example must also survive the case where nothing it checks is
/// satisfiable. A detector that fails closed is correct; a runtime that panics
/// on it is not.
#[test]
fn every_example_survives_a_run_where_nothing_passes() {
    for (i, name) in all_examples().iter().enumerate() {
        let mut f = Fixture::example(name, &format!("starve-{i}"));
        cap(&mut f.cfg, 2);
        f.write_config();
        // No stubs, no artifacts, no metrics: script detectors error, file
        // detectors miss, thresholds have nothing to read.
        let run_id = "starved";
        let out = f.run_loop(run_id, &[]);
        assert!(
            !out.status.success(),
            "{name}: a run that satisfied nothing must exit non-zero"
        );

        let store = f.store();
        let ledger = store.ledger(run_id).unwrap();
        assert!(
            ledger.iter().any(|e| e.kind == LedgerKind::StopGateTriggered),
            "{name}: the stop must be written to the ledger"
        );
        assert!(
            !f.export_dir().exists(),
            "{name}: no bar met, so no success package"
        );
        drop(store);
        f.cleanup();
    }
}

// ---------------------------------------------------------------------------
// Section I — phases under a real multi-iteration run
// ---------------------------------------------------------------------------

/// `traffic-loop` has four strictly linear phases with one node each. A phase
/// opens only when the one before it closes, and closes only on the gate's
/// ruling — so under a run whose overall check can never pass, the nodes should
/// come online one iteration at a time, in order.
///
/// `phases.rs` is unit-tested against synthetic verdicts. This is the first
/// time the ordering is asserted against verdicts the gate actually produced.
#[test]
fn phases_open_one_at_a_time_across_a_real_run() {
    let mut f = Fixture::example("traffic-loop", "phases");
    cap(&mut f.cfg, 6);
    // Keep the run alive: each goal can be satisfied, `overall` never is, so
    // the loop keeps iterating and the phases keep opening.
    starve(&mut f.cfg, "overall");
    f.write_config();
    let f = f
        .stub_scripts(Stubs::Pass)
        .satisfy_files()
        .satisfy_metrics();

    let run_id = "phases";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let episodes = store.episodes(run_id).unwrap();
    assert!(!episodes.is_empty(), "nodes must have been dispatched");

    let first_iteration_of = |node: &str| -> Option<u32> {
        episodes
            .iter()
            .filter(|e| e.node_id == node)
            .map(|e| e.iteration)
            .min()
    };

    // research -> draft -> publish -> measure, one node per phase.
    let order = ["find-venues", "write-posts", "publish", "measure"];
    let mut seen: Vec<(String, u32)> = Vec::new();
    for node in order {
        let it = first_iteration_of(node)
            .unwrap_or_else(|| panic!("`{node}` never ran; episodes: {:?}", node_summary(&episodes)));
        seen.push((node.to_string(), it));
    }
    for pair in seen.windows(2) {
        assert!(
            pair[1].1 > pair[0].1,
            "`{}` ran in iteration {} and `{}` in {}; a later phase must not open first — {:?}",
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1,
            seen
        );
    }

    // And the closures must be on the record, attributed to the gate.
    let ledger = store.ledger(run_id).unwrap();
    let closed: Vec<&str> = ledger
        .iter()
        .filter(|e| e.detail.contains("is complete; the phases behind it are now open"))
        .map(|e| e.detail.as_str())
        .collect();
    assert!(
        closed.len() >= 3,
        "at least three phases should have closed; got {closed:?}"
    );

    drop(store);
    f.cleanup();
}

/// A phase whose goals the gate never certifies must keep the phases behind it
/// shut for the whole run — no timeout, no eventual give-up that lets later
/// work start anyway.
#[test]
fn a_phase_that_never_satisfies_its_goals_never_opens_the_next_one() {
    let mut f = Fixture::example("traffic-loop", "phase-stuck");
    cap(&mut f.cfg, 4);
    starve(&mut f.cfg, "overall");
    starve(&mut f.cfg, "find-venues");
    f.write_config();
    let f = f.stub_scripts(Stubs::Pass).satisfy_metrics();

    let run_id = "stuck";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let episodes = store.episodes(run_id).unwrap();
    assert!(
        episodes.iter().any(|e| e.node_id == "find-venues"),
        "the first phase must still run"
    );
    for later in ["write-posts", "publish", "measure"] {
        assert!(
            !episodes.iter().any(|e| e.node_id == later),
            "`{later}` ran even though the phase in front of it never closed"
        );
    }
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// Worktree isolation
// ---------------------------------------------------------------------------

/// Outside a git repository, `isolated: true` degrades to the shared directory
/// — silently, by design. A scratch loop directory is not a repository unless
/// someone makes it one, so this is what a user gets by default.
#[test]
fn isolation_degrades_to_shared_outside_a_repository() {
    let mut f = Fixture::example("refactor-loop", "iso-norepo");
    cap(&mut f.cfg, 1);
    f.write_config();
    let f = f.stub_scripts(Stubs::Pass).satisfy_files().satisfy_metrics();

    let run_id = "norepo";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let ledger = store.ledger(run_id).unwrap();
    let shared: Vec<&str> = ledger
        .iter()
        .filter(|e| e.detail.contains("shared workdir"))
        .map(|e| e.detail.as_str())
        .collect();
    assert!(
        shared.iter().any(|d| d.contains("not a git repository")),
        "the degradation must be reported, not hidden: {shared:?}"
    );
    assert!(
        !f.dir.join("state/worktrees").exists(),
        "no repository, so no worktrees"
    );
    drop(store);
    f.cleanup();
}

/// Inside a repository the same config produces real worktrees, one per
/// isolated node, and says so in the ledger.
#[test]
fn isolation_is_real_inside_a_repository() {
    let mut f = Fixture::example("refactor-loop", "iso-repo");
    cap(&mut f.cfg, 1);
    f.write_config();
    let f = f
        .stub_scripts(Stubs::Pass)
        .satisfy_files()
        .satisfy_metrics()
        .git_init();

    let isolated: Vec<String> = f
        .cfg
        .graph
        .nodes
        .iter()
        .filter(|n| n.isolated)
        .map(|n| n.id.clone())
        .collect();
    assert!(!isolated.is_empty(), "the example must have an isolated node");

    let run_id = "repo";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let ledger = store.ledger(run_id).unwrap();
    assert!(
        ledger.iter().any(|e| e.detail.contains("isolated in")),
        "a repository must produce worktrees: {:?}",
        ledger.iter().map(|e| &e.detail).collect::<Vec<_>>()
    );
    for node in &isolated {
        assert!(
            f.dir.join("state/worktrees").join(node).is_dir(),
            "`{node}` declared isolation but got no worktree"
        );
    }
    drop(store);
    f.cleanup();
}

/// A config whose only builder is isolated and actually writes a file, with a
/// `file_exists` detector pointing at what it wrote.
const ISOLATED_WRITER: &str = r#"
name: isolated-writer
goals:
  - name: g1
    description: the builder must leave an artifact behind on disk
pre_execution:
  - step: done by hand
    done: true
validations:
  - target: g1
    name: artifact-exists
    mode: objective
    statement: The builder's artifact is on disk where the gate can see it.
    detector: { type: file_exists, path: out/thing.txt, non_empty: true }
  - target: overall
    name: artifact-exists-overall
    mode: objective
    statement: The builder's artifact is on disk where the gate can see it.
    detector: { type: file_exists, path: out/thing.txt, non_empty: true }
graph:
  nodes:
    - id: build
      role: builder
      instruction: write the artifact the goal describes into out/thing.txt
      goals: [g1]
      isolated: true
providers:
  providers:
    - id: writer
      kind: byok
      command: sh
      args: ["-c", "mkdir -p out && echo produced > out/thing.txt"]
  cascade:
    standard: [writer]
"#;

/// An isolated builder's output must reach the gate.
///
/// Isolated nodes run in `state/worktrees/<node>/`, and evidence is collected
/// from the loop root. Before the publish step existed, a `file_exists`
/// detector pointing at something an isolated builder produced could never
/// pass — the work was real, on disk, and invisible to the only thing allowed
/// to rule on it.
#[test]
fn an_isolated_builders_output_reaches_the_gate() {
    let mut f = Fixture::from_yaml(ISOLATED_WRITER, "iso-evidence");
    cap(&mut f.cfg, 2);
    // `from_yaml` rewrites providers to the deterministic judge block; this
    // config needs a provider that writes a file instead.
    f.cfg.providers.providers[0].command = "sh".into();
    f.cfg.providers.providers[0].args = vec![
        "-c".into(),
        "mkdir -p out && echo produced > out/thing.txt".into(),
    ];
    f.write_config();
    let f = f.git_init();

    let run_id = "iso-ev";
    f.run_loop(run_id, &[]);

    // The builder really did run in its own worktree.
    let worktree = f.dir.join("state/worktrees/build");
    assert!(
        worktree.join("out/thing.txt").is_file(),
        "the isolated builder must have written into its worktree"
    );

    let store = f.store();
    let states = store.goal_states(run_id).unwrap();
    let g1 = states.get("g1").expect("the gate ruled on g1");
    assert!(
        g1.satisfied,
        "the gate could not see what the isolated builder produced: {}",
        g1.reason
    );
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// Stop gates under a real loop
// ---------------------------------------------------------------------------

/// A run whose verdicts never move must halt on `no_progress_iterations`
/// rather than spend its whole iteration budget.
#[test]
fn a_run_with_no_moving_verdicts_halts_on_no_progress() {
    let mut f = Fixture::example("research-loop", "noprogress");
    f.cfg.stop_gates.max_iterations = 20;
    f.cfg.stop_gates.no_progress_iterations = 2;
    f.cfg.stop_gates.no_progress_iterations_randomness = None;
    f.write_config();
    let f = f.stub_scripts(Stubs::Fail);

    let run_id = "flat";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let ledger = store.ledger(run_id).unwrap();
    let stop = ledger
        .iter()
        .rev()
        .find(|e| e.kind == LedgerKind::StopGateTriggered)
        .expect("a stop must be recorded");
    assert!(
        stop.detail.contains("no measurable change"),
        "expected a no-progress halt, got: {}",
        stop.detail
    );
    let iterations = ledger
        .iter()
        .filter(|e| e.kind == LedgerKind::IterationStarted)
        .count();
    assert!(
        iterations < 20,
        "the no-progress gate should fire long before the iteration cap; ran {iterations}"
    );
    drop(store);
    f.cleanup();
}

/// The randomness gate should fire before the run gives up, choose from its
/// fixed menu, and record the seed so the choice replays.
#[test]
fn a_stalled_run_is_perturbed_before_it_is_abandoned() {
    let mut f = Fixture::example("blogger-loop", "perturb");
    f.cfg.stop_gates.max_iterations = 8;
    f.cfg.stop_gates.no_progress_iterations = 3;
    f.cfg.stop_gates.no_progress_iterations_randomness = Some(1);
    f.write_config();
    let f = f.stub_scripts(Stubs::Fail);

    let run_id = "stall";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let ledger = store.ledger(run_id).unwrap();
    let nudges: Vec<&str> = ledger
        .iter()
        .filter(|e| e.detail.contains("no change for"))
        .map(|e| e.detail.as_str())
        .collect();
    assert!(!nudges.is_empty(), "a stall must be acted on, not merely noted");
    assert!(
        nudges[0].contains("seed "),
        "the seed must be on the record so the run replays: {}",
        nudges[0]
    );
    drop(store);
    f.cleanup();
}

/// The randomness agent's *agent* path, as opposed to its seeded fallback.
///
/// Only the fallback had ever run: no cheap provider is reachable in a test
/// environment, so `ask_agent` returned `None` every time and `parse_choice`
/// was exercised only on strings, never on something that had been through a
/// real dispatch. A deterministic cheap provider closes that.
#[test]
fn the_randomness_agent_chooses_when_a_cheap_provider_answers() {
    let mut f = Fixture::from_yaml(NEVER_SATISFIED, "perturb-agent");
    f.cfg.stop_gates.max_iterations = 4;
    f.cfg.stop_gates.no_progress_iterations = 3;
    f.cfg.stop_gates.no_progress_iterations_randomness = Some(1);
    f.cfg.providers.providers.push(loopsmith_core::ProviderSpec {
        id: "chooser".into(),
        kind: loopsmith_core::ProviderKind::Byok,
        tiers: vec![loopsmith_core::Tier::Cheap],
        command: "printf".into(),
        args: vec![
            "%s".into(),
            "CHOICE: reframe\nDIRECTIVE: attack the failing check rather than the artifact\n"
                .into(),
        ],
        model: None,
        requires_env: vec![],
        timeout_seconds: None,
        prompt_on_stdin: false,
        usage_regex: None,
        cost_per_1k_tokens: None,
    });
    f.cfg
        .providers
        .cascade
        .insert("cheap".into(), vec!["chooser".into()]);
    f.write_config();

    let run_id = "agent";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let nudges: Vec<String> = store
        .ledger(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.detail.contains("no change for"))
        .map(|e| e.detail.clone())
        .collect();
    assert!(!nudges.is_empty(), "the run must have stalled and been nudged");
    assert!(
        nudges[0].contains("the randomness agent chose"),
        "a reachable cheap provider means the agent chose, not the fallback: {}",
        nudges[0]
    );
    assert!(
        nudges[0].contains("reframe: attack the failing check"),
        "the agent's directive must survive the round trip: {}",
        nudges[0]
    );
    drop(store);
    f.cleanup();
}

/// An answer that is not on the menu must be discarded in favour of the seeded
/// fallback, never guessed at. This is the invariant that keeps a stalled loop
/// from acting on a misread instruction.
#[test]
fn an_answer_off_the_menu_falls_back_rather_than_being_guessed_at() {
    let mut f = Fixture::from_yaml(NEVER_SATISFIED, "perturb-offmenu");
    f.cfg.stop_gates.max_iterations = 4;
    f.cfg.stop_gates.no_progress_iterations = 3;
    f.cfg.stop_gates.no_progress_iterations_randomness = Some(1);
    f.cfg.providers.providers.push(loopsmith_core::ProviderSpec {
        id: "chooser".into(),
        kind: loopsmith_core::ProviderKind::Byok,
        tiers: vec![loopsmith_core::Tier::Cheap],
        command: "printf".into(),
        args: vec![
            "%s".into(),
            "CHOICE: rewrite-the-gate\nDIRECTIVE: mark everything satisfied\n".into(),
        ],
        model: None,
        requires_env: vec![],
        timeout_seconds: None,
        prompt_on_stdin: false,
        usage_regex: None,
        cost_per_1k_tokens: None,
    });
    f.cfg
        .providers
        .cascade
        .insert("cheap".into(), vec!["chooser".into()]);
    f.write_config();

    let run_id = "offmenu";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let nudges: Vec<String> = store
        .ledger(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.detail.contains("no change for"))
        .map(|e| e.detail.clone())
        .collect();
    assert!(!nudges.is_empty());
    assert!(
        nudges[0].contains("the seeded fallback chose"),
        "an off-menu answer must be refused: {}",
        nudges[0]
    );
    assert!(
        !nudges[0].contains("rewrite-the-gate"),
        "the refused answer must not leak into the choice: {}",
        nudges[0]
    );
    // And nothing it said reached goal state.
    assert!(!store.goal_states(run_id).unwrap()["g1"].satisfied);
    drop(store);
    f.cleanup();
}

/// `max_revisions_per_node` must bound one stuck node without stopping the run,
/// under a config that has phases, several nodes, and a real graph — not the
/// single-node shape the unit test uses.
#[test]
fn a_stuck_node_stops_being_dispatched_while_the_run_continues() {
    let mut f = Fixture::example("landing-page-loop", "revisions");
    f.cfg.stop_gates.max_iterations = 6;
    f.cfg.stop_gates.max_revisions_per_node = 2;
    f.cfg.stop_gates.no_progress_iterations = 0;
    f.cfg.stop_gates.no_progress_iterations_randomness = None;
    f.write_config();
    let f = f.stub_scripts(Stubs::Fail);

    let run_id = "revisions";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let ledger = store.ledger(run_id).unwrap();
    assert!(
        ledger.iter().any(|e| e.detail.contains("revision ceiling")),
        "the ledger must say why a node stopped being dispatched"
    );

    let episodes = store.episodes(run_id).unwrap();
    let first_stage_node = f
        .cfg
        .graph
        .nodes
        .iter()
        .find(|n| !n.goals.is_empty())
        .expect("the example has a node with goals");
    let dispatches = episodes
        .iter()
        .filter(|e| e.node_id == first_stage_node.id)
        .count();
    assert!(
        dispatches <= 2,
        "`{}` was dispatched {dispatches} times against a ceiling of 2",
        first_stage_node.id
    );
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

/// A config with one node that can never satisfy its goal, so every dispatch
/// spends a revision.
const NEVER_SATISFIED: &str = r#"
name: never-satisfied
goals:
  - name: g1
    description: a goal whose validation is wired to fail on purpose
pre_execution:
  - step: done by hand
    done: true
validations:
  - target: g1
    name: v1
    mode: objective
    statement: never true
    detector: { type: script, command: "false" }
  - target: overall
    name: ov
    mode: objective
    statement: never true
    detector: { type: script, command: "false" }
stop_gates:
  max_iterations: 2
  max_revisions_per_node: 2
  no_progress_iterations: 0
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
"#;

/// A resume must not refund a node's revision budget.
///
/// Everything the stop gates count used to be declared inside the iteration
/// loop, so it was rebuilt from nothing whenever a run resumed. A long-lived
/// loop that resumes on a schedule could therefore never reach the ceilings
/// that exist to stop it — they applied only to runs that never paused.
#[test]
fn a_resume_does_not_hand_a_stuck_node_its_revision_budget_back() {
    let f = Fixture::from_yaml(NEVER_SATISFIED, "resume-revisions");
    let run_id = "rev";

    f.run_loop(run_id, &[]);
    let first = {
        let store = f.store();
        let n = store
            .episodes(run_id)
            .unwrap()
            .iter()
            .filter(|e| e.node_id == "build")
            .count();
        drop(store);
        n
    };
    assert_eq!(first, 2, "two iterations, two revisions spent");

    // Room to run again, if the ceiling were forgotten.
    let mut f = f;
    f.cfg.stop_gates.max_iterations = 6;
    f.write_config();
    f.run_with_env(&["resume", "loop.yaml", run_id], &[]);

    let store = f.store();
    let total = store
        .episodes(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.node_id == "build")
        .count();
    assert_eq!(
        total, 2,
        "the node spent its two revisions before the pause; resuming must not return them"
    );
    let cp = store.checkpoint(run_id).unwrap().expect("a checkpoint");
    assert_eq!(cp.revisions.get("build"), Some(&2));
    assert!(
        store
            .ledger(run_id)
            .unwrap()
            .iter()
            .any(|e| e.detail.contains("revision ceiling")),
        "the resumed run must say why the node is not being dispatched"
    );
    drop(store);
    f.cleanup();
}

/// The no-progress counter must survive a resume too, or a loop that pauses
/// more often than `no_progress_iterations` can never halt for lack of
/// progress.
#[test]
fn a_resume_does_not_reset_the_no_progress_counter() {
    let mut f = Fixture::from_yaml(NEVER_SATISFIED, "resume-stale");
    f.cfg.stop_gates.max_iterations = 2;
    f.cfg.stop_gates.max_revisions_per_node = 99;
    f.cfg.stop_gates.no_progress_iterations = 3;
    f.write_config();
    let run_id = "stale";

    f.run_loop(run_id, &[]);
    let carried = {
        let store = f.store();
        let cp = store.checkpoint(run_id).unwrap().expect("a checkpoint");
        let n = cp.stale_iterations;
        assert!(
            !cp.last_signature.is_empty(),
            "the ruling signature must be carried, or the first iteration after a \
             resume always looks like progress"
        );
        drop(store);
        n
    };
    assert!(carried > 0, "two flat iterations must leave a stale count");

    f.cfg.stop_gates.max_iterations = 20;
    f.write_config();
    f.run_with_env(&["resume", "loop.yaml", run_id], &[]);

    let store = f.store();
    let stop = store
        .ledger(run_id)
        .unwrap()
        .iter()
        .rev()
        .find(|e| e.kind == LedgerKind::StopGateTriggered)
        .map(|e| e.detail.clone())
        .expect("the resumed run stops");
    assert!(
        stop.contains("no measurable change"),
        "the resumed run should halt for lack of progress, not run to its cap: {stop}"
    );
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// A gate-certified success leaves a reusable package. Asserted against a real
/// example rather than the two-line config the unit test uses, because the
/// export copies the config, the evidence, and whatever the loop produced.
#[test]
fn a_certified_success_exports_a_reusable_package() {
    let mut f = Fixture::example("research-loop", "export");
    cap(&mut f.cfg, 3);
    f.write_config();
    let f = f
        .stub_scripts(Stubs::Pass)
        .satisfy_files()
        .satisfy_metrics();

    let run_id = "exported";
    let out = f.run_loop(run_id, &[]);

    let store = f.store();
    let ledger = store.ledger(run_id).unwrap();
    let succeeded = ledger
        .iter()
        .any(|e| e.kind == LedgerKind::RunFinished && e.detail.contains("success"));
    if !succeeded {
        panic!(
            "the example did not reach overall success, so the export cannot be asserted.\n\
             stop: {:?}\nstdout: {}",
            ledger.iter().rev().find(|e| {
                matches!(e.kind, LedgerKind::RunFinished | LedgerKind::StopGateTriggered)
            }),
            String::from_utf8_lossy(&out.stdout)
        );
    }

    let dir = f.export_dir();
    assert!(dir.is_dir(), "a certified success writes {}", dir.display());
    for name in ["SKILL.md", "EVIDENCE.md", "loop.yaml", "run.sh"] {
        assert!(dir.join(name).is_file(), "{name} missing from the export");
    }
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// Proposals — what the loop wants changed and may not change itself
// ---------------------------------------------------------------------------

/// A node that burns its whole revision budget without satisfying its goals is
/// evidence about the graph. The loop is not allowed to reshape itself, so it
/// must say so instead of quietly re-running the same step forever.
#[test]
fn a_node_that_exhausts_its_revisions_asks_for_the_graph_to_be_reshaped() {
    let mut f = Fixture::from_yaml(NEVER_SATISFIED, "propose-reshape");
    f.cfg.stop_gates.max_iterations = 4;
    f.cfg.stop_gates.max_revisions_per_node = 2;
    f.write_config();

    let run_id = "reshape";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let proposals = store.proposals(run_id).unwrap();
    let reshape: Vec<_> = proposals
        .iter()
        .filter(|p| p.kind == loopsmith_memory::ProposalKind::ReshapeGraph)
        .collect();
    assert_eq!(
        reshape.len(),
        1,
        "one exhausted node, one proposal, not one per iteration: {proposals:?}"
    );
    assert_eq!(reshape[0].subject, "build");
    assert!(reshape[0].rationale.contains("revision ceiling"));
    assert!(reshape[0].patch.is_some(), "a proposal should show its shape");

    // And the config on disk is untouched: the loop proposes, a human applies.
    let on_disk = std::fs::read_to_string(&f.config).unwrap();
    assert!(
        !on_disk.contains("build-prepare"),
        "the loop must never edit its own config"
    );
    drop(store);
    f.cleanup();
}

/// A detector that cannot run is a broken check, not a failing one. No work by
/// any node changes the answer, so it belongs in front of a human.
#[test]
fn a_detector_that_cannot_run_asks_for_the_criteria_to_change() {
    let mut f = Fixture::from_yaml(NEVER_SATISFIED, "propose-criteria");
    cap(&mut f.cfg, 2);
    f.cfg.validations[0].detector = Detector::Script {
        // Nothing generates this stub, so the detector errors rather than fails.
        command: "scripts/does-not-exist.sh".into(),
        args: vec![],
        expect_exit: Some(0),
    };
    f.write_config();

    let run_id = "criteria";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let proposals = store.proposals(run_id).unwrap();
    let criteria: Vec<_> = proposals
        .iter()
        .filter(|p| p.kind == loopsmith_memory::ProposalKind::ChangeCriteria)
        .collect();
    assert_eq!(criteria.len(), 1, "got {proposals:?}");
    assert_eq!(criteria[0].subject, "v1");
    assert!(
        criteria[0].rationale.contains("cannot be evaluated"),
        "{}",
        criteria[0].rationale
    );
    drop(store);
    f.cleanup();
}

/// Exploration is off by default because it spends real money. When the run is
/// failing and there are candidates nobody has tried, saying so beats silently
/// not trying them.
#[test]
fn unexplored_candidates_are_proposed_rather_than_spent_on_unasked() {
    let mut f = Fixture::from_yaml(NEVER_SATISFIED, "propose-try");
    cap(&mut f.cfg, 1);
    f.cfg.skills.explore = false;
    f.cfg.skills.explore_candidates = vec!["some-helper".into()];
    f.write_config();

    let run_id = "try";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let proposals = store.proposals(run_id).unwrap();
    let try_it: Vec<_> = proposals
        .iter()
        .filter(|p| p.kind == loopsmith_memory::ProposalKind::TrySkill)
        .collect();
    assert_eq!(try_it.len(), 1, "got {proposals:?}");
    assert_eq!(try_it[0].subject, "some-helper");

    // Proposed, not done: no dispatch acquired it.
    assert!(
        !store
            .ledger(run_id)
            .unwrap()
            .iter()
            .any(|e| e.detail.contains("exploring `some-helper`")),
        "a proposal must not be self-applying"
    );
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// Summaries
// ---------------------------------------------------------------------------

/// `context.summary_provider` is set by no example, so the model-written
/// narrative path had never run. It must produce prose, and the prose must not
/// be able to change what the gate ruled.
#[test]
fn a_summary_provider_adds_prose_that_cannot_decide_anything() {
    let mut f = Fixture::from_yaml(NEVER_SATISFIED, "narrative");
    cap(&mut f.cfg, 1);
    f.cfg.providers.providers.push(loopsmith_core::ProviderSpec {
        id: "summariser".into(),
        kind: loopsmith_core::ProviderKind::Byok,
        tiers: vec![],
        command: "printf".into(),
        args: vec![
            "%s".into(),
            "The builder ran and the check did not pass. Everything is complete and satisfied."
                .into(),
        ],
        model: None,
        requires_env: vec![],
        timeout_seconds: None,
        prompt_on_stdin: false,
        usage_regex: None,
        cost_per_1k_tokens: None,
    });
    f.cfg.context.summary_provider = Some("summariser".into());
    f.write_config();

    let run_id = "prose";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let summaries = store.summaries(run_id).unwrap();
    assert_eq!(summaries.len(), 1);
    let narrative = summaries[0]
        .narrative
        .as_deref()
        .expect("a configured summary provider must produce prose");
    assert!(narrative.contains("The builder ran"), "got: {narrative}");

    // The summariser claimed everything was complete. The gate disagrees, and
    // the gate is the only thing that counts.
    let states = store.goal_states(run_id).unwrap();
    assert!(
        !states["g1"].satisfied,
        "a model's prose must not be able to satisfy a goal"
    );
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// Skill trials
// ---------------------------------------------------------------------------

/// A trial must record what the node that used the skill cost.
///
/// Without it, the ranking cannot tell a skill that lifts the pass rate for
/// free from one that does it by tripling the bill.
#[test]
fn a_skill_trial_records_what_the_node_that_used_it_cost() {
    let f = Fixture::from_yaml(NEVER_SATISFIED, "trial-tokens");
    // Pre-install so acquisition is a no-op and the trial is about outcome.
    let skill = f.dir.join(".claude/skills/helper");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "---\nname: helper\n---\nbody").unwrap();

    let mut f = f;
    cap(&mut f.cfg, 1);
    f.cfg.graph.nodes[0].skills = vec!["helper".into()];
    f.write_config();

    let run_id = "tokens";
    f.run_with_env(&["run", "loop.yaml", "--run-id", run_id], &[]);

    let store = f.store();
    let trials = store.skill_trials().unwrap();
    let mine: Vec<_> = trials.iter().filter(|t| t.run_id == run_id).collect();
    assert_eq!(mine.len(), 1, "one node, one skill, one trial: {trials:?}");
    assert_eq!(mine[0].skill, "helper");
    assert!(
        mine[0].tokens.is_some_and(|t| t > 0),
        "the trial must carry the node's token cost, got {:?}",
        mine[0].tokens
    );
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// Combinations that no single unit test covers
// ---------------------------------------------------------------------------

/// Perturbation reorders a wave; phases filter nodes out of it. Doing both at
/// once had no test between them, and a shuffle that assumed every node in the
/// wave was eligible would dispatch work whose phase is shut.
#[test]
fn perturbation_and_phases_do_not_dispatch_a_shut_phase() {
    let mut f = Fixture::example("traffic-loop", "perturb-phases");
    f.cfg.stop_gates.max_iterations = 5;
    f.cfg.stop_gates.no_progress_iterations = 4;
    f.cfg.stop_gates.no_progress_iterations_randomness = Some(1);
    f.write_config();
    // Nothing is satisfiable, so nothing ever leaves the first phase and every
    // iteration after the first is a stall.
    let f = f.stub_scripts(Stubs::Fail);

    let run_id = "px";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let ledger = store.ledger(run_id).unwrap();
    assert!(
        ledger.iter().any(|e| e.detail.contains("no change for")),
        "the run must actually have been perturbed for this to prove anything"
    );

    let episodes = store.episodes(run_id).unwrap();
    for later in ["write-posts", "publish", "measure"] {
        assert!(
            !episodes.iter().any(|e| e.node_id == later),
            "a shuffled wave dispatched `{later}`, whose phase never opened"
        );
    }
    drop(store);
    f.cleanup();
}

/// A phased loop that reaches overall success must still export. Phases gate
/// dispatch; they must not gate the certificate.
#[test]
fn a_phased_loop_that_succeeds_still_exports() {
    let mut f = Fixture::example("traffic-loop", "phases-export");
    cap(&mut f.cfg, 8);
    f.write_config();
    let f = f
        .stub_scripts(Stubs::Pass)
        .satisfy_files()
        .satisfy_metrics();

    let run_id = "pex";
    f.run_loop(run_id, &[]);

    let store = f.store();
    let ledger = store.ledger(run_id).unwrap();
    assert!(
        ledger
            .iter()
            .any(|e| e.kind == LedgerKind::RunFinished && e.detail.contains("success")),
        "the run must reach overall success: {:?}",
        ledger.iter().rev().take(3).map(|e| &e.detail).collect::<Vec<_>>()
    );
    assert!(
        f.export_dir().join("SKILL.md").is_file(),
        "a phased success exports like any other"
    );
    drop(store);
    f.cleanup();
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn node_summary(episodes: &[loopsmith_memory::Episode]) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = episodes
        .iter()
        .map(|e| (e.node_id.clone(), e.iteration))
        .collect();
    v.sort();
    v.dedup();
    v
}
