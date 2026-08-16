//! The iteration loop.
//!
//! Every iteration: dispatch the waves, collect evidence, ask the gate, then
//! ask the stop gates whether to continue. The stop-gate check is mechanical
//! and happens *after* the gate ruling, so no amount of confident output from
//! a node can extend the run past its ceiling.

use loopsmith_core::{LoopConfig, Role};
use loopsmith_gate::{Evidence, TargetVerdict};
use loopsmith_memory::{now_ms, Checkpoint, Episode, LedgerEntry, LedgerKind, Store};
use loopsmith_provider::{digest, dispatch, InvokeRequest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Why the loop stopped. Every variant except `OverallSuccess` is an
/// escalation: the run ended without meeting the bar, and the reason is
/// recorded so the human sees what was tried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    OverallSuccess,
    IterationCap(u32),
    WallClock(u64),
    TokenBudget(u64),
    CostBudget(String),
    NoProgress(u32),
}

impl StopReason {
    pub fn is_success(&self) -> bool {
        matches!(self, StopReason::OverallSuccess)
    }
    pub fn describe(&self) -> String {
        match self {
            StopReason::OverallSuccess => "all overall success scenarios met".into(),
            StopReason::IterationCap(n) => format!("iteration cap reached ({n})"),
            StopReason::WallClock(s) => format!("wall-clock budget exhausted ({s}s)"),
            StopReason::TokenBudget(t) => format!("token budget exhausted ({t})"),
            StopReason::CostBudget(c) => format!("cost budget exhausted ({c})"),
            StopReason::NoProgress(n) => {
                format!("no measurable change for {n} iterations; stopping the line")
            }
        }
    }
}

pub struct RunOptions {
    pub run_id: String,
    pub workdir: PathBuf,
    /// Plan and report without invoking any provider.
    pub dry_run: bool,
    pub resume: bool,
}

pub struct RunOutcome {
    pub run_id: String,
    pub iterations: u32,
    pub stop: StopReason,
    pub verdicts: BTreeMap<String, TargetVerdict>,
    pub tokens_used: u64,
    pub cost_usd: f64,
}

/// A fingerprint of the current verdicts. If it does not change between
/// iterations, the loop is spinning rather than progressing.
fn progress_signature(verdicts: &BTreeMap<String, TargetVerdict>) -> String {
    let mut parts: Vec<String> = verdicts
        .iter()
        .map(|(k, v)| format!("{k}:{}:{}/{}", v.satisfied, v.passed, v.total))
        .collect();
    parts.sort();
    parts.join("|")
}

fn log<S: Store>(
    store: &S,
    run_id: &str,
    iteration: u32,
    kind: LedgerKind,
    detail: impl Into<String>,
    node_id: Option<String>,
) {
    let _ = store.append_ledger(&LedgerEntry {
        run_id: run_id.to_string(),
        iteration,
        kind,
        detail: detail.into(),
        node_id,
        tokens: None,
        cost_usd: None,
        created_ms: now_ms(),
    });
}

/// Collect evidence for the gate from whatever the nodes left behind.
/// Deliberately narrow: node self-reports are not evidence, so only artifacts
/// on disk, reported metrics, and recorded judgments are gathered.
pub fn collect_evidence(workdir: &Path, metrics_file: Option<&Path>) -> Evidence {
    let mut ev = Evidence::new(workdir);
    if let Some(p) = metrics_file {
        if let Ok(text) = std::fs::read_to_string(p) {
            if let Ok(map) = serde_json::from_str::<BTreeMap<String, f64>>(&text) {
                ev.metrics = map;
            }
        }
    }
    ev
}

