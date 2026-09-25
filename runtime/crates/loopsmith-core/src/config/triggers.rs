//! What starts a run, and what stops a run from starting itself forever.
//!
//! The triggers themselves are unchanged from section G. What is new is the
//! policy around them, and it exists because of a shape the reference
//! specification names and loopsmith could previously build by accident: a
//! `goal_satisfied` trigger whose run satisfies the goal again fires itself,
//! and a `file_change` trigger watching a directory its own run writes to does
//! the same. Neither is exotic, and before `max_depth` there was nothing
//! between that config and an unbounded fork bomb of runs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
    Manual {},
}

impl Trigger {
    /// Whether this trigger can be fired by the loop's own output, and so
    /// needs the depth cap to mean anything.
    pub fn is_self_reachable(&self) -> bool {
        matches!(self, Trigger::FileChange { .. } | Trigger::GoalSatisfied { .. })
    }
}

/// A trigger plus the bookkeeping that makes firing it safe.
///
/// The trigger is nested under `on:` rather than flattened into this struct.
/// Flattening would read more naturally and would have let the 0.3 spelling
/// parse untouched, but serde refuses `deny_unknown_fields` on any struct with
/// a flattened field — and losing that here would mean a misspelled
/// `idempotency_kye` is silently dropped, leaving a trigger with no dedup and
/// no warning. The legacy transform wraps old bare triggers in `on:` instead,
/// which costs one line there and keeps the guard everywhere.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TriggerSpec {
    #[serde(rename = "on")]
    pub trigger: Trigger,
    /// Stable identity for a firing. Two firings sharing a key inside the
    /// dedup window are the same firing, and the second is dropped.
    ///
    /// Left unset, one is derived from the trigger and its payload, which is
    /// right for cron and interval. Set it explicitly when the natural payload
    /// is noisier than the event — a `file_change` on a directory where six
    /// files land together is one event, not six.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Skip this trigger without deleting it.
    #[serde(default = "super::yes")]
    pub enabled: bool,
}

impl From<Trigger> for TriggerSpec {
    fn from(trigger: Trigger) -> Self {
        TriggerSpec {
            trigger,
            idempotency_key: None,
            enabled: true,
        }
    }
}

/// The triggers and the re-entrancy guards around them.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TriggerPolicy {
    #[serde(default)]
    pub triggers: Vec<TriggerSpec>,
    /// How many times a run may be started by a run that was itself started by
    /// a trigger. Depth 0 is a run a human started.
    ///
    /// Reaching the cap stops the chain and records why. Without it a
    /// self-reachable trigger has no bound at all.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Two firings with the same idempotency key this many seconds apart are
    /// treated as one.
    #[serde(default = "default_dedup_window")]
    pub dedup_window_seconds: u64,
}

fn default_max_depth() -> u32 {
    5
}
fn default_dedup_window() -> u64 {
    300
}

impl Default for TriggerPolicy {
    fn default() -> Self {
        Self {
            triggers: Vec::new(),
            max_depth: default_max_depth(),
            dedup_window_seconds: default_dedup_window(),
        }
    }
}

impl TriggerPolicy {
    /// Whether a run at `depth` is allowed to start another.
    pub fn may_chain(&self, depth: u32) -> bool {
        depth < self.max_depth
    }

    /// Whether any configured trigger can be fired by this loop's own output.
    pub fn has_self_reachable_trigger(&self) -> bool {
        self.triggers
            .iter()
            .any(|t| t.enabled && t.trigger.is_self_reachable())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trigger_spec_defaults_to_enabled_with_a_derived_key() {
        let t: TriggerSpec = serde_yaml::from_str("on:\n  type: cron\n  expr: \"0 2 * * *\"\n")
            .expect("trigger spec parses");
        assert_eq!(
            t.trigger,
            Trigger::Cron {
                expr: "0 2 * * *".into()
            }
        );
        assert!(t.enabled);
        assert!(t.idempotency_key.is_none());
    }

    #[test]
    fn a_misspelled_bookkeeping_key_is_refused_not_dropped() {
        // This is the whole reason the trigger is nested rather than flattened.
        // Under a flattened layout serde cannot deny unknown fields, and this
        // config would parse into a trigger with no dedup key and no warning.
        let err = serde_yaml::from_str::<TriggerSpec>(
            "on:\n  type: manual\nidempotency_kye: dupe\n",
        )
        .expect_err("a misspelled key must be refused");
        assert!(err.to_string().contains("idempotency_kye"), "got: {err}");
    }

    #[test]
    fn chaining_stops_at_the_cap() {
        let p = TriggerPolicy::default();
        assert!(p.may_chain(0));
        assert!(p.may_chain(4));
        assert!(!p.may_chain(5), "depth 5 must not start a sixth run");
        assert!(!p.may_chain(99));
    }

    #[test]
    fn goal_and_file_triggers_are_the_self_reachable_ones() {
        // These are the two a run's own output can fire. Cron and interval are
        // driven by the clock, which the run cannot advance.
        assert!(Trigger::GoalSatisfied { goal: "g".into() }.is_self_reachable());
        assert!(Trigger::FileChange { path: "out".into() }.is_self_reachable());
        assert!(!Trigger::Cron { expr: "* * * * *".into() }.is_self_reachable());
        assert!(!Trigger::Interval { seconds: 60 }.is_self_reachable());
        assert!(!Trigger::Manual {}.is_self_reachable());
    }
}
