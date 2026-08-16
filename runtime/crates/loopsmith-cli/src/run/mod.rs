//! The iteration loop.
//!
//! Each iteration: acquire any missing sub-agents, dispatch the waves (in
//! parallel, in isolated worktrees where asked), collect evidence including
//! judge verdicts, ask the gate, record what each skill was worth, then ask
//! the stop gates whether to continue.
//!
//! The stop-gate check runs *after* the gate ruling and is mechanical, so no
//! amount of confident output from a node can extend a run past its ceiling.
//!
//! The work is split across four neighbours so this file stays the state
//! machine and nothing else:
//!
//! - [`dispatch`] — isolation, skill resolution, the provider call
//! - [`prompts`] — what a node is told
//! - [`evolve`] — trials, judgments, and proposals
//! - [`stop`] — the stop-gate ladder, as a pure function

use crate::logging::{Recorder, RunLog};
use loopsmith_core::{LoopConfig, NodeSpec, Role};
use loopsmith_gate::{Evidence, Judgment, TargetVerdict};
use loopsmith_memory::{now_ms, Checkpoint, Episode, LedgerKind, Store};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub mod dispatch;
pub mod evolve;
pub mod export;
pub mod perturb;
pub mod phases;
pub mod prompts;
pub mod publish;
pub mod stop;
pub mod summary;

pub use stop::StopReason;

use dispatch::{ensure_skills, run_node, NodeOutcome};
use phases::Phases;
use stop::{progress_signature, should_stop, StopInputs};

pub struct RunOptions {
    pub run_id: String,
    pub workdir: PathBuf,
    /// Plan and report without invoking any provider.
    pub dry_run: bool,
    pub resume: bool,
    /// Acquire missing sub-agents. Off means a node with an unresolved skill
    /// runs without it and says so.
    pub acquire_skills: bool,
    /// Mirror the run log to stderr as it is written.
    pub verbose: bool,
    /// Config file name, for the scripts written into a success export.
    pub config_file: String,
}

pub struct RunOutcome {
    pub run_id: String,
    pub iterations: u32,
    pub stop: StopReason,
    pub verdicts: BTreeMap<String, TargetVerdict>,
    pub tokens_used: u64,
    pub tokens_estimated: bool,
    pub cost_usd: f64,
    pub proposals: usize,
    /// Where the plain-text run log was written, when one could be opened.
    pub log_path: Option<PathBuf>,
    /// Where the reusable success package was written. Only ever `Some` when
    /// the gate certified overall success.
    pub export_path: Option<PathBuf>,
}

/// Collect evidence for the gate. Deliberately narrow: a node's own claim that
/// it finished is not evidence, so only artifacts on disk, reported metrics,
/// and parsed judge verdicts count.
pub fn collect_evidence(
    workdir: &Path,
    metrics_file: Option<&Path>,
    judgments: Vec<Judgment>,
) -> Evidence {
    let mut ev = Evidence::new(workdir);
    if let Some(p) = metrics_file {
        if let Ok(text) = std::fs::read_to_string(p) {
            if let Ok(map) = serde_json::from_str::<BTreeMap<String, f64>>(&text) {
                ev.metrics = map;
            }
        }
    }
    ev.judgments = judgments;
    ev
}

/// Blocking checks that failed, as `(target, check name, evidence)`.
fn failing_checks(
    verdicts: Option<&BTreeMap<String, TargetVerdict>>,
) -> Vec<(String, String, String)> {
    let Some(v) = verdicts else {
        return vec![];
    };
    v.values()
        .flat_map(|verdict| {
            verdict
                .checks
                .iter()
                .filter(|c| c.blocking && !c.passed)
                .map(|c| {
                    (
                        verdict.target.clone(),
                        c.name.clone(),
                        c.evidence.clone(),
                    )
                })
        })
        .collect()
}

/// Install section J's declared sub-agents. A failure is recorded and the run
/// continues: a loop whose optional helper could not be fetched is degraded,
/// not broken, and finding that out from the ledger beats finding it out from
/// a stack trace at 4am.
pub fn install_default_skills<S: Store>(cfg: &LoopConfig, root: &Path, rec: &Recorder<S>) {
    for spec in &cfg.default_skills {
        match loopsmith_skills::install_default(spec, &cfg.skills, root) {
            Ok(r) => rec.entry(
                0,
                LedgerKind::SkillAcquired,
                format!(
                    "default skill `{}` ready at {} (via {})",
                    r.name,
                    r.path.display(),
                    spec.source.as_str()
                ),
                None,
            ),
            Err(e) => rec.entry(
                0,
                LedgerKind::NodeFailed,
                format!("default skill `{}` unavailable: {e}", spec.name),
                None,
            ),
        }
    }
}

