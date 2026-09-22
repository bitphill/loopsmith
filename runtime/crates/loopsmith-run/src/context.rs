//! The one value every state works on.
//!
//! A run is a config, a store, the caller's options, the recorder that writes
//! the ledger and the run log together, the checkpoint, and the lifecycle.
//! Each state module takes `&mut Run` and nothing else, so the question "what
//! can this state touch" has one answer.

use crate::logging::{Recorder, RunLog};
use crate::recovering::{self, Failure, Response};
use crate::state::{Halt, Lifecycle, RunState};
use crate::RunOptions;
use loopsmith_core::{FailureClass, LoopConfig};
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
            read_checkpoint(cfg, store, &rec, &opts.run_id)?
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
            // A transition that cannot be persisted is still a transition: the
            // run is in `next` whatever the disk says. What is lost is only
            // the crash report a later resume could have given, so it is
            // written down and the run goes on.
            if let Err(e) = self.save() {
                self.rec.entry(
                    self.checkpoint.iteration,
                    LedgerKind::Recovered,
                    format!("the checkpoint could not be saved on entering `{next}`: {e}"),
                    None,
                );
            }
        } else {
            self.stamp();
        }
        Ok(())
    }

    /// Persist the checkpoint as it stands, stamped with the current state.
    pub fn save(&mut self) -> Result<(), String> {
        self.stamp();
        self.store
            .save_checkpoint(&self.checkpoint)
            .map_err(|e| e.to_string())
    }

    /// Persist the end of an iteration, answering a failed save with the
    /// `corrupted_state` policy. Returns the halt the policy calls for, if
    /// any.
    ///
    /// `restore_checkpoint` restores nothing here — the store keeps one
    /// checkpoint per run and it is the one that just failed to write — so it
    /// fails the run rather than carry on without a resume point.
    pub fn save_or_halt(&mut self) -> Option<Halt> {
        let mut attempt = 0u32;
        loop {
            let err = match self.save() {
                Ok(()) => return None,
                Err(e) => e,
            };
            attempt += 1;
            let failure = Failure {
                class: FailureClass::CorruptedState,
                attempt,
                subject: "the checkpoint".into(),
                detail: format!("could not be saved ({err})"),
                node: None,
                iteration: self.checkpoint.iteration,
            };
            let why = format!("the checkpoint could not be saved: {err}");
            match recovering::answer(&self.rec, &self.cfg.safety.recovery, &failure, true) {
                Response::Retry { delay_seconds } => {
                    std::thread::sleep(std::time::Duration::from_secs(delay_seconds));
                }
                Response::Revise | Response::Continue => return None,
                Response::Escalate => {
                    return Some(Halt {
                        state: RunState::Escalated,
                        why,
                    })
                }
                Response::Halt(RunState::RolledBack) => {
                    return Some(Halt {
                        state: RunState::Failed,
                        why: format!("{why}; no earlier checkpoint is kept to restore"),
                    })
                }
                Response::Halt(state) => return Some(Halt { state, why }),
            }
        }
    }

    /// Stamp the in-memory checkpoint with the current state, without saving.
    fn stamp(&mut self) {
        self.checkpoint.state = Some(self.life.state().as_str().to_string());
        self.checkpoint.outcome = self.life.outcome().map(|s| s.as_str().to_string());
        self.checkpoint.updated_ms = now_ms();
    }
}

/// Read the checkpoint a resume starts from, answering a failed read with the
/// `corrupted_state` policy.
///
/// A retry is honoured: a store behind a network filesystem can fail a read
/// and pass the next. Every other answer ends in a refusal, and deliberately
/// so. `restore_checkpoint` has nothing to restore — the store keeps one
/// checkpoint per run, and it is the one that just failed — and starting the
/// run from a blank checkpoint would hand it a fresh iteration budget, a zero
/// spend, and a no-progress counter of zero, which is every ceiling it had
/// already spent against, refunded.
fn read_checkpoint<S: Store>(
    cfg: &LoopConfig,
    store: &S,
    rec: &Recorder<S>,
    run_id: &str,
) -> Result<Option<Checkpoint>, String> {
    let mut attempt = 0u32;
    loop {
        let err = match store.checkpoint(run_id) {
            Ok(cp) => return Ok(cp),
            Err(e) => e,
        };
        attempt += 1;
        let failure = Failure {
            class: FailureClass::CorruptedState,
            attempt,
            subject: format!("the checkpoint for `{run_id}`"),
            detail: format!("could not be read ({err})"),
            node: None,
            iteration: 0,
        };
        match recovering::answer(rec, &cfg.safety.recovery, &failure, true) {
            Response::Retry { delay_seconds } => {
                std::thread::sleep(std::time::Duration::from_secs(delay_seconds));
            }
            _ => {
                return Err(format!(
                    "the checkpoint for `{run_id}` could not be read ({err}). Nothing was \
                     resumed: no earlier checkpoint is kept to restore, and starting over would \
                     refund every budget the run had spent. Start it under a new run id, or \
                     repair the store."
                ))
            }
        }
    }
}

