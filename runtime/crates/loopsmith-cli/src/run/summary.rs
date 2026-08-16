//! Compressing an iteration down to what the next one needs to know.
//!
//! Two halves, and the split is load-bearing:
//!
//! - **Facts** are written by Rust, from the gate's verdicts and the episode
//!   record. They are always present, cost nothing, and cannot be wrong about
//!   what was satisfied.
//! - **Narrative** is optional prose from a model. It is allowed to be
//!   interesting and is never allowed to matter. The summariser is told
//!   explicitly that it cannot declare anything done, and even if it tried,
//!   nothing reads a summary to decide goal state — `loopsmith-gate` is still
//!   the only writer.
//!
//! What the next iteration receives is the last `context.carry_summaries`
//! summaries, not the episodes. That is the whole cost-control story: prompt
//! size stops growing with the run.

use loopsmith_core::{LoopConfig, Role, Tier};
use loopsmith_gate::TargetVerdict;
use loopsmith_memory::{now_ms, IterationSummary};
use loopsmith_provider::{dispatch, InvokeRequest};
use std::collections::BTreeMap;
use std::path::Path;

/// What the caller observed during one iteration, before it is compressed.
pub struct IterationFacts<'a> {
    pub run_id: &'a str,
    pub iteration: u32,
    /// `(node_id, provider_id, role, ok)` per dispatch.
    pub dispatched: &'a [(String, String, Role, bool)],
    pub verdicts: &'a BTreeMap<String, TargetVerdict>,
    pub previous: Option<&'a BTreeMap<String, TargetVerdict>>,
    pub tokens: u64,
    pub cost_usd: f64,
    pub phases_closed: &'a [String],
}

/// Build the deterministic half. No model is consulted and none can be wrong.
pub fn deterministic(f: &IterationFacts) -> IterationSummary {
    let ran = f.dispatched.len();
    let failed = f.dispatched.iter().filter(|(_, _, _, ok)| !ok).count();
    let satisfied = f.verdicts.values().filter(|v| v.satisfied).count();

    let headline = format!(
        "{ran} node(s) ran ({failed} failed); {satisfied}/{} target(s) satisfied.",
        f.verdicts.len()
    );

    let mut facts = Vec::new();

    if !f.dispatched.is_empty() {
        let names: Vec<String> = f
            .dispatched
            .iter()
            .map(|(id, provider, _, ok)| {
                if *ok {
                    format!("`{id}` via `{provider}`")
                } else {
                    format!("`{id}` FAILED")
                }
            })
            .collect();
        facts.push(format!("Ran: {}", names.join(", ")));
    }

    // Deltas are the part a node actually needs: what changed since last time
    // is the difference between "try something else" and "try harder".
    if let Some(prev) = f.previous {
        let mut gained = Vec::new();
        let mut lost = Vec::new();
        for (target, v) in f.verdicts {
            let was = prev.get(target).map(|p| p.satisfied).unwrap_or(false);
            match (was, v.satisfied) {
                (false, true) => gained.push(target.clone()),
                (true, false) => lost.push(target.clone()),
                _ => {}
            }
        }
        if !gained.is_empty() {
            facts.push(format!("Newly satisfied: {}", gained.join(", ")));
        }
        if !lost.is_empty() {
            // The gate can revoke. When it does, that is the single most
            // important thing the next iteration can be told.
            facts.push(format!(
                "REVOKED — these were satisfied and no longer are: {}",
                lost.join(", ")
            ));
        }
        if gained.is_empty() && lost.is_empty() {
            facts.push("No verdict changed from the previous iteration.".into());
        }
    }

    // Failing blocking checks, with the gate's own evidence line.
    for v in f.verdicts.values() {
        for c in v.checks.iter().filter(|c| c.blocking && !c.passed) {
            facts.push(format!(
                "Still failing `{}` ({}): {}",
                c.name,
                v.target,
                truncate(&c.evidence, 200)
            ));
        }
    }

    if !f.phases_closed.is_empty() {
        facts.push(format!("Phases completed: {}", f.phases_closed.join(", ")));
    }
    if f.tokens > 0 || f.cost_usd > 0.0 {
        facts.push(format!(
            "Spend so far: {} tokens, ${:.4}",
            f.tokens, f.cost_usd
        ));
    }

    IterationSummary {
        run_id: f.run_id.to_string(),
        iteration: f.iteration,
        headline,
        facts,
        narrative: None,
        created_ms: now_ms(),
    }
}

