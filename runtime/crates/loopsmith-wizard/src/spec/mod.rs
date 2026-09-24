//! The wizard, as data.
//!
//! Until 1.0 the same wizard existed twice: 1,127 lines of Rust walking a
//! terminal and 1,107 lines of TypeScript drawing cards, hand-kept in step,
//! with nothing forcing them to agree. This module is the single answer. Rust
//! owns the questions, their wording, their order, and what counts as a valid
//! answer; the terminal renders them through [`crate::io`], and the browser
//! fetches the same list from `/api/wizard/spec` and renders it generically.
//!
//! Three properties make that possible, and each is a constraint on what can
//! be expressed here:
//!
//! - **Every question is data.** No closures, no per-field code. A field names
//!   the config path it fills, how to ask for it, and what a good answer looks
//!   like ([`Validator`]). Adding a question is an entry in [`sections`], not a
//!   new component in two languages.
//! - **Answers are strings.** One flat `BTreeMap<String, String>`, keyed by
//!   the field's path — `intent.goals[0].name`. It survives a JSON round trip,
//!   a resumed draft, and a terminal that only ever has text to give.
//! - **Only Rust types them.** [`crate::answers::assemble`] turns answers into
//!   a `LoopConfig` through the ordinary config loader, so the wizard cannot
//!   accept something `loopsmith validate` would refuse, and the browser needs
//!   no schema of its own.

use serde::Serialize;

mod sections;

pub use sections::spec;

/// Bumped when the shape of what `/api/wizard/spec` returns changes, so a
/// browser holding an older bundle can say so instead of rendering nonsense.
pub const SPEC_VERSION: u32 = 1;

/// The whole wizard.
#[derive(Debug, Clone, Serialize)]
pub struct Spec {
    pub version: u32,
    pub sections: Vec<Section>,
}

impl Spec {
    /// Every field in every section, in order, with list fields flattened out.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.sections.iter().flat_map(|s| s.steps.iter()).flat_map(|step| {
            let (fields, list): (&[Field], &[Field]) = match step {
                Step::Field(f) => (std::slice::from_ref(f), &[]),
                Step::List(l) => (&[], &l.fields),
                Step::Providers(_) => (&[], &[]),
            };
            fields.iter().chain(list.iter())
        })
    }

    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }
}

/// One stretch of the wizard, with its own heading.
#[derive(Debug, Clone, Serialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    /// The yes/no in front of an opt-in section. `None` for a section every
    /// loop needs.
    pub gate: Option<Gate>,
    pub steps: Vec<Step>,
}

impl Section {
    /// The answer key holding whether an opt-in section was accepted.
    pub fn gate_key(&self) -> String {
        format!("gate:{}", self.id)
    }
}

/// The question that opens an opt-in section.
#[derive(Debug, Clone, Serialize)]
pub struct Gate {
    pub question: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    Field(Field),
    List(List),
    /// Providers get their own step in both front ends: the machine has
    /// already been scanned, and the answer is a pick from what was found
    /// rather than a form.
    Providers(Providers),
}

/// One scalar question.
#[derive(Debug, Clone, Serialize)]
pub struct Field {
    /// The config path this fills, and the answer key it is stored under.
    /// Inside a list entry it is relative to the entry: `detector.command`.
    pub id: String,
    pub title: String,
    pub hint: Option<String>,
    /// Longer lines shown under the question, for the reader who wants them.
    pub help: Vec<String>,
    pub input: Input,
    /// What an untouched field answers with. The terminal offers it in
    /// brackets and takes it on a bare Enter; the browser pre-fills the
    /// control with it. Required fields carry one wherever the config itself
    /// has a sensible default, so "Enter through it" stays a real option.
    pub default: Option<String>,
    pub validator: Validator,
    /// Only asked when this holds.
    pub when: Option<When>,
}

/// A repeating section: goals, checks, nodes.
#[derive(Debug, Clone, Serialize)]
pub struct List {
    /// The config path of the list itself: `intent.goals`.
    pub id: String,
    pub title: String,
    pub hint: Option<String>,
    pub help: Vec<String>,
    /// What one entry is called, for "add another goal".
    pub singular: String,
    /// Fewest entries that lets the section be finished.
    pub min: usize,
    /// Field ids that name an entry in a list of them.
    pub summary: Summary,
    pub fields: Vec<Field>,
    pub when: Option<When>,
}

/// How one entry is described in a list of them.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub primary: String,
    pub secondary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Providers {
    pub id: String,
    pub title: String,
    pub hint: Option<String>,
}

/// How a question is asked.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Input {
    Text {
        placeholder: Option<String>,
        /// Monospace: a path, a command, a pattern.
        mono: bool,
    },
    Area {
        placeholder: Option<String>,
        rows: u8,
    },
    Number {
        min: Option<f64>,
        step: Option<f64>,
        suffix: Option<String>,
    },
    Bool {
        true_label: Option<String>,
        false_label: Option<String>,
    },
    Select {
        options: Options,
    },
    /// Several values in one answer, separated as `separator` says.
    Items {
        placeholder: Option<String>,
        separator: Separator,
        mono: bool,
    },
}

