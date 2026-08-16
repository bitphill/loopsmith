//! Section F — the layered exits.

use super::yes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
