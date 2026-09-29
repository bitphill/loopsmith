//! Which deployment this config describes, and which risky capabilities are
//! unlocked in it.
//!
//! Both live at the config root rather than inside a bundle because both are
//! read *before* anything else is interpreted: the environment decides how
//! strictly the rest is enforced, and a feature flag can switch off a whole
//! bundle regardless of what that bundle says.
//!
//! Every flag defaults to the safe answer. A loop that says nothing acquires no
//! marketplace skills, evolves nothing, takes no external side effect, and asks
//! a human before anything irreversible. Turning a capability on is always an
//! explicit act by the author.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::yes;

/// Which deployment this config describes.
///
/// `Prod` is not merely a label: the gate refuses an unreviewed marketplace
/// skill and an unapproved evolution proposal outright in production, where in
/// `Dev` the same config only warns. Authors therefore develop against the
/// looser setting and get told what would have been refused, instead of
/// discovering it on the run that mattered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Dev,
    Staging,
    Prod,
}

impl Environment {
    /// Whether this environment refuses, rather than warns about, a capability
    /// used without the review its policy demands.
    pub fn enforces_strictly(self) -> bool {
        matches!(self, Environment::Staging | Environment::Prod)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Dev => "dev",
            Environment::Staging => "staging",
            Environment::Prod => "prod",
        }
    }
}

/// Capability switches, each defaulting to the conservative answer.
///
/// These are deliberately coarse. A flag answers "is this class of thing
/// allowed at all"; the bundle that owns the capability answers "under what
/// conditions". Keeping the two apart means an operator can disable a whole
/// capability without reading, or trusting, the policy that configures it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Features {
    /// Allow the loop to propose and trial changes to itself. Off by default:
    /// self-modification is the capability most worth opting into knowingly.
    #[serde(default)]
    pub self_evolution: bool,
    /// Allow sub-agents to be acquired from the marketplace. Off by default —
    /// this is the supply-chain surface.
    #[serde(default)]
    pub marketplace_skills: bool,
    /// Allow nodes to take actions that reach outside the loop directory.
    #[serde(default)]
    pub external_side_effects: bool,
    /// Allow independent nodes in a wave to run concurrently. On by default;
    /// turning it off forces strict sequential execution, which is the first
    /// thing to try when debugging a run that behaves differently under load.
    #[serde(default = "yes")]
    pub parallel_execution: bool,
    /// Honour `safety.limits.*.human_checkpoint`. On by default. Turning it
    /// off is refused outright in `prod`.
    #[serde(default = "yes")]
    pub human_approval: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            self_evolution: false,
            marketplace_skills: false,
            external_side_effects: false,
            parallel_execution: true,
            human_approval: true,
        }
    }
}
