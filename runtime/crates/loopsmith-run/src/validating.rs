//! `Validating`: may this run start at all?
//!
//! The config itself was validated before the engine was called — that is the
//! loader's job, and it refuses a config with errors. What is checked here is
//! the world: the entry rules in `safety.gates.entry`, evaluated against what
//! is on disk before anything runs. A missing brief, a lockfile another run
//! holds, a metric that says the target system is down — each is a reason not
//! to spend a single token.

use crate::context::Run;
use crate::evidence::collect_evidence;
use crate::rules;
use crate::waves::Halt;
use loopsmith_core::GateKind;
use loopsmith_memory::{LedgerKind, Store};

pub(crate) fn validate<S: Store>(run: &mut Run<S>) -> Option<Halt> {
    let it = run.checkpoint.iteration;

    // A resume is how a human answers an escalation. Whatever was asked is
    // considered answered, and the nodes it held back are eligible again.
    if run.opts.resume && !run.checkpoint.escalations.is_empty() {
        let answered = std::mem::take(&mut run.checkpoint.escalations);
        run.rec.entry(
            it,
            LedgerKind::Escalated,
            format!(
                "resuming answers {} open escalation(s): {}",
                answered.len(),
                answered.join("; ")
            ),
            None,
        );
    }

    let root = run.root();
    let ev = collect_evidence(run.cfg, root, Some(&root.join("metrics.json")), vec![]);
    rules::apply(run, GateKind::Entry, &ev, it)
}
