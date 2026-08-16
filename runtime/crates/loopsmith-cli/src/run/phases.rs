//! Section I at runtime: which phase is open, and which nodes that lets run.
//!
//! A phase is **active** when every phase before it is complete, and
//! **complete** when its own nodes have all run and every goal they advance is
//! satisfied. A phase with no nodes gates nothing and is complete on sight —
//! there is nothing to wait for.
//!
//! Completion is read off the gate's verdicts, never off a node's own report.
//! That is the same rule as everywhere else here: the thing that decides
//! whether work is finished is not the thing that did the work.

use loopsmith_core::{LoopConfig, NodeSpec, Phase};
use loopsmith_gate::TargetVerdict;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct Phases {
    phases: Vec<Phase>,
    /// Nodes belonging to each phase.
    members: BTreeMap<String, Vec<String>>,
    /// Goals those nodes advance.
    goals: BTreeMap<String, BTreeSet<String>>,
    complete: BTreeSet<String>,
}

impl Phases {
    /// Build the phase graph, or fail the run before anything is dispatched.
    pub fn new(cfg: &LoopConfig) -> Result<Self, String> {
        let phases = cfg.execution_guidelines.phases()?;
        // Reuse the scheduler purely for its cycle and unknown-name checks: a
        // phase graph that cannot be ordered must not start.
        if !phases.is_empty() {
            loopsmith_graph::waves(&phases).map_err(|e| e.to_string())?;
        }

        let mut members: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut goals: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for p in &phases {
            members.entry(p.name.clone()).or_default();
            goals.entry(p.name.clone()).or_default();
        }
        for n in &cfg.graph.nodes {
            if let Some(stage) = &n.stage {
                members.entry(stage.clone()).or_default().push(n.id.clone());
                goals
                    .entry(stage.clone())
                    .or_default()
                    .extend(n.goals.iter().cloned());
            }
        }

        let mut out = Self {
            phases,
            members,
            goals,
            complete: BTreeSet::new(),
        };
        // Empty phases are complete before the first iteration, so they never
        // block the phases behind them.
        out.mark_vacuous_complete();
        Ok(out)
    }

    fn mark_vacuous_complete(&mut self) {
        let vacuous: Vec<String> = self
            .phases
            .iter()
            .filter(|p| {
                self.members
                    .get(&p.name)
                    .map(|m| m.is_empty())
                    .unwrap_or(true)
            })
            .map(|p| p.name.clone())
            .collect();
        self.complete.extend(vacuous);
    }

    /// A phase is active once everything it waits on is complete.
    pub fn is_active(&self, name: &str) -> bool {
        match self.phases.iter().find(|p| p.name == name) {
            Some(p) => p.depends_on.iter().all(|d| self.complete.contains(d)),
            // A stage nobody declared is refused by validation; at runtime,
            // fail closed rather than run work the author did not order.
            None => false,
        }
    }

    pub fn is_complete(&self, name: &str) -> bool {
        self.complete.contains(name)
    }

    /// May this node be dispatched right now?
    ///
    /// Nodes without a stage are always eligible. Gating work that never joined
    /// a phase would make adding section I a breaking change for every config
    /// that does not use it.
    pub fn eligible(&self, node: &NodeSpec) -> bool {
        match &node.stage {
            None => true,
            Some(stage) => self.is_active(stage) && !self.is_complete(stage),
        }
    }

    /// The standing instruction for a node's phase, to be added to its prompt.
    pub fn guideline_for(&self, node: &NodeSpec) -> Option<&str> {
        let stage = node.stage.as_ref()?;
        self.phases
            .iter()
            .find(|p| &p.name == stage)
            .map(|p| p.guideline.as_str())
    }

