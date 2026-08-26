//! The example library shown down the left of the web UI.
//!
//! Thirteen working loops ship inside the binary. That is the point: the
//! fastest way to understand what a validation or a stop gate is for is to
//! load one that already works and read it with every field explained beside
//! it, which beats staring at an empty form and a schema reference.
//!
//! Examples come from three places, in priority order, and the first to claim
//! an id wins:
//!
//! 1. `~/.loopsmith/examples/*.yaml` — the user's own. Theirs beats ours.
//! 2. `config/examples/*.yaml` relative to the working directory — live edits
//!    in a checkout of this repo show up without a rebuild.
//! 3. Compiled in — always present, including for someone who installed from
//!    crates.io and has no checkout at all.
//!
//! Nothing here is hand-written metadata. Titles, blurbs, and counts are
//! parsed out of the configs themselves, so an example that changes cannot
//! disagree with its own card.

use loopsmith_core::LoopConfig;
use serde::Serialize;
use std::path::PathBuf;

/// Compiled-in copies of `config/examples/*.yaml`.
///
/// `include_str!` cannot reach above the package root and `config/` is
/// excluded from the published tarball, so these are copies kept in sync by
/// `tools/sync-examples.sh`. The test at the bottom of this file is what
/// stops the copies from drifting.
const EMBEDDED: &[(&str, &str)] = &[
    ("account-watch-loop.yaml", include_str!("../../templates/examples/account-watch-loop.yaml")),
    ("blogger-loop.yaml", include_str!("../../templates/examples/blogger-loop.yaml")),
    ("cold-outreach-loop.yaml", include_str!("../../templates/examples/cold-outreach-loop.yaml")),
    ("idea-radar-loop.yaml", include_str!("../../templates/examples/idea-radar-loop.yaml")),
    ("landing-page-loop.yaml", include_str!("../../templates/examples/landing-page-loop.yaml")),
    ("marketing-automation-loop.yaml", include_str!("../../templates/examples/marketing-automation-loop.yaml")),
    ("refactor-loop.yaml", include_str!("../../templates/examples/refactor-loop.yaml")),
    ("research-loop.yaml", include_str!("../../templates/examples/research-loop.yaml")),
    ("sales-leads-loop.yaml", include_str!("../../templates/examples/sales-leads-loop.yaml")),
    ("traffic-loop.yaml", include_str!("../../templates/examples/traffic-loop.yaml")),
    ("trend-radar-loop.yaml", include_str!("../../templates/examples/trend-radar-loop.yaml")),
    ("viral-game-loop.yaml", include_str!("../../templates/examples/viral-game-loop.yaml")),
    ("x402-agent-loop.yaml", include_str!("../../templates/examples/x402-agent-loop.yaml")),
];

#[derive(Debug, Clone, Serialize)]
pub struct ExampleCard {
    /// File stem — `blogger-loop`. Stable, and what the load endpoint takes.
    pub id: String,
    /// The loop's own `name`.
    pub name: String,
    /// The loop's own `description`, which is what it is for in one line.
    pub blurb: String,
    /// `embedded`, `repo`, or `user` — where this copy came from.
    pub origin: String,
    pub goals: usize,
    pub validations: usize,
    /// How many of those validations are decided by a model rather than by a
    /// script. Worth surfacing: a loop judged entirely by models is a loop
    /// whose gate is an opinion.
    pub judge_validations: usize,
    pub nodes: usize,
    pub providers: usize,
    /// `manual`, `every 6h`, `cron 0 9 * * *` — how this loop is triggered.
    pub trigger: String,
    /// The stop gate a newcomer most needs to see before pressing run.
    pub max_iterations: u32,
    pub max_cost_usd: Option<f64>,
}

/// Everything on offer, best source first, one card per id.
pub fn list() -> Vec<ExampleCard> {
    let mut cards: Vec<ExampleCard> = Vec::new();

    for (id, text, origin) in sources() {
        if cards.iter().any(|c| c.id == id) {
            continue;
        }
        // An example that will not parse is dropped rather than shown as a
        // card that fails on click. Ours cannot fail — the test below proves
        // it — so in practice this only ever hides a broken user file.
        if let Some(card) = card_for(&id, &text, origin) {
            cards.push(card);
        }
    }
    cards.sort_by(|a, b| a.name.cmp(&b.name));
    cards
}

/// The raw YAML for one example, for the Load button.
pub fn raw(id: &str) -> Option<String> {
    sources()
        .into_iter()
        .find(|(candidate, _, _)| candidate == id)
        .map(|(_, text, _)| text)
}

/// `(id, text, origin)` in priority order: user, then repo, then compiled in.
fn sources() -> Vec<(String, String, &'static str)> {
    let mut out: Vec<(String, String, &'static str)> = Vec::new();

    let mut dirs: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(home) = crate::web::detect::home_dir() {
        dirs.push((home.join(".loopsmith/examples"), "user"));
    }
    dirs.push((PathBuf::from("config/examples"), "repo"));

    for (dir, origin) in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        found.sort();
        for path in found {
            let is_yaml = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "yaml" || e == "yml");
            if !is_yaml {
                continue;
            }
            let (Some(stem), Ok(text)) = (
                path.file_stem().and_then(|s| s.to_str()),
                std::fs::read_to_string(&path),
            ) else {
                continue;
            };
            out.push((stem.to_string(), text, origin));
        }
    }

    for (file, text) in EMBEDDED {
        let stem = file.trim_end_matches(".yaml");
        out.push((stem.to_string(), (*text).to_string(), "embedded"));
    }
    out
}

