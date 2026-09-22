//! The config model, one module per named section.
//!
//! The split follows the config's own section boundaries rather than Rust
//! convenience, so "where does `stop_gates` live" has the same answer in the
//! docs, the schema, and the code.
//!
//! Every struct here carries `deny_unknown_fields`. Without it a misspelled key
//! is silently dropped and the loop runs with a default the author never chose
//! — which is exactly how `max_revisions_per_node` came to be documented in
//! four places and read in none. The one place that constraint shaped the
//! design rather than merely decorating it is [`triggers::TriggerSpec`], which
//! nests where flattening would have read better, because serde will not allow
//! both.
//!
//! Sections are grouped into four bundles — see [`bundles`] for why — and every
//! 0.3 spelling still parses via [`legacy`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod bundles;
pub mod constraints;
pub mod default_skills;
pub mod environment;
pub mod evolution;
pub mod gates;
pub mod goals;
pub mod graph;
pub mod guidelines;
pub mod info;
pub mod legacy;
pub mod memory;
pub mod protected;
pub mod providers;
pub mod recovery;
pub mod skills;
pub mod success;
pub mod triggers;
pub mod validation;
pub mod work;

pub use bundles::{Execution, Intent, Safety};
pub use constraints::{ConstraintSet, Constraints};
pub use default_skills::{is_safe_repo_url, DefaultSkill, SkillOrigin, TrustLevel};
pub use environment::{Environment, Features};
pub use evolution::{Baseline, Evolution, ProposalKind};
pub use gates::{GateKind, GateOutcome, GateRule, Gates, StopGates};
pub use goals::Goal;
pub use graph::{Concurrency, GraphSpec, Isolation, Join, NodeSpec, Role, Tier};
pub use guidelines::{parse_chain, ExecutionGuidelines, Guideline, Phase};
pub use info::InfoItem;
pub use memory::{MemoryPolicy, NamespacePolicy, Namespaces, Promotion};
pub use protected::{Protected, ProtectedComponent};
pub use providers::{ProviderKind, ProviderRouting, ProviderSpec};
pub use recovery::{Backoff, FailureClass, Recovery, RecoveryAction};
pub use skills::{AcquisitionSource, SkillPolicy};
pub use success::SuccessScenario;
pub use triggers::{Trigger, TriggerPolicy, TriggerSpec};
pub use validation::{CompareOp, Detector, Mode, Validation};
pub use work::WorkItem;

/// Reserved target name meaning "the loop as a whole" rather than one goal.
pub const OVERALL: &str = "overall";

/// Shared serde default. Named rather than inlined because several sections
/// default a boolean to true and a literal `true` cannot be a serde default.
pub(crate) fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopConfig {
    /// Loop identity. Becomes the generated skill name.
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,

    /// Which deployment this config describes. Read before anything else,
    /// because it decides how strictly the rest is enforced.
    #[serde(default)]
    pub environment: Environment,
    /// Coarse capability switches, each defaulting to the safe answer.
    #[serde(default)]
    pub features: Features,

    /// What the loop is for, and how anyone would know it worked.
    #[serde(default)]
    pub intent: Intent,
    /// How the work gets done.
    #[serde(default)]
    pub execution: Execution,
    /// What must not happen, and when this stops.
    #[serde(default)]
    pub safety: Safety,
    /// How the loop is allowed to change itself.
    #[serde(default)]
    pub evolution: Evolution,
}

fn default_version() -> String {
    "0.1.0".into()
}

impl LoopConfig {
    pub fn goal_names(&self) -> Vec<&str> {
        self.intent.goals.iter().map(|g| g.name.as_str()).collect()
    }

