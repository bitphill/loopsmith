//! Sub-agent acquisition policy.

use super::default_skills::TrustLevel;
use super::yes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SkillPolicy {
    /// Order in which a missing sub-agent is sourced.
    #[serde(default = "default_acquisition")]
    pub acquisition_order: Vec<AcquisitionSource>,
    /// Where auto-created skills land before a human promotes them.
    #[serde(default = "default_quarantine")]
    pub quarantine_dir: String,
    /// Minimum stars before a marketplace skill is eligible.
    #[serde(default = "default_min_stars")]
    pub min_marketplace_stars: u64,
    #[serde(default = "yes")]
    pub require_human_promotion: bool,
    /// Try a candidate sub-agent that is *not* in the config, so the loop can
    /// discover that something helps rather than only confirming what it was
    /// told. Off by default: exploration spends real money.
    #[serde(default)]
    pub explore: bool,
    /// Candidates to try when exploring, in order. Each is trialled until it
    /// has enough runs to judge.
    #[serde(default)]
    pub explore_candidates: Vec<String>,
    /// Trials needed before a candidate can be proposed or dismissed.
    #[serde(default = "default_min_trials")]
    pub min_trials: usize,

    /// Lowest trust a fetched sub-agent may carry and still be used.
    ///
    /// `Reviewed` by default, which means a freshly fetched skill — which is
    /// [`TrustLevel::Untrusted`] until someone says otherwise — is acquired
    /// into quarantine but not dispatched to. That is the intended friction:
    /// the loop can find a sub-agent on its own, but a human decides whether it
    /// runs.
    #[serde(default = "default_min_trust")]
    pub min_trust_level: TrustLevel,
    /// Refuse a fetched sub-agent that has no `checksum` pinned.
    ///
    /// Off by default because it makes discovery impossible — nothing newly
    /// found has a checksum yet. Turn it on once a loop's sub-agents have
    /// settled, and in `prod` it is the expected setting.
    #[serde(default)]
    pub require_checksum: bool,
    /// Allow a sub-agent to take effects outside the loop directory. Requires
    /// [`TrustLevel::Approved`] on the skill itself.
    #[serde(default)]
    pub allow_external_side_effects: bool,
}

fn default_min_trials() -> usize {
    3
}
fn default_min_trust() -> TrustLevel {
    TrustLevel::Reviewed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionSource {
    Installed,
    Marketplace,
    Generate,
}

impl Default for SkillPolicy {
    fn default() -> Self {
        Self {
            acquisition_order: default_acquisition(),
            quarantine_dir: default_quarantine(),
            min_marketplace_stars: default_min_stars(),
            require_human_promotion: true,
            explore: false,
            explore_candidates: vec![],
            min_trials: default_min_trials(),
            min_trust_level: default_min_trust(),
            require_checksum: false,
            allow_external_side_effects: false,
        }
    }
}

impl SkillPolicy {
    /// Whether a sub-agent at this trust level may be dispatched to.
    pub fn admits(&self, level: TrustLevel) -> bool {
        level >= self.min_trust_level
    }
}

fn default_acquisition() -> Vec<AcquisitionSource> {
    vec![
        AcquisitionSource::Installed,
        AcquisitionSource::Marketplace,
        AcquisitionSource::Generate,
    ]
}
fn default_quarantine() -> String {
    "generated-skills".into()
}
fn default_min_stars() -> u64 {
    100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_freshly_fetched_skill_is_not_dispatched_to_by_default() {
        // Acquisition and use are deliberately separate acts. The loop may
        // find a sub-agent unattended; running it needs a human first.
        let p = SkillPolicy::default();
        assert!(!p.admits(TrustLevel::Untrusted));
        assert!(p.admits(TrustLevel::Reviewed));
        assert!(p.admits(TrustLevel::Approved));
    }

    #[test]
    fn lowering_the_bar_admits_everything_below_it() {
        let p = SkillPolicy {
            min_trust_level: TrustLevel::Untrusted,
            ..SkillPolicy::default()
        };
        assert!(p.admits(TrustLevel::Untrusted));
    }
}
