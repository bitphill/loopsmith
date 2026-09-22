//! `Planning`: schedule the graph and resolve the phases.
//!
//! Both are resolved before anything is dispatched. An unschedulable graph or
//! phase chain is a config bug, and finding it after the first provider call
//! means paying for the discovery.

use crate::context::Run;
use crate::evidence;
use crate::logging::Recorder;
use crate::phases::Phases;
use crate::rules;
use crate::state::RunState;
use crate::state::Halt;
use loopsmith_core::{GateKind, LoopConfig};
use loopsmith_memory::{LedgerKind, Store};
use std::path::Path;

/// Everything `Running` needs that does not change between iterations —
/// except the phases, which open as the gate closes them.
pub(crate) struct Planned {
    pub graph: loopsmith_graph::Plan,
    pub phases: Phases,
}

pub(crate) fn plan<S: Store>(run: &mut Run<S>) -> Result<Planned, String> {
    let cfg = run.cfg;
    let graph = loopsmith_graph::plan(&cfg.execution.graph).map_err(|e| e.to_string())?;
    let phases = Phases::new(cfg)?;

    run.rec.entry(
        run.checkpoint.iteration,
        LedgerKind::RunStarted,
        format!(
            "{} nodes in {} waves, concurrency {}, predicted speedup {:.2}x (ceiling {:.2}x)",
            cfg.execution.graph.nodes.len(),
            graph.waves.len(),
            graph.concurrency,
            graph.predicted_speedup,
            graph.speedup_ceiling
        ),
        None,
    );

    // The sub-agents this loop declared it cannot start without. Idempotent,
    // so running it every time costs a directory check when they are already
    // there. Skipped on a dry run — installing software is not something "plan
    // and report without invoking a provider" should do.
    if run.opts.acquire_skills && !run.opts.dry_run {
        install_default_skills(cfg, run.root(), &run.rec);
    }

    Ok(Planned { graph, phases })
}

/// `AwaitingApproval`: the approval rules, when there are any.
///
/// An approval rule is a detector like any other, which is what lets a human
/// approve without loopsmith growing a UI for it: the rule names an artifact —
/// `APPROVED`, a signed-off ticket, a green deploy check — and the run
/// proceeds once it is there. With `features.human_approval` off the rules are
/// skipped and the ledger says so; validation already refuses that in `prod`.
pub(crate) fn approve<S: Store>(run: &mut Run<S>) -> Result<Option<Halt>, String> {
    let cfg = run.cfg;
    let n = cfg.safety.gates.approval.len();
    if n == 0 {
        return Ok(None);
    }
    let it = run.checkpoint.iteration;
    if !cfg.features.human_approval {
        run.rec.entry(
            it,
            LedgerKind::RuleEvaluated,
            format!("{n} approval rule(s) not checked: `features.human_approval` is off"),
            None,
        );
        return Ok(None);
    }
    run.enter(RunState::AwaitingApproval, format!("{n} approval rule(s)"))?;
    let root = run.root();
    let ev = evidence::at_root(cfg, root, vec![]);
    Ok(rules::apply(run, GateKind::Approval, &ev, it))
}

/// Install the declared default sub-agents. A failure is recorded and the run
/// continues: a loop whose optional helper could not be fetched is degraded,
/// not broken, and finding that out from the ledger beats finding it out from
/// a stack trace at 4am.
pub fn install_default_skills<S: Store>(cfg: &LoopConfig, root: &Path, rec: &Recorder<S>) {
    for spec in &cfg.execution.default_skills {
        match loopsmith_skills::install_default(spec, &cfg.execution.skills, root) {
            Ok(r) => rec.entry(
                0,
                LedgerKind::SkillAcquired,
                format!(
                    "default skill `{}` ready at {} (via {})",
                    r.name,
                    r.path.display(),
                    spec.source.as_str()
                ),
                None,
            ),
            Err(e) => rec.entry(
                0,
                LedgerKind::NodeFailed,
                format!("default skill `{}` unavailable: {e}", spec.name),
                None,
            ),
        }
    }
}
