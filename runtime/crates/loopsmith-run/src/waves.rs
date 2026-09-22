//! Dispatching one iteration: the work queue, join strategies, and recovery.
//!
//! The waves run in order, and inside a wave the nodes are independent by
//! construction. Up to `concurrency` nodes are in flight at once, across waves:
//! when a node finishes, the next one starts, rather than a whole chunk
//! waiting on its slowest member.
//!
//! **Join.** A wave is released — the next wave may start — according to
//! `execution.graph.join`:
//!
//! - `wait_for_all`: when every node has finished, successfully or not. This
//!   is the default and is what the engine always did.
//! - `quorum { count }`: once `count` nodes have succeeded.
//! - `first_success`: once one has.
//!
//! A released wave's stragglers are left to finish; what they produce is
//! recorded and published if it arrives before the iteration ends, and nodes
//! not yet started are not started at all. A `quorum` or `first_success` wave
//! whose nodes all finish without meeting the bar is not released, and the
//! waves after it are not dispatched this iteration — they would be reading
//! answers that are not there.
//!
//! **Recovery.** A failed dispatch is classified and answered per
//! `safety.recovery`: retried with backoff (the wait happens on the retry's
//! own thread, so it blocks nothing else), asked again with a note of what was
//! wrong, escalated, or answered with a halt that stops further dispatch.
//!
//! Only the dispatcher's thread touches the store. Workers run a node and send
//! the outcome back; everything is written down here, in arrival order.

use crate::context::Run;
use crate::dispatch::{ensure_skills, run_node, NodeContext, NodeOutcome};
use crate::judgment;
use crate::perturb;
use crate::planning::Planned;
use crate::publish;
use crate::recovering::{self, Response};
use crate::running::{Dispatched, Inputs, Progress};
use crate::state::RunState;
use crate::evolve;
use loopsmith_core::{FailureClass, Join, LoopConfig, NodeSpec, Role};
use loopsmith_memory::{now_ms, Episode, LedgerKind, Store};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread::Scope;
use std::time::Duration;

/// Why dispatch stopped the run, when it did.
pub(crate) struct Halt {
    pub state: RunState,
    pub why: String,
}

/// How far one wave has got.
struct Tally {
    join: Join,
    total: usize,
    launched: usize,
    finished: usize,
    succeeded: usize,
}

impl Tally {
    fn new(join: Join, total: usize) -> Self {
        Tally {
            join,
            total,
            launched: 0,
            finished: 0,
            succeeded: 0,
        }
    }

    fn required(&self) -> usize {
        self.join.required_successes(self.total)
    }

    /// Whether the next wave may start.
    fn released(&self) -> bool {
        match self.join {
            Join::WaitForAll => self.finished == self.total,
            _ => self.succeeded >= self.required(),
        }
    }

    /// Whether the wave did what its join asked of it.
    fn met(&self) -> bool {
        matches!(self.join, Join::WaitForAll) || self.succeeded >= self.required()
    }

    fn describe(&self) -> String {
        match self.join {
            Join::WaitForAll => "wait_for_all".into(),
            Join::Quorum { count } => format!("quorum of {count}"),
            Join::FirstSuccess => "first_success".into(),
        }
    }
}

/// One node's dispatch, as a worker reports it back.
struct Report<'c> {
    wave: usize,
    node: &'c NodeSpec,
    outcome: NodeOutcome,
    /// This dispatch was a recovery retry or revision, not the first try.
    retry: bool,
}

/// Everything a worker needs to run one node, owned or borrowed for the whole
/// dispatch so the thread can outlive the loop iteration that started it.
struct Job<'c> {
    wave: usize,
    node: &'c NodeSpec,
    skills: Vec<(String, String)>,
    guideline: Option<String>,
    published: Arc<BTreeMap<String, String>>,
    revision: Option<String>,
    delay: Duration,
    retry: bool,
}

/// The borrowed, read-only half of a dispatch: what every worker shares.
struct Shared<'c> {
    cfg: &'c LoopConfig,
    root: &'c Path,
    run_id: &'c str,
    inputs: &'c Inputs,
}

