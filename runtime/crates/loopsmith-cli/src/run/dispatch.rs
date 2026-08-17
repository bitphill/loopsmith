//! Node dispatch: isolation, skill resolution, and the provider call.
//!
//! [`run_node`] is deliberately store-free. It runs on a worker thread, and a
//! thread that can write to the ledger is a thread that can interleave the
//! ledger. Everything it learns comes back as a [`NodeOutcome`] and is written
//! down by the caller, after the join, in a defined order.

use super::perturb::Perturbation;
use super::prompts::{build_node_prompt, build_system_prompt};
use crate::logging::Recorder;
use crate::worktree::{self, Isolation};
use loopsmith_core::{LoopConfig, NodeSpec, Role};
use loopsmith_memory::{LedgerKind, Store};
use loopsmith_provider::{digest, dispatch, InvokeRequest};
use std::collections::BTreeMap;
use std::path::Path;

/// What one node produced, before anything is written down.
pub struct NodeOutcome {
    pub node_id: String,
    pub role: Role,
    pub provider_id: String,
    pub prompt_digest: String,
    pub output: String,
    pub tokens: Option<u64>,
    pub tokens_estimated: bool,
    pub cost_usd: Option<f64>,
    pub duration_ms: u64,
    pub skipped: Vec<String>,
    pub error: Option<String>,
    /// Paths copied in from the loop root before the node ran, so it could see
    /// what its upstream produced.
    pub seeded: Vec<String>,
    /// Where the node actually ran. Carried out structurally rather than as
    /// prose because the caller has to publish an isolated node's work back
    /// into the loop root, and cannot do that from a description of it.
    pub isolation: Isolation,
}

/// Resolve the sub-agents a node declares, acquiring what is missing.
pub fn ensure_skills<S: Store>(
    cfg: &LoopConfig,
    node: &NodeSpec,
    root: &Path,
    rec: &Recorder<S>,
    iteration: u32,
    acquire: bool,
) -> Vec<(String, String)> {
    let mut resolved = Vec::new();
    for name in &node.skills {
        match loopsmith_skills::find_installed(name, root) {
            Some(found) => resolved.push((name.clone(), found.source.as_str().to_string())),
            None if acquire => {
                match loopsmith_skills::acquire(name, &node.instruction, &cfg.skills, root) {
                    Ok(r) => {
                        rec.entry(
                            iteration,
                            LedgerKind::SkillAcquired,
                            format!(
                                "`{}` via {} into {}",
                                r.name,
                                r.source.as_str(),
                                r.path.display()
                            ),
                            Some(node.id.clone()),
                        );
                        resolved.push((r.name, r.source.as_str().to_string()));
                    }
                    Err(e) => rec.entry(
                        iteration,
                        LedgerKind::NodeFailed,
                        format!("could not acquire skill `{name}`: {e}"),
                        Some(node.id.clone()),
                    ),
                }
            }
            None => rec.entry(
                iteration,
                LedgerKind::NodeDispatched,
                format!("skill `{name}` missing and acquisition is off; running without it"),
                Some(node.id.clone()),
            ),
        }
    }
    resolved
}

/// Everything a node is told, other than its own spec.
///
/// Gathered into one struct because the alternative is an eight-argument
/// function where two of the arguments are `&str` and nothing but their order
/// distinguishes them.
pub struct NodeContext<'a> {
    /// Per-goal notes, written by the MCP scratchpad tool.
    pub scratch: &'a BTreeMap<String, String>,
    /// Resolved sub-agents, as `(name, source)`.
    pub skills: &'a [(String, String)],
    /// The standing instruction of this node's phase, if it has one.
    pub guideline: Option<&'a str>,
    /// Compressed history of earlier iterations.
    pub carried: &'a str,
    /// Active perturbation, when the loop has stalled and asked for variation.
    pub perturbation: Option<&'a Perturbation>,
    /// Paths other nodes have published into the loop root this run, and who
    /// published each. An isolated node is seeded with these before it runs,
    /// because a worktree branches from `HEAD` and would otherwise be blind to
    /// everything the run has produced since — including its own upstream.
    pub published: &'a BTreeMap<String, String>,
}

/// Dispatch one node. Pure with respect to the store so it is safe to call
/// from several threads at once.
pub fn run_node(
    cfg: &LoopConfig,
    node: &NodeSpec,
    root: &Path,
    run_id: &str,
    ctx: &NodeContext,
) -> NodeOutcome {
    let iso = if node.isolated {
        worktree::create(root, &node.id, run_id)
    } else {
        Isolation::Shared {
            reason: "not marked isolated".into(),
        }
    };
    let seeded = super::publish::seed(root, &node.id, &iso, ctx.published);
    let workdir = iso.workdir(root).to_path_buf();

    let constraints = loopsmith_core::ConstraintSet::merged(
        &cfg.constraints.global,
        cfg.constraints.per_node.get(&node.id),
    );
    let system = build_system_prompt(cfg, &constraints);
    let prompt = build_node_prompt(cfg, node, ctx);

    // A stalled loop may be told to run its builders one tier stronger. Judges
    // keep their configured tier: escalating the checker alongside the worker
    // would change the bar at the same moment as the work.
    let tier = match (ctx.perturbation, node.role) {
        (Some(p), r) if r != Role::Judge => p.tier_for(node.tier),
        _ => node.tier,
    };

    let req = InvokeRequest {
        node_id: node.id.clone(),
        system,
        prompt: prompt.clone(),
        tier,
        workdir,
    };

    match dispatch(cfg, &req, node.provider.as_deref()) {
        Ok((resp, skipped)) => NodeOutcome {
            node_id: node.id.clone(),
            role: node.role,
            provider_id: resp.provider_id,
            prompt_digest: digest(&prompt),
            output: resp.output,
            tokens: resp.tokens,
            tokens_estimated: resp.tokens_estimated,
            cost_usd: resp.cost_usd,
            duration_ms: resp.duration_ms,
            skipped,
            error: None,
            seeded,
            isolation: iso,
        },
        Err(e) => NodeOutcome {
            node_id: node.id.clone(),
            role: node.role,
            provider_id: String::new(),
            prompt_digest: digest(&prompt),
            output: String::new(),
            tokens: None,
            tokens_estimated: false,
            cost_usd: None,
            duration_ms: 0,
            skipped: vec![],
            error: Some(e.to_string()),
            seeded,
            isolation: iso,
        },
    }
}