/// Where a select's options come from. Some depend on answers already given —
/// a check is aimed at a goal that was named three questions ago — so the
/// choices cannot all be written down here.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Options {
    Fixed { choices: Vec<Choice> },
    /// The loop's goals, plus `overall`.
    GoalTargets,
    /// Provider ids named so far, with an empty first choice.
    ProviderIds,
    /// Phase names named so far.
    PhaseNames,
}

#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
    /// The half-sentence that says when to pick it.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Separator {
    Comma,
    Semicolon,
    Whitespace,
}

impl Separator {
    pub fn split(self, s: &str) -> Vec<String> {
        let parts: Vec<&str> = match self {
            Separator::Comma => s.split(',').collect(),
            Separator::Semicolon => s.split(';').collect(),
            Separator::Whitespace => s.split_whitespace().collect(),
        };
        parts
            .into_iter()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect()
    }

    pub fn join(self, items: &[String]) -> String {
        match self {
            Separator::Comma => items.join(", "),
            Separator::Semicolon => items.join("; "),
            Separator::Whitespace => items.join(" "),
        }
    }
}

/// When a question is worth asking at all.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "when", rename_all = "snake_case")]
pub enum When {
    /// Another answer has this value. Inside a list entry the field is the
    /// sibling's id; elsewhere it is a full path.
    Equals { field: String, value: String },
    /// Another answer is anything but this value.
    NotEquals { field: String, value: String },
    /// A list has at least this many entries — the judge-independence question
    /// means nothing with one provider.
    MinItems { field: String, count: usize },
}

/// What counts as a good answer.
///
/// A serializable enum rather than a predicate, because the browser has to
/// receive it: the terminal and the API both run [`Validator::check`], and the
/// browser shows what the API says rather than keeping its own copy of these
/// rules.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum Validator {
    /// Anything at all, including nothing.
    Anything,
    NonEmpty,
    Uint {
        min: Option<u64>,
        max: Option<u64>,
        required: bool,
    },
    Float {
        min: Option<f64>,
        max: Option<f64>,
        required: bool,
    },
    /// A valid regular expression, checked by the same engine the gate uses.
    Regex,
    OneOf {
        values: Vec<String>,
    },
    Semver,
    /// A path with no leading or trailing whitespace and no NUL.
    Path {
        required: bool,
    },
}

impl Validator {
    /// `Ok(())`, or why the answer will not do, phrased for the person who
    /// typed it.
    pub fn check(&self, value: &str) -> Result<(), String> {
        let v = value.trim();
        match self {
            Validator::Anything => Ok(()),
            Validator::NonEmpty => {
                if v.is_empty() {
                    Err("this one is needed".into())
                } else {
                    Ok(())
                }
            }
            Validator::Uint { min, max, required } => {
                if v.is_empty() {
                    return if *required {
                        Err("enter a whole number".into())
                    } else {
                        Ok(())
                    };
                }
                let n: u64 = v
                    .parse()
                    .map_err(|_| "enter a whole number, with no sign or decimal point".to_string())?;
                bounds(n as f64, min.map(|m| m as f64), max.map(|m| m as f64))
            }
            Validator::Float { min, max, required } => {
                if v.is_empty() {
                    return if *required {
                        Err("enter a number".into())
                    } else {
                        Ok(())
                    };
                }
                let n: f64 = v.parse().map_err(|_| "enter a number".to_string())?;
                if !n.is_finite() {
                    return Err("enter a number".into());
                }
                bounds(n, *min, *max)
            }
            Validator::Regex => {
                if v.is_empty() {
                    return Err("this one is needed".into());
                }
                regex::Regex::new(v)
                    .map(|_| ())
                    .map_err(|e| format!("not a valid regular expression: {e}"))
            }
            Validator::OneOf { values } => {
                if values.iter().any(|x| x == v) {
                    Ok(())
                } else {
                    Err(format!("pick one of: {}", values.join(", ")))
                }
            }
            Validator::Semver => {
                let parts: Vec<&str> = v.split('.').collect();
                let ok = parts.len() == 3
                    && parts
                        .iter()
                        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
                if ok {
                    Ok(())
                } else {
                    Err("three numbers separated by dots, like 0.1.0".into())
                }
            }
            Validator::Path { required } => {
                if v.is_empty() {
                    return if *required {
                        Err("this one is needed".into())
                    } else {
                        Ok(())
                    };
                }
                if value.contains('\0') {
                    return Err("a path cannot contain a NUL byte".into());
                }
                Ok(())
            }
        }
    }

    /// Whether an empty answer is refused.
    pub fn demands_an_answer(&self) -> bool {
        match self {
            Validator::NonEmpty | Validator::Regex | Validator::Semver => true,
            Validator::Uint { required, .. }
            | Validator::Float { required, .. }
            | Validator::Path { required } => *required,
            Validator::OneOf { .. } => true,
            Validator::Anything => false,
        }
    }
}

