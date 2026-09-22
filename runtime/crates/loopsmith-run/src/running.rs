//! `Running`: the iteration loop.
//!
//! Each iteration: dispatch the waves (in parallel, in isolated worktrees
//! where asked), collect evidence including judge verdicts, ask the gate,
//! compress what happened, record what each skill was worth, then ask the stop
//! gates whether to continue.
//!
//! The stop-gate check runs *after* the gate ruling and is mechanical, so no
//! amount of confident output from a node can extend a run past its ceiling.

use crate::context::Run;
use crate::dispatch::{self, ensure_skills, run_node, NodeOutcome};
use crate::evidence::{collect_evidence, failing_checks};
use crate::planning::Planned;
use crate::stop::{progress_signature, should_stop, StopInputs, StopReason};
use crate::{evolve, perturb, publish, summary};
use loopsmith_core::{NodeSpec, Role};
use loopsmith_gate::TargetVerdict;
use loopsmith_memory::{now_ms, Checkpoint, Episode, LedgerKind, Store};
use std::collections::{BTreeMap, BTreeSet};

/// The accounting that outlives an iteration.
///
/// The stop gates' counters are restored from the checkpoint rather than
/// started from nothing, so a resumed run cannot be handed a fresh revision
/// budget and a no-progress counter of zero every time it pauses. On a first
/// run these are the fresh checkpoint's defaults.
pub(crate) struct Progress {
    pub last_signature: String,
    pub stale_iterations: u32,
    pub any_estimated: bool,
    pub proposals_written: usize,
    /// How many times each node has been re-run with its goals still
    /// unsatisfied. This is what `max_revisions_per_node` bounds: one stuck
    /// node must not be allowed to spend the whole iteration budget.
    pub revisions: BTreeMap<String, u32>,
    /// Last iteration's rulings, so the summary can report what *changed*
    /// rather than only what is currently true.
    pub previous_verdicts: Option<BTreeMap<String, TargetVerdict>>,
    /// Every path any isolated node has published this run, and who
    /// published it. Carried across iterations so a worktree created later —
    /// or reused from an earlier one — can be seeded with what its upstream
    /// produced.
    pub published_paths: BTreeMap<String, String>,
}

impl Progress {
    pub fn from_checkpoint(cp: &Checkpoint) -> Self {
        Progress {
            last_signature: cp.last_signature.clone(),
            stale_iterations: cp.stale_iterations,
            any_estimated: false,
            proposals_written: 0,
            revisions: cp.revisions.clone(),
            previous_verdicts: restore_verdicts(cp),
            published_paths: BTreeMap::new(),
        }
    }

