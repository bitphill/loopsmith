//! Behaviour of a whole run, driven through `execute`.

use super::*;
use loopsmith_memory::{Checkpoint, LedgerKind, SledStore};
use std::path::Path;
use std::time::Instant;

fn store(tag: &str) -> (SledStore, PathBuf) {
    let d = loopsmith_util::testing::temp_dir(tag);
    (SledStore::open(d.join("state")).unwrap(), d)
}

fn cfg(extra: &str) -> LoopConfig {
    let text = format!(
        r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
pre_execution:
  - step: done by hand
    done: true
validations:
  - target: g1
    name: v1
    mode: objective
    statement: always true
    detector: {{ type: script, command: "true" }}
  - target: overall
    name: ov
    mode: objective
    statement: always true
    detector: {{ type: script, command: "true" }}
graph:
  nodes:
    - id: build
      role: builder
      instruction: produce the thing described above
      goals: [g1]
providers:
  providers:
    - id: echoer
      kind: byok
      command: echo
      args: ["ok"]
  cascade:
    standard: [echoer]
{extra}
"#
    );
    loopsmith_core::parse_str(&text, "test").expect("parses")
}

fn opts(run: &str, dir: &Path) -> RunOptions {
    RunOptions {
        run_id: run.into(),
        workdir: dir.to_path_buf(),
        dry_run: false,
        resume: false,
        acquire_skills: false,
        verbose: false,
        config_file: "loop.yaml".into(),
    }
}

