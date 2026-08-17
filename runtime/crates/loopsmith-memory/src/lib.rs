//! Shared memory plane.
//!
//! Everything that must survive a crash, a schedule boundary, or a context
//! reset lives here: episodes (what a node did), goal state (what the gate has
//! ruled), the ledger (an append-only audit trail), checkpoints (where to
//! resume), and per-goal scratchpads (reasoning carried between iterations).
//!
//! Two design rules come straight from the corpus:
//!
//! - **Validate before writing.** Bad data compounds — one wrong record
//!   becomes a retrieved "fact", which becomes reasoning, which becomes
//!   another record. [`Store::put_episode`] rejects malformed input rather
//!   than storing it.
//! - **The store is a trait.** `sled` is the shipped backend but is
//!   effectively frozen upstream, so callers depend on [`Store`] and a
//!   different engine can be swapped in without touching them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub mod sled_store;
pub use sled_store::SledStore;

#[derive(Debug, thiserror::Error)]
pub enum MemError {
    #[error("backend error: {0}")]
    Backend(String),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("rejected write: {0}")]
    Rejected(String),
    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, MemError>;

/// Milliseconds since the Unix epoch. Stored as a number so the ledger stays
/// sortable without a date parser. Re-exported from `loopsmith-util` so the
/// whole workspace reads one clock.
pub use loopsmith_util::now_ms;

/// What one node did on one iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub run_id: String,
    pub iteration: u32,
    pub node_id: String,
    pub role: String,
    /// Provider that actually served the call — recorded so the gate can
    /// verify a judge did not run on the same provider as its builder.
    pub provider_id: String,
    pub prompt_digest: String,
    pub output: String,
    #[serde(default)]
    pub tokens: Option<u64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    pub created_ms: u64,
}

impl Episode {
    fn check(&self) -> Result<()> {
        if self.run_id.trim().is_empty() {
            return Err(MemError::Rejected("episode.run_id is empty".into()));
        }
        if self.node_id.trim().is_empty() {
            return Err(MemError::Rejected("episode.node_id is empty".into()));
        }
        if self.provider_id.trim().is_empty() {
            return Err(MemError::Rejected("episode.provider_id is empty".into()));
        }
        Ok(())
    }
}

/// The gate's ruling on one target. Only `loopsmith-gate` should construct
/// these with `satisfied: true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalState {
    pub target: String,
    pub satisfied: bool,
    pub passed: usize,
    pub failed: usize,
    pub total: usize,
    /// Human-readable reason, especially when not satisfied.
    pub reason: String,
    /// Iteration at which this ruling was made.
    pub iteration: u32,
    pub updated_ms: u64,
}

impl GoalState {
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.passed as f64 / self.total as f64
        }
    }
}

/// Append-only audit record. Every stop-gate trigger lands here, not just
/// successes — a node that hits its ceiling constantly is a signal, and that
/// signal is invisible if only completions are logged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub run_id: String,
    pub iteration: u32,
    pub kind: LedgerKind,
    pub detail: String,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub tokens: Option<u64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    pub created_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerKind {
    RunStarted,
    IterationStarted,
    NodeDispatched,
    NodeSucceeded,
    NodeFailed,
    GateEvaluated,
    GoalSatisfied,
    GoalRevoked,
    SkillAcquired,
    ProposalWritten,
    StopGateTriggered,
    RunFinished,
}

/// One observation of "did this skill help?".
///
/// This is the substrate of self-evolution. A loop cannot know which
/// sub-agents earn their place by reasoning about it — it has to try them and
/// watch the gate. Each trial pairs a skill with the gate outcome that
/// followed, so the ranking is grounded in verdicts rather than in the
/// model's opinion of its own tooling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTrial {
    pub run_id: String,
    pub iteration: u32,
    pub node_id: String,
    pub skill: String,
    /// installed | marketplace | generated
    pub source: String,
    /// Blocking pass rate for this node's goals after the node ran.
    pub pass_rate: f64,
    /// Did every goal this node advances end the iteration satisfied?
    pub satisfied: bool,
    #[serde(default)]
    pub tokens: Option<u64>,
    pub created_ms: u64,
}

