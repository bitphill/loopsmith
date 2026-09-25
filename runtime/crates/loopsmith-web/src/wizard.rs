//! The wizard, served to the browser.
//!
//! Until 1.0 the guided wizard existed twice: a thousand lines of Rust walking
//! a terminal and a thousand of TypeScript drawing cards, with nothing forcing
//! them to agree about what the questions were, what order they came in, or
//! what counted as an answer. This module is the seam that ends that. Rust owns
//! the question list; the browser fetches it and renders whatever it is told.
//!
//! Three calls, and the division of labour is the point:
//!
//! - [`spec`] hands over the whole question list as data. The browser caches
//!   it for the session and renders generically — a new question is an entry
//!   in `loopsmith_wizard::spec::sections`, not a new component.
//! - [`assemble`] takes the flat answer map back and types it, through
//!   [`loopsmith_wizard::answers::assemble_over`] and then the ordinary config
//!   loader. The browser never builds a config; it cannot produce one the CLI
//!   would refuse, because it does not produce one at all.
//! - [`unpack`] runs that backwards, so opening an existing loop — or an
//!   example, or a 0.3 file — fills the wizard in.
//!
//! Nothing here validates by hand. The issues come from the wizard's own
//! checks and the review from the same [`crate::assemble::review`] the expert
//! editor's rail uses, so the two views of a draft cannot disagree.

use loopsmith_wizard::answers::{self, Answers};
use loopsmith_wizard::spec::{self, Choice, Spec, Step};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The whole question list, plus the version the browser checks against its
/// own bundle.
pub fn spec() -> Spec {
    spec::spec()
}

/// What the browser posts to have its answers typed.
#[derive(Debug, Deserialize)]
pub struct AnswersBody {
    #[serde(default)]
    pub answers: Answers,
    /// The config these answers were unpacked from, when there was one.
    ///
    /// An edit asks about a subset of the file, so without this the sections
    /// the wizard has no question for would be silently dropped on the first
    /// save — a hand-written `safety.protected` among them.
    #[serde(default)]
    pub base: Option<serde_json::Value>,
}

/// What comes back: the answers that will not do, the config they make when
/// they all will, and the two things about the form itself that only the spec
/// can answer.
///
/// `visible` and `options` are why this is one round trip rather than a
/// re-implementation. Whether a question applies — a detector's own fields, a
/// gated section, the judge-independence question that means nothing with one
/// provider — is [`answers::holds`], and what a dynamic select offers is
/// [`answers::options_for`]. Both are exactly the kind of rule that drifts
/// when it exists in two languages, so the browser asks instead of deciding.
#[derive(Debug, Serialize)]
pub struct Assembled {
    pub issues: Vec<Issue>,
    pub config: Option<serde_json::Value>,
    pub review: Option<crate::assemble::Review>,
    /// The answer key of every question that applies right now, plus the id of
    /// every open section. Anything absent is not asked.
    pub visible: Vec<String>,
    /// Choices for the selects whose options come from earlier answers, keyed
    /// by the same answer key. Fixed selects carry their own in the spec.
    pub options: BTreeMap<String, Vec<Choice>>,
}

/// Walk the spec against the answers and record what is being asked.
///
/// One pass, in the spec's own order, so the browser can render straight down
/// the list it already has.
fn shape(spec: &Spec, answers: &Answers) -> (Vec<String>, BTreeMap<String, Vec<Choice>>) {
    let mut visible = Vec::new();
    let mut options: BTreeMap<String, Vec<Choice>> = BTreeMap::new();

    // A select whose options depend on the answers is recorded under the key
    // it fills; a fixed one is already in the spec and is not repeated here.
    let mut dynamic = |key: &str, f: &spec::Field, answers: &Answers| {
        if let spec::Input::Select { options: source } = &f.input {
            if !matches!(source, spec::Options::Fixed { .. }) {
                options.insert(key.to_string(), answers::options_for(source, answers));
            }
        }
    };

    for section in &spec.sections {
        if section.gate.is_some() {
            visible.push(section.gate_key());
        }
        if !answers::section_open(section, answers) {
            continue;
        }
        visible.push(section.id.clone());
        for step in &section.steps {
            match step {
                Step::Field(f) => {
                    if !answers::holds(f.when.as_ref(), answers, None) {
                        continue;
                    }
                    visible.push(f.id.clone());
                    dynamic(&f.id, f, answers);
                }
                Step::List(l) => {
                    if !answers::holds(l.when.as_ref(), answers, None) {
                        continue;
                    }
                    visible.push(l.id.clone());
                    for i in 0..answers::entry_count(&l.id, answers) {
                        let prefix = format!("{}[{i}]", l.id);
                        for f in &l.fields {
                            if !answers::holds(f.when.as_ref(), answers, Some(&prefix)) {
                                continue;
                            }
                            let key = format!("{prefix}.{}", f.id);
                            dynamic(&key, f, answers);
                            visible.push(key);
                        }
                    }
                }
                Step::Providers(p) => visible.push(p.id.clone()),
            }
        }
    }
    (visible, options)
}