#[test]
fn a_satisfiable_loop_stops_on_overall_success() {
    let (s, d) = store("success");
    let out = execute(&cfg(""), &s, &opts("r1", &d)).unwrap();
    assert_eq!(out.stop, StopReason::OverallSuccess);
    assert_eq!(out.iterations, 1);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn tokens_are_now_accounted_so_the_budget_gate_can_fire() {
    let (s, d) = store("tokens");
    let mut c = cfg("stop_gates:\n  max_iterations: 50\n  no_progress_iterations: 0\n  max_tokens: 1\n");
    // Make success impossible so the token ceiling is what stops it.
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };
    let out = execute(&c, &s, &opts("r2", &d)).unwrap();
    assert_eq!(out.stop, StopReason::TokenBudget(1));
    assert!(out.tokens_used > 0, "usage must actually accumulate");
    assert!(out.tokens_estimated, "echo reports nothing, so this is an estimate");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn the_cost_ceiling_fires_when_a_rate_is_configured() {
    let (s, d) = store("cost");
    let mut c = cfg("stop_gates:\n  max_iterations: 50\n  no_progress_iterations: 0\n  max_cost_usd: 0.000001\n");
    c.execution.providers.providers[0].cost_per_1k_tokens = Some(1000.0);
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };
    let out = execute(&c, &s, &opts("r3", &d)).unwrap();
    assert!(matches!(out.stop, StopReason::CostBudget(_)));
    assert!(out.cost_usd > 0.0);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_judge_verdict_now_reaches_the_gate() {
    let (s, d) = store("judge");
    // Builder on one provider, judge on another, and a judge detector that
    // could never pass before this wiring existed.
    let c = loopsmith_core::parse_str(
        r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
pre_execution:
  - step: done
    done: true
validations:
  - target: g1
    name: prose
    mode: subjective
    statement: reads well
    detector: { type: judge, standard: "the house style guide" }
  - target: overall
    name: prose-overall
    mode: subjective
    statement: reads well
    detector: { type: judge, standard: "the house style guide" }
graph:
  nodes:
    - id: build
      role: builder
      instruction: write the thing described in the goal
      goals: [g1]
      provider: maker
    - id: review
      role: judge
      instruction: check the draft against the named standard and report
      depends_on: [build]
      goals: [g1]
      provider: checker
providers:
  providers:
    - id: maker
      kind: byok
      command: echo
      args: ["a draft"]
    - id: checker
      kind: byok
      command: printf
      args: ["VERDICT: prose PASS\nEVIDENCE: matches the guide\nVERDICT: prose-overall PASS\nEVIDENCE: matches the guide\n"]
  cascade:
    standard: [maker]
"#,
        "test",
    )
    .unwrap();
    let out = execute(&c, &s, &opts("r4", &d)).unwrap();
    assert_eq!(
        out.stop,
        StopReason::OverallSuccess,
        "judge verdicts should satisfy the gate; got {:?}",
        out.stop
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_judge_on_the_builders_provider_still_cannot_satisfy_the_gate() {
    let (s, d) = store("selfjudge");
    let c = loopsmith_core::parse_str(
        r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
pre_execution:
  - step: done
    done: true
validations:
  - target: g1
    name: prose
    mode: subjective
    statement: reads well
    detector: { type: judge, standard: "the house style guide" }
stop_gates:
  max_iterations: 1
graph:
  nodes:
    - id: build
      role: builder
      instruction: write the thing described in the goal
      goals: [g1]
      provider: only
    - id: review
      role: judge
      instruction: check the draft against the named standard and report
      depends_on: [build]
      goals: [g1]
      provider: only
providers:
  providers:
    - id: only
      kind: byok
      command: printf
      args: ["VERDICT: prose PASS\nEVIDENCE: looks great to me\n"]
  cascade:
    standard: [only]
"#,
        "test",
    )
    .unwrap();
    let out = execute(&c, &s, &opts("r5", &d)).unwrap();
    assert!(!out.stop.is_success(), "self-judgment must not pass");
    assert!(!out.verdicts["g1"].satisfied);
    let _ = std::fs::remove_dir_all(d);
}

/// A provider command that reliably takes a beat, on this platform.
///
/// `sleep` is not a Windows command, so the original spelling measured
/// nothing there — the spawn failed instantly, the run took a different path,
/// and the wall-clock assertion tripped for a reason that had nothing to do
/// with concurrency. `ping -n` is the delay every Windows box has.
///
/// Returns the YAML fragment, the per-node delay, and the ceiling below which
/// the wave must finish. The ceiling differs because the two delays do: what
/// matters is that serial execution would take three times the delay and the
/// ceiling sits well under that.
fn sleeper_provider() -> (&'static str, std::time::Duration) {
    if cfg!(windows) {
        // `-n 2` is one second of gap between two pings. Serial: ~3s.
        (
            "    - id: sleeper\n      kind: byok\n      command: ping\n      \
             args: [\"-n\", \"2\", \"127.0.0.1\"]\n",
            std::time::Duration::from_millis(2200),
        )
    } else {
        // Serial: ~1.5s.
        (
            "    - id: sleeper\n      kind: byok\n      command: sleep\n      \
             args: [\"0.5\"]\n",
            std::time::Duration::from_millis(1200),
        )
    }
}

#[test]
fn independent_nodes_in_a_wave_run_concurrently() {
    let (s, d) = store("parallel");
    let (sleeper, ceiling) = sleeper_provider();
    // Three sleepers in one wave. Run serially that is three delays; with the
    // chosen concurrency it should finish in roughly one.
    let c = loopsmith_core::parse_str(
        &format!(
            "{}{sleeper}{}",
            r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
pre_execution:
  - step: done
    done: true
validations:
  - target: g1
    name: v
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
    - id: a
      role: builder
      instruction: one of three independent nodes
      goals: [g1]
    - id: b
      role: builder
      instruction: one of three independent nodes
      goals: [g1]
    - id: c
      role: builder
      instruction: one of three independent nodes
      goals: [g1]
  concurrency:
    mode: fixed
    max_parallel: 3
providers:
  providers:
"#,
            r#"  cascade:
    standard: [sleeper]
"#
        ),
        "test",
    )
    .unwrap();
    let t0 = Instant::now();
    let out = execute(&c, &s, &opts("r6", &d)).unwrap();
    let elapsed = t0.elapsed();
    assert!(out.stop.is_success());
    assert!(
        elapsed < ceiling,
        "three sleepers took {elapsed:?}, over the {ceiling:?} ceiling; \
         they did not run in parallel"
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn skill_trials_are_recorded_and_become_proposals() {
    let (s, d) = store("trials");
    // Pre-install a skill so acquisition is a no-op and the trial is about
    // outcome rather than installation.
    let sk = d.join(".claude/skills/helper");
    std::fs::create_dir_all(&sk).unwrap();
    std::fs::write(sk.join("SKILL.md"), "---\nname: helper\n---\nbody").unwrap();

    let mut c = cfg("");
    c.execution.graph.nodes[0].skills = vec!["helper".into()];
    let mut o = opts("r7", &d);
    o.acquire_skills = true;

    execute(&c, &s, &o).unwrap();
    let trials = s.skill_trials().unwrap();
    assert!(!trials.is_empty(), "using a skill must record a trial");
    assert_eq!(trials[0].skill, "helper");
    assert!(trials[0].satisfied);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn dry_run_dispatches_nothing_but_still_plans() {
    let (s, d) = store("dry");
    let mut o = opts("r8", &d);
    o.dry_run = true;
    let out = execute(&cfg(""), &s, &o).unwrap();
    assert!(s.episodes("r8").unwrap().is_empty());
    assert!(out.iterations >= 1);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn resume_continues_from_the_saved_iteration() {
    let (s, d) = store("resume");
    let mut c = cfg("stop_gates:\n  max_iterations: 2\n  no_progress_iterations: 0\n");
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };
    let first = execute(&c, &s, &opts("r9", &d)).unwrap();
    assert_eq!(first.iterations, 2);
    let mut o = opts("r9", &d);
    o.resume = true;
    let second = execute(&c, &s, &o).unwrap();
    assert_eq!(second.iterations, 3);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_node_that_never_satisfies_its_goals_stops_being_dispatched() {
    // `max_revisions_per_node` was declared, defaulted, schema'd, written
    // by the scaffold, documented in two files — and read nowhere. This is
    // the behaviour it was always supposed to buy: one stuck node must not
    // be able to spend the entire iteration budget.
    let (s, d) = store("revisions");
    let mut c = cfg("stop_gates:\n  max_iterations: 8\n  max_revisions_per_node: 2\n  no_progress_iterations: 0\n");
    // g1 can never be satisfied, so every dispatch of `build` is a revision.
    c.safety.checks[0].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };

    let out = execute(&c, &s, &opts("r11", &d)).unwrap();
    assert_eq!(out.stop, StopReason::IterationCap(8), "the run itself runs on");

    let dispatched = s
        .episodes("r11")
        .unwrap()
        .iter()
        .filter(|e| e.node_id == "build")
        .count();
    assert_eq!(
        dispatched, 2,
        "the node should stop after 2 revisions, not run all 8 iterations"
    );

    assert!(
        s.ledger("r11")
            .unwrap()
            .iter()
            .any(|e| e.detail.contains("revision ceiling")),
        "the ledger must say why the node stopped being dispatched"
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_node_whose_goals_are_satisfied_is_never_capped() {
    // The counter measures *failed* revisions. A node that does its job is
    // not spending revisions, so a long run must not silently stop calling
    // it once it passes some arbitrary count.
    let (s, d) = store("nocap");
    let mut c = cfg("stop_gates:\n  max_iterations: 5\n  max_revisions_per_node: 2\n  no_progress_iterations: 0\n");
    // g1 passes every time; only `overall` fails, so the run keeps going.
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };

    execute(&c, &s, &opts("r12", &d)).unwrap();
    let dispatched = s
        .episodes("r12")
        .unwrap()
        .iter()
        .filter(|e| e.node_id == "build")
        .count();
    assert_eq!(dispatched, 5, "a satisfying node runs every iteration");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_successful_run_leaves_a_reusable_package_behind() {
    let (s, d) = store("export-ok");
    std::fs::create_dir_all(d.join("out")).unwrap();
    std::fs::write(d.join("out/result.md"), "the deliverable").unwrap();

    let out = execute(&cfg(""), &s, &opts("r17", &d)).unwrap();
    assert!(out.stop.is_success());

    let dir = out.export_path.expect("success writes an export");
    assert!(dir.ends_with("t-success"), "got {}", dir.display());
    for f in ["SKILL.md", "EVIDENCE.md", "loop.yaml", "run.sh", "out/result.md"] {
        assert!(dir.join(f).is_file(), "{f} missing");
    }
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_run_that_missed_the_bar_leaves_no_package() {
    // The export is a certificate. A run that did not meet its bar must not
    // produce one, and there is no flag that makes it.
    let (s, d) = store("export-none");
    let mut c = cfg("stop_gates:\n  max_iterations: 1\n  no_progress_iterations: 0\n");
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };

    let out = execute(&c, &s, &opts("r18", &d)).unwrap();
    assert!(!out.stop.is_success());
    assert!(out.export_path.is_none(), "no bar met, no certificate");
    assert!(!d.join("t-success").exists());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_stalled_run_varies_its_approach_before_it_gives_up() {
    let (s, d) = store("perturb");
    let mut c = cfg(
        "stop_gates:\n  max_iterations: 6\n  no_progress_iterations: 3\n  no_progress_iterations_randomness: 1\n",
    );
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };

    let out = execute(&c, &s, &opts("r16", &d)).unwrap();
    assert_eq!(
        out.stop,
        StopReason::NoProgress(3),
        "it must still halt; perturbation delays giving up, it does not prevent it"
    );

    let ledger = s.ledger("r16").unwrap();
    let nudges: Vec<&str> = ledger
        .iter()
        .filter(|e| e.detail.contains("no change for"))
        .map(|e| e.detail.as_str())
        .collect();
    assert!(!nudges.is_empty(), "a stall must be acted on, not just noted");
    assert!(
        nudges[0].contains("seed "),
        "the seed must be recorded so the run replays: {}",
        nudges[0]
    );
    assert!(
        nudges[0].contains("the seeded fallback chose"),
        "with no cheap provider reachable it must fall back, not skip: {}",
        nudges[0]
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn nothing_that_perturbs_or_summarises_can_reach_goal_state() {
    // The gate is the only writer of `goal_satisfied`. Both of the pieces
    // added for stalls — the summariser and the randomness agent — take a
    // model's output as input, so this asserts structurally that neither
    // has a path to the one function that could hand a model the verdict.
    for file in ["perturb.rs", "summary.rs"] {
        let src = std::fs::read_to_string(format!(
            "{}/src/{file}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("{file} is readable: {e}"));
        for forbidden in ["set_goal_state", "to_goal_state", "GoalState"] {
            assert!(
                !src.contains(forbidden),
                "{file} references `{forbidden}`; only the gate may touch goal state"
            );
        }
    }
}

#[test]
fn each_iteration_is_summarised_and_the_next_one_reads_it() {
    // Before this existed, every iteration sent a byte-identical prompt —
    // which is why a stalled loop kept re-running the approach that had
    // already failed. The digests changing is the proof it no longer does.
    let (s, d) = store("summaries");
    let mut c = cfg("stop_gates:\n  max_iterations: 3\n  no_progress_iterations: 0\n");
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };

    execute(&c, &s, &opts("r14", &d)).unwrap();

    let summaries = s.summaries("r14").unwrap();
    assert_eq!(summaries.len(), 3, "one summary per iteration");
    assert_eq!(summaries[0].iteration, 1, "stored oldest first");
    assert!(summaries[0].headline.contains("node(s) ran"));
    assert!(
        summaries[0].narrative.is_none(),
        "no summary provider configured, so no prose is bought"
    );
    assert!(
        summaries[1]
            .facts
            .iter()
            .any(|f| f.contains("No verdict changed")),
        "the second summary should report the stall: {:?}",
        summaries[1].facts
    );

    let episodes = s.episodes("r14").unwrap();
    let first = episodes.iter().find(|e| e.iteration == 1).unwrap();
    let second = episodes.iter().find(|e| e.iteration == 2).unwrap();
    assert_ne!(
        first.prompt_digest, second.prompt_digest,
        "iteration 2 must not be handed the same prompt iteration 1 already failed with"
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn carry_forward_can_be_switched_off() {
    let (s, d) = store("nocarry");
    let mut c = cfg(
        "stop_gates:\n  max_iterations: 2\n  no_progress_iterations: 0\ncontext:\n  carry_summaries: 0\n",
    );
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };

    execute(&c, &s, &opts("r15", &d)).unwrap();

    // Summaries are still recorded — they are the run's history — but the
    // prompt no longer carries them, so it is identical again.
    assert_eq!(s.summaries("r15").unwrap().len(), 2);
    let episodes = s.episodes("r15").unwrap();
    let first = episodes.iter().find(|e| e.iteration == 1).unwrap();
    let second = episodes.iter().find(|e| e.iteration == 2).unwrap();
    assert_eq!(first.prompt_digest, second.prompt_digest);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_run_writes_a_readable_log_beside_the_config() {
    let (s, d) = store("runlog");
    let out = execute(&cfg(""), &s, &opts("r13", &d)).unwrap();

    let path = out.log_path.expect("a run opens a log");
    assert!(
        path.starts_with(d.join("logs")),
        "the log belongs in logs/, not state/: {}",
        path.display()
    );

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("RunStarted"), "got: {text}");
    assert!(text.contains("IterationStarted"), "got: {text}");
    assert!(text.contains("RunFinished"), "got: {text}");

    // The log and the ledger are written through one call, so they must
    // hold the same number of events.
    assert_eq!(
        text.lines().count(),
        s.ledger("r13").unwrap().len(),
        "the log and the ledger disagree about what happened"
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn every_stop_is_written_to_the_ledger() {
    let (s, d) = store("ledger");
    let out = execute(&cfg(""), &s, &opts("r10", &d)).unwrap();
    let entries = s.ledger("r10").unwrap();
    assert!(entries.iter().any(|e| e.kind == LedgerKind::RunStarted));
    assert!(entries
        .iter()
        .any(|e| e.kind == LedgerKind::RunFinished && e.detail == out.stop.describe()));
    let _ = std::fs::remove_dir_all(d);
}

// --- lifecycle ---------------------------------------------------------------

/// The same loop, with its overall check made to fail.
fn failing(mut c: LoopConfig) -> LoopConfig {
    c.safety.checks[1].detector = loopsmith_core::Detector::Script {
        command: "false".into(),
        args: vec![],
        expect_exit: Some(0),
    };
    c
}

/// Every state the ledger says the run entered, in order.
fn road(s: &SledStore, run: &str) -> Vec<String> {
    s.ledger(run)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == LedgerKind::StateChanged)
        .filter_map(|e| {
            let moved = e.detail.split(':').next()?.to_string();
            moved.split(" → ").nth(1).map(str::to_string)
        })
        .collect()
}

#[test]
fn a_certified_run_closes_from_succeeded() {
    let (s, d) = store("life-ok");
    let out = execute(&cfg(""), &s, &opts("life-ok", &d)).unwrap();
    assert_eq!(out.state, RunState::Succeeded);
    assert_eq!(
        road(&s, "life-ok"),
        ["validating", "planning", "running", "succeeded", "closed"]
    );
    let cp = s.checkpoint("life-ok").unwrap().unwrap();
    assert_eq!(cp.state.as_deref(), Some("closed"));
    assert_eq!(cp.outcome.as_deref(), Some("succeeded"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_run_out_of_iterations_is_paused_not_failed() {
    // It did nothing wrong; it ran out of what it was given. A resume with a
    // bigger budget is the ordinary next step.
    let (s, d) = store("life-cap");
    let c = failing(cfg("stop_gates:\n  max_iterations: 1\n  no_progress_iterations: 0\n"));
    let out = execute(&c, &s, &opts("life-cap", &d)).unwrap();
    assert_eq!(out.state, RunState::Paused);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_run_with_nothing_moving_is_blocked() {
    let (s, d) = store("life-stall");
    let c = failing(cfg("stop_gates:\n  max_iterations: 9\n  no_progress_iterations: 2\n"));
    let out = execute(&c, &s, &opts("life-stall", &d)).unwrap();
    assert_eq!(out.state, RunState::Blocked);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn resuming_a_run_that_died_mid_iteration_says_so() {
    let (s, d) = store("life-crash");
    let c = failing(cfg("stop_gates:\n  max_iterations: 1\n  no_progress_iterations: 0\n"));
    // What a killed process leaves behind: a checkpoint still in `running`.
    s.save_checkpoint(&Checkpoint {
        iteration: 1,
        state: Some("running".into()),
        ..Checkpoint::new("life-crash")
    })
    .unwrap();

    let mut o = opts("life-crash", &d);
    o.resume = true;
    let out = execute(&c, &s, &o).unwrap();

    let said = s
        .ledger("life-crash")
        .unwrap()
        .into_iter()
        .any(|e| e.kind == LedgerKind::StateChanged && e.detail.contains("never closed it"));
    assert!(said, "the crash must be reported, not silently resumed over");
    assert_eq!(out.iterations, 2, "the resume continues from the saved iteration");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn an_unschedulable_graph_is_recorded_as_a_failed_run() {
    let (s, d) = store("life-cycle");
    let mut c = cfg("");
    let second = c.execution.graph.nodes[0].clone();
    c.execution.graph.nodes.push(loopsmith_core::NodeSpec {
        id: "other".into(),
        depends_on: vec!["build".into()],
        ..second
    });
    c.execution.graph.nodes[0].depends_on = vec!["other".into()];

    let err = execute(&c, &s, &opts("life-cycle", &d));
    assert!(err.is_err(), "a cycle cannot be planned");
    let cp = s.checkpoint("life-cycle").unwrap().unwrap();
    assert_eq!(cp.outcome.as_deref(), Some("failed"));
    let _ = std::fs::remove_dir_all(d);
}

// --- rules -------------------------------------------------------------------

fn rules(yaml: &str) -> Vec<loopsmith_core::GateRule> {
    serde_yaml::from_str(yaml).expect("rules parse")
}

fn ledger_says(s: &SledStore, run: &str, kind: LedgerKind, text: &str) -> bool {
    s.ledger(run)
        .unwrap()
        .iter()
        .any(|e| e.kind == kind && e.detail.contains(text))
}

fn resumed(run: &str, d: &Path) -> RunOptions {
    let mut o = opts(run, d);
    o.resume = true;
    o
}

#[test]
fn a_failed_entry_rule_stops_the_run_before_it_spends_anything() {
    let (s, d) = store("entry");
    let mut c = cfg("");
    c.safety.gates.entry = rules(
        "- id: brief\n  statement: the brief is written\n  \
         detector: { type: file_exists, path: BRIEF.md }\n  on_fail: pause\n",
    );
    let out = execute(&c, &s, &opts("entry", &d)).unwrap();
    assert_eq!(out.state, RunState::Paused);
    assert_eq!(out.iterations, 0);
    assert!(s.episodes("entry").unwrap().is_empty(), "nothing may be dispatched");

    // Once the world is as the rule asks, the same run goes through.
    std::fs::write(d.join("BRIEF.md"), "go").unwrap();
    let out = execute(&c, &s, &resumed("entry", &d)).unwrap();
    assert_eq!(out.state, RunState::Succeeded);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn an_approval_rule_holds_the_run_until_its_artifact_exists() {
    let (s, d) = store("approval");
    let mut c = cfg("");
    c.safety.gates.approval = rules(
        "- id: signed-off\n  statement: a human approved this run\n  \
         detector: { type: file_exists, path: APPROVED }\n  on_fail: escalate\n",
    );
    let out = execute(&c, &s, &opts("approval", &d)).unwrap();
    assert_eq!(out.state, RunState::Escalated);
    assert!(road(&s, "approval").contains(&"awaiting_approval".to_string()));
    assert!(s.episodes("approval").unwrap().is_empty());

    std::fs::write(d.join("APPROVED"), "yes").unwrap();
    let out = execute(&c, &s, &resumed("approval", &d)).unwrap();
    assert_eq!(out.state, RunState::Succeeded);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn approval_rules_are_skipped_and_said_to_be_when_human_approval_is_off() {
    let (s, d) = store("approval-off");
    let mut c = cfg("");
    c.features.human_approval = false;
    c.safety.gates.approval = rules(
        "- id: signed-off\n  statement: s\n  detector: { type: file_exists, path: APPROVED }\n",
    );
    let out = execute(&c, &s, &opts("approval-off", &d)).unwrap();
    assert_eq!(out.state, RunState::Succeeded);
    assert!(ledger_says(&s, "approval-off", LedgerKind::RuleEvaluated, "not checked"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_failed_rollback_rule_discards_the_iterations_progress_but_not_its_spend() {
    let (s, d) = store("rollback");
    let mut c = cfg("");
    c.safety.gates.rollback = rules(
        "- id: canary\n  statement: the canary survives\n  \
         detector: { type: file_exists, path: canary }\n  on_fail: rollback\n",
    );
    let out = execute(&c, &s, &opts("rollback", &d)).unwrap();
    assert_eq!(out.state, RunState::RolledBack);
    let cp = s.checkpoint("rollback").unwrap().unwrap();
    assert!(cp.completed_nodes.is_empty(), "the iteration's progress is discarded");
    assert!(cp.tokens_used > 0, "what it spent is not refunded");
    let _ = std::fs::remove_dir_all(d);
}

// --- recovery ------------------------------------------------------------------

fn provider_script(c: &mut LoopConfig, script: &str) {
    let p = &mut c.execution.providers.providers[0];
    p.command = "sh".into();
    p.args = vec!["-c".into(), script.into()];
}

#[test]
fn a_transient_provider_failure_is_retried_and_the_run_carries_on() {
    let (s, d) = store("retry");
    let mut c = cfg("");
    provider_script(
        &mut c,
        "if [ -f .tried ]; then echo ok; else touch .tried; \
         echo '429 Too Many Requests' >&2; exit 1; fi",
    );
    c.safety.recovery.transient_error = loopsmith_core::RecoveryAction::Retry {
        max_attempts: 3,
        base_delay_seconds: 0,
        backoff: loopsmith_core::Backoff::Fixed,
    };
    let out = execute(&c, &s, &opts("retry", &d)).unwrap();
    assert_eq!(out.state, RunState::Succeeded);
    assert!(road(&s, "retry").contains(&"retrying".to_string()));
    assert!(ledger_says(&s, "retry", LedgerKind::Recovered, "transient error"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_tool_that_fails_the_same_way_every_time_is_not_retried() {
    // A wrong flag fails identically on the second try. Retrying it with
    // backoff spends the wall-clock budget learning that.
    let (s, d) = store("no-retry");
    let mut c = failing(cfg("stop_gates:\n  max_iterations: 1\n  no_progress_iterations: 0\n"));
    provider_script(&mut c, "echo \"error: unexpected argument '--quiet'\" >&2; exit 2");
    let out = execute(&c, &s, &opts("no-retry", &d)).unwrap();
    assert_eq!(out.state, RunState::Paused);
    assert!(!road(&s, "no-retry").contains(&"retrying".to_string()));
    assert!(ledger_says(&s, "no-retry", LedgerKind::Recovered, "tool unavailable"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_judge_that_ignores_the_output_contract_is_asked_again() {
    let (s, d) = store("revise");
    let c = loopsmith_core::parse_str(
        r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
pre_execution:
  - step: done
    done: true
validations:
  - target: g1
    name: prose
    mode: subjective
    statement: reads well
    detector: { type: judge, standard: "the house style guide" }
  - target: overall
    name: prose-overall
    mode: subjective
    statement: reads well
    detector: { type: judge, standard: "the house style guide" }
graph:
  nodes:
    - id: build
      role: builder
      instruction: write the thing described in the goal
      goals: [g1]
      provider: maker
    - id: review
      role: judge
      instruction: check the draft against the named standard and report
      depends_on: [build]
      goals: [g1]
      provider: checker
providers:
  providers:
    - id: maker
      kind: byok
      command: echo
      args: ["a draft"]
    - id: checker
      kind: byok
      command: sh
      args: ["-c", "if [ -f .judged ]; then printf 'VERDICT: prose PASS\nEVIDENCE: e\nVERDICT: prose-overall PASS\nEVIDENCE: e\n'; else touch .judged; echo looks fine to me; fi"]
  cascade:
    standard: [maker]
"#,
        "test",
    )
    .unwrap();
    let out = execute(&c, &s, &opts("revise", &d)).unwrap();
    assert_eq!(out.state, RunState::Succeeded, "the second answer satisfies the gate");
    assert!(ledger_says(&s, "revise", LedgerKind::Recovered, "invalid output"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_node_out_of_revisions_is_escalated_and_a_resume_answers_it() {
    let (s, d) = store("escalate");
    let mut c = failing(cfg(
        "stop_gates:\n  max_iterations: 4\n  max_revisions_per_node: 1\n  no_progress_iterations: 0\n",
    ));
    c.safety.checks[0].detector = c.safety.checks[1].detector.clone();
    let out = execute(&c, &s, &opts("escalate", &d)).unwrap();
    assert_eq!(out.stop, StopReason::IterationCap(4), "the stop gate still fired");
    assert_eq!(out.state, RunState::Escalated, "but a human is what it waits on");
    assert_eq!(s.checkpoint("escalate").unwrap().unwrap().escalations.len(), 1);

    let _ = execute(&c, &s, &resumed("escalate", &d)).unwrap();
    assert!(ledger_says(&s, "escalate", LedgerKind::Escalated, "resuming answers 1"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_budget_policy_of_stop_fails_the_run_instead_of_pausing_it() {
    let (s, d) = store("exhaust-stop");
    let mut c = failing(cfg("stop_gates:\n  max_iterations: 1\n  no_progress_iterations: 0\n"));
    c.safety.recovery.resource_exhaustion = loopsmith_core::RecoveryAction::Stop;
    let out = execute(&c, &s, &opts("exhaust-stop", &d)).unwrap();
    assert_eq!(out.state, RunState::Failed);
    let _ = std::fs::remove_dir_all(d);
}

fn git(args: &[&str], cwd: &Path) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

#[test]
fn an_isolated_node_that_touches_a_forbidden_path_halts_the_run_and_publishes_nothing() {
    let (s, d) = store("forbidden");
    git(&["init", "-q"], &d);
    git(&["config", "user.email", "t@t.t"], &d);
    git(&["config", "user.name", "t"], &d);
    git(&["config", "commit.gpgsign", "false"], &d);
    std::fs::write(d.join(".gitignore"), "state/\nlogs/\n").unwrap();
    git(&["add", "-A"], &d);
    git(&["commit", "-qm", "seed"], &d);

    let mut c = cfg("");
    c.execution.graph.nodes[0].isolation = loopsmith_core::Isolation::Worktree;
    provider_script(&mut c, "echo leaked > .env; echo fine > out.txt; echo ok");
    c.safety.limits.global.forbidden_paths = vec![".env".into()];

    let out = execute(&c, &s, &opts("forbidden", &d)).unwrap();
    assert_eq!(out.state, RunState::Failed, "a safety violation is never negotiated");
    assert!(!d.join(".env").exists(), "the forbidden file must not reach the root");
    assert!(!d.join("out.txt").exists(), "nor anything else from that worktree");
    assert!(ledger_says(&s, "forbidden", LedgerKind::Recovered, "safety violation"));
    let _ = std::fs::remove_dir_all(d);
}

// --- join ------------------------------------------------------------------------

/// `build` plus two more independent builders, all in one wave.
fn three_wide(extra: &str) -> LoopConfig {
    let mut c = cfg(extra);
    let base = c.execution.graph.nodes[0].clone();
    for id in ["second", "third"] {
        c.execution.graph.nodes.push(loopsmith_core::NodeSpec {
            id: id.into(),
            ..base.clone()
        });
    }
    c.execution.graph.concurrency = loopsmith_core::Concurrency::Fixed { max_parallel: 1 };
    c
}

#[test]
fn first_success_releases_the_wave_without_dispatching_the_rest() {
    let (s, d) = store("first-success");
    let mut c = three_wide("");
    c.execution.graph.join = loopsmith_core::Join::FirstSuccess;
    let out = execute(&c, &s, &opts("first-success", &d)).unwrap();
    assert_eq!(out.state, RunState::Succeeded);
    assert_eq!(s.episodes("first-success").unwrap().len(), 1, "one success was enough");
    assert!(ledger_says(
        &s,
        "first-success",
        LedgerKind::NodeDispatched,
        "released by its first_success"
    ));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn an_unmet_quorum_holds_back_the_waves_after_it() {
    let (s, d) = store("quorum");
    let mut c = failing(cfg("stop_gates:\n  max_iterations: 1\n  no_progress_iterations: 0\n"));
    provider_script(&mut c, "exit 3");
    let base = c.execution.graph.nodes[0].clone();
    c.execution.graph.nodes.push(loopsmith_core::NodeSpec {
        id: "after".into(),
        depends_on: vec!["build".into()],
        ..base
    });
    c.execution.graph.join = loopsmith_core::Join::Quorum { count: 1 };
    let _ = execute(&c, &s, &opts("quorum", &d)).unwrap();
    assert!(ledger_says(&s, "quorum", LedgerKind::NodeFailed, "not dispatched this iteration"));
    let after_ran = s
        .ledger("quorum")
        .unwrap()
        .iter()
        .any(|e| e.node_id.as_deref() == Some("after"));
    assert!(!after_ran, "the downstream wave must not run on answers that are not there");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_budget_reached_mid_iteration_stops_further_dispatch() {
    let (s, d) = store("mid-budget");
    let c = failing(three_wide("stop_gates:\n  max_tokens: 1\n  max_iterations: 5\n"));
    let out = execute(&c, &s, &opts("mid-budget", &d)).unwrap();
    assert_eq!(out.stop, StopReason::TokenBudget(1));
    assert_eq!(s.episodes("mid-budget").unwrap().len(), 1, "one dispatch spent the budget");
    assert!(ledger_says(&s, "mid-budget", LedgerKind::StopGateTriggered, "mid-iteration"));
    let _ = std::fs::remove_dir_all(d);
}

// --- metrics, alerts, baseline, protected ----------------------------------------

#[test]
fn an_alert_fires_once_when_its_metric_crosses_the_line() {
    let (s, d) = store("alert");
    let mut c = failing(cfg("stop_gates:\n  max_iterations: 3\n  no_progress_iterations: 0\n"));
    c.safety.alerts = serde_yaml::from_str(
        "- id: chatty\n  metric: tokens_used\n  above: 1\n  message: spend is running hot\n",
    )
    .unwrap();
    let out = execute(&c, &s, &opts("alert", &d)).unwrap();
    assert_eq!(out.alerts.len(), 1, "an alert fires once per run, not once per iteration");
    assert_eq!(out.alerts[0].id, "chatty");
    assert_eq!(out.alerts[0].iteration, 1);
    assert!(ledger_says(&s, "alert", LedgerKind::AlertRaised, "spend is running hot"));
    assert_eq!(out.metrics.iterations, 3);
    assert_eq!(out.metrics.validation_pass_rate, Some(0.5));
    let _ = std::fs::remove_dir_all(d);
}

fn evolving(mut c: LoopConfig, baseline: &str) -> LoopConfig {
    c.features.self_evolution = true;
    c.evolution.enabled = true;
    c.evolution.baseline = Some(serde_yaml::from_str(baseline).unwrap());
    c
}

#[test]
fn a_run_that_misses_the_bar_regresses_a_baseline_that_always_completed() {
    let (s, d) = store("baseline-bad");
    let c = evolving(
        failing(cfg("stop_gates:\n  max_iterations: 1\n  no_progress_iterations: 0\n")),
        "completion_rate: 1.0\n",
    );
    let out = execute(&c, &s, &opts("baseline-bad", &d)).unwrap();
    assert!(
        matches!(&out.baseline, BaselineVerdict::Regressed(r) if r[0].starts_with("completion_rate")),
        "{:?}",
        out.baseline
    );
    assert!(ledger_says(&s, "baseline-bad", LedgerKind::GateEvaluated, "regressed"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_run_inside_the_baseline_holds_it() {
    let (s, d) = store("baseline-ok");
    let c = evolving(cfg(""), "completion_rate: 1.0\ncost_usd: 1.0\n");
    let out = execute(&c, &s, &opts("baseline-ok", &d)).unwrap();
    assert_eq!(out.baseline, BaselineVerdict::Held);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_proposal_that_would_touch_a_protected_path_is_never_written() {
    let (s, d) = store("protected");
    let mut c = failing(cfg("stop_gates:\n  max_iterations: 1\n  no_progress_iterations: 0\n"));
    // The desk suggests switching exploration on. Protect that switch, and the
    // gate must refuse to let the suggestion be recorded at all.
    c.execution.skills.explore_candidates = vec!["helper".into()];
    c.safety.protected.extra_paths = vec!["execution.skills.explore".into()];
    let out = execute(&c, &s, &opts("protected", &d)).unwrap();
    assert_eq!(out.proposals, 0);
    assert!(s.proposals("protected").unwrap().is_empty());
    assert!(ledger_says(&s, "protected", LedgerKind::GateEvaluated, "the gate refused a proposal"));
    let _ = std::fs::remove_dir_all(d);
}

// --- container isolation -----------------------------------------------------------

#[test]
fn a_container_node_without_an_image_degrades_to_a_worktree_and_still_runs() {
    // No image named anywhere, so there is nothing to run in whether or not
    // this machine has Docker. The run must go on, and say why.
    let (s, d) = store("container");
    let mut c = cfg("");
    c.execution.graph.nodes[0].isolation = loopsmith_core::Isolation::Container {
        image: None,
        network: false,
    };
    let out = execute(&c, &s, &opts("container", &d)).unwrap();
    assert_eq!(out.state, RunState::Succeeded);
    let said: Vec<_> = s
        .ledger("container")
        .unwrap()
        .into_iter()
        .filter(|e| e.detail.contains("asked for container isolation"))
        .collect();
    assert_eq!(said.len(), 1, "said once, not once per iteration");
    assert!(said[0].detail.contains("no image"), "{}", said[0].detail);
    let _ = std::fs::remove_dir_all(d);
}
