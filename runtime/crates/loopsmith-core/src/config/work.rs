//! Section B — the manual work that must happen before automation is allowed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItem {
    pub step: String,
    /// Must be true before the loop is allowed to run. This encodes the
    /// corpus rule that you cannot automate a process you cannot describe.
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub evidence: Option<String>,
}