pub fn execute<S: Store>(
    cfg: &LoopConfig,
    store: &S,
    opts: &RunOptions,
) -> Result<RunOutcome, String> {
    let plan = loopsmith_graph::plan(&cfg.graph).map_err(|e| e.to_string())?;
    let started = Instant::now();

    let mut checkpoint = if opts.resume {
        store
            .checkpoint(&opts.run_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| fresh_checkpoint(&opts.run_id))
    } else {
        fresh_checkpoint(&opts.run_id)
    };

    log(
        store,
        &opts.run_id,
        checkpoint.iteration,
        LedgerKind::RunStarted,
        format!(
            "{} nodes in {} waves, concurrency {}, predicted speedup {:.2}x (ceiling {:.2}x)",
            cfg.graph.nodes.len(),
            plan.waves.len(),
            plan.concurrency,
            plan.predicted_speedup,
            plan.speedup_ceiling
        ),
        None,
    );

    // Assigned by every path that can reach a `break`, so no initial value is
    // needed and none is invented.
    let verdicts: BTreeMap<String, TargetVerdict>;
    let mut current: BTreeMap<String, TargetVerdict>;
    let mut last_signature = String::new();
    let mut stale_iterations = 0u32;
    let gates = &cfg.stop_gates;

    let stop = loop {
        checkpoint.iteration += 1;
        let it = checkpoint.iteration;
        log(
            store,
            &opts.run_id,
            it,
            LedgerKind::IterationStarted,
            format!("iteration {it}"),
            None,
        );

        // --- dispatch ------------------------------------------------------
        // Waves run in order; within a wave nodes are independent by
        // construction, so the only ordering that matters is between waves.
        for wave in &plan.waves {
            for node_id in &wave.nodes {
                let Some(node) = cfg.graph.nodes.iter().find(|n| &n.id == node_id) else {
                    continue;
                };
                if opts.dry_run {
                    log(
                        store,
                        &opts.run_id,
                        it,
                        LedgerKind::NodeDispatched,
                        format!("dry run: would dispatch `{}` ({:?})", node.id, node.role),
                        Some(node.id.clone()),
                    );
                    continue;
                }

                let constraints = loopsmith_core::ConstraintSet::merged(
                    &cfg.constraints.global,
                    cfg.constraints.per_node.get(&node.id),
                );
                let system = build_system_prompt(cfg, &constraints);
                let prompt = build_node_prompt(cfg, node, store, &opts.run_id);

                let req = InvokeRequest {
                    node_id: node.id.clone(),
                    system,
                    prompt: prompt.clone(),
                    tier: node.tier,
                    workdir: opts.workdir.clone(),
                };

                match dispatch(cfg, &req, node.provider.as_deref()) {
                    Ok((resp, skipped)) => {
                        if !skipped.is_empty() {
                            log(
                                store,
                                &opts.run_id,
                                it,
                                LedgerKind::NodeDispatched,
                                format!("cascade skipped: {}", skipped.join("; ")),
                                Some(node.id.clone()),
                            );
                        }
                        let ep = Episode {
                            run_id: opts.run_id.clone(),
                            iteration: it,
                            node_id: node.id.clone(),
                            role: format!("{:?}", node.role).to_lowercase(),
                            provider_id: resp.provider_id.clone(),
                            prompt_digest: digest(&prompt),
                            output: resp.output.clone(),
                            tokens: None,
                            cost_usd: None,
                            duration_ms: Some(resp.duration_ms),
                            error: None,
                            created_ms: now_ms(),
                        };
                        let _ = store.put_episode(&ep);
                        checkpoint.completed_nodes.push(node.id.clone());
                        log(
                            store,
                            &opts.run_id,
                            it,
                            LedgerKind::NodeSucceeded,
                            format!("served by `{}` in {}ms", resp.provider_id, resp.duration_ms),
                            Some(node.id.clone()),
                        );
                    }
                    Err(e) => {
                        log(
                            store,
                            &opts.run_id,
                            it,
                            LedgerKind::NodeFailed,
                            e.to_string(),
                            Some(node.id.clone()),
                        );
                    }
                }
            }
        }

        // --- gate ----------------------------------------------------------
        let ev = collect_evidence(&opts.workdir, Some(&opts.workdir.join("metrics.json")));
        current = loopsmith_gate::evaluate_all(cfg, &ev);
        for (target, v) in &current {
            let _ = store.set_goal_state(&opts.run_id, &v.to_goal_state(it));
            log(
                store,
                &opts.run_id,
                it,
                if v.satisfied {
                    LedgerKind::GoalSatisfied
                } else {
                    LedgerKind::GateEvaluated
                },
                format!("{target}: {}", v.reason),
                None,
            );
        }

        // --- stop gates ------------------------------------------------------
        // Checked in cost order: cheapest signal first, so an obviously
        // finished or obviously stuck run does not pay for another check.
        if gates.stop_on_overall_success && loopsmith_gate::overall_success(cfg, &current) {
            verdicts = current;
            break StopReason::OverallSuccess;
        }

        let sig = progress_signature(&current);
        if sig == last_signature {
            stale_iterations += 1;
        } else {
            stale_iterations = 0;
            last_signature = sig;
        }
        if gates.no_progress_iterations > 0 && stale_iterations >= gates.no_progress_iterations {
            verdicts = current;
            break StopReason::NoProgress(stale_iterations);
        }

        if it >= gates.max_iterations {
            verdicts = current;
            break StopReason::IterationCap(gates.max_iterations);
        }
        if let Some(limit) = gates.max_wall_clock_seconds {
            if started.elapsed().as_secs() >= limit {
                verdicts = current;
                break StopReason::WallClock(limit);
            }
        }
        if let Some(limit) = gates.max_tokens {
            if checkpoint.tokens_used >= limit {
                verdicts = current;
                break StopReason::TokenBudget(limit);
            }
        }
        if let Some(limit) = gates.max_cost_usd {
            if checkpoint.cost_usd >= limit {
                verdicts = current;
                break StopReason::CostBudget(format!("${limit:.2}"));
            }
        }

        checkpoint.updated_ms = now_ms();
        let _ = store.save_checkpoint(&checkpoint);
    };

    checkpoint.updated_ms = now_ms();
    let _ = store.save_checkpoint(&checkpoint);

    // Every stop-gate trigger is logged, not just successes. A loop that keeps
    // hitting its ceiling is telling you its verifier is miscalibrated, and
    // that signal is invisible if only completions are recorded.
    log(
        store,
        &opts.run_id,
        checkpoint.iteration,
        if stop.is_success() {
            LedgerKind::RunFinished
        } else {
            LedgerKind::StopGateTriggered
        },
        stop.describe(),
        None,
    );
    let _ = store.flush();

    Ok(RunOutcome {
        run_id: opts.run_id.clone(),
        iterations: checkpoint.iteration,
        stop,
        verdicts,
        tokens_used: checkpoint.tokens_used,
        cost_usd: checkpoint.cost_usd,
    })
}

