//! What the loop remembers, for how long, and what it takes to promote a
//! recollection into something reused.
//!
//! This section absorbs the old `context` block. That block answered only
//! "how much of the last few iterations does the next prompt carry" — a
//! question about one run. The namespaces below answer the other half: what
//! survives the run, and under what evidence.
//!
//! The distinction that matters is between *observed* and *believed*. An
//! episode is observed: it happened, and recording it costs nothing. A
//! procedure is believed: the loop is asserting this works, and acting on it
//! later. Promotion is the boundary between the two, and it is deliberately not
//! free — a loop that promotes its first success into a standing procedure has
//! learned a superstition.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::yes;

/// What it takes for a record to move from observed to reusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum Promotion {
    /// Never reused beyond the run that wrote it.
    Never,
    /// Reusable as soon as it is written. Only appropriate where writing is
    /// itself the evidence — a recorded failure mode, for instance.
    Automatic,
    /// Reusable once the same record has been independently corroborated this
    /// many times.
    RepeatedValidation {
        #[serde(default = "default_times")]
        times: u32,
    },
    /// Reusable only after a human says so.
    HumanApproval,
}

fn default_times() -> u32 {
    3
}

/// Retention and promotion policy for one namespace.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamespacePolicy {
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Drop records older than this. `None` keeps them for the life of the
    /// store, which is the right answer for episodic history and the wrong one
    /// for anything the loop acts on.
    #[serde(default)]
    pub retention_days: Option<u32>,
    #[serde(default = "default_promotion")]
    pub promotion: Promotion,
    /// A record below this confidence is never returned by retrieval.
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
    /// Refuse to write a record that cannot say where it came from.
    #[serde(default = "yes")]
    pub require_provenance: bool,
}

fn default_promotion() -> Promotion {
    Promotion::RepeatedValidation {
        times: default_times(),
    }
}
fn default_min_confidence() -> f64 {
    0.75
}

impl Default for NamespacePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: None,
            promotion: default_promotion(),
            min_confidence: default_min_confidence(),
            require_provenance: true,
        }
    }
}

/// The four kinds of thing worth remembering.
///
/// Splitting them is what makes differing retention defensible. Episodes are
/// cheap and disposable; procedures are expensive and load-bearing. One
/// retention policy across both either throws away what the loop learned or
/// keeps every transcript forever.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Namespaces {
    /// What happened: dispatches, outputs, verdicts. Written always, promoted
    /// never — an episode is evidence for a belief, not a belief.
    #[serde(default = "episodic_default")]
    pub episodic: NamespacePolicy,
    /// Stable facts about the domain the loop works in.
    #[serde(default)]
    pub semantic: NamespacePolicy,
    /// Reusable ways of doing things that have worked before.
    #[serde(default)]
    pub procedural: NamespacePolicy,
    /// Known failure modes and what got past them. Promoted automatically:
    /// having hit a wall is self-evidencing, and the cost of re-learning it is
    /// the whole reason the section exists.
    #[serde(default = "failure_default")]
    pub failure: NamespacePolicy,
}

fn episodic_default() -> NamespacePolicy {
    NamespacePolicy {
        promotion: Promotion::Never,
        require_provenance: false,
        ..NamespacePolicy::default()
    }
}

fn failure_default() -> NamespacePolicy {
    NamespacePolicy {
        promotion: Promotion::Automatic,
        ..NamespacePolicy::default()
    }
}

impl Default for Namespaces {
    fn default() -> Self {
        Self {
            episodic: episodic_default(),
            semantic: NamespacePolicy::default(),
            procedural: NamespacePolicy::default(),
            failure: failure_default(),
        }
    }
}

/// The whole memory policy: carry-forward within a run, namespaces across runs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryPolicy {
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
    /// Cross-run memory.
    #[serde(default)]
    pub namespaces: Namespaces,
    /// Most records a single retrieval may return.
    #[serde(default = "default_max_retrieved")]
    pub max_retrieved: usize,
}

fn default_carry() -> usize {
    2
}
fn default_max_chars() -> usize {
    1200
}
fn default_max_retrieved() -> usize {
    10
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            carry_summaries: default_carry(),
            summary_provider: None,
            max_summary_chars: default_max_chars(),
            namespaces: Namespaces::default(),
            max_retrieved: default_max_retrieved(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episodes_are_never_promoted_by_default() {
        // An episode is evidence, not a conclusion. Promoting one would let a
        // single run's transcript become a standing belief.
        assert_eq!(Namespaces::default().episodic.promotion, Promotion::Never);
    }

    #[test]
    fn failures_are_promoted_automatically_by_default() {
        // Re-learning a wall the loop already hit is the waste this exists to
        // prevent, and hitting it is its own evidence.
        assert_eq!(
            Namespaces::default().failure.promotion,
            Promotion::Automatic
        );
    }

    #[test]
    fn beliefs_need_corroboration_by_default() {
        for p in [
            Namespaces::default().semantic.promotion,
            Namespaces::default().procedural.promotion,
        ] {
            assert!(
                matches!(p, Promotion::RepeatedValidation { times } if times > 1),
                "a belief should need more than one sighting, got {p:?}"
            );
        }
    }
}
