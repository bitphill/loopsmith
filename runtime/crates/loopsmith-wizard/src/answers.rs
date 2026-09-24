//! Answers in, config out.
//!
//! An answer is a string, keyed by the path the [`spec`](crate::spec) says it
//! fills: `name`, `safety.gates.stop.max_iterations`, `intent.goals[0].name`.
//! That is the whole wire format, and it is deliberately the poorest one that
//! works — a terminal has only text to give, a browser form field has only
//! text to give, and a draft saved half-finished has to survive a round trip
//! through JSON.
//!
//! Typing happens here, once, in Rust. [`assemble`] reads the spec to learn
//! what each answer means — a number, a flag, a comma-separated list — builds
//! the config document, and hands it to the ordinary loader. So the wizard
//! cannot produce a config that `loopsmith validate` would refuse to parse,
//! and the browser needs no schema of its own to post a draft.
//!
//! [`unpack`] is the same journey backwards, for `--edit`: an existing config
//! becomes the answers that would have produced it.

use crate::spec::{Field, Input, List, Options, Separator, Spec, Step, Validator, When};
use loopsmith_core::LoopConfig;
use serde_yaml::{Mapping, Value};
use std::collections::BTreeMap;

/// Every answer given so far.
pub type Answers = BTreeMap<String, String>;

/// An answer that will not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The answer key it concerns.
    pub key: String,
    pub message: String,
}

/// Check every answer the spec can see, in the order it asks for them.
///
/// Only questions that are actually being asked are checked: a field behind an
/// unanswered gate, or one whose `when` does not hold, is not a missing
/// answer.
pub fn check(spec: &Spec, answers: &Answers) -> Vec<Issue> {
    let mut issues = Vec::new();
    for section in &spec.sections {
        if !section_open(section, answers) {
            continue;
        }
        for step in &section.steps {
            match step {
                Step::Field(f) => {
                    if !holds(f.when.as_ref(), answers, None) {
                        continue;
                    }
                    check_one(f, &f.id, answers, &mut issues);
                }
                Step::List(l) => {
                    if !holds(l.when.as_ref(), answers, None) {
                        continue;
                    }
                    let entries = entry_count(&l.id, answers);
                    if entries < l.min {
                        issues.push(Issue {
                            key: l.id.clone(),
                            message: format!(
                                "add at least {} {}{}",
                                l.min,
                                l.singular,
                                if l.min == 1 { "" } else { "s" }
                            ),
                        });
                    }
                    for i in 0..entries {
                        let prefix = format!("{}[{i}]", l.id);
                        for f in &l.fields {
                            if !holds(f.when.as_ref(), answers, Some(&prefix)) {
                                continue;
                            }
                            check_one(f, &format!("{prefix}.{}", f.id), answers, &mut issues);
                        }
                    }
                }
                Step::Providers(_) => {}
            }
        }
    }
    issues
}

fn check_one(f: &Field, key: &str, answers: &Answers, issues: &mut Vec<Issue>) {
    let raw = answers.get(key).map(String::as_str).unwrap_or("");
    if let Err(message) = f.validator.check(raw) {
        issues.push(Issue {
            key: key.to_string(),
            message,
        });
    }
}

/// Turn answers into a config, through the same loader a file goes through.
///
/// `Err` is the list of answers that will not do. A config that parses but
/// fails `loopsmith_core::validate` is *not* an error here: the wizard shows
/// that report at the end and lets the author write the file anyway, which is
/// the same latitude a hand-written file gets.
pub fn assemble(spec: &Spec, answers: &Answers) -> Result<LoopConfig, Vec<Issue>> {
    assemble_over(spec, answers, None)
}