/// A change the loop wants to make but may not apply itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub run_id: String,
    pub iteration: u32,
    pub kind: ProposalKind,
    /// What it concerns — a node id, a skill name, a goal name.
    pub subject: String,
    pub rationale: String,
    /// Suggested config fragment, as YAML.
    #[serde(default)]
    pub patch: Option<String>,
    pub created_ms: u64,
    /// When this proposal stops being worth reading, or `None` for never.
    ///
    /// A proposal is evidence about a moment: "this skill correlated with
    /// satisfied goals across the last three iterations" is a claim about a
    /// graph and a config that have both since been edited by hand. Nothing
    /// expires a proposal automatically and nothing deletes one — the record of
    /// what the loop wanted is worth keeping — but a reviewer needs to be told
    /// which suggestions are answering a question nobody is asking any more.
    ///
    /// `#[serde(default)]` so proposals already written to a sled store by an
    /// earlier build still deserialise.
    #[serde(default)]
    pub expires_ms: Option<u64>,
}

impl Proposal {
    /// How long a proposal stays current, by kind.
    ///
    /// The two that name a specific skill go stale fastest: a marketplace
    /// suggestion is about a listing that may be gone, and an adopt/drop
    /// recommendation is about a skill set the reviewer has probably already
    /// changed. A criteria change never expires, because it is a question about
    /// the goal rather than an observation about a run.
    pub fn default_lifetime_ms(kind: ProposalKind) -> Option<u64> {
        const DAY: u64 = 24 * 60 * 60 * 1000;
        match kind {
            ProposalKind::TrySkill => Some(7 * DAY),
            ProposalKind::AdoptSkill | ProposalKind::DropSkill => Some(30 * DAY),
            ProposalKind::ReshapeGraph => Some(30 * DAY),
            // Goals, validations, and success criteria. Always a proposal,
            // never an action, and never quietly aged out of the list.
            ProposalKind::ChangeCriteria => None,
        }
    }

    /// Stamp the default expiry for this proposal's kind, leaving one that was
    /// set explicitly alone.
    pub fn with_default_expiry(mut self) -> Self {
        if self.expires_ms.is_none() {
            self.expires_ms = Self::default_lifetime_ms(self.kind)
                .and_then(|life| self.created_ms.checked_add(life));
        }
        self
    }

    /// Whether this proposal was already stale at `now_ms`.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.expires_ms.is_some_and(|e| now_ms >= e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    /// Keep a skill that correlates with satisfied goals.
    AdoptSkill,
    /// Drop a skill that does not.
    DropSkill,
    /// Try a skill found on the marketplace.
    TrySkill,
    /// Reshape the graph after repeated node failure.
    ReshapeGraph,
    /// Anything touching goals, validations, or success criteria. Always a
    /// proposal, never an action.
    ChangeCriteria,
}

/// Where to pick up after a crash or a scheduled pause.
///
/// "Where to pick up" includes the stop gates' own accounting. A loop that
/// resumes often would otherwise be handed a fresh revision budget and a
/// no-progress counter of zero every time, so a run that is going nowhere could
/// never reach the halt that exists to stop it — the ceilings would apply only
/// to runs that never paused.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub run_id: String,
    pub iteration: u32,
    /// Node ids completed at any point in this run, not in one iteration.
    ///
    /// It is only ever appended to, and phase completion is computed from it,
    /// so it has to mean "has this node ever run" rather than "did it run just
    /// now" — otherwise a phase would reopen every iteration.
    pub completed_nodes: Vec<String>,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub started_ms: u64,
    pub updated_ms: u64,

    /// How many times each node has run with its goals still unsatisfied. This
    /// is what `max_revisions_per_node` bounds, and it survives a resume so the
    /// ceiling cannot be refunded by pausing.
    #[serde(default)]
    pub revisions: BTreeMap<String, u32>,
    /// Consecutive iterations in which no verdict moved.
    #[serde(default)]
    pub stale_iterations: u32,
    /// The rulings' signature at the last iteration, so the first one after a
    /// resume is compared against something instead of always looking like
    /// progress.
    #[serde(default)]
    pub last_signature: String,
    /// Last iteration's gate rulings, serialised.
    ///
    /// Held as text rather than as the verdict type because the gate crate
    /// depends on this one, and that direction is what keeps the only
    /// constructor of a satisfied [`GoalState`] inside the gate. A resumed run
    /// reads this so its first summary can report deltas rather than claiming
    /// everything is new.
    #[serde(default)]
    pub verdicts_json: Option<String>,
}