fn card_for(id: &str, text: &str, origin: &'static str) -> Option<ExampleCard> {
    let cfg: LoopConfig = serde_yaml::from_str(text).ok()?;
    Some(ExampleCard {
        id: id.to_string(),
        name: cfg.name.clone(),
        blurb: crate::web::detect::truncate(&cfg.description, 180),
        origin: origin.to_string(),
        goals: cfg.goals.len(),
        validations: cfg.validations.len(),
        judge_validations: cfg
            .validations
            .iter()
            .filter(|v| matches!(v.detector, loopsmith_core::Detector::Judge { .. }))
            .count(),
        nodes: cfg.graph.nodes.len(),
        providers: cfg.providers.providers.len(),
        trigger: describe_triggers(&cfg.schedules),
        max_iterations: cfg.stop_gates.max_iterations,
        max_cost_usd: cfg.stop_gates.max_cost_usd,
    })
}

/// The schedule in the words someone would use out loud.
fn describe_triggers(triggers: &[loopsmith_core::Trigger]) -> String {
    use loopsmith_core::Trigger;
    if triggers.is_empty() {
        return "manual".into();
    }
    let parts: Vec<String> = triggers
        .iter()
        .map(|t| match t {
            Trigger::Manual => "manual".to_string(),
            Trigger::Cron { expr } => format!("cron {expr}"),
            Trigger::Interval { seconds } => format!("every {}", human_seconds(*seconds)),
            Trigger::FileChange { path } => format!("when {path} changes"),
            Trigger::GoalSatisfied { goal } => format!("when {goal} is met"),
        })
        .collect();
    parts.join(", ")
}

pub fn human_seconds(s: u64) -> String {
    match s {
        0..=90 => format!("{s}s"),
        91..=5400 => format!("{}m", s / 60),
        5401..=172_800 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_compiled_in_example_parses() {
        // A broken example is worse than a missing one: it is offered on the
        // shelf and fails on click, in front of exactly the newcomer this
        // library exists for.
        assert!(!EMBEDDED.is_empty(), "no examples were compiled in");
        for (file, text) in EMBEDDED {
            let cfg: LoopConfig = serde_yaml::from_str(text)
                .unwrap_or_else(|e| panic!("{file} does not parse: {e}"));
            assert!(!cfg.goals.is_empty(), "{file} has no goals");
            assert!(!cfg.validations.is_empty(), "{file} has no validations");
            assert!(
                !cfg.description.trim().is_empty(),
                "{file} has no description, so its card would be blank"
            );
        }
    }

    #[test]
    fn embedded_examples_match_the_source_of_truth() {
        // Guards `tools/sync-examples.sh` having been run. Skipped when
        // `config/` is absent, which is the published-tarball case: there the
        // compiled-in copy is not a copy of anything, it is the original.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../config/examples");
        if !root.is_dir() {
            return;
        }
        for (file, text) in EMBEDDED {
            let source = root.join(file);
            let expected = std::fs::read_to_string(&source)
                .unwrap_or_else(|e| panic!("{} is missing: {e}", source.display()));
            assert_eq!(
                *text, expected,
                "{file} has drifted from config/examples. Run tools/sync-examples.sh"
            );
        }
        let on_disk = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "yaml"))
            .count();
        assert_eq!(
            on_disk,
            EMBEDDED.len(),
            "config/examples has {on_disk} examples but {} are compiled in. \
             Run tools/sync-examples.sh",
            EMBEDDED.len()
        );
    }

    #[test]
    fn cards_carry_what_a_newcomer_needs_to_choose_one() {
        let cards = list();
        assert!(cards.len() >= EMBEDDED.len(), "every example gets a card");
        for c in &cards {
            assert!(!c.name.is_empty(), "{} has no name", c.id);
            assert!(!c.blurb.is_empty(), "{} has no blurb", c.id);
            assert!(c.goals > 0 && c.validations > 0, "{} looks empty", c.id);
            assert!(!c.trigger.is_empty());
        }
    }

    #[test]
    fn raw_returns_the_yaml_the_card_was_built_from() {
        let first = &list()[0];
        let text = raw(&first.id).expect("the card's own id resolves");
        let cfg: LoopConfig = serde_yaml::from_str(&text).unwrap();
        assert_eq!(cfg.name, first.name);
    }

    #[test]
    fn an_unknown_id_is_none_not_a_panic() {
        assert!(raw("no-such-example").is_none());
    }

    #[test]
    fn intervals_read_as_english_not_as_seconds() {
        assert_eq!(human_seconds(30), "30s");
        assert_eq!(human_seconds(600), "10m");
        assert_eq!(human_seconds(21_600), "6h");
        assert_eq!(human_seconds(604_800), "7d");
    }
}