/// [`assemble`], with the config the answers were unpacked from.
///
/// `--edit` needs this: the wizard asks about a subset of the file, so an
/// assemble that saw only the answers would quietly drop everything it does
/// not ask about. The rule is per section — an open section is the wizard's
/// to own, keys and all, and a section the author never opened keeps whatever
/// the file already said.
pub fn assemble_over(
    spec: &Spec,
    answers: &Answers,
    base: Option<&LoopConfig>,
) -> Result<LoopConfig, Vec<Issue>> {
    let issues = check(spec, answers);
    if !issues.is_empty() {
        return Err(issues);
    }
    let mut doc = Mapping::new();
    for section in &spec.sections {
        if !section_open(section, answers) {
            continue;
        }
        for step in &section.steps {
            match step {
                Step::Field(f) => {
                    if !holds(f.when.as_ref(), answers, None) {
                        continue;
                    }
                    if let Some(v) = value_of(f, &f.id, answers) {
                        insert(&mut doc, &f.id, v);
                    }
                }
                Step::List(l) => {
                    if !holds(l.when.as_ref(), answers, None) {
                        continue;
                    }
                    let seq = entries(l, answers);
                    if !seq.is_empty() {
                        insert(&mut doc, &l.id, Value::Sequence(seq));
                    }
                }
                Step::Providers(p) => {
                    let seq = free_entries(&p.id, answers);
                    if !seq.is_empty() {
                        insert(&mut doc, &p.id, Value::Sequence(seq));
                    }
                    for (key, raw) in answers.range(format!("{}.", cascade_root(&p.id))..) {
                        if !key.starts_with(&format!("{}.", cascade_root(&p.id))) {
                            break;
                        }
                        let ids = Separator::Comma.split(raw);
                        if !ids.is_empty() {
                            insert(&mut doc, key, seq_of(&ids));
                        }
                    }
                }
            }
        }
    }

    let doc = match base {
        Some(cfg) => over(spec, answers, cfg, doc),
        None => doc,
    };
    let text = serde_yaml::to_string(&Value::Mapping(doc)).map_err(|e| {
        vec![Issue {
            key: String::new(),
            message: format!("the answers could not be written as YAML: {e}"),
        }]
    })?;
    loopsmith_core::parse_str(&text, "wizard").map_err(|e| {
        vec![Issue {
            key: String::new(),
            message: e.to_string(),
        }]
    })
}

/// The answers as the config document they describe, whether or not it is a
/// loop yet.
///
/// This is what a `:quit` writes as a draft: the point of saving it is that it
/// is not finished, so it is never parsed on the way out. `--edit` reads it
/// back through the ordinary loader like any other file.
pub fn draft(spec: &Spec, answers: &Answers) -> String {
    let mut doc = Mapping::new();
    for section in &spec.sections {
        if !section_open(section, answers) {
            continue;
        }
        for step in &section.steps {
            match step {
                Step::Field(f) => {
                    if let Some(v) = value_of(f, &f.id, answers) {
                        insert(&mut doc, &f.id, v);
                    }
                }
                Step::List(l) => {
                    let seq = entries(l, answers);
                    if !seq.is_empty() {
                        insert(&mut doc, &l.id, Value::Sequence(seq));
                    }
                }
                Step::Providers(p) => {
                    let seq = free_entries(&p.id, answers);
                    if !seq.is_empty() {
                        insert(&mut doc, &p.id, Value::Sequence(seq));
                    }
                }
            }
        }
    }
    serde_yaml::to_string(&Value::Mapping(doc)).unwrap_or_default()
}

/// Where a provider step keeps its cascade answers.
fn cascade_root(providers_path: &str) -> String {
    providers_path
        .rsplit_once('.')
        .map(|(head, _)| format!("{head}.cascade"))
        .unwrap_or_else(|| "cascade".into())
}

