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
        let pass_rate =
            relevant.iter().map(|v| v.blocking_pass_rate()).sum::<f64>() / relevant.len() as f64;
        let satisfied = relevant.iter().all(|v| v.satisfied);

        for (skill, source) in skills {
            let _ = rec.store.put_skill_trial(&SkillTrial {
                run_id: rec.run_id.to_string(),
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
pub fn write_proposals<S: Store>(cfg: &LoopConfig, rec: &Recorder<S>, iteration: u32) -> usize {
    let Ok(trials) = rec.store.skill_trials() else {
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

    // Do not repeat a proposal already made this run.
    let existing: BTreeSet<String> = rec
        .store
        .proposals(rec.run_id)
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

    for skill in advice.adopt {
        let key = format!("{:?}:{}", ProposalKind::AdoptSkill, skill);
        if existing.contains(&key) {
            continue;
        }
        let (rate, n) = rate_of(&skill);
        let p = Proposal {
            run_id: rec.run_id.to_string(),
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
        if rec.store.put_proposal(&p).is_ok() {
            written += 1;
            rec.entry(
                iteration,
                LedgerKind::ProposalWritten,
                format!("adopt `{skill}`"),
                None,
            );
        }
    }

    for skill in advice.drop {
        let key = format!("{:?}:{}", ProposalKind::DropSkill, skill);
        if existing.contains(&key) {
            continue;
        }
        let (rate, n) = rate_of(&skill);
        let p = Proposal {
            run_id: rec.run_id.to_string(),
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
        if rec.store.put_proposal(&p).is_ok() {
            written += 1;
            rec.entry(
                iteration,
                LedgerKind::ProposalWritten,
                format!("drop `{skill}`"),
                None,
            );
        }
    }
    written
}
