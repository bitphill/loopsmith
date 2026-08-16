//! The A–H config model.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Reserved target name meaning "the loop as a whole" rather than one goal.
pub const OVERALL: &str = "overall";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    /// Loop identity. Becomes the sled tree name and the generated skill name.
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,

    /// A — static context every node receives.
    #[serde(default)]
    pub information: Vec<InfoItem>,
    /// B — the manual work that must happen before automation is allowed.
    #[serde(default)]
    pub pre_execution: Vec<WorkItem>,
    /// C — named goals.
    pub goals: Vec<Goal>,
    /// D — how each goal is checked.
    pub validations: Vec<Validation>,
    /// E — what counts as success.
    #[serde(default)]
    pub success: Vec<SuccessScenario>,
    /// F — the layered exits.
    #[serde(default)]
    pub stop_gates: StopGates,
    /// G — time and event triggers.
    #[serde(default)]
    pub schedules: Vec<Trigger>,
    /// H — constraints applied per node or globally.
    #[serde(default)]
    pub constraints: Constraints,

    /// Execution graph. Nodes are units of work; edges are real dependencies.
    #[serde(default)]
    pub graph: GraphSpec,
    /// Provider routing. Every provider is a command template, so any CLI or
    /// HTTP endpoint reachable from a shell is usable without a Rust change.
    #[serde(default)]
    pub providers: ProviderRouting,
    /// Sub-agent acquisition policy.
    #[serde(default)]
    pub skills: SkillPolicy,
}

fn default_version() -> String {
    "0.1.0".into()
}

// ---------------------------------------------------------------- A

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoItem {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub note: Option<String>,
}

// ---------------------------------------------------------------- B

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub step: String,
    /// Must be true before the loop is allowed to run. This encodes the
    /// corpus rule that you cannot automate a process you cannot describe.
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub evidence: Option<String>,
}

// ---------------------------------------------------------------- C

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub name: String,
    /// Natural language. Subjective phrasing is allowed here; the validation
    /// is what has to be checkable.
    pub description: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub priority: Option<u32>,
}

// ---------------------------------------------------------------- D

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Subjective,
    Objective,
    Percentage,
}

