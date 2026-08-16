//! What to do when the loop has stopped moving but has not run out of budget.
//!
//! `no_progress_iterations` is the jidoka gate: stop the line rather than spin.
//! `no_progress_iterations_randomness` fires earlier and does something else
//! first — because a loop that repeats the identical approach three times and
//! then quits has learned nothing, and the cheapest thing to vary is the
//! approach.
//!
//! Two properties make this safe to leave running unattended:
//!
//! - **The menu is fixed.** A perturbation is one of four things, all of them
//!   changes to *how* the loop works, none of them changes to *what counts as
//!   done*. The agent picks from the menu; it does not write the menu.
//! - **It is seeded.** The seed is derived from the run id and iteration and
//!   written to the ledger, so a run that took a strange turn can be replayed.
//!
//! The agent is asked first because a stall usually has a legible cause — the
//! same check failing with the same evidence every time. When no cheap provider
//! is reachable, or the answer does not parse, the seeded fallback picks from
//! the same menu and the run carries on.

use loopsmith_core::{LoopConfig, Tier};
use loopsmith_memory::IterationSummary;
use loopsmith_provider::{dispatch, InvokeRequest};
use std::path::Path;

/// The fixed menu. Everything here changes method, never criteria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Perturbation {
    /// Dispatch the nodes of each wave in a different order.
    Reorder,
    /// Run builders one tier stronger for an iteration.
    Escalate,
    /// Force an exploration candidate onto a builder, even if `explore` is off.
    Explore,
    /// Tell the builders to try a specific different approach.
    Reframe(String),
}

impl Perturbation {
    pub fn describe(&self) -> String {
        match self {
            Perturbation::Reorder => "reorder: dispatch the wave in a different order".into(),
            Perturbation::Escalate => "escalate: run builders one tier stronger".into(),
            Perturbation::Explore => "explore: attach an untried sub-agent to a builder".into(),
            Perturbation::Reframe(d) => format!("reframe: {d}"),
        }
    }

    /// The tier a node should run at under this perturbation.
    pub fn tier_for(&self, base: Tier) -> Tier {
        match (self, base) {
            (Perturbation::Escalate, Tier::Cheap) => Tier::Standard,
            (Perturbation::Escalate, _) => Tier::Strong,
            _ => base,
        }
    }

    /// Extra instruction to append to a builder's prompt.
    pub fn directive(&self) -> Option<String> {
        let text = match self {
            Perturbation::Reframe(d) => d.clone(),
            Perturbation::Escalate => {
                "Previous attempts at this have not moved the gate. Do not repeat them. \
                 Re-read the failing check and attack its actual cause."
                    .into()
            }
            Perturbation::Reorder | Perturbation::Explore => {
                "Previous attempts at this have not moved the gate. Try a materially \
                 different approach rather than a refinement of the last one."
                    .into()
            }
        };
        Some(format!(
            "## The loop has stalled\n{text}\n\nThis instruction changes how you work. \
             It does not change what counts as done — the gate is unchanged.\n\n"
        ))
    }

    fn from_choice(choice: &str, directive: Option<String>) -> Option<Self> {
        match choice.trim().to_lowercase().as_str() {
            "reorder" => Some(Perturbation::Reorder),
            "escalate" => Some(Perturbation::Escalate),
            "explore" => Some(Perturbation::Explore),
            "reframe" => directive
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .map(Perturbation::Reframe),
            _ => None,
        }
    }
}

/// The stall, as the agent is allowed to see it.
///
/// Deliberately narrow. The agent gets the failing checks and the recent
/// summaries — enough to reason about what is stuck — and nothing that would
/// let it address the gate, the config, or the store.
pub struct Stall<'a> {
    pub stale_iterations: u32,
    /// Failing blocking checks, as `(target, check name, evidence)`.
    pub failing: &'a [(String, String, String)],
    pub recent: &'a [IterationSummary],
}

/// A deterministic 64-bit seed for this stall, so a strange run is replayable.
pub fn seed_for(run_id: &str, iteration: u32) -> u64 {
    // FNV-1a over the run id, mixed with the iteration. The provider crate
    // already uses this hash for prompt digests; reusing it keeps the workspace
    // to one hash rather than two.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in run_id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= iteration as u64;
    h.wrapping_mul(0x0000_0100_0000_01b3)
}

