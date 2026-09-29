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
use crate::evidence::{self, failing_checks};
use crate::planning::Planned;
use crate::recovering::{self, Failure, Response};
use crate::state::RunState;
use crate::stop::{progress_signature, should_stop, StopInputs, StopReason};
use crate::state::Halt;
use crate::waves;
use crate::{evolve, perturb, rules, summary};
use loopsmith_core::{FailureClass, GateKind, Role};
use loopsmith_gate::{Evidence, TargetVerdict};
use loopsmith_memory::{Checkpoint, LedgerKind, Store};
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
    /// Nodes escalated to a human this run. Not dispatched again until a
    /// resume, which is how a human answers.
    pub escalated_nodes: BTreeSet<String>,
    /// Final dispatch failures this run, after recovery had its say.
    pub failed_dispatches: u32,
    /// Dispatches recovery sent round again.
    pub retries: u32,
    /// Alerts raised this run. Each fires at most once.
    pub alerts: Vec<crate::metrics::RaisedAlert>,
    /// Container nodes already told they are running in a worktree instead,
    /// so the ledger says it once per run rather than once per iteration.
    pub degraded: BTreeSet<String>,
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
            // Held across a resume until someone answers for them.
            escalated_nodes: cp
                .escalations
                .iter()
                .filter_map(|e| e.node_id.clone())
                .collect(),
            failed_dispatches: cp.failed_dispatches,
            retries: cp.retries,
            alerts: Vec::new(),
            degraded: BTreeSet::new(),
        }
    }

    /// Write the counters back, with the rulings that were current.
    ///
    /// A run halted before its first ruling has no verdicts to store, and
    /// overwriting the previous run's with nothing would throw away the only
    /// record of where it had got to.
    pub fn store_into(&self, cp: &mut Checkpoint, verdicts: &BTreeMap<String, TargetVerdict>) {
        cp.revisions = self.revisions.clone();
        cp.stale_iterations = self.stale_iterations;
        cp.last_signature = self.last_signature.clone();
        cp.retries = self.retries;
        cp.failed_dispatches = self.failed_dispatches;
        for a in &self.alerts {
            if !cp.alerts_raised.contains(&a.id) {
                cp.alerts_raised.push(a.id.clone());
            }
        }
        if !verdicts.is_empty() {
            cp.verdicts_json = serde_json::to_string(verdicts).ok();
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
pub(crate) struct Inputs {
    /// Per-goal scratchpad notes. Read once per iteration and shared, so a
    /// thread never touches the store mid-dispatch.
    pub scratch: BTreeMap<String, String>,
    /// Compressed history from earlier iterations, the same for every node.
    pub carried: String,
    /// The untried sub-agent to attach to this iteration's first builder.
    pub explore_now: Option<String>,
    pub perturbation: Option<perturb::Perturbation>,
    pub seed: u64,
    /// Promoted cross-run memory, one line per record.
    pub learned: Vec<String>,
}

/// What one iteration dispatched, before the gate ruled on it.
#[derive(Default)]
pub(crate) struct Dispatched {
    pub episodes: Vec<evolve::RanNode>,
    /// Every dispatch including failures, for the summary.
    pub log: Vec<(String, String, Role, bool)>,
    pub outputs: Vec<(String, String)>,
    pub node_skills: BTreeMap<String, Vec<(String, String)>>,
    /// Which node published each path *this iteration*, so two isolated
    /// builders writing the same file is reported rather than resolved by
    /// whichever thread happened to finish last. Deliberately narrower than
    /// `Progress::published_paths`: a node rewriting its own output next
    /// iteration is the normal case and is not a collision.
    pub claimed_paths: BTreeMap<String, String>,
}

/// Where the run stood when an iteration began, for a rollback to return to.
///
/// Spend is deliberately not in it. A rollback discards what an iteration
/// *achieved*; what it cost stays charged, or a run could refund its own
/// budget by tripping a rollback rule.
struct Snapshot {
    completed_nodes: Vec<String>,
    previous_verdicts: Option<BTreeMap<String, TargetVerdict>>,
    last_signature: String,
    stale_iterations: u32,
    published_paths: BTreeMap<String, String>,
}

impl Snapshot {
    fn take(cp: &Checkpoint, p: &Progress) -> Self {
        Snapshot {
            completed_nodes: cp.completed_nodes.clone(),
            previous_verdicts: p.previous_verdicts.clone(),
            last_signature: p.last_signature.clone(),
            stale_iterations: p.stale_iterations,
            published_paths: p.published_paths.clone(),
        }
    }

    fn restore(self, cp: &mut Checkpoint, p: &mut Progress) -> BTreeMap<String, TargetVerdict> {
        cp.completed_nodes = self.completed_nodes;
        p.last_signature = self.last_signature;
        p.stale_iterations = self.stale_iterations;
        p.published_paths = self.published_paths;
        p.previous_verdicts = self.previous_verdicts.clone();
        self.previous_verdicts.unwrap_or_default()
    }
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

        let before = Snapshot::take(&run.checkpoint, progress);
        let mut inputs = prepare(run, progress, it);
        let mut explore = inputs.explore_now.take();
        let mut dispatched = Dispatched::default();
        let mut halt = waves::dispatch(
            run,
            planned,
            progress,
            &inputs,
            &mut explore,
            &mut dispatched,
            it,
        );
        let (current, phases_closed, ev) = rule(run, planned, it);

        // Rollback rules run after every ruling, even one that follows a halt:
        // a halt decides how the run ends, a rollback decides whether what
        // this iteration achieved is kept, and one does not answer the other.
        let rollback = rules::apply(run, GateKind::Rollback, &ev, it);
        let rolled_back = [&halt, &rollback]
            .into_iter()
            .flatten()
            .any(|h| h.state == RunState::RolledBack);
        if halt.is_none() {
            halt = rollback;
        }
        // The rulings reach the store only once no rollback rule has refused
        // them. Written earlier, `status` and a `goal_satisfied` trigger would
        // both act on a ruling the run has already thrown away.
        if !rolled_back {
            persist_rulings(run, &current, it);
        }

        compress(run, progress, &dispatched, &current, &phases_closed, it);
        crate::metrics::watch(run, progress, it);
        let exhausted = spend_revisions(run, progress, &dispatched, &current);
        let repeated = answer_repeated_failures(run, progress, &exhausted, it);
        if halt.is_none() {
            halt = repeated;
        }

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

        if let Some(Halt { state, why }) = halt {
            let verdicts = if rolled_back {
                roll_back(run, progress, before, &dispatched, it)
            } else {
                current
            };
            return Stopped {
                reason: StopReason::Halted { state, why },
                verdicts,
            };
        }

        if let Some(reason) = decide(run, progress, &current, it) {
            return Stopped {
                reason,
                verdicts: current,
            };
        }

        progress.store_into(&mut run.checkpoint, &current);
        if let Some(Halt { state, why }) = run.save_or_halt() {
            return Stopped {
                reason: StopReason::Halted { state, why },
                verdicts: current,
            };
        }
    }
}

/// Write the gate's rulings to the store, where `status`, the MCP server,
/// and `goal_satisfied` triggers read them.
fn persist_rulings<S: Store>(run: &Run<S>, verdicts: &BTreeMap<String, TargetVerdict>, it: u32) {
    for v in verdicts.values() {
        let _ = run
            .store
            .set_goal_state(&run.opts.run_id, &v.to_goal_state(it));
    }
}

/// Undo what this iteration achieved, and say what it left on disk.
fn roll_back<S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    before: Snapshot,
    dispatched: &Dispatched,
    it: u32,
) -> BTreeMap<String, TargetVerdict> {
    let verdicts = before.restore(&mut run.checkpoint, progress);
    let written: Vec<String> = dispatched
        .claimed_paths
        .keys()
        .map(|p| format!("`{p}`"))
        .collect();
    run.rec.entry(
        it,
        LedgerKind::Recovered,
        if written.is_empty() {
            format!("rolled back iteration {it}: its progress is discarded; spend stays charged")
        } else {
            format!(
                "rolled back iteration {it}: its progress is discarded and spend stays \
                 charged. It published {} into the loop root, which loopsmith does not \
                 revert — check them before resuming",
                written.join(", ")
            )
        },
        None,
    );
    verdicts
}

