//! What a run leaves behind for the next one, and what it picks up.
//!
//! The policy lives in `loopsmith_memory::namespaces`; this module decides
//! what the engine itself writes. Two things, both cheap to state honestly:
//!
//! - **Failure modes**, as a node's final failure after recovery had its say.
//!   The failure namespace promotes on write by default: having hit a wall is
//!   its own evidence, and re-learning it is what the namespace exists to
//!   spare the next run.
//! - **Procedures**, as the shape of a run the gate certified. The procedural
//!   namespace promotes only after several runs agree, so one lucky success
//!   does not become a standing instruction.
//!
//! Facts about the domain are the semantic namespace's, and the engine does
//! not guess at them; an agent writes them through the MCP server, with
//! provenance.

use crate::context::Run;
use crate::recovering;
use loopsmith_core::FailureClass;
use loopsmith_memory::namespaces::{self, Note, Remembered};
use loopsmith_memory::{LedgerKind, Namespace, Store};

/// Record a node's final failure as a known failure mode.
pub(crate) fn failure<S: Store>(run: &Run<S>, node: &str, class: FailureClass, detail: &str, it: u32) {
    let key = format!("`{node}`: {}", recovering::class_name(class));
    let content = truncate(detail, 300);
    let provenance = format!("run {}, iteration {it}", run.opts.run_id);
    write(run, Namespace::Failure, &key, &content, &provenance, it);
}

/// Record how a certified run went, as a procedure later runs may reuse once
/// enough of them agree.
pub(crate) fn procedure<S: Store>(run: &Run<S>, it: u32) {
    let mut nodes: Vec<&str> = run
        .checkpoint
        .completed_nodes
        .iter()
        .map(String::as_str)
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    let key = format!("`{}` reached overall success", run.cfg.name);
    let content = format!(
        "the graph ran as designed ({}) and the gate certified overall success",
        if nodes.is_empty() {
            "no nodes".to_string()
        } else {
            nodes.join(", ")
        }
    );
    let provenance = format!("run {}, iteration {it}", run.opts.run_id);
    write(run, Namespace::Procedural, &key, &content, &provenance, it);
}

fn write<S: Store>(run: &Run<S>, ns: Namespace, key: &str, content: &str, provenance: &str, it: u32) {
    let note = Note {
        namespace: ns,
        key,
        content,
        provenance: Some(provenance),
        confidence: 1.0,
        run_id: &run.opts.run_id,
    };
    match namespaces::remember(run.store, &run.cfg.execution.memory, &note) {
        Ok(Remembered::Written { newly: true, .. }) => run.rec.entry(
            it,
            LedgerKind::Remembered,
            format!("promoted to {} memory: {key}", ns.as_str()),
            None,
        ),
        Ok(Remembered::Refused(why)) => run.rec.entry(
            it,
            LedgerKind::Remembered,
            format!("{} memory refused {key}: {why}", ns.as_str()),
            None,
        ),
        Err(e) => run.rec.entry(
            it,
            LedgerKind::Remembered,
            format!("{} memory could not be written: {e}", ns.as_str()),
            None,
        ),
        Ok(Remembered::Written { .. }) | Ok(Remembered::Disabled) => {}
    }
}

/// Promoted records, one line each, for a node's prompt.
pub(crate) fn recall<S: Store>(run: &Run<S>) -> Vec<String> {
    let wanted = [Namespace::Failure, Namespace::Procedural, Namespace::Semantic];
    namespaces::recall(run.store, &run.cfg.execution.memory, &wanted)
        .unwrap_or_default()
        .into_iter()
        .map(|r| format!("[{}] {} — {}", r.namespace.as_str(), r.key, r.content))
        .collect()
}

/// Apply every namespace's retention at the start of a run.
pub(crate) fn expire<S: Store>(run: &Run<S>) {
    let now = loopsmith_memory::now_ms();
    match namespaces::expire(run.store, &run.cfg.execution.memory, now) {
        Ok(0) => {}
        Ok(n) => run.rec.entry(
            run.checkpoint.iteration,
            LedgerKind::Remembered,
            format!("retention expired {n} record(s) and episode(s)"),
            None,
        ),
        Err(e) => run.rec.entry(
            run.checkpoint.iteration,
            LedgerKind::Remembered,
            format!("retention could not be applied: {e}"),
            None,
        ),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}
