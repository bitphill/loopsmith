//! The iteration loop.
//!
//! Each iteration: acquire any missing sub-agents, dispatch the waves (in
//! parallel, in isolated worktrees where asked), collect evidence including
//! judge verdicts, ask the gate, record what each skill was worth, then ask
//! the stop gates whether to continue.
//!
//! The stop-gate check runs *after* the gate ruling and is mechanical, so no
//! amount of confident output from a node can extend a run past its ceiling.

use loopsmith_core::{LoopConfig, NodeSpec, Role};
use loopsmith_gate::{Evidence, Judgment, TargetVerdict};
use loopsmith_memory::{
    now_ms, score_skills, Checkpoint, Episode, LedgerEntry, LedgerKind, Proposal, ProposalKind,
    SkillTrial, Store,
};
use loopsmith_provider::{digest, dispatch, InvokeRequest};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::judgment;
use crate::worktree::{self, Isolation};

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
    /// Acquire missing sub-agents. Off means a node with an unresolved skill
    /// runs without it and says so.
    pub acquire_skills: bool,
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
}

/// What one node produced, before anything is written down.
struct NodeOutcome {
    node_id: String,
    role: Role,
    provider_id: String,
    prompt_digest: String,
    output: String,
    tokens: Option<u64>,
    tokens_estimated: bool,
    cost_usd: Option<f64>,
    duration_ms: u64,
    skipped: Vec<String>,
    error: Option<String>,
    isolation: String,
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

/// Which candidate should this iteration try, if any?
///
/// Exploration is what separates "confirm what I was told" from "find out what
/// works". A candidate is trialled until it has `min_trials` behind it, then
/// the recommendation logic decides whether it earns a proposal.
fn next_candidate(cfg: &LoopConfig, trials: &[SkillTrial]) -> Option<String> {
    if !cfg.skills.explore || cfg.skills.explore_candidates.is_empty() {
        return None;
    }
    let configured: BTreeSet<&str> = cfg
        .graph
        .nodes
        .iter()
        .flat_map(|n| n.skills.iter().map(|s| s.as_str()))
        .collect();

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for t in trials {
        *counts.entry(t.skill.as_str()).or_insert(0) += 1;
    }
    // Least-tried candidate first, so evidence accumulates evenly instead of
    // piling onto whichever name happens to sort first.
    cfg.skills
        .explore_candidates
        .iter()
        .filter(|c| !configured.contains(c.as_str()))
        .filter(|c| counts.get(c.as_str()).copied().unwrap_or(0) < cfg.skills.min_trials)
        .min_by_key(|c| counts.get(c.as_str()).copied().unwrap_or(0))
        .cloned()
}

/// Resolve the sub-agents a node declares, acquiring what is missing.
fn ensure_skills<S: Store>(
    cfg: &LoopConfig,
    node: &NodeSpec,
    root: &Path,
    store: &S,
    run_id: &str,
    iteration: u32,
    acquire: bool,
) -> Vec<(String, String)> {
    let mut resolved = Vec::new();
    for name in &node.skills {
        match loopsmith_skills::find_installed(name, root) {
            Some(found) => resolved.push((name.clone(), found.source.as_str().to_string())),
            None if acquire => {
                match loopsmith_skills::acquire(name, &node.instruction, &cfg.skills, root) {
                    Ok(r) => {
                        log(
                            store,
                            run_id,
                            iteration,
                            LedgerKind::SkillAcquired,
                            format!(
                                "`{}` via {} into {}",
                                r.name,
                                r.source.as_str(),
                                r.path.display()
                            ),
                            Some(node.id.clone()),
                        );
                        resolved.push((r.name, r.source.as_str().to_string()));
                    }
                    Err(e) => log(
                        store,
                        run_id,
                        iteration,
                        LedgerKind::NodeFailed,
                        format!("could not acquire skill `{name}`: {e}"),
                        Some(node.id.clone()),
                    ),
                }
            }
            None => log(
                store,
                run_id,
                iteration,
                LedgerKind::NodeDispatched,
                format!("skill `{name}` missing and acquisition is off; running without it"),
                Some(node.id.clone()),
            ),
        }
    }
    resolved
}

/// Dispatch one node. Pure with respect to the store so it is safe to call
/// from several threads at once.
fn run_node(
    cfg: &LoopConfig,
    node: &NodeSpec,
    root: &Path,
    run_id: &str,
    scratch: &BTreeMap<String, String>,
    skills: &[(String, String)],
) -> NodeOutcome {
    let iso = if node.isolated {
        worktree::create(root, &node.id, run_id)
    } else {
        Isolation::Shared {
            reason: "not marked isolated".into(),
        }
    };
    let workdir = iso.workdir(root).to_path_buf();

    let constraints = loopsmith_core::ConstraintSet::merged(
        &cfg.constraints.global,
        cfg.constraints.per_node.get(&node.id),
    );
    let system = build_system_prompt(cfg, &constraints);
    let prompt = build_node_prompt(cfg, node, scratch, skills);

    let req = InvokeRequest {
        node_id: node.id.clone(),
        system,
        prompt: prompt.clone(),
        tier: node.tier,
        workdir,
    };

    match dispatch(cfg, &req, node.provider.as_deref()) {
        Ok((resp, skipped)) => NodeOutcome {
            node_id: node.id.clone(),
            role: node.role,
            provider_id: resp.provider_id,
            prompt_digest: digest(&prompt),
            output: resp.output,
            tokens: resp.tokens,
            tokens_estimated: resp.tokens_estimated,
            cost_usd: resp.cost_usd,
            duration_ms: resp.duration_ms,
            skipped,
            error: None,
            isolation: iso.describe(),
        },
        Err(e) => NodeOutcome {
            node_id: node.id.clone(),
            role: node.role,
            provider_id: String::new(),
            prompt_digest: digest(&prompt),
            output: String::new(),
            tokens: None,
            tokens_estimated: false,
            cost_usd: None,
            duration_ms: 0,
            skipped: vec![],
            error: Some(e.to_string()),
            isolation: iso.describe(),
        },
    }
}

pub fn execute<S: Store>(
    cfg: &LoopConfig,
    store: &S,
    opts: &RunOptions,
) -> Result<RunOutcome, String> {
    let plan = loopsmith_graph::plan(&cfg.graph).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let root = &opts.workdir;

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

    let verdicts: BTreeMap<String, TargetVerdict>;
    let mut current: BTreeMap<String, TargetVerdict>;
    let mut last_signature = String::new();
    let mut stale_iterations = 0u32;
    let mut any_estimated = false;
    let mut proposals_written = 0usize;
    let gates = &cfg.stop_gates;
    let width = plan.concurrency.max(1);

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

        let mut episodes_this_iteration: Vec<(String, String, Role, Vec<String>)> = Vec::new();
        let mut node_skills: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        let mut explore_now =
            next_candidate(cfg, &store.skill_trials().unwrap_or_default());

        for wave in &plan.waves {
            // Nodes inside a wave are independent by construction, so the only
            // ordering that matters is between waves. Chunking by the chosen
            // concurrency keeps the fleet at the size `plan` justified.
            for chunk in wave.nodes.chunks(width) {
                let nodes: Vec<&NodeSpec> = chunk
                    .iter()
                    .filter_map(|id| cfg.graph.nodes.iter().find(|n| &n.id == id))
                    .collect();

                if opts.dry_run {
                    for n in &nodes {
                        log(
                            store,
                            &opts.run_id,
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
                    let mut resolved = ensure_skills(
                        cfg,
                        n,
                        root,
                        store,
                        &opts.run_id,
                        it,
                        opts.acquire_skills,
                    );
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
                                    log(
                                        store,
                                        &opts.run_id,
                                        it,
                                        LedgerKind::SkillAcquired,
                                        format!("exploring `{}` on `{}`", r.name, n.id),
                                        Some(n.id.clone()),
                                    );
                                    resolved.push((r.name, r.source.as_str().to_string()));
                                }
                                Err(e) => log(
                                    store,
                                    &opts.run_id,
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
                            let scratch = &scratch;
                            s.spawn(move || run_node(cfg, n, root, &opts.run_id, scratch, &skills))
                        })
                        .collect();
                    handles
                        .into_iter()
                        .filter_map(|h| h.join().ok())
                        .collect()
                });

                // Writes happen after the join so the ledger stays ordered.
                for o in outcomes {
                    if let Some(err) = &o.error {
                        log(
                            store,
                            &opts.run_id,
                            it,
                            LedgerKind::NodeFailed,
                            err.clone(),
                            Some(o.node_id.clone()),
                        );
                        continue;
                    }
                    if !o.skipped.is_empty() {
                        log(
                            store,
                            &opts.run_id,
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
                    log(
                        store,
                        &opts.run_id,
                        it,
                        LedgerKind::NodeSucceeded,
                        format!(
                            "served by `{}` in {}ms, {} tokens{}; {}",
                            o.provider_id,
                            o.duration_ms,
                            o.tokens.unwrap_or(0),
                            if o.tokens_estimated { " (est)" } else { "" },
                            o.isolation
                        ),
                        Some(o.node_id.clone()),
                    );
                    episodes_this_iteration.push((
                        o.node_id.clone(),
                        o.provider_id.clone(),
                        o.role,
                        node_goals,
                    ));
                }
            }
        }

        // --- judgments ------------------------------------------------------
        // A judge's verdict is only worth reading once we know which provider
        // produced the work it judged; that comes from the episode record, not
        // from the judge's own claim.
        let judgments = harvest_judgments(cfg, store, &opts.run_id, it);
        if !judgments.is_empty() {
            log(
                store,
                &opts.run_id,
                it,
                LedgerKind::GateEvaluated,
                format!("{} judge verdict(s) parsed", judgments.len()),
                None,
            );
        }

        // --- gate ----------------------------------------------------------
        let ev = collect_evidence(root, Some(&root.join("metrics.json")), judgments);
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

        // --- what was each skill worth? ------------------------------------
        record_trials(store, &opts.run_id, it, &episodes_this_iteration, &node_skills, &current);
        proposals_written += write_proposals(cfg, store, &opts.run_id, it);

        // --- stop gates ------------------------------------------------------
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
        tokens_estimated: any_estimated,
        cost_usd: checkpoint.cost_usd,
        proposals: proposals_written,
    })
}