    pub fn blocking_validations_for(&self, target: &str) -> Vec<&Validation> {
        self.safety.blocking_checks_for(target)
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderSpec> {
        self.execution
            .providers
            .providers
            .iter()
            .find(|p| p.id == id)
    }

    /// Resolve a tier to the ordered list of provider ids to try.
    pub fn cascade_for(&self, tier: Tier) -> Vec<&ProviderSpec> {
        let key = match tier {
            Tier::Cheap => "cheap",
            Tier::Standard => "standard",
            Tier::Strong => "strong",
        };
        if let Some(ids) = self.execution.providers.cascade.get(key) {
            return ids.iter().filter_map(|id| self.provider(id)).collect();
        }
        self.execution
            .providers
            .providers
            .iter()
            .filter(|p| p.tiers.is_empty() || p.tiers.contains(&tier))
            .collect()
    }

    /// Whether self-evolution is on at both the feature switch and the section.
    ///
    /// Two switches rather than one because they answer to different people:
    /// `features` is an operator's blanket answer for the machine, `evolution`
    /// is the author's answer for this loop. Either being off is off.
    pub fn evolution_enabled(&self) -> bool {
        self.features.self_evolution && self.evolution.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
name: t
intent:
  goals:
    - name: g1
      description: a goal with a long enough description
safety:
  checks:
    - target: g1
      name: v1
      mode: objective
      statement: it works
      detector: { type: file_exists, path: out.txt }
"#;

    /// The same loop in the 0.3 spelling.
    const LEGACY: &str = r#"
name: t
goals:
  - name: g1
    description: a goal with a long enough description
validations:
  - target: g1
    name: v1
    mode: objective
    statement: it works
    detector: { type: file_exists, path: out.txt }
"#;

    fn parse(text: &str) -> Result<LoopConfig, serde_yaml::Error> {
        serde_yaml::from_str::<LoopConfig>(text)
    }

    /// Route a document through the legacy transform the way the loader does.
    fn parse_any(text: &str) -> Result<(LoopConfig, Vec<legacy::Moved>), serde_yaml::Error> {
        let doc: serde_yaml::Value = serde_yaml::from_str(text)?;
        let (doc, moved) = legacy::migrate(&doc);
        Ok((serde_yaml::from_value(doc)?, moved))
    }

    #[test]
    fn the_minimal_config_parses() {
        let cfg = parse(MINIMAL).expect("minimal config parses");
        assert_eq!(cfg.name, "t");
        assert_eq!(cfg.version, "0.1.0");
        assert_eq!(cfg.safety.gates.stop.max_iterations, 10);
        assert_eq!(cfg.environment, Environment::Dev);
    }

    #[test]
    fn a_legacy_config_parses_to_exactly_the_same_thing() {
        // The migration is only trustworthy if the two spellings are the same
        // loop. Comparing the serialised form catches a field the transform
        // relocated but subtly altered.
        let (from_legacy, moved) = parse_any(LEGACY).expect("legacy config parses");
        let modern = parse(MINIMAL).expect("modern config parses");
        assert_eq!(
            serde_yaml::to_string(&from_legacy).unwrap(),
            serde_yaml::to_string(&modern).unwrap()
        );
        assert_eq!(moved.len(), 2, "goals and validations moved");
    }

    #[test]
    fn a_misspelled_top_level_section_is_refused_not_ignored() {
        // Without `deny_unknown_fields` this parses happily and the loop runs
        // with its gates at their defaults — the author's ceilings silently
        // discarded. That is how a budget cap becomes a surprise invoice.
        let typo = MINIMAL.to_string() + "saftey:\n  gates: {}\n";
        let err = parse(&typo).expect_err("a misspelled section must be refused");
        assert!(err.to_string().contains("saftey"), "got: {err}");
    }

    #[test]
    fn a_misspelled_nested_field_is_refused_not_ignored() {
        let typo = MINIMAL.to_string() + "  gates:\n    stop:\n      max_iteration: 2\n";
        let err = parse(&typo).expect_err("a misspelled field must be refused");
        assert!(err.to_string().contains("max_iteration"), "got: {err}");
    }

    #[test]
    fn evolution_needs_both_switches() {
        let mut cfg = parse(MINIMAL).unwrap();
        assert!(!cfg.evolution_enabled(), "off by default");

        cfg.evolution.enabled = true;
        assert!(!cfg.evolution_enabled(), "the feature switch still gates it");

        cfg.features.self_evolution = true;
        assert!(cfg.evolution_enabled());

        cfg.evolution.enabled = false;
        assert!(!cfg.evolution_enabled(), "the section still gates it");
    }

    #[test]
    fn provider_kind_aliases_still_resolve() {
        // The aliases are the reason nobody has to remember that snake_case
        // renders `OpenAi` as `open_ai`. They must survive the bundle move.
        for (written, expected) in [
            ("claude", ProviderKind::ClaudeCode),
            ("claude-code", ProviderKind::ClaudeCode),
            ("openai", ProviderKind::OpenAi),
            ("open_ai", ProviderKind::OpenAi),
            ("OpenAI", ProviderKind::OpenAi),
            ("grok", ProviderKind::GrokCli),
            ("custom", ProviderKind::Byok),
            ("MCP", ProviderKind::Mcp),
        ] {
            let text = format!(
                "{MINIMAL}execution:\n  providers:\n    providers:\n      - id: p\n        kind: {written}\n        command: echo\n"
            );
            let cfg = parse(&text).unwrap_or_else(|e| panic!("`{written}` should parse: {e}"));
            assert_eq!(
                cfg.execution.providers.providers[0].kind, expected,
                "for `{written}`"
            );
        }
    }
}
