//! The four bundles a loop config is grouped into.
//!
//! Until 1.0 the config was fourteen flat top-level keys, ten of them known by
//! a letter — `F` was `stop_gates`, `J` was `default_skills`. The letters came
//! from the document the format was derived from and meant nothing on their
//! own, so every author had to hold a lookup table in their head to read their
//! own file.
//!
//! The grouping below replaces that table with a question. Each bundle answers
//! one:
//!
//! - `intent` — what is this loop *for*, and how would we know it worked?
//! - `execution` — how does the work actually get done?
//! - `safety` — what must not happen, and when does this stop?
//! - `evolution` — how is this allowed to change itself?
//!
//! The test is whether a reader can place an unfamiliar key without being
//! told. `max_cost_usd` is a thing that stops a run, so it is under `safety`.
//! `carry_summaries` shapes how work is done, so it is under `execution`.
//!
//! No section changed meaning in the move. Every 0.3 key still parses, is
//! reported once as deprecated, and is relocated by the same transform
//! `loopsmith migrate` uses to rewrite the file — see [`super::legacy`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::constraints::Constraints;
use super::default_skills::DefaultSkill;
use super::gates::Gates;
use super::goals::Goal;
use super::graph::GraphSpec;
use super::guidelines::ExecutionGuidelines;
use super::info::InfoItem;
use super::memory::MemoryPolicy;
use super::protected::Protected;
use super::providers::ProviderRouting;
use super::recovery::Recovery;
use super::skills::SkillPolicy;
use super::success::SuccessScenario;
use super::triggers::TriggerPolicy;
use super::validation::Validation;
use super::work::WorkItem;

/// What the loop is for, and how anyone would know it worked.
///
/// This is the bundle a human writes first and reads most. Nothing in it
/// describes machinery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    /// Static facts every node receives. Formerly section A, `information`.
    ///
    /// Not called `context`: 0.3 already used that word for the memory policy,
    /// and a markdown config headed `## Context` would then mean one thing in
    /// an old file and another in a new one, with no way for the parser to
    /// tell which.
    #[serde(default)]
    pub background: Vec<InfoItem>,
    /// Manual work that must be finished before automation is allowed to
    /// start. Formerly section B — and still the one section that makes a
    /// fresh config refuse to run, on the principle that you cannot automate a
    /// process you cannot yet describe.
    #[serde(default)]
    pub prerequisites: Vec<WorkItem>,
    /// Named goals. Formerly section C.
    #[serde(default)]
    pub goals: Vec<Goal>,
    /// What counts as done. Formerly section E.
    #[serde(default)]
    pub success: Vec<SuccessScenario>,
}

/// How the work gets done.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    /// Nodes, edges, concurrency and join strategy.
    #[serde(default)]
    pub graph: GraphSpec,
    /// Provider routing. Every provider is a command template, so any CLI or
    /// HTTP endpoint reachable from a shell is usable without a Rust change.
    #[serde(default)]
    pub providers: ProviderRouting,
    /// Named phases with their own standing instruction and ordering.
    /// Formerly section I.
    #[serde(default)]
    pub phases: ExecutionGuidelines,
    /// Sub-agents installed before the loop starts. Formerly section J.
    #[serde(default)]
    pub default_skills: Vec<DefaultSkill>,
    /// How missing sub-agents are sourced, and how far they are trusted.
    #[serde(default)]
    pub skills: SkillPolicy,
    /// What is carried between iterations, and what survives the run.
    /// Absorbs the old top-level `context` block.
    #[serde(default)]
    pub memory: MemoryPolicy,
    /// What starts a run, and the guards against a run starting itself
    /// forever. Formerly section G.
    #[serde(default)]
    pub triggers: TriggerPolicy,
}

/// What must not happen, and when this stops.
///
/// Everything here is enforced by compiled code rather than asked of a model,
/// and every part of it is a [`Protected`] component by default — the loop can
/// propose changes to what it does, never to what constrains it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Safety {
    /// How each goal is checked. Formerly section D.
    ///
    /// Named `checks` rather than `validations` because the old name read as a
    /// synonym for `success`, and authors routinely put success criteria here
    /// and detectors there.
    #[serde(default)]
    pub checks: Vec<Validation>,
    /// The layered exits, plus the entry, approval and rollback checkpoints.
    /// Absorbs section F as `gates.stop`.
    #[serde(default)]
    pub gates: Gates,
    /// Rules, forbidden paths and commands, per-node ceilings, human
    /// checkpoints. Formerly section H.
    #[serde(default)]
    pub limits: Constraints,
    /// What to do about each class of failure.
    #[serde(default)]
    pub recovery: Recovery,
    /// What evolution may never touch.
    #[serde(default)]
    pub protected: Protected,
}

impl Safety {
    /// Blocking checks aimed at one target.
    pub fn blocking_checks_for(&self, target: &str) -> Vec<&Validation> {
        self.checks
            .iter()
            .filter(|v| v.target == target && v.blocking)
            .collect()
    }
}