    /// Recompute completion after a gate ruling. Returns the phases that closed
    /// on this pass, so the caller can say so in the ledger.
    pub fn refresh(
        &mut self,
        verdicts: &BTreeMap<String, TargetVerdict>,
        dispatched: &BTreeSet<String>,
    ) -> Vec<String> {
        let mut closed = Vec::new();
        for p in &self.phases {
            if self.complete.contains(&p.name) {
                continue;
            }
            let members = self.members.get(&p.name).cloned().unwrap_or_default();
            let all_ran = members.iter().all(|m| dispatched.contains(m));
            let goals_met = self
                .goals
                .get(&p.name)
                .map(|gs| {
                    gs.iter()
                        .all(|g| verdicts.get(g).map(|v| v.satisfied).unwrap_or(false))
                })
                .unwrap_or(true);
            if all_ran && goals_met {
                closed.push(p.name.clone());
            }
        }
        self.complete.extend(closed.iter().cloned());
        closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(extra: &str) -> LoopConfig {
        let text = format!(
            r#"
name: t
goals:
  - name: g-gather
    description: a sufficiently long goal description
  - name: g-draft
    description: a sufficiently long goal description
validations:
  - target: g-gather
    name: v1
    mode: objective
    statement: always
    detector: {{ type: script, command: "true" }}
{extra}
"#
        );
        loopsmith_core::parse_str(&text, "test").expect("parses")
    }

    const TWO_PHASES: &str = r#"
execution_guidelines:
  items:
    - name: gather
      guideline: Collect sources. Write nothing yet.
    - name: draft
      guideline: Write only from what gather collected.
  dependency:
    - gather -> draft
graph:
  nodes:
    - id: search
      role: researcher
      instruction: find the sources the goal needs
      goals: [g-gather]
      stage: gather
    - id: write
      role: builder
      instruction: write the draft from the gathered sources
      goals: [g-draft]
      stage: draft
"#;

    fn verdict(target: &str, satisfied: bool) -> (String, TargetVerdict) {
        (
            target.to_string(),
            TargetVerdict {
                target: target.into(),
                satisfied,
                checks: vec![],
                passed: satisfied as usize,
                failed: !satisfied as usize,
                total: 1,
                reason: "test".into(),
            },
        )
    }

    fn node<'a>(cfg: &'a LoopConfig, id: &str) -> &'a NodeSpec {
        cfg.graph.nodes.iter().find(|n| n.id == id).unwrap()
    }

    #[test]
    fn a_later_phase_is_shut_until_the_earlier_one_closes() {
        let cfg = cfg(TWO_PHASES);
        let mut p = Phases::new(&cfg).unwrap();

        assert!(p.eligible(node(&cfg, "search")), "the first phase is open");
        assert!(
            !p.eligible(node(&cfg, "write")),
            "the second phase must wait for the first"
        );

        // `search` ran but its goal is not satisfied: still not enough.
        let dispatched: BTreeSet<String> = ["search".to_string()].into_iter().collect();
        let unsatisfied = [verdict("g-gather", false)].into_iter().collect();
        p.refresh(&unsatisfied, &dispatched);
        assert!(!p.eligible(node(&cfg, "write")), "an unmet goal keeps it shut");

        // Now the gate says the goal is satisfied.
        let satisfied = [verdict("g-gather", true)].into_iter().collect();
        let closed = p.refresh(&satisfied, &dispatched);
        assert_eq!(closed, vec!["gather"]);
        assert!(p.eligible(node(&cfg, "write")), "the next phase opens");
        assert!(
            !p.eligible(node(&cfg, "search")),
            "a closed phase stops re-running"
        );
    }

    #[test]
    fn a_node_without_a_stage_is_never_gated() {
        let cfg = cfg(r#"
execution_guidelines:
  items:
    - name: gather
      guideline: Collect sources. Write nothing yet.
graph:
  nodes:
    - id: always
      role: builder
      instruction: this node joined no phase at all
      goals: [g-gather]
"#);
        let p = Phases::new(&cfg).unwrap();
        assert!(p.eligible(node(&cfg, "always")));
    }

    #[test]
    fn a_phase_with_no_nodes_does_not_block_the_one_behind_it() {
        let cfg = cfg(r#"
execution_guidelines:
  items:
    - name: paperwork
      guideline: A phase nobody assigned a node to.
    - name: work
      guideline: The phase that actually does something.
  dependency:
    - paperwork -> work
graph:
  nodes:
    - id: build
      role: builder
      instruction: do the work described by the goal
      goals: [g-draft]
      stage: work
"#);
        let p = Phases::new(&cfg).unwrap();
        assert!(p.is_complete("paperwork"));
        assert!(p.eligible(node(&cfg, "build")));
    }

    #[test]
    fn independent_phases_are_both_open_at_once() {
        let cfg = cfg(r#"
execution_guidelines:
  items:
    - name: seo
      guideline: Work the search side of the problem.
    - name: social
      guideline: Work the social side of the problem.
graph:
  nodes:
    - id: a
      role: builder
      instruction: one half of the parallel work
      goals: [g-gather]
      stage: seo
    - id: b
      role: builder
      instruction: the other half of the parallel work
      goals: [g-draft]
      stage: social
"#);
        let p = Phases::new(&cfg).unwrap();
        assert!(p.eligible(node(&cfg, "a")));
        assert!(p.eligible(node(&cfg, "b")));
    }

    #[test]
    fn a_cyclic_phase_graph_refuses_to_start() {
        let cfg = cfg(r#"
execution_guidelines:
  items:
    - name: a
      guideline: the first of two phases in a cycle
    - name: b
      guideline: the second of two phases in a cycle
  dependency:
    - a -> b
    - b -> a
"#);
        let err = Phases::new(&cfg).unwrap_err();
        assert!(err.contains("cycle"), "got: {err}");
    }

    #[test]
    fn the_guideline_text_is_available_for_the_prompt() {
        let cfg = cfg(TWO_PHASES);
        let p = Phases::new(&cfg).unwrap();
        assert_eq!(
            p.guideline_for(node(&cfg, "search")),
            Some("Collect sources. Write nothing yet.")
        );
    }
}
