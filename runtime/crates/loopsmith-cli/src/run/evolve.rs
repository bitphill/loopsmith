//! Self-evolution: what the loop learns about its own tooling.
//!
//! Everything here is evidence-gathering and proposal-writing. Nothing here
//! changes the config, and nothing here can reach goal state. The loop may
//! discover that a sub-agent helps; adopting it is a human's edit.

use crate::judgment;
use crate::logging::Recorder;
use loopsmith_core::{LoopConfig, Role};
use loopsmith_gate::{Judgment, TargetVerdict};
use loopsmith_memory::{
    now_ms, score_skills, Episode, LedgerKind, Proposal, ProposalKind, SkillTrial, Store,
};
use std::collections::{BTreeMap, BTreeSet};

/// One node's dispatch, as the evolution machinery needs to see it.
///
/// This used to be a four-tuple threaded through three functions, which is how
/// `SkillTrial.tokens` came to be permanently `None`: the caller had the number
/// and there was nowhere in the tuple to put it that would not have made every
/// call site worse.
pub struct RanNode {
    pub node_id: String,
    /// Goals this node advances, which is what its skills are scored against.
    pub goals: Vec<String>,
    /// What the dispatch cost, when the provider reported or estimated it.
    pub tokens: Option<u64>,
}

/// Which candidate should this iteration try, if any?
///
/// Exploration is what separates "confirm what I was told" from "find out what
/// works". A candidate is trialled until it has `min_trials` behind it, then
/// the recommendation logic decides whether it earns a proposal.
pub fn next_candidate(cfg: &LoopConfig, trials: &[SkillTrial]) -> Option<String> {
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

/// Read this iteration's judge episodes and turn them into verdicts.
pub fn harvest_judgments<S: Store>(
    cfg: &LoopConfig,
    rec: &Recorder<S>,
    iteration: u32,
) -> Vec<Judgment> {
    let Ok(episodes) = rec.store.episodes(rec.run_id) else {
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
pub fn record_trials<S: Store>(
    rec: &Recorder<S>,
    iteration: u32,
    episodes: &[RanNode],
    node_skills: &BTreeMap<String, Vec<(String, String)>>,
    verdicts: &BTreeMap<String, TargetVerdict>,
) {
    for ep in episodes {
        let Some(skills) = node_skills.get(&ep.node_id) else {
            continue;
        };
        if skills.is_empty() {
            continue;
        }
        // Score against the goals this node advances, not the whole loop —
        // otherwise every skill in the graph shares one verdict.
        let relevant: Vec<&TargetVerdict> = ep
            .goals
            .iter()
            .filter_map(|g| verdicts.get(g))
            .collect();
        if relevant.is_empty() {
            continue;
        }
        let pass_rate =
            relevant.iter().map(|v| v.blocking_pass_rate()).sum::<f64>() / relevant.len() as f64;
        let satisfied = relevant.iter().all(|v| v.satisfied);

        for (skill, source) in skills {
            let _ = rec.store.put_skill_trial(&SkillTrial {
                run_id: rec.run_id.to_string(),
                iteration,
                node_id: ep.node_id.clone(),
                skill: skill.clone(),
                source: source.clone(),
                pass_rate: pass_rate.clamp(0.0, 1.0),
                satisfied,
                // What the node that used this skill actually cost. A skill
                // that lifts the pass rate while tripling the bill is not the
                // same proposition as one that does it for free, and without
                // this the ranking could not tell them apart.
                tokens: ep.tokens,
                created_ms: now_ms(),
            });
        }
    }
}

/// What the loop noticed this iteration that only a human can act on.
pub struct Observed<'a> {
    /// Nodes that reached `max_revisions_per_node` with their goals still
    /// unsatisfied.
    pub exhausted_nodes: &'a [String],
    /// The gate's current rulings, for checks that can never pass.
    pub verdicts: &'a BTreeMap<String, TargetVerdict>,
}

/// The proposal desk for one iteration.
///
/// Holds what has already been said this run, so the same advice is not
/// repeated every iteration for the length of the loop — a proposals file with
/// forty identical entries is a proposals file nobody reads.
struct Desk<'a, S: Store> {
    rec: &'a Recorder<'a, S>,
    iteration: u32,
    said: BTreeSet<String>,
}

impl<S: Store> Desk<'_, S> {
    /// Write one proposal. Returns 1 when something was written, so callers
    /// can sum without a mutable counter each.
    fn write(
        &self,
        kind: ProposalKind,
        subject: &str,
        rationale: String,
        patch: Option<String>,
        headline: String,
    ) -> usize {
        if self.said.contains(&format!("{kind:?}:{subject}")) {
            return 0;
        }
        let p = Proposal {
            run_id: self.rec.run_id.to_string(),
            iteration: self.iteration,
            kind,
            subject: subject.to_string(),
            rationale,
            patch,
            created_ms: now_ms(),
            expires_ms: None,
        }
        // The expiry is derived from the kind rather than passed in by each
        // caller, so a new proposal kind cannot be added without deciding how
        // long its evidence stays true.
        .with_default_expiry();
        if self.rec.store.put_proposal(&p).is_err() {
            return 0;
        }
        self.rec
            .entry(self.iteration, LedgerKind::ProposalWritten, headline, None);
        1
    }
}