impl Checkpoint {
    /// A checkpoint for a run that has not started yet.
    pub fn new(run_id: &str) -> Self {
        Self {
            run_id: run_id.to_string(),
            iteration: 0,
            completed_nodes: vec![],
            tokens_used: 0,
            cost_usd: 0.0,
            started_ms: now_ms(),
            updated_ms: now_ms(),
            revisions: BTreeMap::new(),
            stale_iterations: 0,
            last_signature: String::new(),
            verdicts_json: None,
        }
    }
}

/// What one iteration amounted to, compressed.
///
/// This is the record that makes a long run affordable. Without it, iteration
/// N+1 either re-sends every prior episode (which grows without bound) or sends
/// nothing at all (which is what the runtime did before, and is why a stalled
/// loop kept producing the byte-identical prompt it had already failed with).
///
/// `facts` is written by Rust from the gate's own verdicts and is always
/// present. `narrative` is optional prose from a model. The split matters: a
/// model may describe what happened, but the record of *what was satisfied* is
/// never something a model wrote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationSummary {
    pub run_id: String,
    pub iteration: u32,
    /// One line: the shape of the iteration.
    pub headline: String,
    /// Deterministic bullet facts, derived from verdicts and episodes.
    pub facts: Vec<String>,
    /// Optional model-written prose. Never load-bearing.
    pub narrative: Option<String>,
    pub created_ms: u64,
}

impl IterationSummary {
    /// Render for injection into a later prompt.
    pub fn render(&self) -> String {
        let mut s = format!("### Iteration {}\n{}\n", self.iteration, self.headline);
        for f in &self.facts {
            s.push_str(&format!("- {f}\n"));
        }
        if let Some(n) = &self.narrative {
            if !n.trim().is_empty() {
                s.push_str(&format!("\n{}\n", n.trim()));
            }
        }
        s
    }
}

/// Backend-agnostic persistence contract.
pub trait Store: Send + Sync {
    fn put_episode(&self, ep: &Episode) -> Result<u64>;
    fn episodes(&self, run_id: &str) -> Result<Vec<Episode>>;

    fn set_goal_state(&self, run_id: &str, st: &GoalState) -> Result<()>;
    fn goal_state(&self, run_id: &str, target: &str) -> Result<Option<GoalState>>;
    fn goal_states(&self, run_id: &str) -> Result<BTreeMap<String, GoalState>>;

    fn append_ledger(&self, entry: &LedgerEntry) -> Result<u64>;
    fn ledger(&self, run_id: &str) -> Result<Vec<LedgerEntry>>;

    fn save_checkpoint(&self, cp: &Checkpoint) -> Result<()>;
    fn checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>>;

    fn set_scratchpad(&self, run_id: &str, key: &str, value: &str) -> Result<()>;
    fn scratchpad(&self, run_id: &str, key: &str) -> Result<Option<String>>;

    fn put_summary(&self, s: &IterationSummary) -> Result<()>;
    /// Every iteration summary for a run, oldest first.
    fn summaries(&self, run_id: &str) -> Result<Vec<IterationSummary>>;

    fn put_skill_trial(&self, t: &SkillTrial) -> Result<u64>;
    /// Trials across every run, so a skill's record survives one bad loop.
    fn skill_trials(&self) -> Result<Vec<SkillTrial>>;

    fn put_proposal(&self, p: &Proposal) -> Result<u64>;
    fn proposals(&self, run_id: &str) -> Result<Vec<Proposal>>;

    fn runs(&self) -> Result<Vec<String>>;
    fn flush(&self) -> Result<()>;
}

/// How a skill has performed across every trial recorded for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillScore {
    pub skill: String,
    pub trials: usize,
    pub satisfied: usize,
    pub mean_pass_rate: f64,
    pub source: String,
}

impl SkillScore {
    pub fn satisfaction_rate(&self) -> f64 {
        if self.trials == 0 {
            0.0
        } else {
            self.satisfied as f64 / self.trials as f64
        }
    }
}