/// Ask a provider to add prose to a summary. Best effort: a failed or slow
/// summariser must not fail the iteration it is describing.
pub fn add_narrative(
    cfg: &LoopConfig,
    workdir: &Path,
    summary: &mut IterationSummary,
    outputs: &[(String, String)],
) {
    let Some(provider_id) = cfg.context.summary_provider.as_deref() else {
        return;
    };
    if cfg.provider(provider_id).is_none() {
        return;
    }

    let mut prompt = String::from(
        "Summarise one iteration of an automated loop for the next iteration to read.\n\n\
         Rules:\n\
         - Two to four sentences. No preamble.\n\
         - Say what was attempted and what went wrong, concretely.\n\
         - You are NOT deciding whether anything is finished. A separate \
           deterministic gate does that, and your text has no effect on it. \
           Do not write that a goal is met, complete, done, or satisfied.\n\n",
    );
    prompt.push_str(&format!("Established facts:\n{}\n", summary.headline));
    for f in &summary.facts {
        prompt.push_str(&format!("- {f}\n"));
    }
    if !outputs.is_empty() {
        prompt.push_str("\nNode output (truncated):\n");
        for (node, text) in outputs {
            prompt.push_str(&format!("\n[{node}]\n{}\n", truncate(text, 1500)));
        }
    }

    let req = InvokeRequest {
        node_id: "summary".into(),
        system: "You compress an agent loop's iteration into a short, factual note.".into(),
        prompt,
        tier: Tier::Cheap,
        workdir: workdir.to_path_buf(),
    };

    if let Ok((resp, _)) = dispatch(cfg, &req, Some(provider_id)) {
        let text = truncate(resp.output.trim(), cfg.context.max_summary_chars);
        if !text.is_empty() {
            summary.narrative = Some(text);
        }
    }
}

/// The last `carry` summaries, rendered for a prompt. Empty when carry-forward
/// is switched off or nothing has happened yet.
pub fn carry_forward(cfg: &LoopConfig, summaries: &[IterationSummary]) -> String {
    let carry = cfg.context.carry_summaries;
    if carry == 0 || summaries.is_empty() {
        return String::new();
    }
    let start = summaries.len().saturating_sub(carry);
    let mut s = String::from("## What earlier iterations already tried\n\n");
    for entry in &summaries[start..] {
        s.push_str(&entry.render());
        s.push('\n');
    }
    s.push_str("Do not repeat an approach that is listed above as already failing.\n\n");
    s
}