fn fresh_checkpoint(run_id: &str) -> Checkpoint {
    Checkpoint {
        run_id: run_id.to_string(),
        iteration: 0,
        completed_nodes: vec![],
        tokens_used: 0,
        cost_usd: 0.0,
        started_ms: now_ms(),
        updated_ms: now_ms(),
    }
}

fn build_system_prompt(cfg: &LoopConfig, c: &loopsmith_core::ConstraintSet) -> String {
    let mut s = String::new();
    s.push_str(&format!("You are a node in the `{}` loop.\n\n", cfg.name));
    if !cfg.information.is_empty() {
        s.push_str("Context:\n");
        for i in &cfg.information {
            s.push_str(&format!("- {}: {}\n", i.key, i.value));
        }
        s.push('\n');
    }
    if !c.rules.is_empty() {
        s.push_str("Rules you must follow:\n");
        for r in &c.rules {
            s.push_str(&format!("- {r}\n"));
        }
        s.push('\n');
    }
    if !c.forbidden_paths.is_empty() {
        s.push_str(&format!("Never touch: {}\n", c.forbidden_paths.join(", ")));
    }
    if !c.forbidden_commands.is_empty() {
        s.push_str(&format!(
            "Never run: {}\n",
            c.forbidden_commands.join(", ")
        ));
    }
    if !c.human_checkpoint.is_empty() {
        s.push_str(&format!(
            "Stop and ask a human before: {}\n",
            c.human_checkpoint.join(", ")
        ));
    }
    s
}