/// Read this iteration's judge episodes and turn them into verdicts.
fn harvest_judgments<S: Store>(
    cfg: &LoopConfig,
    store: &S,
    run_id: &str,
    iteration: u32,
) -> Vec<Judgment> {
    let Ok(episodes) = store.episodes(run_id) else {
        return vec![];
    };
    let this: Vec<&Episode> = episodes.iter().filter(|e| e.iteration == iteration).collect();

    let mut out = Vec::new();
    for ep in &this {
        let Some(node) = cfg.graph.nodes.iter().find(|n| n.id == ep.node_id) else {
            continue;
        };
        if node.role != Role::Judge {
            continue;
        }
        // The builder this judge reviewed is whichever upstream node it
        // depends on. Its provider is what independence is measured against.
        let builder_provider = node
            .depends_on
            .iter()
            .find_map(|dep| {
                this.iter()
                    .find(|e| &e.node_id == dep)
                    .map(|e| e.provider_id.clone())
            })
            .unwrap_or_default();
        out.extend(judgment::parse(&ep.output, &ep.provider_id, &builder_provider));
    }
    out
}

/// Pair each skill used this iteration with the gate outcome that followed.
fn record_trials<S: Store>(
    store: &S,
    run_id: &str,
    iteration: u32,
    episodes: &[(String, String, Role, Vec<String>)],
    node_skills: &BTreeMap<String, Vec<(String, String)>>,
    verdicts: &BTreeMap<String, TargetVerdict>,
) {
    for (node_id, _provider, _role, goals) in episodes {
        let Some(skills) = node_skills.get(node_id) else {
            continue;
        };
        if skills.is_empty() {
            continue;
        }
        // Score against the goals this node advances, not the whole loop —
        // otherwise every skill in the graph shares one verdict.
        let relevant: Vec<&TargetVerdict> = goals.iter().filter_map(|g| verdicts.get(g)).collect();
        if relevant.is_empty() {
            continue;
        }
        let pass_rate = relevant.iter().map(|v| v.blocking_pass_rate()).sum::<f64>()
            / relevant.len() as f64;
        let satisfied = relevant.iter().all(|v| v.satisfied);

        for (skill, source) in skills {
            let _ = store.put_skill_trial(&SkillTrial {
                run_id: run_id.to_string(),
                iteration,
                node_id: node_id.clone(),
                skill: skill.clone(),
                source: source.clone(),
                pass_rate: pass_rate.clamp(0.0, 1.0),
                satisfied,
                tokens: None,
                created_ms: now_ms(),
            });
        }
    }
}

