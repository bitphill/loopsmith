//! `Validating`: may this run start at all?
//!
//! The config itself was validated before the engine was called — that is the
//! loader's job, and it refuses a config with errors. What is checked here is
//! the world: the entry rules in `safety.gates.entry`, evaluated against what
//! is on disk before anything runs. A missing brief, a lockfile another run
//! holds, a metric that says the target system is down — each is a reason not
//! to spend a single token.

use crate::context::Run;
use crate::evidence;
use crate::rules;
use crate::state::Halt;
use loopsmith_core::GateKind;
use loopsmith_memory::{LedgerKind, Store};

pub(crate) fn validate<S: Store>(run: &mut Run<S>) -> Option<Halt> {
    let it = run.checkpoint.iteration;

    // Answering an escalation is a human's act, asked for explicitly: a
    // resume alone leaves every question open and every escalated node held.
    // When it is answered, a node held back for running out of revisions gets
    // them back — otherwise the answer changes nothing, and the revision
    // ceiling keeps it out exactly as the escalation did.
    if run.opts.resume && !run.checkpoint.escalations.is_empty() && !run.opts.answer_escalations {
        run.rec.entry(
            it,
            LedgerKind::Escalated,
            format!(
                "{} escalation(s) still open, and their nodes stay held; resume with \
                 `--answer` once a human has dealt with them",
                run.checkpoint.escalations.len()
            ),
            None,
        );
    }
    if run.opts.resume && run.opts.answer_escalations && !run.checkpoint.escalations.is_empty() {
        let answered = std::mem::take(&mut run.checkpoint.escalations);
        let mut freed = Vec::new();
        for e in &answered {
            if let Some(node) = &e.node_id {
                if run.checkpoint.revisions.remove(node).is_some() {
                    freed.push(format!("`{node}`"));
                }
            }
        }
        run.rec.entry(
            it,
            LedgerKind::Escalated,
            format!(
                "resuming answers {} open escalation(s): {}{}",
                answered.len(),
                answered
                    .iter()
                    .map(|e| e.question.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
                if freed.is_empty() {
                    String::new()
                } else {
                    format!(". {} may be dispatched again", freed.join(", "))
                }
            ),
            None,
        );
    }

    let root = run.root();
    let ev = evidence::at_root(run.cfg, root, vec![]);
    rules::apply(run, GateKind::Entry, &ev, it)
}
