//! Sub-agent acquisition policy.

use super::yes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

fn default_min_trials() -> usize {
    3
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        }
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
