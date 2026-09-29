//! Section C — named goals.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub name: String,
    /// Natural language. Subjective phrasing is allowed here; the validation
    /// is what has to be checkable.
    pub description: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub priority: Option<u32>,
}