fn launch<'scope, 'env, 'c: 'env>(
    s: &'scope Scope<'scope, 'env>,
    tx: &mpsc::Sender<Report<'c>>,
    shared: &'env Shared<'c>,
    job: Job<'c>,
) {
    let tx = tx.clone();
    s.spawn(move || {
        if !job.delay.is_zero() {
            std::thread::sleep(job.delay);
        }
        let outcome = run_node(
            shared.cfg,
            job.node,
            shared.root,
            shared.run_id,
            &NodeContext {
                scratch: &shared.inputs.scratch,
                skills: &job.skills,
                guideline: job.guideline.as_deref(),
                carried: &shared.inputs.carried,
                perturbation: shared.inputs.perturbation.as_ref(),
                published: &job.published,
                revision: job.revision.as_deref(),
            },
        );
        // The receiver outlives every worker; a send can only fail if the
        // dispatcher panicked, and then there is nobody to tell.
        let _ = tx.send(Report {
            wave: job.wave,
            node: job.node,
            outcome,
            retry: job.retry,
        });
    });
}

/// The dispatcher's own mutable state for one iteration.
struct Queue {
    tallies: Vec<Tally>,
    in_flight: usize,
    /// Retries launched and not yet reported.
    retrying: usize,
    /// Failed dispatches per node this iteration, for the recovery policy.
    attempts: BTreeMap<String, u32>,
    halt: Option<Halt>,
    /// A budget ceiling was reached mid-iteration: finish what is running,
    /// start nothing more.
    spent: bool,
    /// What each launched node was given, so a retry is the same dispatch.
    jobs: BTreeMap<String, JobInputs>,
}

/// What a node was launched with, kept so a retry is the same dispatch.
type JobInputs = (Vec<(String, String)>, Option<String>, Arc<BTreeMap<String, String>>);

impl Queue {
    fn may_launch(&self) -> bool {
        self.halt.is_none() && !self.spent
    }
}

pub(crate) fn dispatch<S: Store>(
    run: &mut Run<S>,
    planned: &Planned,
    progress: &mut Progress,
    inputs: &Inputs,
    explore: &mut Option<String>,
    out: &mut Dispatched,
    it: u32,
) -> Option<Halt> {
    let cfg = run.cfg;
    let shared = Shared {
        cfg,
        root: run.root(),
        run_id: run.opts.run_id.as_str(),
        inputs,
    };
    let width = planned.graph.concurrency.max(1);
    let join = cfg.execution.graph.join;

    let mut q = Queue {
        tallies: Vec::new(),
        in_flight: 0,
        retrying: 0,
        attempts: BTreeMap::new(),
        halt: None,
        spent: false,
        jobs: BTreeMap::new(),
    };

    std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel::<Report>();

        'waves: for (w, wave) in planned.graph.waves.iter().enumerate() {
            if !q.may_launch() {
                break;
            }
            let mut ids = wave.nodes.clone();
            if matches!(&inputs.perturbation, Some(perturb::Perturbation::Reorder)) {
                perturb::shuffle(&mut ids, inputs.seed);
            }
            let nodes = eligible_nodes(run, planned, progress, &ids, it);
            q.tallies.push(Tally::new(join, nodes.len()));
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

            // Acquisition touches the store, so it happens here, before any
            // worker starts.
            resolve_skills(run, &nodes, explore, out, it);

            // Snapshotted per wave: every node in one wave sees the same
            // published set, which is the honest answer — they run together.
            let published = Arc::new(progress.published_paths.clone());
            let mut queue = nodes.into_iter().peekable();
            loop {
                while q.in_flight < width && !q.tallies[w].released() && q.may_launch() {
                    let Some(n) = queue.next() else { break };
                    let skills = out.node_skills.get(&n.id).cloned().unwrap_or_default();
                    let guideline = planned.phases.guideline_for(n).map(str::to_string);
                    q.jobs.insert(
                        n.id.clone(),
                        (skills.clone(), guideline.clone(), published.clone()),
                    );
                    launch(
                        s,
                        &tx,
                        &shared,
                        Job {
                            wave: w,
                            node: n,
                            skills,
                            guideline,
                            published: published.clone(),
                            revision: None,
                            delay: Duration::ZERO,
                            retry: false,
                        },
                    );
                    q.in_flight += 1;
                    q.tallies[w].launched += 1;
                }

                let t = &q.tallies[w];
                let nothing_left = queue.peek().is_none() || !q.may_launch();
                if t.released() || (nothing_left && t.finished == t.launched) {
                    break;
                }
                let report = rx.recv().expect("a launched node always reports back");
                handle(run, progress, out, &mut q, s, &tx, &shared, report, it);
            }

            let t = &q.tallies[w];
            let skipped: Vec<String> = queue.map(|n| format!("`{}`", n.id)).collect();
            if t.released() && !skipped.is_empty() && q.may_launch() {
                run.rec.entry(
                    it,
                    LedgerKind::NodeDispatched,
                    format!(
                        "wave {} released by its {} after {} success(es); not dispatched: {}",
                        w + 1,
                        t.describe(),
                        t.succeeded,
                        skipped.join(", ")
                    ),
                    None,
                );
            }
            if t.finished == t.launched && !t.met() {
                run.rec.entry(
                    it,
                    LedgerKind::NodeFailed,
                    format!(
                        "wave {} needed {} success(es) for its {} and got {}; the waves after \
                         it are not dispatched this iteration",
                        w + 1,
                        t.required(),
                        t.describe(),
                        t.succeeded
                    ),
                    None,
                );
                break 'waves;
            }
        }

        // Stragglers from released waves, and any retries still pending.
        while q.in_flight > 0 {
            let report = rx.recv().expect("a launched node always reports back");
            handle(run, progress, out, &mut q, s, &tx, &shared, report, it);
        }
    });

    if run.life.state() == RunState::Retrying {
        let _ = run.enter(RunState::Running, "retries settled");
    }
    q.halt
}

