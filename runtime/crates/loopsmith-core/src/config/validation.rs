//! Section D — how each goal is checked.

use super::yes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Subjective,
    Objective,
    Percentage,
}

/// How a validation is actually decided. Ordered by the independence ladder
/// from the cheat sheet: `Judge` is rung 3, everything else is rung 4.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Detector {
    /// Run a command; exit code 0 passes. The strongest detector available.
    Script {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        expect_exit: Option<i32>,
    },
    /// A path must exist (optionally non-empty).
    FileExists {
        path: String,
        #[serde(default)]
        non_empty: bool,
    },
    /// A regex must match the named artifact.
    RegexMatch { artifact: String, pattern: String },
    /// A numeric metric compared against a threshold.
    Threshold {
        metric: String,
        op: CompareOp,
        value: f64,
    },
    /// A model verdict. Requires a judge whose provider differs from the
    /// builder's, otherwise the gate refuses it as non-independent.
    Judge {
        /// Name the external standard the judge checks against. Naming a
        /// standard is what turns an opinion into a check.
        standard: String,
        #[serde(default)]
        min_score: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl CompareOp {
    pub fn apply(self, lhs: f64, rhs: f64) -> bool {
        match self {
            CompareOp::Gt => lhs > rhs,
            CompareOp::Gte => lhs >= rhs,
            CompareOp::Lt => lhs < rhs,
            CompareOp::Lte => lhs <= rhs,
            CompareOp::Eq => (lhs - rhs).abs() < f64::EPSILON,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    /// Goal name, or `overall`.
    pub target: String,
    pub name: String,
    pub mode: Mode,
    /// Natural-language statement of what is being checked.
    pub statement: String,
    pub detector: Detector,
    /// A validation that must pass for the target to be satisfied. Non-blocking
    /// validations are recorded but do not hold the gate shut.
    #[serde(default = "yes")]
    pub blocking: bool,
}