/// The answers that would have produced this config.
///
/// Only paths the spec asks about are recovered; anything else in the file —
/// `safety.protected`, per-node limits, a hand-written recovery policy — is
/// left alone by the wizard rather than flattened into a question it does not
/// ask. The caller keeps the original config and applies the wizard's answers over
/// it, so untouched sections survive an edit.
pub fn unpack(spec: &Spec, cfg: &LoopConfig) -> Answers {
    let mut answers = Answers::new();
    let Ok(doc) = serde_yaml::to_value(cfg) else {
        return answers;
    };
    // Every config serialises its own defaults, so "this path has a value" is
    // true of all eleven opt-in sections in every file ever written. What
    // opens a gate is a value the author chose — one that differs from what a
    // config with nothing but a name would have had.
    let plain = plain_config();
    for section in &spec.sections {
        let mut used = false;
        for step in &section.steps {
            match step {
                Step::Field(f) => {
                    if let Some(s) = read(&doc, &f.id, &f.input) {
                        let untouched = s.trim().is_empty()
                            || plain
                                .as_ref()
                                .is_some_and(|p| read(p, &f.id, &f.input).as_deref() == Some(&s));
                        used |= !untouched;
                        answers.insert(f.id.clone(), s);
                    }
                }
                Step::List(l) => {
                    let Some(Value::Sequence(items)) = at(&doc, &l.id) else {
                        continue;
                    };
                    for (i, item) in items.iter().enumerate() {
                        used = true;
                        for f in &l.fields {
                            if let Some(s) = read(item, &f.id, &f.input) {
                                answers.insert(format!("{}[{i}].{}", l.id, f.id), s);
                            }
                        }
                    }
                }
                Step::Providers(p) => {
                    let Some(Value::Sequence(items)) = at(&doc, &p.id) else {
                        continue;
                    };
                    for (i, item) in items.iter().enumerate() {
                        used = true;
                        let Value::Mapping(m) = item else { continue };
                        for (k, v) in m {
                            let Some(k) = k.as_str() else { continue };
                            if let Some(s) = scalar(v) {
                                answers.insert(format!("{}[{i}].{k}", p.id, k = k), s);
                            }
                        }
                    }
                }
            }
        }
        if used {
            if let Some(gate) = &section.gate {
                let _ = gate;
                answers.insert(section.gate_key(), "true".into());
            }
        }
    }
    answers
}

/// A config with nothing in it but a name, as a document — the baseline every
/// `unpack` compares against. `None` only if the loader itself refuses the
/// two words, which would be a bug elsewhere.
fn plain_config() -> Option<Value> {
    let cfg = loopsmith_core::parse_str("name: baseline\n", "<defaults>").ok()?;
    serde_yaml::to_value(&cfg).ok()
}

// --- reading --------------------------------------------------------------

fn at<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = doc;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn read(doc: &Value, path: &str, input: &Input) -> Option<String> {
    let v = at(doc, path)?;
    match (input, v) {
        (Input::Items { separator, .. }, Value::Sequence(items)) => {
            let parts: Vec<String> = items.iter().filter_map(scalar).collect();
            (!parts.is_empty()).then(|| separator.join(&parts))
        }
        _ => scalar(v),
    }
}

// --- writing --------------------------------------------------------------

fn seq_of(items: &[String]) -> Value {
    Value::Sequence(items.iter().map(|s| Value::String(s.clone())).collect())
}

/// The typed value of one answer, or `None` when it was left blank.
fn value_of(f: &Field, key: &str, answers: &Answers) -> Option<Value> {
    let raw = answers.get(key)?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(match &f.input {
        Input::Bool { .. } => Value::Bool(raw == "true" || raw == "yes"),
        Input::Number { .. } => match &f.validator {
            Validator::Uint { .. } => Value::Number(raw.parse::<u64>().ok()?.into()),
            _ => Value::Number(serde_yaml::Number::from(raw.parse::<f64>().ok()?)),
        },
        Input::Items { separator, .. } => {
            let items = separator.split(raw);
            if items.is_empty() {
                return None;
            }
            seq_of(&items)
        }
        Input::Text { .. } | Input::Area { .. } | Input::Select { .. } => {
            Value::String(raw.to_string())
        }
    })
}

/// One entry per index, with the fields that apply to it.
fn entries(l: &List, answers: &Answers) -> Vec<Value> {
    let mut out = Vec::new();
    for i in 0..entry_count(&l.id, answers) {
        let prefix = format!("{}[{i}]", l.id);
        let mut entry = Mapping::new();
        for f in &l.fields {
            if !holds(f.when.as_ref(), answers, Some(&prefix)) {
                continue;
            }
            if let Some(v) = value_of(f, &format!("{prefix}.{}", f.id), answers) {
                insert(&mut entry, &f.id, v);
            }
        }
        if !entry.is_empty() {
            out.push(Value::Mapping(entry));
        }
    }
    out
}

/// Entries whose fields the spec does not describe — the provider picker's,
/// built from a catalog rather than from questions. Types are read off the
/// answer itself, which is all that is available and all that is needed:
/// providers are written by loopsmith, not typed by a person.
fn free_entries(path: &str, answers: &Answers) -> Vec<Value> {
    let mut out: Vec<Mapping> = Vec::new();
    for i in 0..entry_count(path, answers) {
        let prefix = format!("{path}[{i}].");
        let mut entry = Mapping::new();
        for (key, raw) in answers.range(prefix.clone()..) {
            let Some(leaf) = key.strip_prefix(&prefix) else {
                break;
            };
            if raw.trim().is_empty() {
                continue;
            }
            insert(&mut entry, leaf, provider_value(leaf, raw));
        }
        out.push(entry);
    }
    out.into_iter()
        .filter(|m| !m.is_empty())
        .map(Value::Mapping)
        .collect()
}

