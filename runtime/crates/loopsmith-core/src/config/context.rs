//! What each iteration is allowed to remember.
//!
//! A loop that re-sends every prior episode grows its own prompt without bound
//! and bills accordingly; a loop that sends nothing produces the byte-identical
//! prompt it already failed with. Neither is acceptable for a run measured in
//! weeks, so each iteration is compressed to a summary and only the last few
//! summaries are carried forward.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPolicy {
    /// How many previous iteration summaries a node's prompt carries.
    ///
    /// `0` disables carry-forward entirely. The default of 2 is enough for a
    /// node to see what it just tried and what it tried before that, which is
    /// what "do not repeat yourself" needs, without the prompt growing with the
    /// run.
    #[serde(default = "default_carry")]
    pub carry_summaries: usize,
    /// Provider id used to write the optional narrative half of a summary.
    ///
    /// Omit it and summaries are still written — the deterministic facts are
    /// always there. This only buys prose, and prose costs tokens every
    /// iteration, so it is opt-in.
    #[serde(default)]
    pub summary_provider: Option<String>,
    /// Ceiling on the narrative, in characters. A summary that grows without
    /// limit defeats the purpose of having one.
    #[serde(default = "default_max_chars")]
    pub max_summary_chars: usize,
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self {
            carry_summaries: default_carry(),
            summary_provider: None,
            max_summary_chars: default_max_chars(),
        }
    }
}

fn default_carry() -> usize {
    2
}
fn default_max_chars() -> usize {
    1200
}
