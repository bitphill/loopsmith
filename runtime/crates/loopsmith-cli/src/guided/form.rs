//! A driver that turns an ordered list of fields into a map of answers, with
//! `:back` and `:next` moving the cursor rather than losing progress.
//!
//! The wizard never constructs a config field-by-field as it goes. It fills a
//! `BTreeMap<String, String>` of raw answers here, and a pure converter in
//! [`super::sections`] turns a completed map into a typed value that is then
//! validated. Keeping navigation (this file) apart from typing (that file) is
//! what makes `:back` safe: stepping backwards only moves a cursor over strings,
//! so there is no half-built struct to corrupt and no partial state to unwind.

use super::io::{Choice, Io, Nav};
use std::collections::BTreeMap;

/// A predicate a text field's answer must satisfy; its `Err` is shown before
/// the field is re-asked.
pub type Validator = Box<dyn Fn(&str) -> Result<(), String>>;

/// One question in a section.
pub struct Step {
    pub id: &'static str,
    pub label: String,
    pub help: Vec<String>,
    pub kind: Kind,
}

pub enum Kind {
    /// Free text. `optional` decides whether an empty answer is allowed and
    /// changes the hint; `validate` gates anything non-empty (and, for a
    /// required field, rejects the empty string with its own message).
    Text {
        default: String,
        optional: bool,
        validate: Validator,
    },
    Bool {
        default: bool,
    },
    /// A numbered menu. `default` is a 0-based index into `choices`.
    Select {
        choices: Vec<Choice>,
        default: usize,
    },
}

impl Step {
    pub fn text(id: &'static str, label: impl Into<String>, default: impl Into<String>) -> Self {
        Step {
            id,
            label: label.into(),
            help: Vec::new(),
            kind: Kind::Text {
                default: default.into(),
                optional: false,
                validate: Box::new(|_| Ok(())),
            },
        }
    }

    pub fn optional_text(
        id: &'static str,
        label: impl Into<String>,
        default: impl Into<String>,
    ) -> Self {
        let mut s = Step::text(id, label, default);
        if let Kind::Text { optional, .. } = &mut s.kind {
            *optional = true;
        }
        s
    }

    pub fn boolean(id: &'static str, label: impl Into<String>, default: bool) -> Self {
        Step {
            id,
            label: label.into(),
            help: Vec::new(),
            kind: Kind::Bool { default },
        }
    }

    pub fn select(
        id: &'static str,
        label: impl Into<String>,
        choices: Vec<Choice>,
        default: usize,
    ) -> Self {
        Step {
            id,
            label: label.into(),
            help: Vec::new(),
            kind: Kind::Select { choices, default },
        }
    }

    pub fn help(mut self, lines: &[&str]) -> Self {
        self.help = lines.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn validated(
        mut self,
        f: impl Fn(&str) -> Result<(), String> + 'static,
    ) -> Self {
        if let Kind::Text { validate, .. } = &mut self.kind {
            *validate = Box::new(f);
        }
        self
    }
}

/// Ask every step in order, honouring `:back`. Returns the answers, or `Back`
/// if the user walked off the front (the caller decides what that means), or
/// `Quit`.
pub fn run(io: &mut Io, steps: &[Step]) -> Result<BTreeMap<String, String>, Nav> {
    run_seeded(io, steps, BTreeMap::new())
}

/// As [`run`], but pre-filled — used when editing an existing config, where
/// each field's current value is the default shown in `[brackets]`.
pub fn run_seeded(
    io: &mut Io,
    steps: &[Step],
    seed: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Nav> {
    let mut answers = seed;
    let mut cursor = 0usize;
    while cursor < steps.len() {
        let step = &steps[cursor];
        let help: Vec<&str> = step.help.iter().map(String::as_str).collect();
        let prior = answers.get(step.id).cloned();
        let outcome = match &step.kind {
            Kind::Text { default, optional, validate } => {
                // Precedence for the shown default: a prior answer wins, then a
                // configured default, then the optional-skip hint, then nothing.
                let shown: Option<String> = prior.or_else(|| {
                    if !default.is_empty() {
                        Some(default.clone())
                    } else if *optional {
                        Some(String::new())
                    } else {
                        None
                    }
                });
                io.ask_text(&step.label, &help, shown.as_deref(), &|v| {
                    if v.is_empty() && !optional {
                        return Err("this field is required".into());
                    }
                    if v.is_empty() {
                        return Ok(());
                    }
                    validate(v)
                })
            }
            Kind::Bool { default } => {
                let d = prior
                    .as_deref()
                    .map(|p| p == "true")
                    .unwrap_or(*default);
                io.ask_bool(&step.label, &help, d).map(|b| b.to_string())
            }
            Kind::Select { choices, default } => {
                let d = prior
                    .as_deref()
                    .and_then(|p| choices.iter().position(|c| c.value == p))
                    .unwrap_or(*default);
                io.ask_select(&step.label, &help, choices, Some(d))
            }
        };
        match outcome {
            Ok(v) => {
                answers.insert(step.id.to_string(), v);
                cursor += 1;
            }
            Err(Nav::Back) => {
                if cursor == 0 {
                    return Err(Nav::Back);
                }
                cursor -= 1;
            }
            Err(Nav::Quit) => return Err(Nav::Quit),
        }
    }
    Ok(answers)
}

/// Small helpers the converters use to read a completed answer map.
pub trait Answers {
    fn s(&self, id: &str) -> String;
    fn opt(&self, id: &str) -> Option<String>;
    fn flag(&self, id: &str) -> bool;
}

impl Answers for BTreeMap<String, String> {
    fn s(&self, id: &str) -> String {
        self.get(id).cloned().unwrap_or_default()
    }
    fn opt(&self, id: &str) -> Option<String> {
        self.get(id).filter(|v| !v.is_empty()).cloned()
    }
    fn flag(&self, id: &str) -> bool {
        self.get(id).map(|v| v == "true").unwrap_or(false)
    }
}