/// Answer each node that has just spent its last revision.
fn answer_repeated_failures<S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    exhausted: &[String],
    it: u32,
) -> Option<Halt> {
    let mut halt = None;
    for node in exhausted {
        let why = format!(
            "revised {} times without satisfying its goals",
            run.cfg.safety.gates.stop.max_revisions_per_node
        );
        let failure = Failure {
            class: FailureClass::RepeatedFailure,
            attempt: 1,
            subject: format!("`{node}`"),
            detail: why.clone(),
            node: Some(node.clone()),
            iteration: it,
        };
        match recovering::answer(&run.rec, &run.cfg.safety.recovery, &failure, false) {
            Response::Escalate => waves::escalate(run, progress, node, &why, it),
            Response::Halt(state) => {
                halt.get_or_insert(Halt {
                    state,
                    why: format!("`{node}`: {why}"),
                });
            }
            Response::Retry { .. } | Response::Revise | Response::Continue => {}
        }
    }
    halt
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
        learned: crate::remembering::recall(run),
    }
}

/// Harvest judgments, ask the gate, and close any phase the ruling completes.
fn rule<S: Store>(
    run: &mut Run<S>,
    planned: &mut Planned,
    it: u32,
) -> (BTreeMap<String, TargetVerdict>, Vec<String>, Evidence) {
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

    let ev = evidence::at_root(cfg, root, judgments);
    let current = loopsmith_gate::evaluate_all(cfg, &ev);
    for (target, v) in &current {
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
    (current, phases_closed, ev)
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