#[derive(Debug, Serialize)]
pub struct Issue {
    /// The answer key it concerns, which is also the field's id in the spec,
    /// so the browser can scroll to the question rather than describe it.
    pub key: String,
    pub message: String,
}

impl From<answers::Issue> for Issue {
    fn from(i: answers::Issue) -> Self {
        Issue {
            key: i.key,
            message: i.message,
        }
    }
}

/// Answers in, config out — the browser's half of the one conversion there is.
pub fn assemble(body: &AnswersBody) -> Result<Assembled, String> {
    let spec = spec::spec();
    let base = match &body.base {
        // Read the way the loader reads a file, so a 0.3 config the browser
        // opened is relocated rather than refused.
        Some(value) => Some(crate::assemble::parse_value(value)?),
        None => None,
    };

    // An untouched question that has a default is answered by it. The browser
    // holds only what was typed, so the defaults are applied here — which is
    // also what lets a condition depend on one.
    let filled = answers::with_defaults(&spec, &body.answers);

    let (visible, options) = shape(&spec, &filled);
    let issues = answers::check(&spec, &filled);

    // The config comes back whether or not every answer is in yet. The rail
    // and the expert editor are looking at this draft the whole time someone
    // is walking it, and a wizard that showed nothing about the loop until
    // its last question was answered would be a wizard with no feedback.
    let (config, review) = match answers::draft_over(&spec, &filled, base.as_ref()) {
        Ok(cfg) => {
            let value = serde_json::to_value(&cfg).map_err(|e| e.to_string())?;
            let review = crate::assemble::review(&value);
            (Some(value), Some(review))
        }
        Err(_) => (None, None),
    };

    Ok(Assembled {
        issues: issues.into_iter().map(Issue::from).collect(),
        config,
        review,
        visible,
        options,
    })
}