/// How a validation is actually decided. Ordered by the independence ladder
/// from the cheat sheet: `Judge` is rung 3, everything else is rung 4.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Detector {
    /// Run a command; exit code 0 passes. The strongest detector available.
    Script {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        expect_exit: Option<i32>,
    },
    /// A path must exist (optionally non-empty).
    FileExists {
        path: String,
        #[serde(default)]
        non_empty: bool,
    },
    /// A regex must match the named artifact.
    RegexMatch { artifact: String, pattern: String },
    /// A numeric metric compared against a threshold.
    Threshold {
        metric: String,
        op: CompareOp,
        value: f64,
    },
    /// A model verdict. Requires a judge whose provider differs from the
    /// builder's, otherwise the gate refuses it as non-independent.
    Judge {
        /// Name the external standard the judge checks against. Naming a
        /// standard is what turns an opinion into a check.
        standard: String,
        #[serde(default)]
        min_score: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl CompareOp {
    pub fn apply(self, lhs: f64, rhs: f64) -> bool {
        match self {
            CompareOp::Gt => lhs > rhs,
            CompareOp::Gte => lhs >= rhs,
            CompareOp::Lt => lhs < rhs,
            CompareOp::Lte => lhs <= rhs,
            CompareOp::Eq => (lhs - rhs).abs() < f64::EPSILON,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validation {
    /// Goal name, or `overall`.
    pub target: String,
    pub name: String,
    pub mode: Mode,
    /// Natural-language statement of what is being checked.
    pub statement: String,
    pub detector: Detector,
    /// A validation that must pass for the target to be satisfied. Non-blocking
    /// validations are recorded but do not hold the gate shut.
    #[serde(default = "yes")]
    pub blocking: bool,
}

fn yes() -> bool {
    true
}

// ---------------------------------------------------------------- E

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessScenario {
    /// Goal name, or `overall`.
    pub target: String,
    pub name: String,
    pub mode: Mode,
    pub statement: String,
    /// Fraction of blocking validations that must pass, when mode is
    /// `percentage`. Ignored otherwise.
    #[serde(default)]
    pub threshold: Option<f64>,
}

// ---------------------------------------------------------------- F

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopGates {
    /// Hard ceiling on whole-loop iterations.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    /// Per-node revision ceiling before escalation.
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

// ---------------------------------------------------------------- G

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Five-field cron expression.
    Cron { expr: String },
    /// Fire every N seconds. Timezone-independent, which makes it the right
    /// choice for cadence that does not need to land at a wall-clock time.
    Interval { seconds: u64 },
    /// Fire when a path changes.
    FileChange { path: String },
    /// Fire when a named upstream goal becomes satisfied.
    GoalSatisfied { goal: String },
    /// Fire on demand only.
    Manual,
}

// ---------------------------------------------------------------- H

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Constraints {
    /// Applied to every node unless overridden.
    #[serde(default)]
    pub global: ConstraintSet,
    /// Keyed by node id.
    #[serde(default)]
    pub per_node: BTreeMap<String, ConstraintSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConstraintSet {
    /// Literal rules injected into the node prompt.
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
    #[serde(default)]
    pub forbidden_commands: Vec<String>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub max_seconds: Option<u64>,
    /// Any action matching these requires a human before proceeding. Bezos
    /// Type 1: irreversible decisions do not get made at machine speed.
    #[serde(default)]
    pub human_checkpoint: Vec<String>,
}

impl ConstraintSet {
    /// The frozen git rule set that made large parallel runs safe in the
    /// corpus. Emitted into every parallel node unless the author opts out.
    pub fn frozen_git_rules() -> Vec<String> {
        vec![
            "Never git stash. Never git reset.".into(),
            "No git command except committing a specific file.".into(),
            "No slow commands before the test phase.".into(),
        ]
    }

    /// Merge a global set with a node override; node rules append, node
    /// limits win where present.
    pub fn merged(global: &ConstraintSet, node: Option<&ConstraintSet>) -> ConstraintSet {
        let mut out = global.clone();
        if let Some(n) = node {
            out.rules.extend(n.rules.iter().cloned());
            out.forbidden_paths.extend(n.forbidden_paths.iter().cloned());
            out.forbidden_commands
                .extend(n.forbidden_commands.iter().cloned());
            out.human_checkpoint
                .extend(n.human_checkpoint.iter().cloned());
            if n.max_tokens.is_some() {
                out.max_tokens = n.max_tokens;
            }
            if n.max_seconds.is_some() {
                out.max_seconds = n.max_seconds;
            }
        }
        out
    }
}

// ---------------------------------------------------------------- graph

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

// ---------------------------------------------------------------- providers

/// Every provider is a command template. `{prompt}`, `{system}`, `{model}`
/// and `{tier}` are substituted before spawn. This keeps BYOK support out of
/// the Rust build entirely: if you can invoke it from a shell, loopsmith can
/// route to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSpec {
    pub id: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub tiers: Vec<Tier>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Environment variables that must be present. Values are never read by
    /// loopsmith, only checked for presence, so keys stay out of the ledger.
    #[serde(default)]
    pub requires_env: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    /// Send the prompt on stdin instead of substituting into args.
    #[serde(default)]
    pub prompt_on_stdin: bool,
    /// Regex with one capture group pulling a token count out of the
    /// provider's own output. Without it, usage is estimated from character
    /// count, which is enough to make a budget ceiling real but is not exact.
    #[serde(default)]
    pub usage_regex: Option<String>,
    /// Price per thousand tokens, for the cost ceiling.
    #[serde(default)]
    pub cost_per_1k_tokens: Option<f64>,
}

/// Provider families. Each variant accepts the spellings people actually
/// write, because a config that rejects `openai` in favour of `open_ai` is a
/// config that wastes the author's afternoon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[serde(rename = "claude_code", alias = "claude-code", alias = "claude", alias = "claudecode")]
    ClaudeCode,
    #[serde(alias = "Ollama")]
    Ollama,
    #[serde(rename = "grok_cli", alias = "grok-cli", alias = "grok")]
    GrokCli,
    #[serde(rename = "grok_build", alias = "grok-build")]
    GrokBuild,
    #[serde(alias = "Hermes")]
    Hermes,
    #[serde(rename = "openai", alias = "open_ai", alias = "open-ai", alias = "OpenAI")]
    OpenAi,
    #[serde(alias = "google_gemini", alias = "google-gemini", alias = "Gemini")]
    Gemini,
    /// Any OpenAI-compatible or bespoke endpoint driven by a command.
    #[serde(alias = "BYOK", alias = "custom")]
    Byok,
    /// An MCP server spoken to over stdio.
    #[serde(alias = "MCP")]
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderRouting {
    #[serde(default)]
    pub providers: Vec<ProviderSpec>,
    /// Ordered fallback chain per tier. First reachable provider wins.
    #[serde(default)]
    pub cascade: BTreeMap<String, Vec<String>>,
    /// A judge must not run on the provider that produced the work.
    #[serde(default = "yes")]
    pub enforce_judge_independence: bool,
}

// ---------------------------------------------------------------- skills

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPolicy {
    /// Order in which a missing sub-agent is sourced.
    #[serde(default = "default_acquisition")]
    pub acquisition_order: Vec<AcquisitionSource>,
    /// Where auto-created skills land before a human promotes them.
    #[serde(default = "default_quarantine")]
    pub quarantine_dir: String,
    /// Minimum stars before a marketplace skill is eligible.
    #[serde(default = "default_min_stars")]
    pub min_marketplace_stars: u64,
    #[serde(default = "yes")]
    pub require_human_promotion: bool,
    /// Try a candidate sub-agent that is *not* in the config, so the loop can
    /// discover that something helps rather than only confirming what it was
    /// told. Off by default: exploration spends real money.
    #[serde(default)]
    pub explore: bool,
    /// Candidates to try when exploring, in order. Each is trialled until it
    /// has enough runs to judge.
    #[serde(default)]
    pub explore_candidates: Vec<String>,
    /// Trials needed before a candidate can be proposed or dismissed.
    #[serde(default = "default_min_trials")]
    pub min_trials: usize,
}

fn default_min_trials() -> usize {
    3
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionSource {
    Installed,
    Marketplace,
    Generate,
}

impl Default for SkillPolicy {
    fn default() -> Self {
        Self {
            acquisition_order: default_acquisition(),
            quarantine_dir: default_quarantine(),
            min_marketplace_stars: default_min_stars(),
            require_human_promotion: true,
            explore: false,
            explore_candidates: vec![],
            min_trials: default_min_trials(),
        }
    }
}

fn default_acquisition() -> Vec<AcquisitionSource> {
    vec![
        AcquisitionSource::Installed,
        AcquisitionSource::Marketplace,
        AcquisitionSource::Generate,
    ]
}
fn default_quarantine() -> String {
    "generated-skills".into()
}
fn default_min_stars() -> u64 {
    100
}

impl LoopConfig {
    pub fn goal_names(&self) -> Vec<&str> {
        self.goals.iter().map(|g| g.name.as_str()).collect()
    }

    pub fn blocking_validations_for(&self, target: &str) -> Vec<&Validation> {
        self.validations
            .iter()
            .filter(|v| v.target == target && v.blocking)
            .collect()
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderSpec> {
        self.providers.providers.iter().find(|p| p.id == id)
    }

    /// Resolve a tier to the ordered list of provider ids to try.
    pub fn cascade_for(&self, tier: Tier) -> Vec<&ProviderSpec> {
        let key = match tier {
            Tier::Cheap => "cheap",
            Tier::Standard => "standard",
            Tier::Strong => "strong",
        };
        if let Some(ids) = self.providers.cascade.get(key) {
            return ids.iter().filter_map(|id| self.provider(id)).collect();
        }
        self.providers
            .providers
            .iter()
            .filter(|p| p.tiers.is_empty() || p.tiers.contains(&tier))
            .collect()
    }
}
