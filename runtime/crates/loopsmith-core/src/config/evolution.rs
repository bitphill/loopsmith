//! How the loop is allowed to improve itself.
//!
//! loopsmith could already trial sub-agents and write proposals. What it could
//! not do was say whether a proposal was an *improvement*, because there was
//! nothing to compare against. A loop that measures a change only against its
//! own most recent run will ratchet toward whatever it happened to do last.
//!
//! The baseline fixes that: it is a frozen set of numbers a proposal must beat,
//! and it is a protected component, so the loop cannot move the goalposts it is
//! being measured against. Everything here is off unless
//! `features.self_evolution` is on.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::yes;

/// The frozen numbers a proposal is measured against.
///
/// Every field is optional: a loop that only cares about cost sets only cost.
/// A metric left unset is not compared, which is different from being compared
/// against zero.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    /// Fraction of runs that reached overall success.
    #[serde(default)]
    pub completion_rate: Option<f64>,
    /// Fraction of blocking validations that passed.
    #[serde(default)]
    pub validation_pass_rate: Option<f64>,
    /// Mean cost of a successful run.
    #[serde(default)]
    pub cost_usd: Option<f64>,
    /// Mean wall-clock of a successful run.
    #[serde(default)]
    pub latency_seconds: Option<f64>,
    /// Mean iterations to reach overall success.
    #[serde(default)]
    pub iterations_to_success: Option<f64>,
    /// When these numbers were measured, as an ISO-8601 date. Recorded rather
    /// than enforced: a baseline nobody can date is a baseline nobody can
    /// argue with.
    #[serde(default)]
    pub measured_at: Option<String>,
}

/// What kind of change a proposal is asking for.
///
/// The list is closed, and deliberately does not include anything under
/// `safety`. A proposal that wants to relax a limit is not a proposal; it is a
/// request for a human to edit the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    NewSkill,
    SkillUpdate,
    PromptChange,
    GraphChange,
    ProviderRouting,
    ValidationChange,
    SuccessCriteria,
}

/// The evolution policy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Evolution {
    /// Master switch for this section. Even with `features.self_evolution` on,
    /// a loop can leave evolution off for a single run.
    #[serde(default)]
    pub enabled: bool,
    /// What proposals are measured against. Without one, no proposal can be
    /// adopted — only recorded.
    #[serde(default)]
    pub baseline: Option<Baseline>,
    /// How much a metric may worsen and still count as an improvement overall.
    /// Expressed as a fraction: `0.02` allows a two-percent regression.
    ///
    /// Nonzero on purpose. A change that trades a hair of accuracy for half the
    /// cost is usually right, and a zero-tolerance gate refuses every such
    /// trade while admitting any change that touches nothing measured.
    #[serde(default = "default_max_regression")]
    pub max_regression: f64,
    /// Which kinds of change may be proposed at all.
    #[serde(default = "default_kinds")]
    pub allowed_kinds: Vec<ProposalKind>,
    /// Require a proposal to have been trialled in isolation before adoption.
    #[serde(default = "yes")]
    pub require_sandbox: bool,
    /// Require a human to approve adoption. Turning this off is refused in
    /// `prod`.
    #[serde(default = "yes")]
    pub require_approval: bool,
    /// Keep the previous known-good configuration so an adoption can be undone.
    #[serde(default = "yes")]
    pub keep_rollback: bool,
}

fn default_max_regression() -> f64 {
    0.02
}

fn default_kinds() -> Vec<ProposalKind> {
    use ProposalKind::*;
    vec![NewSkill, SkillUpdate, PromptChange, ValidationChange]
}

impl Default for Evolution {
    fn default() -> Self {
        Self {
            enabled: false,
            baseline: None,
            max_regression: default_max_regression(),
            allowed_kinds: default_kinds(),
            require_sandbox: true,
            require_approval: true,
            keep_rollback: true,
        }
    }
}

impl Evolution {
    /// Whether a proposal of this kind may even be written.
    pub fn allows(&self, kind: ProposalKind) -> bool {
        self.enabled && self.allowed_kinds.contains(&kind)
    }

    /// Whether `measured` beats `baseline` on every metric the baseline names,
    /// within [`Evolution::max_regression`].
    ///
    /// Returns `None` when there is no baseline — which is not "pass", and the
    /// caller must not treat it as one.
    pub fn is_improvement(&self, measured: &Baseline) -> Option<bool> {
        let base = self.baseline.as_ref()?;
        let tol = self.max_regression;

        // Higher is better.
        let up = [
            (base.completion_rate, measured.completion_rate),
            (base.validation_pass_rate, measured.validation_pass_rate),
        ];
        // Lower is better.
        let down = [
            (base.cost_usd, measured.cost_usd),
            (base.latency_seconds, measured.latency_seconds),
            (base.iterations_to_success, measured.iterations_to_success),
        ];

        for (b, m) in up {
            if let (Some(b), Some(m)) = (b, m) {
                if m < b - (b.abs() * tol) {
                    return Some(false);
                }
            }
        }
        for (b, m) in down {
            if let (Some(b), Some(m)) = (b, m) {
                if m > b + (b.abs() * tol) {
                    return Some(false);
                }
            }
        }
        Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Baseline {
        Baseline {
            completion_rate: Some(0.80),
            cost_usd: Some(1.00),
            ..Baseline::default()
        }
    }

    fn with_baseline() -> Evolution {
        Evolution {
            enabled: true,
            baseline: Some(base()),
            ..Evolution::default()
        }
    }

    #[test]
    fn no_baseline_is_not_a_pass() {
        // The dangerous failure would be treating "nothing to compare against"
        // as "compared, and fine".
        let e = Evolution {
            enabled: true,
            ..Evolution::default()
        };
        assert_eq!(e.is_improvement(&base()), None);
    }

    #[test]
    fn a_real_regression_is_refused() {
        let worse = Baseline {
            completion_rate: Some(0.50),
            ..base()
        };
        assert_eq!(with_baseline().is_improvement(&worse), Some(false));
    }

    #[test]
    fn a_regression_inside_tolerance_is_allowed() {
        // 0.792 is 1% below 0.80, inside the 2% default.
        let slightly_worse = Baseline {
            completion_rate: Some(0.792),
            cost_usd: Some(0.50),
            ..base()
        };
        assert_eq!(with_baseline().is_improvement(&slightly_worse), Some(true));
    }

    #[test]
    fn cost_going_up_counts_against_a_proposal() {
        let pricier = Baseline {
            cost_usd: Some(2.00),
            ..base()
        };
        assert_eq!(with_baseline().is_improvement(&pricier), Some(false));
    }

    #[test]
    fn safety_sections_are_not_a_proposable_kind() {
        // The enum is the enforcement: there is no variant that names a limit,
        // a gate, or a permission, so no proposal can ask to change one.
        let names = format!("{:?}", default_kinds());
        for forbidden in ["Limit", "Gate", "Permission", "Protected"] {
            assert!(!names.contains(forbidden), "{forbidden} must not be proposable");
        }
    }
}