/// Decide what one report means: record it, or send the node round again.
#[allow(clippy::too_many_arguments)]
fn handle<'scope, 'env, 'c: 'env, S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    out: &mut Dispatched,
    q: &mut Queue,
    s: &'scope Scope<'scope, 'env>,
    tx: &mpsc::Sender<Report<'c>>,
    shared: &'env Shared<'c>,
    report: Report<'c>,
    it: u32,
) {
    let Report {
        wave,
        node,
        outcome,
        retry,
    } = report;
    q.in_flight -= 1;
    if retry {
        q.retrying -= 1;
    }

    let Some((class, detail)) = failure_of(node, &outcome) else {
        let violation = record_outcome(run, progress, out, outcome, it);
        finish(run, progress, q, wave, violation.is_none(), it);
        if let Some((class, why)) = violation {
            answer_final(run, progress, q, node, class, why, it);
        }
        return;
    };

    let attempt = q.attempts.entry(node.id.clone()).or_insert(0);
    *attempt += 1;
    let attempt = *attempt;
    let mut response = recovering::respond(&run.cfg.safety.recovery, class, attempt);
    if matches!(response, Response::Retry { .. } | Response::Revise) && !q.may_launch() {
        response = Response::Continue;
    }
    run.rec.entry(
        it,
        LedgerKind::Recovered,
        format!(
            "`{}`: {detail} — {}",
            node.id,
            recovering::describe(class, &response, attempt)
        ),
        Some(node.id.clone()),
    );

    match response {
        Response::Retry { .. } | Response::Revise => {
            let (delay, revision) = match response {
                Response::Retry { delay_seconds } => (Duration::from_secs(delay_seconds), None),
                _ => (Duration::ZERO, Some(detail.clone())),
            };
            // A revised dispatch was a real dispatch: what it spent is charged
            // now, even though its output is being thrown away.
            charge(run, progress, &outcome);
            let (skills, guideline, published) = q
                .jobs
                .get(&node.id)
                .cloned()
                .unwrap_or_else(|| (Vec::new(), None, Arc::new(BTreeMap::new())));
            if q.retrying == 0 && run.life.state() == RunState::Running {
                let _ = run.enter(
                    RunState::Retrying,
                    format!("`{}` hit a {}", node.id, recovering::class_name(class)),
                );
            }
            q.retrying += 1;
            progress.retries += 1;
            launch(
                s,
                tx,
                shared,
                Job {
                    wave,
                    node,
                    skills,
                    guideline,
                    published,
                    revision,
                    delay,
                    retry: true,
                },
            );
            q.in_flight += 1;
        }
        other => {
            let _ = record_outcome(run, progress, out, outcome, it);
            finish(run, progress, q, wave, false, it);
            apply_final(run, progress, q, node, other, detail, it);
        }
    }

    if q.retrying == 0 && run.life.state() == RunState::Retrying {
        let _ = run.enter(RunState::Running, "retries settled");
    }
}

