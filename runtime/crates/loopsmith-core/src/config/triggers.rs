//! Section G — time and event triggers.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Five-field cron expression.
    Cron { expr: String },
    /// Fire every N seconds. Timezone-independent, which makes it the right
    /// choice for cadence that does not need to land at a wall-clock time.
    Interval { seconds: u64 },
    /// Fire when a path changes.
    FileChange { path: String },
    /// Fire when a named upstream goal becomes satisfied.
    GoalSatisfied { goal: String },
    /// Fire on demand only.
    Manual,
}
