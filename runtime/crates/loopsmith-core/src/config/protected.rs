//! What the loop may never change about itself.
//!
//! Self-evolution is only safe if the thing being evolved cannot reach the
//! machinery that constrains it. A loop that may rewrite its own stop gates has
//! no stop gates; a loop that may rewrite its own permission grant has no
//! permissions. The list here is the fixed point of that argument — it is
//! checked by the gate, which is compiled code the loop cannot dispatch to.
//!
//! The defaults are deliberately the whole reference list. Removing an entry is
//! possible but is exactly the kind of edit that should be visible in a diff.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A part of the configuration that evolution proposals may not touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedComponent {
    /// `safety.gates` — the layered exits.
    Gates,
    /// `safety.limits` — forbidden paths and commands, budget ceilings.
    Limits,
    /// `safety.recovery` — how failures are answered.
    Recovery,
    /// This list itself. Always protected; see [`Protected::is_protected`].
    Protected,
    /// Human checkpoints and approval requirements.
    Approvals,
    /// Credential and secret configuration.
    Credentials,
    /// The ledger, what is recorded in it, and the alerts that watch it.
    Audit,
    /// `evolution.baseline` — what a proposal is measured against. A loop that
    /// can move its own baseline can declare any change an improvement.
    Baselines,
    /// Memory retention and expiry policy.
    Retention,
    /// `environment` and `features`.
    Environment,
}

impl ProtectedComponent {
    /// The config paths this component covers, as dotted keys.
    pub fn paths(self) -> &'static [&'static str] {
        match self {
            ProtectedComponent::Gates => &["safety.gates"],
            ProtectedComponent::Limits => &["safety.limits"],
            ProtectedComponent::Recovery => &["safety.recovery"],
            ProtectedComponent::Protected => &["safety.protected"],
            ProtectedComponent::Approvals => &[
                "safety.limits.global.human_checkpoint",
                "safety.limits.per_node",
                "safety.gates.approval",
            ],
            ProtectedComponent::Credentials => {
                &["execution.providers.providers.requires_env", "secrets"]
            }
            ProtectedComponent::Audit => &["safety.alerts"],
            ProtectedComponent::Baselines => &["evolution.baseline"],
            ProtectedComponent::Retention => &["execution.memory.namespaces"],
            ProtectedComponent::Environment => &["environment", "features"],
        }
    }
}

/// The set of components evolution may not propose changes to.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Protected {
    #[serde(default = "default_components")]
    pub components: Vec<ProtectedComponent>,
    /// Extra dotted config paths to protect beyond the named components, for
    /// anything this enum does not anticipate.
    #[serde(default)]
    pub extra_paths: Vec<String>,
}

fn default_components() -> Vec<ProtectedComponent> {
    use ProtectedComponent::*;
    vec![
        Gates,
        Limits,
        Recovery,
        Protected,
        Approvals,
        Credentials,
        Audit,
        Baselines,
        Retention,
        Environment,
    ]
}

impl Default for Protected {
    fn default() -> Self {
        Self {
            components: default_components(),
            extra_paths: Vec::new(),
        }
    }
}

impl Protected {
    /// Whether a dotted config path is off limits to evolution.
    ///
    /// `safety.protected` is hard-coded rather than looked up: a config that
    /// dropped `Protected` from its own component list would otherwise be free
    /// to propose putting everything else back, which defeats the section.
    pub fn is_protected(&self, path: &str) -> bool {
        if path == "safety.protected" || path.starts_with("safety.protected.") {
            return true;
        }
        let covered = self
            .components
            .iter()
            .flat_map(|c| c.paths().iter())
            .copied()
            .chain(self.extra_paths.iter().map(String::as_str));
        covered.into_iter().any(|p| under(path, p))
    }

    /// Whether writing `path` would change anything protected: the path is
    /// protected itself, or a protected path sits beneath it.
    ///
    /// The second half is the one that matters for a patch. Replacing all of
    /// `safety` touches no *protected* key by name, and overwrites every one.
    pub fn touches(&self, path: &str) -> bool {
        self.is_protected(path) || self.paths().iter().any(|p| under(p, path))
    }

    /// Every protected path, for reporting.
    pub fn paths(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .components
            .iter()
            .flat_map(|c| c.paths().iter().map(|p| p.to_string()))
            .chain(self.extra_paths.iter().cloned())
            .collect();
        out.push("safety.protected".into());
        out.sort();
        out.dedup();
        out
    }
}

/// Whether `path` is `prefix` or sits beneath it.
///
/// The segment check matters: without it `safety.limits_extra` would be judged
/// protected by the prefix `safety.limits`, silently freezing a section the
/// author never listed.
fn under(path: &str, prefix: &str) -> bool {
    path == prefix
        || (path.len() > prefix.len()
            && path.starts_with(prefix)
            && path.as_bytes()[prefix.len()] == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protected_list_protects_itself_even_if_removed_from_its_own_list() {
        let p = Protected {
            components: vec![],
            extra_paths: vec![],
        };
        assert!(p.is_protected("safety.protected"));
        assert!(p.is_protected("safety.protected.components"));
    }

    #[test]
    fn a_sibling_sharing_a_prefix_is_not_protected_by_accident() {
        let p = Protected::default();
        assert!(p.is_protected("safety.limits"));
        assert!(p.is_protected("safety.limits.global.rules"));
        assert!(!p.is_protected("safety.limits_extra"));
    }

    #[test]
    fn replacing_a_parent_touches_the_protected_children_beneath_it() {
        let p = Protected::default();
        assert!(p.touches("safety"), "all of safety includes its gates");
        assert!(p.touches("safety.gates.stop.max_iterations"));
        assert!(!p.touches("safety.checks"), "checks are not protected");
        assert!(!p.touches("execution.skills.explore"));
    }

    #[test]
    fn an_unlisted_section_stays_editable() {
        let p = Protected::default();
        assert!(!p.is_protected("intent.goals"));
        assert!(!p.is_protected("execution.graph.nodes"));
    }
}