    /// Write the counters back, with the rulings that were current.
    pub fn store_into(&self, cp: &mut Checkpoint, verdicts: &BTreeMap<String, TargetVerdict>) {
        cp.revisions = self.revisions.clone();
        cp.stale_iterations = self.stale_iterations;
        cp.last_signature = self.last_signature.clone();
        cp.verdicts_json = serde_json::to_string(verdicts).ok();
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

/// Why the loop ended, and the rulings that were current when it did.
///
/// Carrying them out together is what removed the old two-variable dance,
/// where every break site had to remember to copy the current rulings into an
/// outer variable first.
pub(crate) struct Stopped {
    pub reason: StopReason,
    pub verdicts: BTreeMap<String, TargetVerdict>,
}

/// What every node in one iteration is shown, gathered once.
struct Inputs {
    /// Per-goal scratchpad notes. Read once per iteration and shared, so a
    /// thread never touches the store mid-dispatch.
    scratch: BTreeMap<String, String>,
    /// Compressed history from earlier iterations, the same for every node.
    carried: String,
    /// The untried sub-agent to attach to this iteration's first builder.
    explore_now: Option<String>,
    perturbation: Option<perturb::Perturbation>,
    seed: u64,
}

/// What one iteration dispatched, before the gate ruled on it.
#[derive(Default)]
struct Dispatched {
    episodes: Vec<evolve::RanNode>,
    /// Every dispatch including failures, for the summary.
    log: Vec<(String, String, Role, bool)>,
    outputs: Vec<(String, String)>,
    node_skills: BTreeMap<String, Vec<(String, String)>>,
    /// Which node published each path *this iteration*, so two isolated
    /// builders writing the same file is reported rather than resolved by
    /// whichever thread happened to finish last. Deliberately narrower than
    /// `Progress::published_paths`: a node rewriting its own output next
    /// iteration is the normal case and is not a collision.
    claimed_paths: BTreeMap<String, String>,
}

pub(crate) fn iterate<S: Store>(
    run: &mut Run<S>,
    planned: &mut Planned,
    progress: &mut Progress,
) -> Stopped {
    loop {
        run.checkpoint.iteration += 1;
        let it = run.checkpoint.iteration;
        run.rec
            .entry(it, LedgerKind::IterationStarted, format!("iteration {it}"), None);

        let mut inputs = prepare(run, progress, it);
        let dispatched = dispatch_waves(run, planned, progress, &mut inputs, it);
        let (current, phases_closed) = rule(run, planned, it);

        compress(run, progress, &dispatched, &current, &phases_closed, it);
        let exhausted = spend_revisions(run, progress, &dispatched, &current);

        // --- what was each skill worth? ------------------------------------
        evolve::record_trials(
            &run.rec,
            it,
            &dispatched.episodes,
            &dispatched.node_skills,
            &current,
        );
        progress.proposals_written += evolve::write_proposals(
            run.cfg,
            &run.rec,
            it,
            &evolve::Observed {
                exhausted_nodes: &exhausted,
                verdicts: &current,
            },
        );

        if let Some(reason) = decide(run, progress, &current, it) {
            return Stopped {
                reason,
                verdicts: current,
            };
        }

        progress.store_into(&mut run.checkpoint, &current);
        run.save();
    }
}

/// Gather what every node in this iteration is shown, and decide whether a
/// stall calls for doing something different.
fn prepare<S: Store>(run: &Run<S>, progress: &Progress, it: u32) -> Inputs {
    let cfg = run.cfg;
    let run_id = run.opts.run_id.as_str();

    let mut scratch: BTreeMap<String, String> = BTreeMap::new();
    for g in &cfg.intent.goals {
        if let Ok(Some(pad)) = run.store.scratchpad(run_id, &g.name) {
            if !pad.trim().is_empty() {
                scratch.insert(g.name.clone(), pad);
            }
        }
    }

    let carried = summary::carry_forward(cfg, &run.store.summaries(run_id).unwrap_or_default());
    let mut explore_now =
        evolve::next_candidate(cfg, &run.store.skill_trials().unwrap_or_default());

    // --- stalled? try something different before giving up ----------------
    let seed = perturb::seed_for(run_id, it);
    let stale = progress.stale_iterations;
    let perturbation = match cfg.safety.gates.stop.no_progress_iterations_randomness {
        Some(threshold) if stale >= threshold => {
            let recent = run.store.summaries(run_id).unwrap_or_default();
            let tail = recent.split_at(recent.len().saturating_sub(2)).1.to_vec();
            let failing = failing_checks(progress.previous_verdicts.as_ref());
            let (chosen, by_agent) = perturb::choose(
                cfg,
                run.root(),
                &perturb::Stall {
                    stale_iterations: stale,
                    failing: &failing,
                    recent: &tail,
                },
                seed,
            );
            run.rec.entry(
                it,
                LedgerKind::NodeDispatched,
                format!(
                    "no change for {stale} iteration(s); seed {seed:016x}; {} chose {}",
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
        explore_now = cfg.execution.skills.explore_candidates.first().cloned();
        if explore_now.is_none() {
            run.rec.entry(
                it,
                LedgerKind::NodeDispatched,
                "wanted to explore, but `skills.explore_candidates` is empty",
                None,
            );
        }
    }

    Inputs {
        scratch,
        carried,
        explore_now,
        perturbation,
        seed,
    }
}

/// Dispatch every wave, in order, and record what came back.
fn dispatch_waves<S: Store>(
    run: &mut Run<S>,
    planned: &Planned,
    progress: &mut Progress,
    inputs: &mut Inputs,
    it: u32,
) -> Dispatched {
    let mut out = Dispatched::default();
    let width = planned.graph.concurrency.max(1);

    for wave in &planned.graph.waves {
        // Nodes inside a wave are independent by construction, so the only
        // ordering that matters is between waves. Chunking by the chosen
        // concurrency keeps the fleet at the size `plan` justified.
        let mut wave_nodes = wave.nodes.clone();
        if matches!(&inputs.perturbation, Some(perturb::Perturbation::Reorder)) {
            perturb::shuffle(&mut wave_nodes, inputs.seed);
        }
        for chunk in wave_nodes.chunks(width) {
            let nodes = eligible_nodes(run, planned, progress, chunk, it);
            if nodes.is_empty() {
                continue;
            }

            if run.opts.dry_run {
                for n in &nodes {
                    run.rec.entry(
                        it,
                        LedgerKind::NodeDispatched,
                        format!("dry run: would dispatch `{}` ({:?})", n.id, n.role),
                        Some(n.id.clone()),
                    );
                }
                continue;
            }

            // Acquisition touches the store, so it happens before the threads
            // start.
            resolve_skills(run, &nodes, inputs, &mut out, it);

            let outcomes = run_chunk(run, planned, progress, inputs, &out, &nodes);

            // Writes happen after the join so the ledger stays ordered.
            for o in outcomes {
                record_outcome(run, progress, &mut out, o, it);
            }
        }
    }
    out
}

/// The nodes in `chunk` that should run this iteration.
fn eligible_nodes<'c, S: Store>(
    run: &Run<'c, S>,
    planned: &Planned,
    progress: &Progress,
    chunk: &[String],
    it: u32,
) -> Vec<&'c NodeSpec> {
    let cfg = run.cfg;
    let ceiling = cfg.safety.gates.stop.max_revisions_per_node;
    let mut nodes = Vec::new();
    for id in chunk {
        let Some(n) = cfg.execution.graph.nodes.iter().find(|n| &n.id == id) else {
            continue;
        };
        if !planned.phases.eligible(n) {
            // Silently skipped rather than logged every iteration: a node
            // waiting on its phase is the normal state of affairs, and one line
            // per node per iteration would bury the events that matter.
            continue;
        }
        let spent = progress.revisions.get(&n.id).copied().unwrap_or(0);
        if spent >= ceiling {
            run.rec.entry(
                it,
                LedgerKind::NodeDispatched,
                format!(
                    "`{}` has been revised {spent} times without satisfying its goals; \
                     revision ceiling is {ceiling}, so it is not dispatched again",
                    n.id
                ),
                Some(n.id.clone()),
            );
            continue;
        }
        nodes.push(n);
    }
    nodes
}

/// Resolve each node's sub-agents, and attach the exploration candidate to the
/// first builder in the chunk. Judges and adversaries keep a fixed toolset so
/// the check itself does not drift while the work does.
fn resolve_skills<S: Store>(
    run: &Run<S>,
    nodes: &[&NodeSpec],
    inputs: &mut Inputs,
    out: &mut Dispatched,
    it: u32,
) {
    let cfg = run.cfg;
    let root = run.root();
    for n in nodes {
        let mut resolved = ensure_skills(cfg, n, root, &run.rec, it, run.opts.acquire_skills);
        if n.role == Role::Builder {
            if let Some(cand) = inputs.explore_now.take() {
                match loopsmith_skills::acquire(&cand, &n.instruction, &cfg.execution.skills, root)
                {
                    Ok(r) => {
                        run.rec.entry(
                            it,
                            LedgerKind::SkillAcquired,
                            format!("exploring `{}` on `{}`", r.name, n.id),
                            Some(n.id.clone()),
                        );
                        resolved.push((r.name, r.source.as_str().to_string()));
                    }
                    Err(e) => run.rec.entry(
                        it,
                        LedgerKind::NodeFailed,
                        format!("could not explore `{cand}`: {e}"),
                        Some(n.id.clone()),
                    ),
                }
            }
        }
        out.node_skills.insert(n.id.clone(), resolved);
    }
}

/// Run one chunk of nodes concurrently and collect their outcomes.
fn run_chunk<S: Store>(
    run: &Run<S>,
    planned: &Planned,
    progress: &Progress,
    inputs: &Inputs,
    out: &Dispatched,
    nodes: &[&NodeSpec],
) -> Vec<NodeOutcome> {
    let cfg = run.cfg;
    let root = run.root();
    let run_id = run.opts.run_id.as_str();
    // Snapshotted per chunk rather than borrowed: the map is written to as each
    // outcome is published, and the threads are still reading it. Every node
    // in one chunk therefore sees the same published set, which is also the
    // honest answer — they ran at the same time.
    let published_now = progress.published_paths.clone();
    std::thread::scope(|s| {
        let handles: Vec<_> = nodes
            .iter()
            .map(|n| {
                let skills = out.node_skills.get(&n.id).cloned().unwrap_or_default();
                let guideline = planned.phases.guideline_for(n).map(str::to_string);
                let scratch = &inputs.scratch;
                let carried = inputs.carried.as_str();
                // Borrowed out here: taking the reference inside the `move`
                // closure would capture the Option itself.
                let nudge = inputs.perturbation.as_ref();
                let published = &published_now;
                s.spawn(move || {
                    run_node(
                        cfg,
                        n,
                        root,
                        run_id,
                        &dispatch::NodeContext {
                            scratch,
                            skills: &skills,
                            guideline: guideline.as_deref(),
                            carried,
                            perturbation: nudge,
                            published,
                        },
                    )
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

/// Write down one node's outcome: ledger, episode, spend, and publication.
fn record_outcome<S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    out: &mut Dispatched,
    o: NodeOutcome,
    it: u32,
) {
    let cfg = run.cfg;
    out.log
        .push((o.node_id.clone(), o.provider_id.clone(), o.role, o.error.is_none()));
    if let Some(err) = &o.error {
        run.rec
            .entry(it, LedgerKind::NodeFailed, err.clone(), Some(o.node_id.clone()));
        return;
    }
    if !o.skipped.is_empty() {
        run.rec.entry(
            it,
            LedgerKind::NodeDispatched,
            format!("cascade skipped: {}", o.skipped.join("; ")),
            Some(o.node_id.clone()),
        );
    }
    if o.tokens_estimated {
        progress.any_estimated = true;
    }
    run.checkpoint.tokens_used += o.tokens.unwrap_or(0);
    run.checkpoint.cost_usd += o.cost_usd.unwrap_or(0.0);

    let node_goals: Vec<String> = cfg
        .execution
        .graph
        .nodes
        .iter()
        .find(|n| n.id == o.node_id)
        .map(|n| n.goals.clone())
        .unwrap_or_default();

    let _ = run.store.put_episode(&Episode {
        run_id: run.opts.run_id.clone(),
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
    run.checkpoint.completed_nodes.push(o.node_id.clone());
    run.rec.entry(
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

    // Isolation is a property of the wave, not of the run. The node wrote in
    // its own worktree so its neighbours could not tread on it; now the wave
    // has joined, what it produced is published into the loop root, because
    // the gate collects evidence there and nowhere else.
    if !o.seeded.is_empty() {
        run.rec.entry(
            it,
            LedgerKind::NodeDispatched,
            format!(
                "`{}` was seeded with {} path(s) published by other nodes: {}",
                o.node_id,
                o.seeded.len(),
                o.seeded.join(", ")
            ),
            Some(o.node_id.clone()),
        );
    }
    let published = publish::publish(run.root(), &o.node_id, &o.isolation, &mut out.claimed_paths);
    for path in &published.published {
        progress
            .published_paths
            .insert(path.clone(), o.node_id.clone());
    }
    if let Some(line) = published.describe(&o.node_id) {
        run.rec.entry(
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
    out.outputs.push((o.node_id.clone(), o.output.clone()));
    out.episodes.push(evolve::RanNode {
        node_id: o.node_id,
        goals: node_goals,
        tokens: o.tokens,
    });
}

/// Harvest judgments, ask the gate, and close any phase the ruling completes.
fn rule<S: Store>(
    run: &mut Run<S>,
    planned: &mut Planned,
    it: u32,
) -> (BTreeMap<String, TargetVerdict>, Vec<String>) {
    let cfg = run.cfg;
    let root = run.root();

    // A judge's verdict is only worth reading once we know which provider
    // produced the work it judged; that comes from the episode record, not
    // from the judge's own claim.
    let judgments = evolve::harvest_judgments(cfg, &run.rec, it);
    if !judgments.is_empty() {
        run.rec.entry(
            it,
            LedgerKind::GateEvaluated,
            format!("{} judge verdict(s) parsed", judgments.len()),
            None,
        );
    }

    let ev = collect_evidence(cfg, root, Some(&root.join("metrics.json")), judgments);
    let current = loopsmith_gate::evaluate_all(cfg, &ev);
    for (target, v) in &current {
        let _ = run
            .store
            .set_goal_state(&run.opts.run_id, &v.to_goal_state(it));
        run.rec.entry(
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
    let dispatched: BTreeSet<String> = run.checkpoint.completed_nodes.iter().cloned().collect();
    let phases_closed = planned.phases.refresh(&current, &dispatched);
    for closed in &phases_closed {
        run.rec.entry(
            it,
            LedgerKind::GateEvaluated,
            format!("phase `{closed}` is complete; the phases behind it are now open"),
            None,
        );
    }
    (current, phases_closed)
}

/// Compress this iteration into a summary the next one reads.
///
/// Written after the gate so the summary quotes rulings rather than
/// predictions, and stored so the next iteration reads this instead of every
/// episode that produced it.
fn compress<S: Store>(
    run: &Run<S>,
    progress: &mut Progress,
    dispatched: &Dispatched,
    current: &BTreeMap<String, TargetVerdict>,
    phases_closed: &[String],
    it: u32,
) {
    let mut digest = summary::deterministic(&summary::IterationFacts {
        run_id: &run.opts.run_id,
        iteration: it,
        dispatched: &dispatched.log,
        verdicts: current,
        previous: progress.previous_verdicts.as_ref(),
        tokens: run.checkpoint.tokens_used,
        cost_usd: run.checkpoint.cost_usd,
        phases_closed,
    });
    summary::add_narrative(run.cfg, run.root(), &mut digest, &dispatched.outputs);
    let _ = run.store.put_summary(&digest);
    run.rec
        .entry(it, LedgerKind::GateEvaluated, digest.headline.clone(), None);
    progress.previous_verdicts = Some(current.clone());
}

/// Charge a revision to every node that ran and left its goals unsatisfied,
/// and return the ones that have now spent their last.
///
/// Nodes with no declared goals are never counted: there is nothing to measure
/// them against, so capping them would be arbitrary.
fn spend_revisions<S: Store>(
    run: &Run<S>,
    progress: &mut Progress,
    dispatched: &Dispatched,
    current: &BTreeMap<String, TargetVerdict>,
) -> Vec<String> {
    let ceiling = run.cfg.safety.gates.stop.max_revisions_per_node;
    let mut exhausted = Vec::new();
    for ep in &dispatched.episodes {
        if ep.goals.is_empty() {
            continue;
        }
        let unsatisfied = ep
            .goals
            .iter()
            .any(|g| current.get(g).map(|v| !v.satisfied).unwrap_or(true));
        if unsatisfied {
            let spent = progress.revisions.entry(ep.node_id.clone()).or_insert(0);
            *spent += 1;
            if *spent >= ceiling {
                exhausted.push(ep.node_id.clone());
            }
        }
    }
    exhausted
}

/// Ask the stop gates whether this is the last iteration.
fn decide<S: Store>(
    run: &Run<S>,
    progress: &mut Progress,
    current: &BTreeMap<String, TargetVerdict>,
    it: u32,
) -> Option<StopReason> {
    let sig = progress_signature(current);
    if sig == progress.last_signature {
        progress.stale_iterations += 1;
    } else {
        progress.stale_iterations = 0;
        progress.last_signature = sig;
    }

    should_stop(&StopInputs {
        cfg: run.cfg,
        gates: &run.cfg.safety.gates.stop,
        verdicts: current,
        iteration: it,
        stale_iterations: progress.stale_iterations,
        elapsed_seconds: run.started.elapsed().as_secs(),
        tokens_used: run.checkpoint.tokens_used,
        cost_usd: run.checkpoint.cost_usd,
    })
}