/// Last iteration's rulings, as the checkpoint carries them.
///
/// The checkpoint holds them as text because the gate crate depends on the
/// memory crate and not the other way round. Unreadable stored verdicts are
/// dropped rather than guessed at: a resumed run that reports no deltas on its
/// first iteration is a small loss, and one that reports invented deltas is
/// not.
fn restore_verdicts(cp: &Checkpoint) -> Option<BTreeMap<String, TargetVerdict>> {
    serde_json::from_str(cp.verdicts_json.as_deref()?).ok()
}

pub fn execute<S: Store>(
    cfg: &LoopConfig,
    store: &S,
    opts: &RunOptions,
) -> Result<RunOutcome, String> {
    let plan = loopsmith_graph::plan(&cfg.graph).map_err(|e| e.to_string())?;
    // Section I is resolved before anything is dispatched: an unschedulable
    // phase graph is a config bug, and finding it after the first provider call
    // means paying for the discovery.
    let mut phases = Phases::new(cfg)?;
    let started = Instant::now();
    let root = &opts.workdir;

    // Every event from here on goes to the ledger and the run log together, so
    // the queryable record and the readable one cannot disagree.
    let rec = Recorder::new(
        store,
        &opts.run_id,
        RunLog::open(root, &opts.run_id, opts.verbose),
    );
    let log_path = rec.log.path().map(|p| p.to_path_buf());

    let mut checkpoint = if opts.resume {
        store
            .checkpoint(&opts.run_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| Checkpoint::new(&opts.run_id))
    } else {
        Checkpoint::new(&opts.run_id)
    };

    rec.entry(
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

    // Section J: the sub-agents this loop declared it cannot start without.
    // Idempotent, so running it every time costs a directory check when they
    // are already there. Skipped on a dry run — installing software is not
    // something "plan and report without invoking a provider" should do.
    if opts.acquire_skills && !opts.dry_run {
        install_default_skills(cfg, root, &rec);
    }

    // The stop gates' accounting is restored from the checkpoint rather than
    // started from nothing, so a resumed run cannot be handed a fresh revision
    // budget and a no-progress counter of zero every time it pauses. On a
    // first run these are the fresh checkpoint's defaults, which is what they
    // were before.
    let mut last_signature = checkpoint.last_signature.clone();
    let mut stale_iterations = checkpoint.stale_iterations;
    let mut any_estimated = false;
    let mut proposals_written = 0usize;
    // How many times each node has been re-run with its goals still
    // unsatisfied. This is what `max_revisions_per_node` bounds: one stuck
    // node must not be allowed to spend the whole iteration budget.
    let mut revisions: BTreeMap<String, u32> = checkpoint.revisions.clone();
    // Last iteration's rulings, so the summary can report what *changed*
    // rather than only what is currently true.
    let mut previous_verdicts: Option<BTreeMap<String, TargetVerdict>> =
        restore_verdicts(&checkpoint);
    let gates = &cfg.stop_gates;
    let width = plan.concurrency.max(1);

    // The loop yields both the reason it stopped and the verdicts that were
    // current when it did. Carrying them out together is what removed the old
    // two-variable dance, where every one of six break sites had to remember
    // to copy `current` into an outer `verdicts` first.
    let (stop, verdicts) = loop {
        checkpoint.iteration += 1;
        let it = checkpoint.iteration;
        rec.entry(
            it,
            LedgerKind::IterationStarted,
            format!("iteration {it}"),
            None,
        );

        // Scratchpad notes are read once per iteration and shared with every
        // node, so a thread never touches the store mid-dispatch.
        let mut scratch: BTreeMap<String, String> = BTreeMap::new();
        for g in &cfg.goals {
            if let Ok(Some(pad)) = store.scratchpad(&opts.run_id, &g.name) {
                if !pad.trim().is_empty() {
                    scratch.insert(g.name.clone(), pad);
                }
            }
        }

        // Compressed history from earlier iterations. Read once and shared, so
        // every node in the iteration sees the same account of what happened.
        let carried = summary::carry_forward(
            cfg,
            &store.summaries(&opts.run_id).unwrap_or_default(),
        );

        let mut episodes_this_iteration: Vec<evolve::RanNode> = Vec::new();
        // Nodes that spent their last revision this iteration, so the graph
        // can be questioned rather than the node re-run forever.
        let mut exhausted_nodes: Vec<String> = Vec::new();
        // Every dispatch including failures, for the summary.
        let mut dispatch_log: Vec<(String, String, Role, bool)> = Vec::new();
        let mut outputs_this_iteration: Vec<(String, String)> = Vec::new();
        let mut node_skills: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        // Which node published each path this iteration, so two isolated
        // builders writing the same file is reported rather than resolved by
        // whichever thread happened to finish last.
        let mut claimed_paths: BTreeMap<String, String> = BTreeMap::new();
        let mut explore_now =
            evolve::next_candidate(cfg, &store.skill_trials().unwrap_or_default());

        // --- stalled? try something different before giving up --------------
        let seed = perturb::seed_for(&opts.run_id, it);
        let perturbation = match gates.no_progress_iterations_randomness {
            Some(threshold) if stale_iterations >= threshold => {
                let recent = store.summaries(&opts.run_id).unwrap_or_default();
                let tail = recent.split_at(recent.len().saturating_sub(2)).1.to_vec();
                let failing = failing_checks(previous_verdicts.as_ref());
                let (chosen, by_agent) = perturb::choose(
                    cfg,
                    root,
                    &perturb::Stall {
                        stale_iterations,
                        failing: &failing,
                        recent: &tail,
                    },
                    seed,
                );
                rec.entry(
                    it,
                    LedgerKind::NodeDispatched,
                    format!(
                        "no change for {stale_iterations} iteration(s); seed {seed:016x}; \
                         {} chose {}",
                        if by_agent {
                            "the randomness agent"
                        } else {
                            "the seeded fallback"
                        },
                        chosen.describe()
                    ),
                    None,
                );
                Some(chosen)
            }
            _ => None,
        };

        // `explore` normally requires opting in. A stall is the one case where
        // trying an untried sub-agent is worth the money without being asked.
        if matches!(&perturbation, Some(perturb::Perturbation::Explore)) && explore_now.is_none() {
            explore_now = cfg.skills.explore_candidates.first().cloned();
            if explore_now.is_none() {
                rec.entry(
                    it,
                    LedgerKind::NodeDispatched,
                    "wanted to explore, but `skills.explore_candidates` is empty",
                    None,
                );
            }
        }

        for wave in &plan.waves {
            // Nodes inside a wave are independent by construction, so the only
            // ordering that matters is between waves. Chunking by the chosen
            // concurrency keeps the fleet at the size `plan` justified.
            let mut wave_nodes = wave.nodes.clone();
            if matches!(&perturbation, Some(perturb::Perturbation::Reorder)) {
                perturb::shuffle(&mut wave_nodes, seed);
            }
            for chunk in wave_nodes.chunks(width) {
                let mut nodes: Vec<&NodeSpec> = Vec::new();
                for id in chunk {
                    let Some(n) = cfg.graph.nodes.iter().find(|n| &n.id == id) else {
                        continue;
                    };
                    if !phases.eligible(n) {
                        // Silently skipped rather than logged every iteration:
                        // a node waiting on its phase is the normal state of
                        // affairs, and one line per node per iteration would
                        // bury the events that matter.
                        continue;
                    }
                    let spent = revisions.get(&n.id).copied().unwrap_or(0);
                    if spent >= gates.max_revisions_per_node {
                        rec.entry(
                            it,
                            LedgerKind::NodeDispatched,
                            format!(
                                "`{}` has been revised {spent} times without satisfying its goals; \
                                 revision ceiling is {}, so it is not dispatched again",
                                n.id, gates.max_revisions_per_node
                            ),
                            Some(n.id.clone()),
                        );
                        continue;
                    }
                    nodes.push(n);
                }
                if nodes.is_empty() {
                    continue;
                }

                if opts.dry_run {
                    for n in &nodes {
                        rec.entry(
                            it,
                            LedgerKind::NodeDispatched,
                            format!("dry run: would dispatch `{}` ({:?})", n.id, n.role),
                            Some(n.id.clone()),
                        );
                    }
                    continue;
                }

                // Acquisition touches the store, so it happens before the
                // threads start.
                for n in &nodes {
                    let mut resolved =
                        ensure_skills(cfg, n, root, &rec, it, opts.acquire_skills);
                    // Attach the exploration candidate to the first builder in
                    // the wave; judges and adversaries keep a fixed toolset so
                    // the check itself does not drift while the work does.
                    if n.role == Role::Builder {
                        if let Some(cand) = explore_now.take() {
                            match loopsmith_skills::acquire(
                                &cand,
                                &n.instruction,
                                &cfg.skills,
                                root,
                            ) {
                                Ok(r) => {
                                    rec.entry(
                                        it,
                                        LedgerKind::SkillAcquired,
                                        format!("exploring `{}` on `{}`", r.name, n.id),
                                        Some(n.id.clone()),
                                    );
                                    resolved.push((r.name, r.source.as_str().to_string()));
                                }
                                Err(e) => rec.entry(
                                    it,
                                    LedgerKind::NodeFailed,
                                    format!("could not explore `{cand}`: {e}"),
                                    Some(n.id.clone()),
                                ),
                            }
                        }
                    }
                    node_skills.insert(n.id.clone(), resolved);
                }

                let outcomes: Vec<NodeOutcome> = std::thread::scope(|s| {
                    let handles: Vec<_> = nodes
                        .iter()
                        .map(|n| {
                            let skills = node_skills.get(&n.id).cloned().unwrap_or_default();
                            let guideline = phases.guideline_for(n).map(str::to_string);
                            let scratch = &scratch;
                            let carried = carried.as_str();
                            // Borrowed out here: taking the reference inside the
                            // `move` closure would capture the Option itself.
                            let nudge = perturbation.as_ref();
                            s.spawn(move || {
                                run_node(
                                    cfg,
                                    n,
                                    root,
                                    &opts.run_id,
                                    &dispatch::NodeContext {
                                        scratch,
                                        skills: &skills,
                                        guideline: guideline.as_deref(),
                                        carried,
                                        perturbation: nudge,
                                    },
                                )
                            })
                        })
                        .collect();
                    handles.into_iter().filter_map(|h| h.join().ok()).collect()
                });

                // Writes happen after the join so the ledger stays ordered.
                for o in outcomes {
                    dispatch_log.push((
                        o.node_id.clone(),
                        o.provider_id.clone(),
                        o.role,
                        o.error.is_none(),
                    ));
                    if let Some(err) = &o.error {
                        rec.entry(
                            it,
                            LedgerKind::NodeFailed,
                            err.clone(),
                            Some(o.node_id.clone()),
                        );
                        continue;
                    }
                    if !o.skipped.is_empty() {
                        rec.entry(
                            it,
                            LedgerKind::NodeDispatched,
                            format!("cascade skipped: {}", o.skipped.join("; ")),
                            Some(o.node_id.clone()),
                        );
                    }
                    if o.tokens_estimated {
                        any_estimated = true;
                    }
                    checkpoint.tokens_used += o.tokens.unwrap_or(0);
                    checkpoint.cost_usd += o.cost_usd.unwrap_or(0.0);

                    let node_goals: Vec<String> = cfg
                        .graph
                        .nodes
                        .iter()
                        .find(|n| n.id == o.node_id)
                        .map(|n| n.goals.clone())
                        .unwrap_or_default();

                    let _ = store.put_episode(&Episode {
                        run_id: opts.run_id.clone(),
                        iteration: it,
                        node_id: o.node_id.clone(),
                        role: format!("{:?}", o.role).to_lowercase(),
                        provider_id: o.provider_id.clone(),
                        prompt_digest: o.prompt_digest.clone(),
                        output: o.output.clone(),
                        tokens: o.tokens,
                        cost_usd: o.cost_usd,
                        duration_ms: Some(o.duration_ms),
                        error: None,
                        created_ms: now_ms(),
                    });
                    checkpoint.completed_nodes.push(o.node_id.clone());
                    rec.entry(
                        it,
                        LedgerKind::NodeSucceeded,
                        format!(
                            "served by `{}` in {}ms, {} tokens{}; {}",
                            o.provider_id,
                            o.duration_ms,
                            o.tokens.unwrap_or(0),
                            if o.tokens_estimated { " (est)" } else { "" },
                            o.isolation.describe()
                        ),
                        Some(o.node_id.clone()),
                    );

                    // Isolation is a property of the wave, not of the run. The
                    // node wrote in its own worktree so its neighbours could
                    // not tread on it; now the wave has joined, what it
                    // produced is published into the loop root, because the
                    // gate collects evidence there and nowhere else.
                    let published =
                        publish::publish(root, &o.node_id, &o.isolation, &mut claimed_paths);
                    if let Some(line) = published.describe(&o.node_id) {
                        rec.entry(
                            it,
                            if published.conflicts.is_empty() {
                                LedgerKind::NodeSucceeded
                            } else {
                                LedgerKind::NodeFailed
                            },
                            line,
                            Some(o.node_id.clone()),
                        );
                    }
                    outputs_this_iteration.push((o.node_id.clone(), o.output.clone()));
                    episodes_this_iteration.push(evolve::RanNode {
                        node_id: o.node_id.clone(),
                        goals: node_goals,
                        tokens: o.tokens,
                    });
                }
            }
        }

        // --- judgments ------------------------------------------------------
        // A judge's verdict is only worth reading once we know which provider
        // produced the work it judged; that comes from the episode record, not
        // from the judge's own claim.
        let judgments = evolve::harvest_judgments(cfg, &rec, it);
        if !judgments.is_empty() {
            rec.entry(
                it,
                LedgerKind::GateEvaluated,
                format!("{} judge verdict(s) parsed", judgments.len()),
                None,
            );
        }

        // --- gate ----------------------------------------------------------
        let ev = collect_evidence(root, Some(&root.join("metrics.json")), judgments);
        let current = loopsmith_gate::evaluate_all(cfg, &ev);
        for (target, v) in &current {
            let _ = store.set_goal_state(&opts.run_id, &v.to_goal_state(it));
            rec.entry(
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

        // A phase closes only on the gate's ruling, never on a node's report.
        let dispatched: std::collections::BTreeSet<String> =
            checkpoint.completed_nodes.iter().cloned().collect();
        let phases_closed = phases.refresh(&current, &dispatched);
        for closed in &phases_closed {
            rec.entry(
                it,
                LedgerKind::GateEvaluated,
                format!("phase `{closed}` is complete; the phases behind it are now open"),
                None,
            );
        }

        // --- compress this iteration --------------------------------------
        // Written after the gate so the summary quotes rulings rather than
        // predictions, and stored so the next iteration reads this instead of
        // every episode that produced it.
        let mut digest = summary::deterministic(&summary::IterationFacts {
            run_id: &opts.run_id,
            iteration: it,
            dispatched: &dispatch_log,
            verdicts: &current,
            previous: previous_verdicts.as_ref(),
            tokens: checkpoint.tokens_used,
            cost_usd: checkpoint.cost_usd,
            phases_closed: &phases_closed,
        });
        summary::add_narrative(cfg, root, &mut digest, &outputs_this_iteration);
        let _ = store.put_summary(&digest);
        rec.entry(it, LedgerKind::GateEvaluated, digest.headline.clone(), None);
        previous_verdicts = Some(current.clone());

        // A node that ran and left its goals unsatisfied has spent a revision.
        // Nodes with no declared goals are never counted: there is nothing to
        // measure them against, so capping them would be arbitrary.
        for ep in &episodes_this_iteration {
            if ep.goals.is_empty() {
                continue;
            }
            let unsatisfied = ep
                .goals
                .iter()
                .any(|g| current.get(g).map(|v| !v.satisfied).unwrap_or(true));
            if unsatisfied {
                let spent = revisions.entry(ep.node_id.clone()).or_insert(0);
                *spent += 1;
                if *spent >= gates.max_revisions_per_node {
                    exhausted_nodes.push(ep.node_id.clone());
                }
            }
        }

        // --- what was each skill worth? ------------------------------------
        evolve::record_trials(
            &rec,
            it,
            &episodes_this_iteration,
            &node_skills,
            &current,
        );
        proposals_written += evolve::write_proposals(
            cfg,
            &rec,
            it,
            &evolve::Observed {
                exhausted_nodes: &exhausted_nodes,
                verdicts: &current,
            },
        );

        // --- stop gates ------------------------------------------------------
        let sig = progress_signature(&current);
        if sig == last_signature {
            stale_iterations += 1;
        } else {
            stale_iterations = 0;
            last_signature = sig;
        }

        let decision = should_stop(&StopInputs {
            cfg,
            gates,
            verdicts: &current,
            iteration: it,
            stale_iterations,
            elapsed_seconds: started.elapsed().as_secs(),
            tokens_used: checkpoint.tokens_used,
            cost_usd: checkpoint.cost_usd,
        });
        if let Some(reason) = decision {
            break (reason, current);
        }

        checkpoint.revisions = revisions.clone();
        checkpoint.stale_iterations = stale_iterations;
        checkpoint.last_signature = last_signature.clone();
        checkpoint.verdicts_json = serde_json::to_string(&current).ok();
        checkpoint.updated_ms = now_ms();
        let _ = store.save_checkpoint(&checkpoint);
    };

    checkpoint.revisions = revisions;
    checkpoint.stale_iterations = stale_iterations;
    checkpoint.last_signature = last_signature;
    checkpoint.verdicts_json = serde_json::to_string(&verdicts).ok();
    checkpoint.updated_ms = now_ms();
    let _ = store.save_checkpoint(&checkpoint);

    rec.entry(
        checkpoint.iteration,
        if stop.is_success() {
            LedgerKind::RunFinished
        } else {
            LedgerKind::StopGateTriggered
        },
        stop.describe(),
        None,
    );
    // The export is gated on the gate. `stop.is_success()` is true only for
    // `StopReason::OverallSuccess`, which only `should_stop` produces, and only
    // from `loopsmith_gate::overall_success`.
    let export_path = if stop.is_success() {
        match export::export_success(
            cfg,
            root,
            &verdicts,
            &store.summaries(&opts.run_id).unwrap_or_default(),
            checkpoint.iteration,
            &opts.config_file,
        ) {
            Ok(p) => {
                rec.entry(
                    checkpoint.iteration,
                    LedgerKind::RunFinished,
                    format!("reusable success package written to {}", p.display()),
                    None,
                );
                Some(p)
            }
            Err(e) => {
                rec.entry(
                    checkpoint.iteration,
                    LedgerKind::NodeFailed,
                    format!("could not write the success package: {e}"),
                    None,
                );
                None
            }
        }
    } else {
        None
    };

    let _ = store.flush();

    Ok(RunOutcome {
        run_id: opts.run_id.clone(),
        iterations: checkpoint.iteration,
        stop,
        verdicts,
        tokens_used: checkpoint.tokens_used,
        tokens_estimated: any_estimated,
        cost_usd: checkpoint.cost_usd,
        proposals: proposals_written,
        log_path,
        export_path,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_memory::SledStore;

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
        c.validations[1].detector = loopsmith_core::Detector::Script {
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
        c.providers.providers[0].cost_per_1k_tokens = Some(1000.0);
        c.validations[1].detector = loopsmith_core::Detector::Script {
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

    #[test]
    fn independent_nodes_in_a_wave_run_concurrently() {
        let (s, d) = store("parallel");
        // Three sleepers in one wave. Run serially that is ~1.5s; with the
        // chosen concurrency it should be well under that.
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
    - id: sleeper
      kind: byok
      command: sleep
      args: ["0.5"]
  cascade:
    standard: [sleeper]
"#,
            "test",
        )
        .unwrap();
        let t0 = Instant::now();
        let out = execute(&c, &s, &opts("r6", &d)).unwrap();
        let elapsed = t0.elapsed();
        assert!(out.stop.is_success());
        assert!(
            elapsed < std::time::Duration::from_millis(1200),
            "three 0.5s nodes took {elapsed:?}; they did not run in parallel"
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
        c.graph.nodes[0].skills = vec!["helper".into()];
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
        c.validations[1].detector = loopsmith_core::Detector::Script {
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
        c.validations[0].detector = loopsmith_core::Detector::Script {
            command: "false".into(),
            args: vec![],
            expect_exit: Some(0),
        };
        c.validations[1].detector = loopsmith_core::Detector::Script {
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
        c.validations[1].detector = loopsmith_core::Detector::Script {
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
        c.validations[1].detector = loopsmith_core::Detector::Script {
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
        c.validations[1].detector = loopsmith_core::Detector::Script {
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
                "{}/src/run/{file}",
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
        c.validations[1].detector = loopsmith_core::Detector::Script {
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
        c.validations[1].detector = loopsmith_core::Detector::Script {
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
}
