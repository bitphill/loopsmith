//! The one value every state works on.
//!
//! A run is a config, a store, the caller's options, the recorder that writes
//! the ledger and the run log together, the checkpoint, and the lifecycle.
//! Each state module takes `&mut Run` and nothing else, so the question "what
//! can this state touch" has one answer.

use crate::logging::{Recorder, RunLog};
use crate::state::{Lifecycle, RunState};
use crate::RunOptions;
use loopsmith_core::LoopConfig;
use loopsmith_memory::{now_ms, Checkpoint, LedgerKind, Store};
use std::path::Path;
use std::time::Instant;

pub(crate) struct Run<'a, S: Store> {
    pub cfg: &'a LoopConfig,
    pub store: &'a S,
    pub opts: &'a RunOptions,
    pub rec: Recorder<'a, S>,
    pub checkpoint: Checkpoint,
    pub life: Lifecycle,
    pub started: Instant,
}

impl<'a, S: Store> Run<'a, S> {
    /// Open the recorder and pick up the checkpoint.
    ///
    /// Nothing is dispatched and no state is entered here; the caller moves
    /// the run into `Validating` itself, so the first ledger line of every
    /// run — fresh or resumed — is the same transition.
    pub fn open(cfg: &'a LoopConfig, store: &'a S, opts: &'a RunOptions) -> Result<Self, String> {
        // Every event from here on goes to the ledger and the run log together,
        // so the queryable record and the readable one cannot disagree.
        let rec = Recorder::new(
            store,
            &opts.run_id,
            RunLog::open(&opts.workdir, &opts.run_id, opts.verbose),
        );

        let stored = if opts.resume {
            store.checkpoint(&opts.run_id).map_err(|e| e.to_string())?
        } else {
            None
        };
        let (checkpoint, life) = match stored {
            Some(cp) => {
                let (life, crashed) =
                    Lifecycle::resume(cp.state.as_deref().and_then(RunState::parse));
                if let Some(was) = crashed {
                    rec.entry(
                        cp.iteration,
                        LedgerKind::StateChanged,
                        format!(
                            "the previous process stopped while the run was `{was}` and never \
                             closed it; resuming from the last checkpoint (iteration {})",
                            cp.iteration
                        ),
                        None,
                    );
                }
                (cp, life)
            }
            None => (Checkpoint::new(&opts.run_id), Lifecycle::new()),
        };

        Ok(Run {
            cfg,
            store,
            opts,
            rec,
            checkpoint,
            life,
            started: Instant::now(),
        })
    }

    pub fn root(&self) -> &'a Path {
        &self.opts.workdir
    }

    /// Move the run to `next` and write the move down.
    ///
    /// Entering a state the engine spends time or money in — `Running`,
    /// `Retrying`, `AwaitingApproval` — saves the checkpoint, so a process that
    /// dies there leaves behind the state it died in, and the next resume says
    /// so instead of guessing. So does `Closed`. The states in between are
    /// passed through in milliseconds; every save is an fsync, and paying five
    /// of them to record a walk from `Created` to `Running` bought nothing a
    /// crash could use.
    pub fn enter(&mut self, next: RunState, why: impl AsRef<str>) -> Result<(), String> {
        let from = self.life.advance(next).map_err(|e| e.to_string())?;
        let why = why.as_ref();
        self.rec.entry(
            self.checkpoint.iteration,
            LedgerKind::StateChanged,
            if why.is_empty() {
                format!("{from} → {next}")
            } else {
                format!("{from} → {next}: {why}")
            },
            None,
        );
        if matches!(
            next,
            RunState::Running | RunState::Retrying | RunState::AwaitingApproval | RunState::Closed
        ) {
            self.save();
        } else {
            self.stamp();
        }
        Ok(())
    }

    /// Persist the checkpoint as it stands, stamped with the current state.
    pub fn save(&mut self) {
        self.stamp();
        let _ = self.store.save_checkpoint(&self.checkpoint);
    }

    /// Stamp the in-memory checkpoint with the current state, without saving.
    fn stamp(&mut self) {
        self.checkpoint.state = Some(self.life.state().as_str().to_string());
        self.checkpoint.outcome = self.life.outcome().map(|s| s.as_str().to_string());
        self.checkpoint.updated_ms = now_ms();
    }
}
