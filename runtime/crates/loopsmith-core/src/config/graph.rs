//! The execution graph: nodes are units of work, edges are real dependencies.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Produces the work. Most latitude, least constraint.
    Builder,
    /// Evaluates the builder's output against a written standard. Must not be
    /// the same provider instance as the builder it judges.
    Judge,
    /// Routes on the verdict and owns the stop condition.
    Manager,
    /// Argues the other side. Cheap insurance against consensus.
    Adversary,
    /// Gathers material without producing a deliverable.
    Researcher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// High volume, low judgment. Extraction, classification, formatting.
    Cheap,
    #[default]
    Standard,
    /// Low volume, high judgment. Final review, multi-hop reasoning.
    Strong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSpec {
    pub id: String,
    pub role: Role,
    /// What this node is for, in natural language. Tight descriptions produce
    /// tight output; vague ones produce whatever the model felt like.
    pub instruction: String,
    /// Node ids this node genuinely reads the output of. Only list an edge if
    /// the answer to "does this step read that step's output?" is yes.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Goals this node advances.
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub tier: Tier,
    /// Pin a provider; otherwise routing picks by tier.
    #[serde(default)]
    pub provider: Option<String>,
    /// Skills this node needs. Acquired per the skill policy.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Execution guideline (section I) this node belongs to. A node with a
    /// stage is not dispatched until that phase is active. A node without one
    /// is always eligible — unstaged work is not gated by a phase it never
    /// joined.
    #[serde(default)]
    pub stage: Option<String>,
    /// Relative cost weight used for critical-path calculation.
    #[serde(default = "one")]
    pub weight: f64,
    /// Run in its own git worktree. Required for parallel writers.
    #[serde(default)]
    pub isolated: bool,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GraphSpec {
    #[serde(default)]
    pub nodes: Vec<NodeSpec>,
    /// How much parallelism to use. `auto` derives it from the graph itself.
    #[serde(default)]
    pub concurrency: Concurrency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Concurrency {
    /// One node at a time.
    Sequential,
    /// Fixed width.
    Fixed { max_parallel: usize },
    /// Derived from the graph: widest wave, capped, and trimmed to the point
    /// where marginal Amdahl speedup still beats marginal cost.
    Auto {
        #[serde(default = "default_cap")]
        cap: usize,
        /// Stop adding workers once the next one buys less than this fraction
        /// of additional speedup.
        #[serde(default = "default_min_gain")]
        min_marginal_gain: f64,
    },
}

fn default_cap() -> usize {
    16
}
fn default_min_gain() -> f64 {
    0.05
}

impl Default for Concurrency {
    fn default() -> Self {
        Concurrency::Auto {
            cap: default_cap(),
            min_marginal_gain: default_min_gain(),
        }
    }
}
