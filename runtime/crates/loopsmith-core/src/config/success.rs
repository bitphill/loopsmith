//! Section E — what counts as success.

use super::validation::Mode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuccessScenario {
    /// Goal name, or `overall`.
    pub target: String,
    pub name: String,
    pub mode: Mode,
    pub statement: String,
    /// Fraction of blocking validations that must pass, when mode is
    /// `percentage`. Ignored otherwise.
    #[serde(default)]
    pub threshold: Option<f64>,
}