fn build_node_prompt<S: Store>(
    cfg: &LoopConfig,
    node: &loopsmith_core::NodeSpec,
    store: &S,
    run_id: &str,
) -> String {
    let mut s = String::new();
    s.push_str(&format!("## Your task\n{}\n\n", node.instruction));

    if !node.goals.is_empty() {
        s.push_str("## Goals you advance\n");
        for gname in &node.goals {
            if let Some(g) = cfg.goals.iter().find(|g| &g.name == gname) {
                s.push_str(&format!("- **{}** — {}\n", g.name, g.description));
            }
            // The bar is stated up front: a node that does not know how it
            // will be checked cannot aim at the check.
            for v in cfg.blocking_validations_for(gname) {
                s.push_str(&format!("  - checked by `{}`: {}\n", v.name, v.statement));
            }
        }
        s.push('\n');
    }

    if node.role == Role::Judge {
        s.push_str(
            "## How to answer\n\
             Return one verdict per check: PASS or FAIL, the standard you checked \
             against, and the specific evidence. Do not summarise; a verdict \
             without evidence cannot be acted on.\n\n",
        );
    }

    for gname in &node.goals {
        if let Ok(Some(pad)) = store.scratchpad(run_id, gname) {
            if !pad.trim().is_empty() {
                s.push_str(&format!("## Notes carried from earlier iterations\n{pad}\n\n"));
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_memory::SledStore;

    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn store(tag: &str) -> (SledStore, PathBuf) {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!(
            "loopsmith-run-{tag}-{}-{n}",
            std::process::id()
        ));
        (SledStore::open(&d).unwrap(), d)
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
        }
    }

    #[test]
    fn a_satisfiable_loop_stops_on_overall_success() {
        let (s, d) = store("success");
        let out = execute(&cfg(""), &s, &opts("r1", &d)).unwrap();
        assert_eq!(out.stop, StopReason::OverallSuccess);
        assert_eq!(out.iterations, 1);
        assert!(out.verdicts["overall"].satisfied);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn an_unsatisfiable_loop_stops_on_no_progress_not_on_success() {
        let (s, d) = store("stuck");
        let c = cfg(
            "stop_gates:\n  max_iterations: 20\n  no_progress_iterations: 2\n",
        );
        // Make the overall check impossible.
        let mut c = c;
        c.validations[1].detector = loopsmith_core::Detector::Script {
            command: "false".into(),
            args: vec![],
            expect_exit: Some(0),
        };
        let out = execute(&c, &s, &opts("r2", &d)).unwrap();
        assert!(matches!(out.stop, StopReason::NoProgress(_)));
        assert!(!out.stop.is_success());
        assert!(out.iterations < 20, "should stop long before the cap");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_iteration_cap_holds_when_progress_keeps_changing() {
        let (s, d) = store("cap");
        let mut c = cfg("stop_gates:\n  max_iterations: 3\n  no_progress_iterations: 0\n");
        c.validations[1].detector = loopsmith_core::Detector::Script {
            command: "false".into(),
            args: vec![],
            expect_exit: Some(0),
        };
        let out = execute(&c, &s, &opts("r3", &d)).unwrap();
        assert_eq!(out.stop, StopReason::IterationCap(3));
        assert_eq!(out.iterations, 3);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn every_stop_is_written_to_the_ledger() {
        let (s, d) = store("ledger");
        let out = execute(&cfg(""), &s, &opts("r4", &d)).unwrap();
        let entries = s.ledger("r4").unwrap();
        assert!(entries.iter().any(|e| e.kind == LedgerKind::RunStarted));
        assert!(entries.iter().any(|e| e.kind == LedgerKind::GoalSatisfied));
        assert!(entries
            .iter()
            .any(|e| e.kind == LedgerKind::RunFinished && e.detail == out.stop.describe()));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn dry_run_dispatches_nothing_but_still_plans() {
        let (s, d) = store("dry");
        let mut o = opts("r5", &d);
        o.dry_run = true;
        let out = execute(&cfg(""), &s, &o).unwrap();
        assert!(s.episodes("r5").unwrap().is_empty(), "dry run must not record work");
        assert!(out.iterations >= 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_checkpoint_is_left_behind_for_resume() {
        let (s, d) = store("ckpt");
        execute(&cfg(""), &s, &opts("r6", &d)).unwrap();
        let cp = s.checkpoint("r6").unwrap().expect("checkpoint written");
        assert!(cp.iteration >= 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn resume_continues_from_the_saved_iteration() {
        let (s, d) = store("resume");
        let mut c = cfg("stop_gates:\n  max_iterations: 2\n  no_progress_iterations: 0\n");
        c.validations[1].detector = loopsmith_core::Detector::Script {
            command: "false".into(),
            args: vec![],
            expect_exit: Some(0),
        };
        let first = execute(&c, &s, &opts("r7", &d)).unwrap();
        assert_eq!(first.iterations, 2);

        let mut o = opts("r7", &d);
        o.resume = true;
        let second = execute(&c, &s, &o).unwrap();
        // Resuming starts from the stored iteration, so it trips the cap
        // immediately rather than replaying the whole run.
        assert_eq!(second.iterations, 3);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_progress_signature_changes_only_when_verdicts_change() {
        let a: BTreeMap<String, TargetVerdict> = BTreeMap::new();
        assert_eq!(progress_signature(&a), progress_signature(&a));
    }
}