/// A provider field's type, by name. The list is short and fixed because a
/// `ProviderSpec` is: everything else about a provider is a string.
fn provider_value(leaf: &str, raw: &str) -> Value {
    match leaf {
        "args" => seq_of(&Separator::Whitespace.split(raw)),
        "tiers" | "requires_env" => seq_of(&Separator::Comma.split(raw)),
        "prompt_on_stdin" => Value::Bool(raw == "true"),
        "cost_per_1k_tokens" => raw
            .parse::<f64>()
            .map(|n| Value::Number(serde_yaml::Number::from(n)))
            .unwrap_or_else(|_| Value::String(raw.into())),
        "timeout_seconds" => raw
            .parse::<u64>()
            .map(|n| Value::Number(n.into()))
            .unwrap_or_else(|_| Value::String(raw.into())),
        _ => Value::String(raw.into()),
    }
}

/// Write `value` at a dotted path, creating the mappings on the way.
fn insert(doc: &mut Mapping, path: &str, value: Value) {
    let mut parts = path.split('.').peekable();
    let mut cursor = doc;
    loop {
        let Some(part) = parts.next() else { return };
        let key = Value::String(part.to_string());
        if parts.peek().is_none() {
            cursor.insert(key, value);
            return;
        }
        let slot = cursor
            .entry(key)
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        let Value::Mapping(next) = slot else { return };
        cursor = next;
    }
}

/// The answer document laid over the config it came from.
///
/// Every path an *open* section owns is cleared from the original first, so a
/// goal the author deleted really goes and a field they blanked really
/// unsets. Sections they never opened are not touched at all.
fn over(spec: &Spec, answers: &Answers, base: &LoopConfig, doc: Mapping) -> Mapping {
    let Ok(Value::Mapping(mut out)) = serde_yaml::to_value(base) else {
        return doc;
    };
    for section in spec.sections.iter().filter(|s| section_open(s, answers)) {
        for step in &section.steps {
            match step {
                Step::Field(f) => strip(&mut out, &f.id),
                Step::List(l) => strip(&mut out, &l.id),
                Step::Providers(p) => {
                    strip(&mut out, &p.id);
                    strip(&mut out, &cascade_root(&p.id));
                }
            }
        }
    }
    merge(&mut out, doc);
    out
}

/// Remove one dotted path, leaving the branches above it in place.
fn strip(doc: &mut Mapping, path: &str) {
    let Some((head, rest)) = path.split_once('.') else {
        doc.remove(Value::String(path.to_string()));
        return;
    };
    if let Some(Value::Mapping(next)) = doc.get_mut(Value::String(head.to_string())) {
        strip(next, rest);
    }
}

/// `src` over `dst`, recursing into mappings so a sibling key survives.
fn merge(dst: &mut Mapping, src: Mapping) {
    for (key, value) in src {
        match (dst.get_mut(&key), value) {
            (Some(Value::Mapping(into)), Value::Mapping(from)) => merge(into, from),
            (_, value) => {
                dst.insert(key, value);
            }
        }
    }
}

// --- conditions -----------------------------------------------------------

/// How many entries a list has, counted from the answer keys.
pub fn entry_count(path: &str, answers: &Answers) -> usize {
    let prefix = format!("{path}[");
    let mut highest: Option<usize> = None;
    for key in answers.keys() {
        let Some(rest) = key.strip_prefix(&prefix) else {
            continue;
        };
        let Some((index, _)) = rest.split_once(']') else {
            continue;
        };
        if let Ok(i) = index.parse::<usize>() {
            highest = Some(highest.map_or(i, |h: usize| h.max(i)));
        }
    }
    highest.map(|h| h + 1).unwrap_or(0)
}

/// Whether an opt-in section was accepted. A section with no gate is always
/// open.
pub fn section_open(section: &crate::spec::Section, answers: &Answers) -> bool {
    section.gate.is_none()
        || answers
            .get(&section.gate_key())
            .is_some_and(|v| v == "true" || v == "yes")
}

