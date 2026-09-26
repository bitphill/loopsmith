//! What a run measures about itself, and when a number should get a human's
//! attention.
//!
//! A stop gate ends a run; an alert does not. It exists for the numbers that
//! are worth knowing about long before they are worth stopping for — spend
//! running at twice the usual rate, retries climbing, the pass rate sliding
//! after a config change. Each alert fires at most once per run and is written
//! to the ledger, the run log, and the run's outcome, so a scheduler's email or
//! a CI step can act on it without parsing prose.
//!
//! Alerts are covered by the `audit` protected component: a loop that could
//! edit its own alert thresholds could silence the one signal meant to catch
//! it drifting.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A number the engine keeps for every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Iterations completed so far.
    Iterations,
    /// Tokens charged so far, estimated where a provider reported none.
    TokensUsed,
    /// Dollars charged so far.
    CostUsd,
    /// Seconds since the run started.
    WallClockSeconds,
    /// Node dispatches that failed, after recovery had its say.
    FailedDispatches,
    /// Dispatches recovery sent round again: retries and revisions.
    Retries,
    /// Consecutive iterations in which no ruling moved.
    StaleIterations,
    /// Fraction of blocking checks passing at the latest ruling, 0 to 1.
    ValidationPassRate,
}

impl Metric {
    pub fn as_str(self) -> &'static str {
        match self {
            Metric::Iterations => "iterations",
            Metric::TokensUsed => "tokens_used",
            Metric::CostUsd => "cost_usd",
            Metric::WallClockSeconds => "wall_clock_seconds",
            Metric::FailedDispatches => "failed_dispatches",
            Metric::Retries => "retries",
            Metric::StaleIterations => "stale_iterations",
            Metric::ValidationPassRate => "validation_pass_rate",
        }
    }
}

/// One threshold on one metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Alert {
    /// Stable name, used in the ledger and the run's outcome.
    pub id: String,
    pub metric: Metric,
    /// Fire when the metric rises above this.
    #[serde(default)]
    pub above: Option<f64>,
    /// Fire when the metric falls below this.
    #[serde(default)]
    pub below: Option<f64>,
    /// What to tell the human, in their words. Defaults to a sentence built
    /// from the metric and the threshold.
    #[serde(default)]
    pub message: Option<String>,
}

impl Alert {
    /// Whether `value` crosses this alert's threshold.
    pub fn fires_at(&self, value: f64) -> bool {
        self.above.is_some_and(|t| value > t) || self.below.is_some_and(|t| value < t)
    }

    /// The line written when it fires.
    pub fn describe(&self, value: f64) -> String {
        if let Some(m) = &self.message {
            return format!("{} ({} = {})", m, self.metric.as_str(), trim(value));
        }
        let bound = match (self.above, self.below) {
            (Some(t), _) if value > t => format!("above {}", trim(t)),
            (_, Some(t)) if value < t => format!("below {}", trim(t)),
            _ => "outside its bounds".to_string(),
        };
        format!("{} is {} ({bound})", self.metric.as_str(), trim(value))
    }
}

fn trim(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(above: Option<f64>, below: Option<f64>) -> Alert {
        Alert {
            id: "a".into(),
            metric: Metric::CostUsd,
            above,
            below,
            message: None,
        }
    }

    #[test]
    fn an_alert_fires_strictly_past_its_threshold() {
        let a = alert(Some(2.0), None);
        assert!(!a.fires_at(2.0), "at the threshold is not past it");
        assert!(a.fires_at(2.01));
        let b = alert(None, Some(0.5));
        assert!(b.fires_at(0.4));
        assert!(!b.fires_at(0.5));
    }

    #[test]
    fn the_default_message_names_the_metric_and_the_bound() {
        assert_eq!(
            alert(Some(2.0), None).describe(3.5),
            "cost_usd is 3.5 (above 2)"
        );
    }
}
