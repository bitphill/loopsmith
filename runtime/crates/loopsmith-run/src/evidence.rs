//! What the gate is shown.
//!
//! Deliberately narrow: a node's own claim that it finished is not evidence,
//! so only artifacts on disk, reported metrics, and parsed judge verdicts
//! count.

use loopsmith_core::LoopConfig;
use loopsmith_gate::{Evidence, Judgment, TargetVerdict};
use std::collections::BTreeMap;
use std::path::Path;

/// Collect evidence for the gate.
///
/// Artifacts are the files the config's own `file_exists` detectors name, read
/// from disk and registered under both their full path and their stem. Without
/// this a `regex_match` detector has nothing to match against and reports
/// "artifact was not collected" forever — a check that looks like rigour while
/// being permanently unsatisfiable, which is the worst kind.
pub fn collect_evidence(
    cfg: &LoopConfig,
    workdir: &Path,
    metrics_file: Option<&Path>,
    judgments: Vec<Judgment>,
) -> Evidence {
    let mut ev = Evidence::new(workdir);
    if let Some(p) = metrics_file {
        if let Ok(text) = std::fs::read_to_string(p) {
            if let Ok(map) = serde_json::from_str::<BTreeMap<String, f64>>(&text) {
                ev.metrics = map;
            }
        }
    }
    for path in artifact_paths(cfg) {
        let Ok(text) = std::fs::read_to_string(workdir.join(&path)) else {
            continue;
        };
        if let Some(stem) = Path::new(&path).file_stem().and_then(|s| s.to_str()) {
            ev.artifacts.insert(stem.to_string(), text.clone());
        }
        ev.artifacts.insert(path, text);
    }
    ev.judgments = judgments;
    ev
}

/// Every file the config's `file_exists` detectors name.
///
/// That set is the config's own answer to "what is this loop supposed to
/// produce", so it is the right thing to make readable to regex checks. A
/// regex naming anything else is reported by validation rather than failing
/// silently at runtime.
fn artifact_paths(cfg: &LoopConfig) -> Vec<String> {
    cfg.safety
        .checks
        .iter()
        .filter_map(|v| match &v.detector {
            loopsmith_core::Detector::FileExists { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect()
}

/// Blocking checks that failed, as `(target, check name, evidence)`.
pub(crate) fn failing_checks(
    verdicts: Option<&BTreeMap<String, TargetVerdict>>,
) -> Vec<(String, String, String)> {
    let Some(v) = verdicts else {
        return vec![];
    };
    v.values()
        .flat_map(|verdict| {
            verdict
                .checks
                .iter()
                .filter(|c| c.blocking && !c.passed)
                .map(|c| (verdict.target.clone(), c.name.clone(), c.evidence.clone()))
        })
        .collect()
}
