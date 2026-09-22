//! The layered exits, and the three checkpoints either side of them.
//!
//! `stop` is the original section F: the ceilings that end a run. The three
//! lists beside it answer questions a ceiling cannot — may this run start at
//! all, does this need a human before it proceeds, and has something gone
//! badly enough to undo.
//!
//! All four reuse [`Detector`] rather than introducing an expression language.
//! That is the whole design: a gate condition is the same kind of object as a
//! validation condition, so it is evaluated by the same compiled code, obeys
//! the same independence rules, and an author who has learned one has learned
//! both. The reference specification models gate conditions as strings like
//! `"risk_score < 0.40"`; parsing those would mean a new evaluator, a new
//! failure surface, and a second answer to "how is a condition decided".

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::validation::Detector;
use super::yes;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopGates {
    /// Hard ceiling on whole-loop iterations.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    /// Per-node revision ceiling. A node that has been dispatched this many
    /// times without its goals being satisfied stops being dispatched, so one
    /// stuck node cannot burn the whole iteration budget.
    #[serde(default = "default_max_revisions")]
    pub max_revisions_per_node: u32,
    /// Wall-clock budget for the whole run.
    #[serde(default)]
    pub max_wall_clock_seconds: Option<u64>,
    /// Token budget for the whole run, summed across providers.
    #[serde(default)]
    pub max_tokens: Option<u64>,
    /// Currency budget for the whole run.
    #[serde(default)]
    pub max_cost_usd: Option<f64>,
    /// Halt when this many consecutive iterations produce no measurable
    /// change. Jidoka: stop the line rather than spin.
    #[serde(default = "default_no_progress")]
    pub no_progress_iterations: u32,
    /// Perturb the run after this many stalled iterations, instead of waiting
    /// to halt at `no_progress_iterations`.
    ///
    /// Must be strictly less than `no_progress_iterations`: the point is to try
    /// something different *before* giving up, and a threshold at or past the
    /// halt point never fires. Leave it unset to halt without ever varying —
    /// perturbation costs a provider call and changes what the loop does, so it
    /// is opt-in.
    #[serde(default)]
    pub no_progress_iterations_randomness: Option<u32>,
    /// Stop as soon as every `overall` success scenario is met.
    #[serde(default = "yes")]
    pub stop_on_overall_success: bool,
}

impl Default for StopGates {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            max_revisions_per_node: default_max_revisions(),
            max_wall_clock_seconds: None,
            max_tokens: None,
            max_cost_usd: None,
            no_progress_iterations: default_no_progress(),
            no_progress_iterations_randomness: None,
            stop_on_overall_success: true,
        }
    }
}

fn default_max_iterations() -> u32 {
    10
}
fn default_max_revisions() -> u32 {
    3
}
fn default_no_progress() -> u32 {
    3
}

/// What happens when a gate rule does not pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateOutcome {
    /// End the run. Nothing further is dispatched.
    Stop,
    /// Halt and record an escalation for a human to answer. Resumable.
    Escalate,
    /// Halt without an escalation record. Resumable.
    Pause,
    /// Restore the last good checkpoint and continue from there.
    Rollback,
    /// Record it and carry on. The only non-blocking outcome.
    Warn,
}

impl GateOutcome {
    /// Whether this outcome stops the run from proceeding.
    pub fn is_blocking(self) -> bool {
        !matches!(self, GateOutcome::Warn)
    }
}

/// One checkpoint, decided by a detector.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GateRule {
    pub id: String,
    /// Natural-language statement of what this gate is for. Shown verbatim
    /// when the gate blocks, so it is the whole explanation a stopped operator
    /// gets — write it for them, not for the author.
    pub statement: String,
    pub detector: Detector,
    #[serde(default = "default_on_fail")]
    pub on_fail: GateOutcome,
}

fn default_on_fail() -> GateOutcome {
    GateOutcome::Stop
}

/// Every gate the run is subject to.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Gates {
    /// The ceilings that end a run. Formerly the whole of section F.
    #[serde(default)]
    pub stop: StopGates,
    /// Checked once, before the first iteration. A failing entry gate means
    /// the run never starts — which is the cheapest possible failure.
    #[serde(default)]
    pub entry: Vec<GateRule>,
    /// Checked after each iteration. A failing approval gate halts for a human
    /// rather than ending the run.
    #[serde(default)]
    pub approval: Vec<GateRule>,
    /// Checked after each iteration. A failing rollback gate restores the last
    /// good checkpoint — for the case where continuing is worse than undoing.
    #[serde(default)]
    pub rollback: Vec<GateRule>,
}

impl Gates {
    /// Every rule across the three lists, with the list it came from.
    pub fn rules(&self) -> impl Iterator<Item = (GateKind, &GateRule)> {
        self.entry
            .iter()
            .map(|r| (GateKind::Entry, r))
            .chain(self.approval.iter().map(|r| (GateKind::Approval, r)))
            .chain(self.rollback.iter().map(|r| (GateKind::Rollback, r)))
    }
}

/// Which list a rule came from, and therefore when it is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    Entry,
    Approval,
    Rollback,
}

impl GateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            GateKind::Entry => "entry",
            GateKind::Approval => "approval",
            GateKind::Rollback => "rollback",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gate_defaults_to_blocking() {
        // The safe default. A gate the author forgot to annotate should stop
        // the run, not shrug.
        assert_eq!(default_on_fail(), GateOutcome::Stop);
        assert!(default_on_fail().is_blocking());
    }

    #[test]
    fn only_warn_is_non_blocking() {
        for o in [
            GateOutcome::Stop,
            GateOutcome::Escalate,
            GateOutcome::Pause,
            GateOutcome::Rollback,
        ] {
            assert!(o.is_blocking(), "{o:?} must block");
        }
        assert!(!GateOutcome::Warn.is_blocking());
    }
}