/// Mark a node's final report against its wave, and check the budget.
fn finish<S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    q: &mut Queue,
    wave: usize,
    succeeded: bool,
    it: u32,
) {
    let t = &mut q.tallies[wave];
    t.finished += 1;
    if succeeded {
        t.succeeded += 1;
    } else {
        progress.failed_dispatches += 1;
    }
    if !q.spent {
        if let Some(why) = over_budget(run) {
            q.spent = true;
            run.rec.entry(
                it,
                LedgerKind::StopGateTriggered,
                format!("{why} mid-iteration; nothing further is dispatched this iteration"),
                None,
            );
        }
    }
}

/// A failure found after the node was recorded — today, a safety violation.
/// There is nothing to retry: the dispatch already happened.
#[allow(clippy::too_many_arguments)]
fn answer_final<S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    q: &mut Queue,
    node: &NodeSpec,
    class: FailureClass,
    why: String,
    it: u32,
) {
    let response = match recovering::respond(&run.cfg.safety.recovery, class, 1) {
        Response::Retry { .. } | Response::Revise => Response::Continue,
        r => r,
    };
    run.rec.entry(
        it,
        LedgerKind::Recovered,
        format!(
            "`{}`: {why} — {}",
            node.id,
            recovering::describe(class, &response, 1)
        ),
        Some(node.id.clone()),
    );
    apply_final(run, progress, q, node, response, why, it);
}

/// Carry out a response that ends this node's part in the iteration.
fn apply_final<S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    q: &mut Queue,
    node: &NodeSpec,
    response: Response,
    why: String,
    it: u32,
) {
    match response {
        Response::Escalate => escalate(run, progress, &node.id, &why, it),
        Response::Halt(state) => {
            if q.halt.is_none() {
                q.halt = Some(Halt {
                    state,
                    why: format!("`{}`: {why}", node.id),
                });
            }
        }
        Response::Continue | Response::Retry { .. } | Response::Revise => {}
    }
}

/// Stop dispatching a node for the rest of the run and record the question.
pub(crate) fn escalate<S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    node_id: &str,
    why: &str,
    it: u32,
) {
    if !progress.escalated_nodes.insert(node_id.to_string()) {
        return;
    }
    let question = format!("`{node_id}`: {why}");
    run.rec.entry(
        it,
        LedgerKind::Escalated,
        format!("{question}; it is not dispatched again until a human resumes the run"),
        Some(node_id.to_string()),
    );
    run.checkpoint.escalations.push(question);
}

/// The class of a report that did not do its job, and why.
///
/// A dispatch error is classified by the provider. A judge that answered
/// without a single `VERDICT:` block is invalid output: it ran, and the gate
/// cannot read a word of it.
fn failure_of(node: &NodeSpec, o: &NodeOutcome) -> Option<(FailureClass, String)> {
    if let Some(err) = &o.error {
        return Some((o.failure.unwrap_or(FailureClass::ToolUnavailable), err.clone()));
    }
    if node.role == Role::Judge && judgment::parse(&o.output, "", "").is_empty() {
        return Some((
            FailureClass::InvalidOutput,
            "the response contained no `VERDICT:` block, so the gate could not read it. \
             Use exactly the required output format"
                .into(),
        ));
    }
    None
}

/// Whether a budget ceiling has been reached, and which.
fn over_budget<S: Store>(run: &Run<S>) -> Option<String> {
    let g = &run.cfg.safety.gates.stop;
    if let Some(limit) = g.max_tokens {
        if run.checkpoint.tokens_used >= limit {
            return Some(format!("token budget ({limit}) reached"));
        }
    }
    if let Some(limit) = g.max_cost_usd {
        if run.checkpoint.cost_usd >= limit {
            return Some(format!("cost budget (${limit:.2}) reached"));
        }
    }
    None
}