/// Rank skills by observed outcome. A skill with too few trials is reported
/// but should not be acted on — one lucky run is not evidence.
pub fn score_skills(trials: &[SkillTrial]) -> Vec<SkillScore> {
    let mut by: BTreeMap<&str, (usize, usize, f64, &str)> = BTreeMap::new();
    for t in trials {
        let e = by
            .entry(t.skill.as_str())
            .or_insert((0, 0, 0.0, t.source.as_str()));
        e.0 += 1;
        if t.satisfied {
            e.1 += 1;
        }
        e.2 += t.pass_rate;
    }
    let mut out: Vec<SkillScore> = by
        .into_iter()
        .map(|(skill, (n, sat, sum, src))| SkillScore {
            skill: skill.to_string(),
            trials: n,
            satisfied: sat,
            mean_pass_rate: if n == 0 { 0.0 } else { sum / n as f64 },
            source: src.to_string(),
        })
        .collect();
    out.sort_by(|a, b| {
        b.satisfaction_rate()
            .partial_cmp(&a.satisfaction_rate())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.trials.cmp(&a.trials))
    });
    out
}

/// Open the shipped backend.
pub fn open(path: impl AsRef<Path>) -> Result<SledStore> {
    SledStore::open(path)
}

#[cfg(test)]
pub(crate) fn sample_episode(run: &str, node: &str) -> Episode {
    Episode {
        run_id: run.into(),
        iteration: 1,
        node_id: node.into(),
        role: "builder".into(),
        provider_id: "p1".into(),
        prompt_digest: "abc".into(),
        output: "did the thing".into(),
        tokens: Some(10),
        cost_usd: Some(0.01),
        duration_ms: Some(5),
        error: None,
        created_ms: now_ms(),
    }
}

#[cfg(test)]
mod proposal_tests {
    use super::*;

    fn proposal(kind: ProposalKind, created_ms: u64) -> Proposal {
        Proposal {
            run_id: "r1".into(),
            iteration: 1,
            kind,
            subject: "s".into(),
            rationale: "because".into(),
            patch: None,
            created_ms,
            expires_ms: None,
        }
    }

    #[test]
    fn every_proposal_kind_has_a_decided_lifetime() {
        // The point of the exhaustive match in `default_lifetime_ms` is that a
        // new kind cannot be added without deciding this. The test states which
        // way each one was decided so a change to one is visible in a diff.
        const DAY: u64 = 24 * 60 * 60 * 1000;
        for (kind, want) in [
            (ProposalKind::TrySkill, Some(7 * DAY)),
            (ProposalKind::AdoptSkill, Some(30 * DAY)),
            (ProposalKind::DropSkill, Some(30 * DAY)),
            (ProposalKind::ReshapeGraph, Some(30 * DAY)),
            // A question about the goal, not an observation about a run.
            (ProposalKind::ChangeCriteria, None),
        ] {
            assert_eq!(
                Proposal::default_lifetime_ms(kind),
                want,
                "{kind:?} lifetime changed"
            );
        }
    }

    #[test]
    fn the_default_expiry_is_stamped_from_the_kind_and_never_overwritten() {
        let p = proposal(ProposalKind::TrySkill, 1_000).with_default_expiry();
        assert_eq!(p.expires_ms, Some(1_000 + 7 * 24 * 60 * 60 * 1000));

        // A criteria change is never aged out.
        let c = proposal(ProposalKind::ChangeCriteria, 1_000).with_default_expiry();
        assert_eq!(c.expires_ms, None);
        assert!(!c.is_expired(u64::MAX), "criteria changes never expire");

        // An explicit expiry survives the stamping.
        let mut explicit = proposal(ProposalKind::TrySkill, 1_000);
        explicit.expires_ms = Some(42);
        assert_eq!(explicit.with_default_expiry().expires_ms, Some(42));
    }

    #[test]
    fn expiry_is_reported_and_the_boundary_counts_as_expired() {
        let p = proposal(ProposalKind::AdoptSkill, 0).with_default_expiry();
        let at = p.expires_ms.unwrap();
        assert!(!p.is_expired(at - 1));
        assert!(p.is_expired(at), "the expiry instant is already stale");
        assert!(p.is_expired(at + 1));
    }

    #[test]
    fn a_proposal_written_before_the_field_existed_still_deserialises() {
        // Proposals live in a sled store that outlives the binary that wrote
        // them. Without `#[serde(default)]` this JSON fails to parse and
        // `loopsmith proposals` reports a backend error on a healthy store.
        let old = r#"{"run_id":"r1","iteration":2,"kind":"adopt_skill",
            "subject":"s","rationale":"why","patch":null,"created_ms":5}"#;
        let p: Proposal = serde_json::from_str(old).expect("old records still read");
        assert_eq!(p.expires_ms, None);
        assert!(!p.is_expired(u64::MAX));
    }
}
