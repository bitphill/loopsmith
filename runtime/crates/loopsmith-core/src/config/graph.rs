//! The execution graph: nodes are units of work, edges are real dependencies.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// High volume, low judgment. Extraction, classification, formatting.
    Cheap,
    #[default]
    Standard,
    /// Low volume, high judgment. Final review, multi-hop reasoning.
    Strong,
}

/// How much of the machine a node is kept away from.
///
/// The ladder is real but not linear in cost: `Worktree` is nearly free and is
/// what makes parallel writers safe, while `Container` buys filesystem and
/// network separation at the price of a Docker daemon and an image pull.
///
/// `Container` degrades rather than fails. If Docker is not present the node
/// runs under `Worktree` and the run records a warning — a config written on a
/// machine with Docker must still work on one without, because the same loop
/// directory gets checked out on developer laptops, CI runners and servers, and
/// refusing to start there would make container isolation unusable in practice
/// rather than merely unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Isolation {
    /// Runs directly in the loop directory. Correct for a single writer or a
    /// read-only node.
    None {},
    /// Runs in its own git worktree, published back on success. Required for
    /// parallel writers.
    Worktree {},
    /// Runs in a container over its own worktree.
    Container {
        /// Image to run in. Defaults to the loop-wide image when unset.
        #[serde(default)]
        image: Option<String>,
        /// Allow the container to reach the network. Off by default: a node
        /// that does not need the network should not have it.
        #[serde(default)]
        network: bool,
    },
}

impl Default for Isolation {
    fn default() -> Self {
        Isolation::None {}
    }
}

impl Isolation {
    /// Whether this node needs a worktree of its own. Container isolation
    /// implies one, because the container mounts it.
    pub fn needs_worktree(&self) -> bool {
        matches!(self, Isolation::Worktree {} | Isolation::Container { .. })
    }

    /// What this becomes when Docker is unavailable.
    pub fn is_container(&self) -> bool {
        matches!(self, Isolation::Container { .. })
    }

    /// The image a container node runs in: its own, or the graph's default.
    /// `None` for a non-container node, or when neither names one.
    pub fn container_image<'a>(&'a self, graph_default: Option<&'a str>) -> Option<&'a str> {
        match self {
            Isolation::Container { image, .. } => image
                .as_deref()
                .or(graph_default)
                .filter(|s| !s.trim().is_empty()),
            _ => None,
        }
    }

    pub fn without_container(&self) -> Isolation {
        match self {
            Isolation::Container { .. } => Isolation::Worktree {},
            other => other.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    /// Execution phase (`execution.phases`) this node belongs to. A node with a
    /// stage is not dispatched until that phase is active. A node without one
    /// is always eligible — unstaged work is not gated by a phase it never
    /// joined.
    #[serde(default)]
    pub stage: Option<String>,
    /// Relative cost weight used for critical-path calculation.
    #[serde(default = "one")]
    pub weight: f64,
    /// How far this node is kept from the rest of the machine.
    ///
    /// The 0.3 spelling was `isolated: true`, meaning a worktree. That spelling
    /// still parses and still means [`Isolation::Worktree`]; the migration
    /// rewrites it.
    #[serde(default)]
    pub isolation: Isolation,
}

fn one() -> f64 {
    1.0
}

/// What it takes for a wave to count as finished.
///
/// The default waits for every node, which is the only correct answer when
/// later waves read every output. The other two exist for the fan-out shape the
/// reference calls out: several nodes attacking the same question, where the
/// run does not need all the answers to proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "strategy", rename_all = "snake_case", deny_unknown_fields)]
pub enum Join {
    /// Every node in the wave must finish.
    WaitForAll {},
    /// Proceed once this many nodes have finished successfully. Nodes still
    /// running are left to finish; their output is used if it arrives in time.
    Quorum { count: usize },
    /// Proceed as soon as any one node succeeds.
    FirstSuccess {},
}

impl Default for Join {
    fn default() -> Self {
        Join::WaitForAll {}
    }
}

impl Join {
    /// How many successes release the wave, given its width.
    pub fn required_successes(self, wave_width: usize) -> usize {
        match self {
            Join::WaitForAll {} => wave_width,
            // A quorum wider than the wave would never be reached and would
            // hang the run; clamping turns a config mistake into wait-for-all.
            Join::Quorum { count } => count.clamp(1, wave_width.max(1)),
            Join::FirstSuccess {} => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphSpec {
    #[serde(default)]
    pub nodes: Vec<NodeSpec>,
    /// How much parallelism to use. `auto` derives it from the graph itself.
    #[serde(default)]
    pub concurrency: Concurrency,
    /// What it takes for a wave to count as finished.
    #[serde(default)]
    pub join: Join,
    /// Default image for nodes whose isolation is `container` without one.
    #[serde(default)]
    pub container_image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Concurrency {
    /// One node at a time.
    Sequential {},
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quorum_wider_than_its_wave_does_not_hang_the_run() {
        // Asking for 9 successes from a 3-node wave can never be met. Clamping
        // turns an unreachable config into wait-for-all; not clamping would
        // block the run forever with no stop gate able to explain why.
        assert_eq!(Join::Quorum { count: 9 }.required_successes(3), 3);
    }

    #[test]
    fn a_zero_quorum_still_needs_one_success() {
        assert_eq!(Join::Quorum { count: 0 }.required_successes(3), 1);
    }

    #[test]
    fn container_isolation_implies_a_worktree() {
        // The container mounts the worktree, so asking for one without the
        // other would mount the live loop directory into the container.
        let c = Isolation::Container {
            image: None,
            network: false,
        };
        assert!(c.needs_worktree());
        assert_eq!(c.without_container(), Isolation::Worktree {});
    }

    #[test]
    fn degrading_a_non_container_isolation_changes_nothing() {
        assert_eq!(Isolation::None {}.without_container(), Isolation::None {});
        assert_eq!(Isolation::Worktree {}.without_container(), Isolation::Worktree {});
    }
}