/// The nodes in one wave that should run this iteration.
fn eligible_nodes<'c, S: Store>(
    run: &Run<'c, S>,
    planned: &Planned,
    progress: &Progress,
    ids: &[String],
    it: u32,
) -> Vec<&'c NodeSpec> {
    let cfg = run.cfg;
    let ceiling = cfg.safety.gates.stop.max_revisions_per_node;
    let mut nodes = Vec::new();
    for id in ids {
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
        if progress.escalated_nodes.contains(&n.id) {
            // Logged once, when it was escalated.
            continue;
        }
        nodes.push(n);
    }
    nodes
}

/// Resolve each node's sub-agents, and attach the exploration candidate to the
/// first builder in the wave. Judges and adversaries keep a fixed toolset so
/// the check itself does not drift while the work does.
fn resolve_skills<S: Store>(
    run: &Run<S>,
    nodes: &[&NodeSpec],
    explore: &mut Option<String>,
    out: &mut Dispatched,
    it: u32,
) {
    let cfg = run.cfg;
    let root = run.root();
    for n in nodes {
        let mut resolved = ensure_skills(cfg, n, root, &run.rec, it, run.opts.acquire_skills);
        if n.role == Role::Builder {
            if let Some(cand) = explore.take() {
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

/// Charge what a dispatch spent.
fn charge<S: Store>(run: &mut Run<S>, progress: &mut Progress, o: &NodeOutcome) {
    if o.tokens_estimated {
        progress.any_estimated = true;
    }
    run.checkpoint.tokens_used += o.tokens.unwrap_or(0);
    run.checkpoint.cost_usd += o.cost_usd.unwrap_or(0.0);
}

/// Write down one node's final outcome: ledger, episode, spend, publication.
///
/// Returns a failure found in the course of publishing — a change to a path
/// the node's constraints forbid — for the caller to answer.
fn record_outcome<S: Store>(
    run: &mut Run<S>,
    progress: &mut Progress,
    out: &mut Dispatched,
    o: NodeOutcome,
    it: u32,
) -> Option<(FailureClass, String)> {
    let cfg = run.cfg;
    out.log
        .push((o.node_id.clone(), o.provider_id.clone(), o.role, o.error.is_none()));
    if let Some(err) = &o.error {
        run.rec
            .entry(it, LedgerKind::NodeFailed, err.clone(), Some(o.node_id.clone()));
        return None;
    }
    if !o.skipped.is_empty() {
        run.rec.entry(
            it,
            LedgerKind::NodeDispatched,
            format!("cascade skipped: {}", o.skipped.join("; ")),
            Some(o.node_id.clone()),
        );
    }
    charge(run, progress, &o);

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

    // A node that touched a path its constraints forbid publishes nothing:
    // the whole worktree is suspect, not just the offending file.
    let constraints = loopsmith_core::ConstraintSet::merged(
        &cfg.safety.limits.global,
        cfg.safety.limits.per_node.get(&o.node_id),
    );
    let forbidden = publish::forbidden_changes(&o.isolation, &constraints.forbidden_paths);
    let violation = if forbidden.is_empty() {
        // Isolation is a property of the wave, not of the run. The node wrote
        // in its own worktree so its neighbours could not tread on it; now it
        // has finished, what it produced is published into the loop root,
        // because the gate collects evidence there and nowhere else.
        let published =
            publish::publish(run.root(), &o.node_id, &o.isolation, &mut out.claimed_paths);
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
        None
    } else {
        let why = format!(
            "changed {} which its constraints forbid; nothing it produced was published",
            forbidden
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        run.rec.entry(
            it,
            LedgerKind::NodeFailed,
            format!("`{}` {why}", o.node_id),
            Some(o.node_id.clone()),
        );
        Some((FailureClass::SafetyViolation, why))
    };

    out.outputs.push((o.node_id.clone(), o.output.clone()));
    out.episodes.push(evolve::RanNode {
        node_id: o.node_id.clone(),
        goals: node_goals,
        tokens: o.tokens,
    });
    violation
}