/// Cut on a character boundary, and say that it was cut.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}… [truncated]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_gate::CheckResult;

    fn verdict(target: &str, satisfied: bool) -> TargetVerdict {
        TargetVerdict {
            target: target.into(),
            satisfied,
            checks: vec![CheckResult {
                name: format!("{target}-check"),
                text: "the statement".into(),
                passed: satisfied,
                blocking: true,
                evidence: "the evidence line".into(),
            }],
            passed: satisfied as usize,
            failed: !satisfied as usize,
            total: 1,
            reason: "test".into(),
        }
    }

    fn map(pairs: &[(&str, bool)]) -> BTreeMap<String, TargetVerdict> {
        pairs
            .iter()
            .map(|(t, s)| (t.to_string(), verdict(t, *s)))
            .collect()
    }

    fn facts<'a>(
        verdicts: &'a BTreeMap<String, TargetVerdict>,
        previous: Option<&'a BTreeMap<String, TargetVerdict>>,
        dispatched: &'a [(String, String, Role, bool)],
    ) -> IterationFacts<'a> {
        IterationFacts {
            run_id: "r",
            iteration: 2,
            dispatched,
            verdicts,
            previous,
            tokens: 100,
            cost_usd: 0.01,
            phases_closed: &[],
        }
    }

    #[test]
    fn a_summary_records_what_ran_and_what_the_gate_said() {
        let v = map(&[("g1", true)]);
        let ran = vec![("build".to_string(), "echoer".to_string(), Role::Builder, true)];
        let s = deterministic(&facts(&v, None, &ran));

        assert!(s.headline.contains("1 node(s) ran (0 failed)"));
        assert!(s.headline.contains("1/1 target(s) satisfied"));
        assert!(s.facts.iter().any(|f| f.contains("`build` via `echoer`")));
        assert!(s.narrative.is_none(), "no provider configured, no prose");
    }

    #[test]
    fn a_revoked_goal_is_shouted_about() {
        // The gate can take `done` back, and the next iteration must not have
        // to infer that from silence.
        let before = map(&[("g1", true)]);
        let after = map(&[("g1", false)]);
        let s = deterministic(&facts(&after, Some(&before), &[]));
        assert!(
            s.facts.iter().any(|f| f.starts_with("REVOKED")),
            "got: {:?}",
            s.facts
        );
    }

    #[test]
    fn an_unchanged_iteration_says_so() {
        let before = map(&[("g1", false)]);
        let after = map(&[("g1", false)]);
        let s = deterministic(&facts(&after, Some(&before), &[]));
        assert!(s.facts.iter().any(|f| f.contains("No verdict changed")));
    }

    #[test]
    fn failing_blocking_checks_carry_their_evidence() {
        let v = map(&[("g1", false)]);
        let s = deterministic(&facts(&v, None, &[]));
        assert!(s
            .facts
            .iter()
            .any(|f| f.contains("Still failing `g1-check`") && f.contains("the evidence line")));
    }

    #[test]
    fn carry_forward_honours_the_configured_depth() {
        let cfg = loopsmith_core::parse_str(
            r#"
name: t
goals: [{ name: g1, description: a sufficiently long goal description }]
validations:
  - target: g1
    name: v
    mode: objective
    statement: it exists
    detector: { type: file_exists, path: out.txt }
context:
  carry_summaries: 2
"#,
            "test",
        )
        .unwrap();

        let all: Vec<IterationSummary> = (1..=5)
            .map(|i| IterationSummary {
                run_id: "r".into(),
                iteration: i,
                headline: format!("headline {i}"),
                facts: vec![],
                narrative: None,
                created_ms: 0,
            })
            .collect();

        let text = carry_forward(&cfg, &all);
        assert!(text.contains("headline 5"), "the newest must be included");
        assert!(text.contains("headline 4"));
        assert!(!text.contains("headline 3"), "only the last 2 are carried");
    }

    #[test]
    fn zero_disables_carry_forward_entirely() {
        let cfg = loopsmith_core::parse_str(
            r#"
name: t
goals: [{ name: g1, description: a sufficiently long goal description }]
validations:
  - target: g1
    name: v
    mode: objective
    statement: it exists
    detector: { type: file_exists, path: out.txt }
context:
  carry_summaries: 0
"#,
            "test",
        )
        .unwrap();
        let all = vec![IterationSummary {
            run_id: "r".into(),
            iteration: 1,
            headline: "headline".into(),
            facts: vec![],
            narrative: None,
            created_ms: 0,
        }];
        assert!(carry_forward(&cfg, &all).is_empty());
    }

    #[test]
    fn truncation_is_announced_and_lands_on_a_character_boundary() {
        let s = truncate("héllo wörld", 5);
        assert_eq!(s, "héllo… [truncated]");
        assert_eq!(truncate("short", 50), "short");
    }
}
