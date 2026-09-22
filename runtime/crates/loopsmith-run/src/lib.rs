//! The loopsmith run engine.
//!
//! [`execute`] takes a config, a store, and options, and returns a
//! [`RunOutcome`]. Everything between is a state machine ([`state`]), and each
//! state has its own module:
//!
//! - `validating` — entry rules, checked against the world before anything
//!   runs.
//! - `planning` — schedule the graph, resolve the phases, install declared
//!   sub-agents, then the approval rules.
//! - `running` — the iteration loop: dispatch, gate, rollback rules, compress,
//!   decide. Dispatch itself is `waves`: the work queue, join strategies, and
//!   typed recovery (`recovering`).
//! - `closing` — enter the outcome state, save, export on certified success.
//!
//! The work inside an iteration is split across neighbours so the state
//! modules stay about *when*, not *how*:
//!
//! - [`dispatch`] — isolation, skill resolution, the provider call
//! - [`prompts`] — what a node is told
//! - [`evolve`] — trials, judgments, and proposals
//! - [`stop`] — the stop-gate ladder, as a pure function
//! - [`publish`] — moving an isolated node's work into the loop root
//!
//! The gate is not in this crate. The engine hands it evidence and records its
//! ruling; nothing here can mark a goal satisfied.

pub mod dispatch;
pub mod evolve;
pub mod export;
pub mod judgment;
pub mod logging;
pub mod perturb;
pub mod phases;
pub mod prompts;
pub mod publish;
pub mod schedule;
pub mod state;
pub mod stop;
pub mod summary;
pub mod worktree;

mod closing;
mod context;
mod evidence;
mod planning;
mod recovering;
mod rules;
mod running;
mod validating;
mod waves;

pub use evidence::collect_evidence;
pub use planning::install_default_skills;
pub use state::{IllegalTransition, Lifecycle, RunState};
pub use stop::StopReason;

use loopsmith_core::LoopConfig;
use loopsmith_gate::TargetVerdict;
use loopsmith_memory::Store;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct RunOptions {
    pub run_id: String,
    pub workdir: PathBuf,
    /// Plan and report without invoking any provider.
    pub dry_run: bool,
    pub resume: bool,
    /// Acquire missing sub-agents. Off means a node with an unresolved skill
    /// runs without it and says so.
    pub acquire_skills: bool,
    /// Mirror the run log to stderr as it is written.
    pub verbose: bool,
    /// Config file name, for the scripts written into a success export.
    pub config_file: String,
}

pub struct RunOutcome {
    pub run_id: String,
    pub iterations: u32,
    /// The state the run closed from: `succeeded`, `paused`, `blocked`, …
    pub state: RunState,
    pub stop: StopReason,
    pub verdicts: BTreeMap<String, TargetVerdict>,
    pub tokens_used: u64,
    pub tokens_estimated: bool,
    pub cost_usd: f64,
    pub proposals: usize,
    /// Where the plain-text run log was written, when one could be opened.
    pub log_path: Option<PathBuf>,
    /// Where the reusable success package was written. Only ever `Some` when
    /// the gate certified overall success.
    pub export_path: Option<PathBuf>,
}

/// Run a loop from wherever its checkpoint says it is, until something stops
/// it.
///
/// An `Err` is a config the engine could not start at all — an unschedulable
/// graph, an unresolvable phase chain. It is still written to the ledger as a
/// run that moved to `failed`, so the record and the return value agree.
pub fn execute<S: Store>(
    cfg: &LoopConfig,
    store: &S,
    opts: &RunOptions,
) -> Result<RunOutcome, String> {
    let mut run = context::Run::open(cfg, store, opts)?;
    let mut progress = running::Progress::from_checkpoint(&run.checkpoint);

    run.enter(RunState::Validating, if opts.resume { "resuming" } else { "" })?;
    if let Some(halt) = validating::validate(&mut run) {
        return closing::close(run, progress, halted(halt));
    }

    run.enter(RunState::Planning, "")?;
    let mut planned = match planning::plan(&mut run) {
        Ok(p) => p,
        Err(e) => {
            run.enter(RunState::Failed, &e)?;
            run.enter(RunState::Closed, "")?;
            return Err(e);
        }
    };
    if let Some(halt) = planning::approve(&mut run)? {
        return closing::close(run, progress, halted(halt));
    }

    run.enter(RunState::Running, "")?;
    let stopped = running::iterate(&mut run, &mut planned, &mut progress);
    closing::close(run, progress, stopped)
}

/// A run stopped by a rule before its first iteration: no rulings to report.
fn halted(h: waves::Halt) -> running::Stopped {
    running::Stopped {
        reason: StopReason::Halted {
            state: h.state,
            why: h.why,
        },
        verdicts: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests;
