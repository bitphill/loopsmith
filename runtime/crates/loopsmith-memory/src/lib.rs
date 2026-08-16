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
/// sortable without a date parser.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

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

/// Where to pick up after a crash or a scheduled pause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub run_id: String,
    pub iteration: u32,
    /// Node ids already completed in this iteration.
    pub completed_nodes: Vec<String>,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub started_ms: u64,
    pub updated_ms: u64,
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

    fn runs(&self) -> Result<Vec<String>>;
    fn flush(&self) -> Result<()>;
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