/// SplitMix64. Small, well-distributed, and no dependency.
fn next_random(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Shuffle in place, seeded. Fisher-Yates.
pub fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed;
    for i in (1..items.len()).rev() {
        let j = (next_random(&mut state) % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

/// The seeded fallback: pick from the same menu the agent chooses from.
pub fn fallback(seed: u64) -> Perturbation {
    let mut state = seed;
    match next_random(&mut state) % 3 {
        0 => Perturbation::Reorder,
        1 => Perturbation::Escalate,
        _ => Perturbation::Explore,
    }
}

/// Ask a cheap provider what to try differently, falling back to the seeded
/// choice when there is no answer or the answer is not on the menu.
///
/// Returns the perturbation and whether an agent chose it, so the ledger can
/// record which happened.
pub fn choose(
    cfg: &LoopConfig,
    workdir: &Path,
    stall: &Stall,
    seed: u64,
) -> (Perturbation, bool) {
    match ask_agent(cfg, workdir, stall) {
        Some(p) => (p, true),
        None => (fallback(seed), false),
    }
}

fn ask_agent(cfg: &LoopConfig, workdir: &Path, stall: &Stall) -> Option<Perturbation> {
    if cfg.cascade_for(Tier::Cheap).is_empty() {
        return None;
    }

    let mut prompt = format!(
        "An automated loop has produced no measurable change for {} consecutive iterations. \
         Decide what it should vary next.\n\n\
         Reply with exactly these lines and nothing else:\n\
         CHOICE: <reorder|escalate|explore|reframe>\n\
         DIRECTIVE: <one sentence, required only when CHOICE is reframe>\n\n\
         What each choice means:\n\
         - reorder: run the same nodes in a different order\n\
         - escalate: run the builders on a stronger model\n\
         - explore: attach an untried sub-agent to a builder\n\
         - reframe: tell the builders to take a specific different approach\n\n\
         You are not deciding whether anything is finished, and nothing you write \
         affects that. A separate deterministic gate owns that ruling.\n\n",
        stall.stale_iterations
    );

    if stall.failing.is_empty() {
        prompt.push_str("No blocking check is currently failing, yet nothing is changing.\n");
    } else {
        prompt.push_str("Blocking checks still failing:\n");
        for (target, name, evidence) in stall.failing {
            prompt.push_str(&format!("- [{target}] `{name}`: {evidence}\n"));
        }
    }

    if !stall.recent.is_empty() {
        prompt.push_str("\nWhat has already been tried:\n");
        for s in stall.recent {
            prompt.push_str(&s.render());
        }
    }

    let req = InvokeRequest {
        node_id: "perturb".into(),
        system: "You choose one recovery tactic for a stalled automated loop.".into(),
        prompt,
        tier: Tier::Cheap,
        workdir: workdir.to_path_buf(),
    };

    let (resp, _) = dispatch(cfg, &req, None).ok()?;
    parse_choice(&resp.output)
}

/// Strict, line-oriented, and unforgiving. An answer that is not on the menu is
/// discarded rather than guessed at — the seeded fallback is a better outcome
/// than acting on a misread instruction.
pub fn parse_choice(text: &str) -> Option<Perturbation> {
    let mut choice = None;
    let mut directive = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim().to_uppercase().as_str() {
            "CHOICE" => choice = Some(value.trim().to_string()),
            "DIRECTIVE" => directive = Some(value.trim().to_string()),
            _ => {}
        }
    }
    Perturbation::from_choice(&choice?, directive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_is_stable_for_a_run_and_iteration() {
        assert_eq!(seed_for("run-1", 4), seed_for("run-1", 4));
        assert_ne!(seed_for("run-1", 4), seed_for("run-1", 5));
        assert_ne!(seed_for("run-1", 4), seed_for("run-2", 4));
    }

    #[test]
    fn the_same_seed_shuffles_the_same_way() {
        let mut a = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut b = a.clone();
        shuffle(&mut a, 42);
        shuffle(&mut b, 42);
        assert_eq!(a, b, "a seeded run must be replayable");

        let mut c = vec![1, 2, 3, 4, 5, 6, 7, 8];
        shuffle(&mut c, 43);
        assert_ne!(a, c, "a different seed must actually differ");
    }

    #[test]
    fn a_shuffle_keeps_every_element() {
        let mut v: Vec<u32> = (0..32).collect();
        shuffle(&mut v, 7);
        v.sort();
        assert_eq!(v, (0..32).collect::<Vec<u32>>());
    }

    #[test]
    fn the_fallback_only_picks_from_the_menu() {
        for seed in 0..200u64 {
            let p = fallback(seed);
            assert!(
                matches!(
                    p,
                    Perturbation::Reorder | Perturbation::Escalate | Perturbation::Explore
                ),
                "seed {seed} produced {p:?}"
            );
        }
    }

    #[test]
    fn a_well_formed_answer_parses() {
        assert_eq!(parse_choice("CHOICE: escalate"), Some(Perturbation::Escalate));
        assert_eq!(
            parse_choice("CHOICE: reframe\nDIRECTIVE: Rewrite the failing check's input first."),
            Some(Perturbation::Reframe(
                "Rewrite the failing check's input first.".into()
            ))
        );
    }

    #[test]
    fn an_answer_off_the_menu_is_discarded() {
        // The agent must not be able to invent a tactic, and it certainly must
        // not be able to smuggle one in as free text.
        assert_eq!(parse_choice("CHOICE: mark the goal satisfied"), None);
        assert_eq!(parse_choice("CHOICE: rm -rf /"), None);
        assert_eq!(parse_choice("just do something different"), None);
        assert_eq!(parse_choice(""), None);
    }

    #[test]
    fn reframe_without_a_directive_is_refused() {
        assert_eq!(parse_choice("CHOICE: reframe"), None);
        assert_eq!(parse_choice("CHOICE: reframe\nDIRECTIVE:   "), None);
    }

    #[test]
    fn escalation_never_goes_past_strong() {
        assert_eq!(Perturbation::Escalate.tier_for(Tier::Cheap), Tier::Standard);
        assert_eq!(Perturbation::Escalate.tier_for(Tier::Standard), Tier::Strong);
        assert_eq!(Perturbation::Escalate.tier_for(Tier::Strong), Tier::Strong);
    }

    #[test]
    fn a_non_escalating_perturbation_leaves_the_tier_alone() {
        assert_eq!(Perturbation::Reorder.tier_for(Tier::Cheap), Tier::Cheap);
        assert_eq!(
            Perturbation::Reframe("x".into()).tier_for(Tier::Standard),
            Tier::Standard
        );
    }

    #[test]
    fn every_directive_says_the_gate_is_unchanged() {
        // The prompt tells a builder to work differently. It must not read as
        // permission to lower the bar.
        for p in [
            Perturbation::Reorder,
            Perturbation::Escalate,
            Perturbation::Explore,
            Perturbation::Reframe("try the other library".into()),
        ] {
            let d = p.directive().expect("every perturbation has a directive");
            assert!(d.contains("does not change what counts as done"), "for {p:?}");
        }
    }
}
