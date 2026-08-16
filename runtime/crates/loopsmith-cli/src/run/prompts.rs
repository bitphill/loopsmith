//! Prompt construction.
//!
//! Two prompts per node: a system prompt carrying the loop's static context
//! and constraints, and a task prompt carrying the node's own instruction, the
//! goals it advances, and the checks those goals face.
//!
//! Stating the bar in the prompt is not politeness. A node that does not know
//! how it will be checked cannot aim at the check, and the gate will refuse
//! work that missed a target nobody mentioned.

use super::dispatch::NodeContext;
use crate::judgment;
use loopsmith_core::{ConstraintSet, LoopConfig, NodeSpec, Role};

pub fn build_system_prompt(cfg: &LoopConfig, c: &ConstraintSet) -> String {
    let mut s = String::new();
    s.push_str(&format!("You are a node in the `{}` loop.\n\n", cfg.name));
    if !cfg.information.is_empty() {
        s.push_str("Context:\n");
        for i in &cfg.information {
            s.push_str(&format!("- {}: {}\n", i.key, i.value));
        }
        s.push('\n');
    }
    if !c.rules.is_empty() {
        s.push_str("Rules you must follow:\n");
        for r in &c.rules {
            s.push_str(&format!("- {r}\n"));
        }
        s.push('\n');
    }
    if !c.forbidden_paths.is_empty() {
        s.push_str(&format!("Never touch: {}\n", c.forbidden_paths.join(", ")));
    }
    if !c.forbidden_commands.is_empty() {
        s.push_str(&format!("Never run: {}\n", c.forbidden_commands.join(", ")));
    }
    if !c.human_checkpoint.is_empty() {
        s.push_str(&format!(
            "Stop and ask a human before: {}\n",
            c.human_checkpoint.join(", ")
        ));
    }
    s
}

pub fn build_node_prompt(cfg: &LoopConfig, node: &NodeSpec, ctx: &NodeContext) -> String {
    let skills = ctx.skills;
    let mut s = String::new();
    s.push_str(&format!("## Your task\n{}\n\n", node.instruction));

    // The phase instruction comes before the goals: it is the standing rule for
    // this stretch of the run, and it is usually about what *not* to do yet.
    if let (Some(stage), Some(text)) = (node.stage.as_deref(), ctx.guideline) {
        s.push_str(&format!(
            "## Phase: {stage}\n{text}\n\nStay inside this phase. Work that belongs to a later \
             phase is not yours to do yet.\n\n"
        ));
    }

    if !skills.is_empty() {
        s.push_str("## Sub-agents available to you\n");
        for (name, source) in skills {
            s.push_str(&format!("- `{name}` ({source})\n"));
        }
        s.push_str("\nUse them where they fit. If one does not help, say so — that is recorded.\n\n");
    }

    if !node.goals.is_empty() {
        s.push_str("## Goals you advance\n");
        for gname in &node.goals {
            if let Some(g) = cfg.goals.iter().find(|g| &g.name == gname) {
                s.push_str(&format!("- **{}** — {}\n", g.name, g.description));
            }
            // The bar is stated up front: a node that does not know how it
            // will be checked cannot aim at the check.
            for v in cfg.blocking_validations_for(gname) {
                s.push_str(&format!("  - checked by `{}`: {}\n", v.name, v.statement));
            }
        }
        s.push('\n');
    }

    if node.role == Role::Judge {
        s.push_str(judgment::JUDGE_OUTPUT_CONTRACT);
        s.push_str("\n\n");
    } else if let Some(p) = ctx.perturbation {
        // Never handed to a judge. Telling the thing that checks the work to
        // "try a different approach" is how a stalled loop talks itself into a
        // lower bar.
        if let Some(directive) = p.directive() {
            s.push_str(&directive);
        }
    }

    // Compressed history, not raw episodes. This is the only thing standing
    // between a week-long run and a prompt that grows until it is unaffordable.
    if !ctx.carried.is_empty() {
        s.push_str(ctx.carried);
    }

    for gname in &node.goals {
        if let Some(pad) = ctx.scratch.get(gname) {
            s.push_str(&format!("## Notes carried from earlier iterations\n{pad}\n\n"));
        }
    }
    s
}
