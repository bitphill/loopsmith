//! Section A — static context every node receives.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InfoItem {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub note: Option<String>,
}