/// Whether a `when` holds. `entry` is the answer-key prefix of the list entry
/// being filled in, if any: inside an entry a condition names a sibling.
pub fn holds(when: Option<&When>, answers: &Answers, entry: Option<&str>) -> bool {
    let Some(when) = when else { return true };
    let lookup = |field: &str| -> String {
        let key = match entry {
            Some(prefix) => format!("{prefix}.{field}"),
            None => field.to_string(),
        };
        answers.get(&key).cloned().unwrap_or_default()
    };
    match when {
        When::Equals { field, value } => &lookup(field) == value,
        When::NotEquals { field, value } => &lookup(field) != value,
        When::MinItems { field, count } => entry_count(field, answers) >= *count,
    }
}

/// The choices a dynamic select offers, given what has been answered so far.
pub fn options_for(options: &Options, answers: &Answers) -> Vec<crate::spec::Choice> {
    use crate::spec::Choice;
    let named = |path: &str, field: &str| -> Vec<String> {
        (0..entry_count(path, answers))
            .filter_map(|i| answers.get(&format!("{path}[{i}].{field}")).cloned())
            .filter(|s| !s.trim().is_empty())
            .collect()
    };
    match options {
        Options::Fixed { choices } => choices.clone(),
        Options::GoalTargets => named("intent.goals", "name")
            .into_iter()
            .map(|name| Choice {
                value: name.clone(),
                label: name,
                note: None,
            })
            .chain(std::iter::once(Choice {
                value: "overall".into(),
                label: "overall".into(),
                note: Some("the loop as a whole".into()),
            }))
            .collect(),
        Options::ProviderIds => std::iter::once(Choice {
            value: String::new(),
            label: "(none — route by tier)".into(),
            note: None,
        })
        .chain(
            named("execution.providers.providers", "id")
                .into_iter()
                .map(|id| Choice {
                    value: id.clone(),
                    label: id,
                    note: None,
                }),
        )
        .collect(),
        Options::PhaseNames => named("execution.phases.items", "name")
            .into_iter()
            .map(|name| Choice {
                value: name.clone(),
                label: name,
                note: None,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::spec;

    fn answers(pairs: &[(&str, &str)]) -> Answers {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// The smallest set of answers that makes a loop.
    fn minimal() -> Answers {
        answers(&[
            ("name", "demo"),
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
    fn the_smallest_set_of_answers_makes_a_config() {
        let cfg = assemble(&spec(), &minimal()).expect("assembles");
        assert_eq!(cfg.name, "demo");
        assert_eq!(cfg.intent.goals.len(), 1);
        assert_eq!(cfg.safety.checks[0].target, "g1");
        assert_eq!(cfg.safety.gates.stop.max_iterations, 4);
        assert!(matches!(
            cfg.safety.checks[0].detector,
            loopsmith_core::Detector::FileExists { .. }
        ));
    }

    #[test]
    fn a_missing_required_answer_is_reported_against_its_own_key() {
        let mut a = minimal();
        a.remove("intent.goals[0].name");
        let issues = assemble(&spec(), &a).expect_err("a goal needs a name");
        assert!(
            issues.iter().any(|i| i.key == "intent.goals[0].name"),
            "{issues:?}"
        );
    }

    #[test]
    fn a_question_behind_an_unanswered_gate_is_not_a_missing_answer() {
        // The graph section is opt-in; its node list is not missing when
        // nobody opened it.
        let cfg = assemble(&spec(), &minimal()).expect("assembles");
        assert!(cfg.execution.graph.nodes.is_empty());
    }

    #[test]
    fn a_field_whose_condition_fails_is_left_out_of_the_config() {
        // A `file_exists` detector must not carry the script detector's
        // command, even if an earlier answer left one behind.
        let mut a = minimal();
        a.insert("safety.checks[0].detector.command".into(), "npm".into());
        let cfg = assemble(&spec(), &a).expect("assembles");
        match &cfg.safety.checks[0].detector {
            loopsmith_core::Detector::FileExists { path, .. } => assert_eq!(path, "out.txt"),
            other => panic!("expected file_exists, got {other:?}"),
        }
    }

    #[test]
    fn typed_answers_reach_the_config_as_their_own_types() {
        let mut a = minimal();
        a.insert("gate:limits".into(), "true".into());
        a.insert("safety.limits.global.rules".into(), "no force-push; no CI edits".into());
        a.insert("safety.limits.global.max_tokens".into(), "5000".into());
        a.insert("safety.gates.stop.max_cost_usd".into(), "2.5".into());
        let cfg = assemble(&spec(), &a).expect("assembles");
        assert_eq!(cfg.safety.limits.global.rules.len(), 2);
        assert_eq!(cfg.safety.limits.global.max_tokens, Some(5000));
        assert_eq!(cfg.safety.gates.stop.max_cost_usd, Some(2.5));
    }

    #[test]
    fn an_answer_that_is_not_a_number_is_refused_before_the_loader_sees_it() {
        let mut a = minimal();
        a.insert("safety.gates.stop.max_iterations".into(), "lots".into());
        let issues = assemble(&spec(), &a).expect_err("refused");
        assert_eq!(issues[0].key, "safety.gates.stop.max_iterations");
    }

    #[test]
    fn a_config_unpacks_into_the_answers_that_would_rebuild_it() {
        let first = assemble(&spec(), &minimal()).expect("assembles");
        let recovered = unpack(&spec(), &first);
        let second = assemble(&spec(), &recovered).expect("re-assembles");
        assert_eq!(
            serde_yaml::to_string(&first).unwrap(),
            serde_yaml::to_string(&second).unwrap(),
            "a round trip through the wizard must not change the loop"
        );
    }

    #[test]
    fn editing_keeps_the_sections_the_wizard_never_asks_about() {
        // `safety.protected` has no question. An assemble that saw only the
        // answers would drop it, and the author would find their protected
        // list quietly reset to the default after one `--edit`.
        let base = loopsmith_core::parse_str(
            "name: kept\nsafety:\n  protected:\n    components: [gates, credentials]\n",
            "test",
        )
        .expect("the base parses");
        let cfg = assemble_over(&spec(), &minimal(), Some(&base)).expect("assembles");
        assert_eq!(cfg.name, "demo", "the answers still win where they exist");
        assert_eq!(cfg.safety.protected.components.len(), 2, "{:?}", cfg.safety.protected);
    }

    #[test]
    fn an_open_section_owns_its_paths_even_when_the_answer_is_emptier() {
        // Removing the last goal has to reach the config. The goals section is
        // always open, so its list is the wizard's to overwrite — including
        // with fewer entries than the file had.
        let base = assemble(&spec(), &minimal()).expect("assembles");
        let mut a = minimal();
        a.insert("intent.goals[1].name".into(), "g2".into());
        a.insert("intent.goals[1].description".into(), "a second goal, long enough".into());
        let two = assemble_over(&spec(), &a, Some(&base)).expect("assembles");
        assert_eq!(two.intent.goals.len(), 2);
        let back = assemble_over(&spec(), &minimal(), Some(&two)).expect("assembles");
        assert_eq!(back.intent.goals.len(), 1, "a removed goal came back");
    }

    #[test]
    fn a_gate_opens_only_for_a_section_its_author_filled_in() {
        // Every config serialises its own defaults, so an unpack that opened a
        // gate for "this path has a value" would open all eleven opt-in
        // sections on any edit — and then ask eleven sections of questions
        // about settings nobody touched.
        let plain = assemble(&spec(), &minimal()).expect("assembles");
        let recovered = unpack(&spec(), &plain);
        for section in spec().sections.iter().filter(|s| s.gate.is_some()) {
            assert_eq!(
                recovered.get(&section.gate_key()).map(String::as_str),
                None,
                "the `{}` gate opened on a config that never used it",
                section.id
            );
        }

        // One that was used does open.
        let mut a = minimal();
        a.insert("gate:alerts".into(), "true".into());
        a.insert("safety.alerts[0].id".into(), "spend".into());
        a.insert("safety.alerts[0].metric".into(), "cost_usd".into());
        a.insert("safety.alerts[0].above".into(), "10".into());
        let with_alert = assemble(&spec(), &a).expect("assembles");
        assert_eq!(
            unpack(&spec(), &with_alert).get("gate:alerts").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn a_dynamic_select_offers_what_has_been_answered_so_far() {
        let a = minimal();
        let choices = options_for(&Options::GoalTargets, &a);
        assert_eq!(choices[0].value, "g1");
        assert_eq!(choices.last().unwrap().value, "overall");
    }
}
