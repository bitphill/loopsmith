//! Section A — static context every node receives.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfoItem {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub note: Option<String>,
}
