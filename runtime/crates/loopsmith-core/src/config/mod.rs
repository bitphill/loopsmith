//! The A–H config model, one module per section.
//!
//! The split follows the template's own section boundaries rather than Rust
//! convenience, so "where does `stop_gates` live" has the same answer in the
//! docs, the schema, and the code.
//!
//! Every struct here carries `deny_unknown_fields`. Without it a misspelled key
//! is silently dropped and the loop runs with a default the author never chose
//! — which is exactly how `max_revisions_per_node` came to be documented in
//! four places and read in none.

use serde::{Deserialize, Serialize};

pub mod constraints;
pub mod context;
pub mod default_skills;
pub mod gates;
pub mod goals;
pub mod graph;
pub mod guidelines;
pub mod info;
pub mod providers;
pub mod skills;
pub mod success;
pub mod triggers;
pub mod validation;
pub mod work;

pub use constraints::{ConstraintSet, Constraints};
pub use context::ContextPolicy;
pub use default_skills::{is_safe_repo_url, DefaultSkill, SkillOrigin};
pub use gates::StopGates;
pub use goals::Goal;
pub use graph::{Concurrency, GraphSpec, NodeSpec, Role, Tier};
pub use guidelines::{parse_chain, ExecutionGuidelines, Guideline, Phase};
pub use info::InfoItem;
pub use providers::{ProviderKind, ProviderRouting, ProviderSpec};
pub use skills::{AcquisitionSource, SkillPolicy};
pub use success::SuccessScenario;
pub use triggers::Trigger;
pub use validation::{CompareOp, Detector, Mode, Validation};
pub use work::WorkItem;

/// Reserved target name meaning "the loop as a whole" rather than one goal.
pub const OVERALL: &str = "overall";

/// Shared serde default. Named rather than inlined because four sections
/// default a boolean to true and a literal `true` cannot be a serde default.
pub(crate) fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopConfig {
    /// Loop identity. Becomes the generated skill name.
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,

    /// A — static context every node receives.
    #[serde(default)]
    pub information: Vec<InfoItem>,
    /// B — the manual work that must happen before automation is allowed.
    #[serde(default)]
    pub pre_execution: Vec<WorkItem>,
    /// C — named goals.
    pub goals: Vec<Goal>,
    /// D — how each goal is checked.
    pub validations: Vec<Validation>,
    /// E — what counts as success.
    #[serde(default)]
    pub success: Vec<SuccessScenario>,
    /// F — the layered exits.
    #[serde(default)]
    pub stop_gates: StopGates,
    /// G — time and event triggers.
    #[serde(default)]
    pub schedules: Vec<Trigger>,
    /// H — constraints applied per node or globally.
    #[serde(default)]
    pub constraints: Constraints,
    /// I — named phases with their own standing instruction and ordering.
    #[serde(default)]
    pub execution_guidelines: ExecutionGuidelines,
    /// J — sub-agents installed before the loop starts.
    #[serde(default)]
    pub default_skills: Vec<DefaultSkill>,

    /// Execution graph. Nodes are units of work; edges are real dependencies.
    #[serde(default)]
    pub graph: GraphSpec,
    /// Provider routing. Every provider is a command template, so any CLI or
    /// HTTP endpoint reachable from a shell is usable without a Rust change.
    #[serde(default)]
    pub providers: ProviderRouting,
    /// Sub-agent acquisition policy.
    #[serde(default)]
    pub skills: SkillPolicy,
    /// How much of the previous iterations each prompt carries.
    #[serde(default)]
    pub context: ContextPolicy,
}

fn default_version() -> String {
    "0.1.0".into()
}

impl LoopConfig {
    pub fn goal_names(&self) -> Vec<&str> {
        self.goals.iter().map(|g| g.name.as_str()).collect()
    }

    pub fn blocking_validations_for(&self, target: &str) -> Vec<&Validation> {
        self.validations
            .iter()
            .filter(|v| v.target == target && v.blocking)
            .collect()
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderSpec> {
        self.providers.providers.iter().find(|p| p.id == id)
    }

    /// Resolve a tier to the ordered list of provider ids to try.
    pub fn cascade_for(&self, tier: Tier) -> Vec<&ProviderSpec> {
        let key = match tier {
            Tier::Cheap => "cheap",
            Tier::Standard => "standard",
            Tier::Strong => "strong",
        };
        if let Some(ids) = self.providers.cascade.get(key) {
            return ids.iter().filter_map(|id| self.provider(id)).collect();
        }
        self.providers
            .providers
            .iter()
            .filter(|p| p.tiers.is_empty() || p.tiers.contains(&tier))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
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

    #[test]
    fn the_minimal_config_parses() {
        let cfg = parse(MINIMAL).expect("minimal config parses");
        assert_eq!(cfg.name, "t");
        assert_eq!(cfg.version, "0.1.0");
        assert_eq!(cfg.stop_gates.max_iterations, 10);
    }

    #[test]
    fn a_misspelled_top_level_section_is_refused_not_ignored() {
        // Without `deny_unknown_fields` this parses happily and the loop runs
        // with `stop_gates` at its defaults — the author's ceilings silently
        // discarded. That is how a budget cap becomes a surprise invoice.
        let typo = MINIMAL.to_string() + "stop_gate:\n  max_iterations: 2\n";
        let err = parse(&typo).expect_err("a misspelled section must be refused");
        assert!(
            err.to_string().contains("stop_gate"),
            "the error must name the offending key, got: {err}"
        );
    }

    #[test]
    fn a_misspelled_nested_field_is_refused_not_ignored() {
        let typo = MINIMAL.to_string() + "stop_gates:\n  max_iteration: 2\n";
        let err = parse(&typo).expect_err("a misspelled field must be refused");
        assert!(err.to_string().contains("max_iteration"), "got: {err}");
    }

    #[test]
    fn provider_kind_aliases_still_resolve() {
        // The aliases are the reason nobody has to remember that snake_case
        // renders `OpenAi` as `open_ai`. They must survive the section split.
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
                "{MINIMAL}providers:\n  providers:\n    - id: p\n      kind: {written}\n      command: echo\n"
            );
            let cfg = parse(&text).unwrap_or_else(|e| panic!("`{written}` should parse: {e}"));
            assert_eq!(cfg.providers.providers[0].kind, expected, "for `{written}`");
        }
    }
}
