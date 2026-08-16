//! Section H — constraints applied per node or globally.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Constraints {
    /// Applied to every node unless overridden.
    #[serde(default)]
    pub global: ConstraintSet,
    /// Keyed by node id.
    #[serde(default)]
    pub per_node: BTreeMap<String, ConstraintSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ConstraintSet {
    /// Literal rules injected into the node prompt.
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
    #[serde(default)]
    pub forbidden_commands: Vec<String>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub max_seconds: Option<u64>,
    /// Any action matching these requires a human before proceeding. Bezos
    /// Type 1: irreversible decisions do not get made at machine speed.
    #[serde(default)]
    pub human_checkpoint: Vec<String>,
}

impl ConstraintSet {
    /// The frozen git rule set that made large parallel runs safe in the
    /// corpus. Emitted into every parallel node unless the author opts out.
    pub fn frozen_git_rules() -> Vec<String> {
        vec![
            "Never git stash. Never git reset.".into(),
            "No git command except committing a specific file.".into(),
            "No slow commands before the test phase.".into(),
        ]
    }

    /// Merge a global set with a node override; node rules append, node
    /// limits win where present.
    pub fn merged(global: &ConstraintSet, node: Option<&ConstraintSet>) -> ConstraintSet {
        let mut out = global.clone();
        if let Some(n) = node {
            out.rules.extend(n.rules.iter().cloned());
            out.forbidden_paths.extend(n.forbidden_paths.iter().cloned());
            out.forbidden_commands
                .extend(n.forbidden_commands.iter().cloned());
            out.human_checkpoint
                .extend(n.human_checkpoint.iter().cloned());
            if n.max_tokens.is_some() {
                out.max_tokens = n.max_tokens;
            }
            if n.max_seconds.is_some() {
                out.max_seconds = n.max_seconds;
            }
        }
        out
    }
}