fn bounds(n: f64, min: Option<f64>, max: Option<f64>) -> Result<(), String> {
    if let Some(m) = min {
        if n < m {
            return Err(format!("{m} or more"));
        }
    }
    if let Some(m) = max {
        if n > m {
            return Err(format!("{m} or less"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_field_refuses_prose_and_accepts_its_bounds() {
        let v = Validator::Uint {
            min: Some(1),
            max: Some(10),
            required: true,
        };
        assert!(v.check("3").is_ok());
        assert!(v.check("").is_err(), "required means required");
        assert!(v.check("-1").is_err());
        assert!(v.check("2.5").is_err());
        assert!(v.check("11").is_err());
        assert!(v.check("lots").is_err());
    }

    #[test]
    fn an_optional_number_accepts_nothing_at_all() {
        let v = Validator::Uint {
            min: None,
            max: None,
            required: false,
        };
        assert!(v.check("   ").is_ok());
        assert!(v.check("7").is_ok());
        assert!(v.check("soon").is_err());
    }

    #[test]
    fn a_pattern_that_cannot_compile_is_refused_where_it_is_typed() {
        // Not three iterations later, when the gate tries to run it.
        assert!(Validator::Regex.check("^ok$").is_ok());
        assert!(Validator::Regex.check("(unclosed").is_err());
    }

    #[test]
    fn a_version_must_look_like_one() {
        assert!(Validator::Semver.check("0.1.0").is_ok());
        assert!(Validator::Semver.check("1.0").is_err());
        assert!(Validator::Semver.check("v1.0.0").is_err());
    }

    #[test]
    fn separators_round_trip_a_list_of_items() {
        for sep in [Separator::Comma, Separator::Semicolon, Separator::Whitespace] {
            let items = vec!["one".to_string(), "two".to_string()];
            assert_eq!(sep.split(&sep.join(&items)), items, "{sep:?}");
        }
        assert_eq!(Separator::Comma.split(" a , , b "), vec!["a", "b"]);
    }

    #[test]
    fn every_field_in_the_spec_has_a_question_and_a_path() {
        let spec = spec();
        assert_eq!(spec.version, SPEC_VERSION);
        for f in spec.fields() {
            assert!(!f.id.trim().is_empty(), "a field with no path");
            assert!(!f.title.trim().is_empty(), "{} has no question", f.id);
            assert!(
                !f.title.ends_with(' '),
                "{} has a trailing space in its question",
                f.id
            );
        }
    }

    #[test]
    fn every_preset_is_an_answer_its_own_field_would_accept() {
        // A preset is offered on a bare Enter, so a typo in one turns a
        // question into a loop the user cannot get past.
        for f in spec().fields() {
            let Some(d) = &f.default else { continue };
            assert!(
                f.validator.check(d).is_ok(),
                "{} presets `{d}`, which it would then refuse: {:?}",
                f.id,
                f.validator.check(d)
            );
        }
    }

    #[test]
    fn no_two_sections_or_fields_share_an_id() {
        let spec = spec();
        let mut seen = std::collections::BTreeSet::new();
        for s in &spec.sections {
            assert!(seen.insert(s.id.clone()), "two sections called `{}`", s.id);
        }
        for step in spec.sections.iter().flat_map(|s| &s.steps) {
            let mut keys = std::collections::BTreeSet::new();
            let fields = match step {
                Step::List(l) => &l.fields,
                _ => continue,
            };
            for f in fields {
                assert!(keys.insert(f.id.clone()), "two `{}` fields in one entry", f.id);
            }
        }
    }

    #[test]
    fn every_condition_names_a_field_that_exists() {
        // A `when` pointing at a field that was renamed would silently hide
        // the question it guards.
        let spec = spec();
        let absolute: std::collections::BTreeSet<String> =
            spec.fields().map(|f| f.id.clone()).collect();
        let lists: std::collections::BTreeSet<String> = spec
            .sections
            .iter()
            .flat_map(|s| &s.steps)
            .filter_map(|s| match s {
                Step::List(l) => Some(l.id.clone()),
                Step::Providers(p) => Some(p.id.clone()),
                Step::Field(_) => None,
            })
            .collect();
        for step in spec.sections.iter().flat_map(|s| &s.steps) {
            let (own, when) = match step {
                Step::Field(f) => (None, f.when.as_ref()),
                Step::List(l) => (Some(&l.fields), l.when.as_ref()),
                Step::Providers(_) => (None, None),
            };
            for w in when.into_iter().chain(
                own.into_iter()
                    .flatten()
                    .filter_map(|f| f.when.as_ref())
                    .collect::<Vec<_>>(),
            ) {
                match w {
                    When::Equals { field, .. } | When::NotEquals { field, .. } => assert!(
                        absolute.contains(field)
                            || own.is_some_and(|fs| fs.iter().any(|f| &f.id == field)),
                        "`when` names the unknown field `{field}`"
                    ),
                    When::MinItems { field, .. } => {
                        assert!(lists.contains(field), "`when` names the unknown list `{field}`")
                    }
                }
            }
        }
    }
}