/// The answers that would have produced this config.
pub fn unpack(value: &serde_json::Value) -> Result<Answers, String> {
    let cfg = crate::assemble::parse_value(value)?;
    Ok(answers::unpack(&spec::spec(), &cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(pairs: &[(&str, &str)]) -> AnswersBody {
        AnswersBody {
            answers: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            base: None,
        }
    }

    fn minimal() -> AnswersBody {
        body(&[
            ("name", "browser-loop"),
            ("version", "0.1.0"),
            ("environment", "dev"),
            ("intent.goals[0].name", "g1"),
            ("intent.goals[0].description", "a goal with a long enough description"),
            ("safety.checks[0].target", "g1"),
            ("safety.checks[0].name", "v1"),
            ("safety.checks[0].mode", "objective"),
            ("safety.checks[0].statement", "it works"),
            ("safety.checks[0].detector.type", "file_exists"),
            ("safety.checks[0].detector.path", "out.txt"),
            ("safety.checks[0].blocking", "true"),
            ("safety.gates.stop.max_iterations", "4"),
            ("safety.gates.stop.max_revisions_per_node", "2"),
            ("safety.gates.stop.no_progress_iterations", "2"),
            ("safety.gates.stop.stop_on_overall_success", "true"),
        ])
    }

    #[test]
    fn the_browser_never_has_to_build_a_config() {
        let out = assemble(&minimal()).expect("assembles");
        assert!(out.issues.is_empty(), "{:?}", out.issues);
        let cfg = out.config.expect("a config came back");
        assert_eq!(cfg["name"], "browser-loop");
        // And the rail's numbers come from the same pass, so the two cannot
        // disagree about a config the user is looking at.
        assert!(out.review.expect("a review came back").parsed);
    }

    #[test]
    fn a_bad_answer_names_the_question_it_came_from() {
        let mut b = minimal();
        b.answers
            .insert("safety.gates.stop.max_iterations".into(), "lots".into());
        let out = assemble(&b).expect("answers are always readable");
        assert_eq!(out.issues[0].key, "safety.gates.stop.max_iterations");
    }

    #[test]
    fn a_half_answered_draft_still_has_a_config_to_show() {
        // The rail runs on this the whole time someone is walking the
        // questions. Holding the config back until the last one is answered
        // would leave the walk with no feedback at all.
        let mut b = minimal();
        b.answers.remove("intent.goals[0].name");
        b.answers.remove("intent.goals[0].description");
        let out = assemble(&b).expect("answers are always readable");
        assert!(!out.issues.is_empty(), "the missing goal is still reported");
        let cfg = out.config.expect("a draft config came back");
        assert_eq!(cfg["name"], "browser-loop");
        assert!(out.review.is_some(), "the rail has something to show");
    }

    #[test]
    fn an_opened_config_fills_the_wizard_in() {
        let cfg = assemble(&minimal()).expect("assembles").config.unwrap();
        let recovered = unpack(&cfg).expect("unpacks");
        assert_eq!(recovered.get("name").map(String::as_str), Some("browser-loop"));
        assert_eq!(recovered.get("intent.goals[0].name").map(String::as_str), Some("g1"));
    }

    #[test]
    fn a_03_config_the_browser_opened_is_relocated_not_refused() {
        // The browser can be holding a file written against 0.3. Going through
        // the loader is what makes the wizard agree with `loopsmith validate`
        // about the very file it is showing.
        let legacy = serde_json::json!({
            "name": "old",
            "goals": [{ "name": "g1", "description": "a goal with a long enough description" }],
            "stop_gates": { "max_iterations": 7 },
        });
        let recovered = unpack(&legacy).expect("a 0.3 config unpacks");
        assert_eq!(recovered.get("intent.goals[0].name").map(String::as_str), Some("g1"));
        assert_eq!(
            recovered.get("safety.gates.stop.max_iterations").map(String::as_str),
            Some("7")
        );
    }

    #[test]
    fn editing_keeps_what_the_wizard_never_asks_about() {
        let base = serde_json::json!({
            "name": "kept",
            "safety": { "protected": { "components": ["gates", "credentials"] } },
        });
        let mut b = minimal();
        b.base = Some(base);
        let out = assemble(&b).expect("assembles");
        let cfg = out.config.expect("a config came back");
        assert_eq!(cfg["name"], "browser-loop", "the answers still win");
        assert_eq!(
            cfg["safety"]["protected"]["components"]
                .as_array()
                .map(Vec::len),
            Some(2),
            "an unasked section was dropped: {cfg}"
        );
    }

    #[test]
    fn the_browser_is_told_which_questions_apply() {
        let out = assemble(&minimal()).expect("assembles");
        // A gate is always offered; the section behind it is not open.
        assert!(out.visible.iter().any(|v| v == "gate:alerts"));
        assert!(!out.visible.iter().any(|v| v == "alerts"));
        // A detector's own fields follow the detector that was chosen.
        assert!(out
            .visible
            .iter()
            .any(|v| v == "safety.checks[0].detector.path"));
        assert!(!out
            .visible
            .iter()
            .any(|v| v == "safety.checks[0].detector.pattern"));
    }

    #[test]
    fn a_select_that_depends_on_an_earlier_answer_arrives_resolved() {
        // The browser cannot work out that a check is aimed at a goal named
        // three questions ago without owning a copy of that rule.
        let out = assemble(&minimal()).expect("assembles");
        let targets = out
            .options
            .get("safety.checks[0].target")
            .expect("the target select was resolved");
        assert_eq!(targets[0].value, "g1");
        assert_eq!(targets.last().unwrap().value, "overall");
        // A fixed select is already in the spec and is not sent twice.
        assert!(!out.options.contains_key("safety.checks[0].mode"));
    }

    #[test]
    fn a_draft_that_will_not_assemble_still_says_what_to_render() {
        let mut b = minimal();
        b.answers.remove("intent.goals[0].name");
        let out = assemble(&b).expect("answers are always readable");
        assert!(!out.issues.is_empty());
        assert!(
            out.visible.iter().any(|v| v == "name"),
            "a half-finished draft must still render its form"
        );
    }

    #[test]
    fn the_spec_the_browser_fetches_is_the_one_the_terminal_walks() {
        let served = spec();
        assert_eq!(served.version, spec::SPEC_VERSION);
        assert!(!served.sections.is_empty());
        // It has to survive the wire, or the browser renders nothing.
        let json = serde_json::to_value(&served).expect("the spec serialises");
        assert!(json["sections"].as_array().is_some_and(|s| !s.is_empty()));
    }
}