/// Turn the accumulated trial record into proposals. Never applied here —
/// changing which sub-agents a loop uses is a config edit, and config edits
/// are the human's.
fn write_proposals<S: Store>(
    cfg: &LoopConfig,
    store: &S,
    run_id: &str,
    iteration: u32,
) -> usize {
    let Ok(trials) = store.skill_trials() else {
        return 0;
    };
    if trials.is_empty() {
        return 0;
    }
    let configured: Vec<String> = cfg
        .graph
        .nodes
        .iter()
        .flat_map(|n| n.skills.iter().cloned())
        .collect();

    let rec = loopsmith_skills::recommend(&configured, &trials, cfg.skills.min_trials, 0.8, 0.2);
    let mut written = 0;

    // Do not repeat a proposal already made this run.
    let existing: BTreeSet<String> = store
        .proposals(run_id)
        .unwrap_or_default()
        .into_iter()
        .map(|p| format!("{:?}:{}", p.kind, p.subject))
        .collect();

    let scored = score_skills(&trials);
    let rate_of = |name: &str| {
        scored
            .iter()
            .find(|s| s.skill == name)
            .map(|s| (s.satisfaction_rate(), s.trials))
            .unwrap_or((0.0, 0))
    };

    for skill in rec.adopt {
        let key = format!("{:?}:{}", ProposalKind::AdoptSkill, skill);
        if existing.contains(&key) {
            continue;
        }
        let (rate, n) = rate_of(&skill);
        let p = Proposal {
            run_id: run_id.to_string(),
            iteration,
            kind: ProposalKind::AdoptSkill,
            subject: skill.clone(),
            rationale: format!(
                "goals were satisfied in {:.0}% of {n} trials using `{skill}`; it is not in the config",
                rate * 100.0
            ),
            patch: Some(format!("skills: [{skill}]")),
            created_ms: now_ms(),
        };
        if store.put_proposal(&p).is_ok() {
            written += 1;
            log(
                store,
                run_id,
                iteration,
                LedgerKind::ProposalWritten,
                format!("adopt `{skill}`"),
                None,
            );
        }
    }

    for skill in rec.drop {
        let key = format!("{:?}:{}", ProposalKind::DropSkill, skill);
        if existing.contains(&key) {
            continue;
        }
        let (rate, n) = rate_of(&skill);
        let p = Proposal {
            run_id: run_id.to_string(),
            iteration,
            kind: ProposalKind::DropSkill,
            subject: skill.clone(),
            rationale: format!(
                "goals were satisfied in only {:.0}% of {n} trials using `{skill}`; it is costing context without earning it",
                rate * 100.0
            ),
            patch: None,
            created_ms: now_ms(),
        };
        if store.put_proposal(&p).is_ok() {
            written += 1;
            log(
                store,
                run_id,
                iteration,
                LedgerKind::ProposalWritten,
                format!("drop `{skill}`"),
                None,
            );
        }
    }
    written
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
        s.push_str(&format!("Never run: {}\n", c.forbidden_commands.join(", ")));
    }
    if !c.human_checkpoint.is_empty() {
        s.push_str(&format!(
            "Stop and ask a human before: {}\n",
            c.human_checkpoint.join(", ")
        ));
    }
    s
}

fn build_node_prompt(
    cfg: &LoopConfig,
    node: &NodeSpec,
    scratch: &BTreeMap<String, String>,
    skills: &[(String, String)],
) -> String {
    let mut s = String::new();
    s.push_str(&format!("## Your task\n{}\n\n", node.instruction));

    if !skills.is_empty() {
        s.push_str("## Sub-agents available to you\n");
        for (name, source) in skills {
            s.push_str(&format!("- `{name}` ({source})\n"));
        }
        s.push_str("\nUse them where they fit. If one does not help, say so — that is recorded.\n\n");
    }

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
        s.push_str(judgment::JUDGE_OUTPUT_CONTRACT);
        s.push_str("\n\n");
    }

    for gname in &node.goals {
        if let Some(pad) = scratch.get(gname) {
            s.push_str(&format!("## Notes carried from earlier iterations\n{pad}\n\n"));
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
        let d = std::env::temp_dir().join(format!("loopsmith-run-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
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