/// A node that has spent its revision budget without satisfying its goals is
/// evidence about the *graph*, not about the node: one unit of work was asked
/// to do something it cannot do in one step.
///
/// The loop says so and stops there. Rewriting the graph would be the loop
/// editing its own config, which it may never do — so this is a proposal with
/// a suggested split, and a human decides.
fn propose_reshape<S: Store>(cfg: &LoopConfig, desk: &Desk<'_, S>, exhausted: &[String]) -> usize {
    let mut written = 0;
    for node_id in exhausted {
        let Some(node) = cfg.graph.nodes.iter().find(|n| &n.id == node_id) else {
            continue;
        };
        let goals = node.goals.join(", ");
        written += desk.write(
            ProposalKind::ReshapeGraph,
            node_id,
            format!(
                "`{node_id}` reached its revision ceiling of {} with [{goals}] still unsatisfied. \
                 Repeating the same single step is not going to close them; consider splitting it, \
                 giving it a dependency that prepares its input, or adding a judge that says what \
                 is missing",
                cfg.stop_gates.max_revisions_per_node
            ),
            Some(format!(
                "graph:\n  nodes:\n    - id: {node_id}-prepare\n      role: researcher\n      \
                 instruction: gather what `{node_id}` needs before it runs\n    - id: {node_id}\n      \
                 depends_on: [{node_id}-prepare]"
            )),
            format!("reshape the graph around `{node_id}`"),
        );
    }
    written
}

/// A check whose detector cannot run is not a failing check, it is a broken
/// one. No amount of work by any node will change the answer, so it belongs in
/// front of a human rather than in the next iteration's prompt.
fn propose_criteria_changes<S: Store>(
    desk: &Desk<'_, S>,
    verdicts: &BTreeMap<String, TargetVerdict>,
) -> usize {
    let mut written = 0;
    for v in verdicts.values() {
        for c in v.checks.iter().filter(|c| c.blocking && !c.passed) {
            if !c.evidence.starts_with("detector error") {
                continue;
            }
            written += desk.write(
                ProposalKind::ChangeCriteria,
                &c.name,
                format!(
                    "`{}` on target `{}` cannot be evaluated at all: {}. \
                     A detector that never runs fails closed forever, so this is a criteria \
                     problem rather than a work problem",
                    c.name, v.target, c.evidence
                ),
                None,
                format!("`{}` cannot be evaluated", c.name),
            );
        }
    }
    written
}

/// The loop has candidate sub-agents listed and is not allowed to try them.
///
/// `skills.explore` is off by default because exploration spends real money.
/// When the run is failing anyway, saying "there is something here you have
/// switched off" is worth more than silently not doing it.
fn propose_try_skill<S: Store>(
    cfg: &LoopConfig,
    desk: &Desk<'_, S>,
    verdicts: &BTreeMap<String, TargetVerdict>,
) -> usize {
    if cfg.skills.explore || cfg.skills.explore_candidates.is_empty() {
        return 0;
    }
    if verdicts.values().all(|v| v.satisfied) {
        return 0;
    }
    let configured: BTreeSet<&str> = cfg
        .graph
        .nodes
        .iter()
        .flat_map(|n| n.skills.iter().map(|s| s.as_str()))
        .collect();
    let Some(candidate) = cfg
        .skills
        .explore_candidates
        .iter()
        .find(|c| !configured.contains(c.as_str()))
    else {
        return 0;
    };
    desk.write(
        ProposalKind::TrySkill,
        candidate,
        format!(
            "`{candidate}` is listed under `skills.explore_candidates` but `skills.explore` is \
             off, so it has never been tried. Targets are still unsatisfied; switching exploration \
             on would let the loop find out whether it helps, at the cost of the extra dispatch"
        ),
        Some("skills:\n  explore: true".into()),
        format!("try `{candidate}`"),
    )
}

/// Turn the accumulated trial record into proposals. Never applied here —
/// changing which sub-agents a loop uses is a config edit, and config edits
/// are the human's.
pub fn write_proposals<S: Store>(
    cfg: &LoopConfig,
    rec: &Recorder<S>,
    iteration: u32,
    observed: &Observed,
) -> usize {
    let desk = Desk {
        rec,
        iteration,
        said: rec
            .store
            .proposals(rec.run_id)
            .unwrap_or_default()
            .into_iter()
            .map(|p| format!("{:?}:{}", p.kind, p.subject))
            .collect(),
    };

    // These three read the run's own behaviour; only the fourth needs the
    // accumulated trial record.
    propose_reshape(cfg, &desk, observed.exhausted_nodes)
        + propose_criteria_changes(&desk, observed.verdicts)
        + propose_try_skill(cfg, &desk, observed.verdicts)
        + skill_proposals(cfg, &desk)
}

/// Adopt-or-drop advice, which needs the accumulated trial record.
fn skill_proposals<S: Store>(cfg: &LoopConfig, desk: &Desk<'_, S>) -> usize {
    let Ok(trials) = desk.rec.store.skill_trials() else {
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

    let advice =
        loopsmith_skills::recommend(&configured, &trials, cfg.skills.min_trials, 0.8, 0.2);
    let mut written = 0;

    let scored = score_skills(&trials);
    let rate_of = |name: &str| {
        scored
            .iter()
            .find(|s| s.skill == name)
            .map(|s| (s.satisfaction_rate(), s.trials))
            .unwrap_or((0.0, 0))
    };

    for skill in advice.adopt {
        let (rate, n) = rate_of(&skill);
        written += desk.write(
            ProposalKind::AdoptSkill,
            &skill,
            format!(
                "goals were satisfied in {:.0}% of {n} trials using `{skill}`; it is not in the config",
                rate * 100.0
            ),
            Some(format!("skills: [{skill}]")),
            format!("adopt `{skill}`"),
        );
    }

    for skill in advice.drop {
        let (rate, n) = rate_of(&skill);
        written += desk.write(
            ProposalKind::DropSkill,
            &skill,
            format!(
                "goals were satisfied in only {:.0}% of {n} trials using `{skill}`; it is costing context without earning it",
                rate * 100.0
            ),
            None,
            format!("drop `{skill}`"),
        );
    }
    written
}
