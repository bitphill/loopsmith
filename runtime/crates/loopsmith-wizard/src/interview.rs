//! Walking the spec in a terminal.
//!
//! One question per prompt, in the spec's order, with `:back` moving a cursor
//! rather than losing what has been typed. Nothing here knows what any
//! particular question means: it reads [`crate::spec`], asks what the field's
//! [`Input`] says to ask, stores the answer under the field's path, and moves
//! on. The browser walks the same list with its own widgets.
//!
//! The one question that is not a form is providers: the machine has already
//! been scanned, so it is a pick from what was found, pre-filled from the
//! catalog.

use crate::answers::{self, Answers};
use crate::catalog::Known;
use crate::detect::{self, Found};
use crate::io::{Choice, Io, Nav};
use crate::spec::{Field, Input, List, Providers, Section, Spec, Step};

/// How the walk ended.
pub enum Flow {
    /// Every section was walked to the end.
    Completed,
    /// The user left. `answers` is what they had typed, for a draft.
    Quit,
}

/// Ask the whole spec, filling `answers` as it goes.
pub fn walk(io: &mut Io, spec: &Spec, answers: &mut Answers) -> Result<Flow, String> {
    let mut at = 0usize;
    while at < spec.sections.len() {
        match section(io, spec, &spec.sections[at], answers) {
            Ok(()) => at += 1,
            Err(Nav::Back) => at = at.saturating_sub(1),
            Err(Nav::Quit) => return Ok(Flow::Quit),
        }
    }
    Ok(Flow::Completed)
}

/// One section: its gate, then its steps, with `:back` walking between them.
fn section(io: &mut Io, spec: &Spec, s: &Section, answers: &mut Answers) -> Result<(), Nav> {
    io.heading(&s.title);
    if let Some(gate) = &s.gate {
        let key = s.gate_key();
        let current = answers.get(&key).is_some_and(|v| v == "true");
        let hint: Vec<&str> = gate.hint.iter().map(String::as_str).collect();
        let yes = io.ask_bool(&gate.question, &hint, current)?;
        answers.insert(key, yes.to_string());
        if !yes {
            return Ok(());
        }
    }

    let mut at = 0usize;
    while at < s.steps.len() {
        let step = &s.steps[at];
        let skip = match step {
            Step::Field(f) => !answers::holds(f.when.as_ref(), answers, None),
            Step::List(l) => !answers::holds(l.when.as_ref(), answers, None),
            Step::Providers(_) => false,
        };
        if skip {
            at += 1;
            continue;
        }
        let asked = match step {
            Step::Field(f) => ask(io, f, &f.id.clone(), None, answers),
            Step::List(l) => list(io, spec, l, answers),
            Step::Providers(p) => providers(io, p, answers),
        };
        match asked {
            Ok(()) => at += 1,
            Err(Nav::Back) if at == 0 => return Err(Nav::Back),
            Err(Nav::Back) => at -= 1,
            Err(Nav::Quit) => return Err(Nav::Quit),
        }
    }
    Ok(())
}

/// Ask one field and store the answer under `key`.
///
/// `entry` is the answer-key prefix of the list entry being filled in, which
/// is what lets a condition inside an entry name its sibling.
fn ask(
    io: &mut Io,
    f: &Field,
    key: &str,
    entry: Option<&str>,
    answers: &mut Answers,
) -> Result<(), Nav> {
    let mut help: Vec<String> = f.hint.iter().cloned().collect();
    help.extend(f.help.iter().cloned());
    let help: Vec<&str> = help.iter().map(String::as_str).collect();
    // What Enter answers with: what is already there, else the field's own
    // preset. A required field without either is one the user must type.
    let current = answers
        .get(key)
        .cloned()
        .or_else(|| f.default.clone());

    let value = match &f.input {
        Input::Bool { true_label, false_label } => {
            let mut lines = help.clone();
            if let (Some(t), Some(no)) = (true_label, false_label) {
                lines.push(t);
                lines.push(no);
            }
            let default = current.as_deref().map(|v| v == "true").unwrap_or(true);
            io.ask_bool(&f.title, &lines, default)?.to_string()
        }
        Input::Select { options } => {
            let choices: Vec<Choice> = answers::options_for(options, answers)
                .into_iter()
                .map(|c| {
                    let mut choice = Choice::new(c.value, c.label);
                    if let Some(note) = c.note {
                        choice = choice.noted(note);
                    }
                    choice
                })
                .collect();
            if choices.is_empty() {
                return Ok(());
            }
            let default = current
                .as_deref()
                .and_then(|v| choices.iter().position(|c| c.value == v))
                .unwrap_or(0);
            io.ask_select(&f.title, &help, &choices, Some(default))?
        }
        _ => {
            let validator = f.validator.clone();
            io.ask_text(&f.title, &help, current.as_deref(), &move |v| validator.check(v))?
        }
    };

    let _ = entry;
    if value.trim().is_empty() {
        answers.remove(key);
    } else {
        answers.insert(key.to_string(), value);
    }
    Ok(())
}

/// A repeating section: add, edit, remove, done.
fn list(io: &mut Io, spec: &Spec, l: &List, answers: &mut Answers) -> Result<(), Nav> {
    io.heading(&l.title);
    for line in l.hint.iter().chain(l.help.iter()) {
        io.note(line);
    }
    loop {
        let count = answers::entry_count(&l.id, answers);
        for i in 0..count {
            io.println(&format!("  {}. {}", i + 1, describe(l, i, answers)));
        }
        if count == 0 {
            io.note(&format!("No {}s yet.", l.singular));
        }

        let mut choices = vec![Choice::new("add", format!("Add a {}", l.singular))];
        if count > 0 {
            choices.push(Choice::new("edit", format!("Edit a {}", l.singular)));
            choices.push(Choice::new("remove", format!("Remove a {}", l.singular)));
        }
        choices.push(Choice::new("done", "This part is done"));

        match io.ask_select("What next?", &[], &choices, Some(0))?.as_str() {
            "add" => {
                if entry(io, spec, l, count, answers).is_err() {
                    // `:back` out of the first field of a new entry discards
                    // it — nothing half-typed is left in the list.
                    clear_entry(&l.id, count, answers);
                }
            }
            "edit" => {
                if let Some(i) = pick(io, l, count, "Edit which one?", answers)? {
                    let _ = entry(io, spec, l, i, answers);
                }
            }
            "remove" => {
                if let Some(i) = pick(io, l, count, "Remove which one?", answers)? {
                    remove_entry(&l.id, i, count, answers);
                }
            }
            _ => {
                let count = answers::entry_count(&l.id, answers);
                if count < l.min {
                    io.error(&format!(
                        "at least {} {} is needed before this section is done",
                        l.min, l.singular
                    ));
                    continue;
                }
                return Ok(());
            }
        }
    }
}

/// Fill in one entry, field by field, with `:back` walking between them.
fn entry(
    io: &mut Io,
    _spec: &Spec,
    l: &List,
    index: usize,
    answers: &mut Answers,
) -> Result<(), Nav> {
    let prefix = format!("{}[{index}]", l.id);
    let mut at = 0usize;
    while at < l.fields.len() {
        let f = &l.fields[at];
        if !answers::holds(f.when.as_ref(), answers, Some(&prefix)) {
            at += 1;
            continue;
        }
        let key = format!("{prefix}.{}", f.id);
        match ask(io, f, &key, Some(&prefix), answers) {
            Ok(()) => at += 1,
            Err(Nav::Back) if at == 0 => return Err(Nav::Back),
            Err(Nav::Back) => at -= 1,
            Err(Nav::Quit) => return Err(Nav::Quit),
        }
    }
    Ok(())
}

fn pick(
    io: &mut Io,
    l: &List,
    count: usize,
    question: &str,
    answers: &Answers,
) -> Result<Option<usize>, Nav> {
    let mut choices: Vec<Choice> = (0..count)
        .map(|i| Choice::new(i.to_string(), describe(l, i, answers)))
        .collect();
    choices.push(Choice::new("cancel", "Never mind"));
    let pick = io.ask_select(question, &[], &choices, Some(0))?;
    Ok(pick.parse::<usize>().ok())
}

/// One entry, as a line in a list of them.
fn describe(l: &List, index: usize, answers: &Answers) -> String {
    let read = |field: &str| answers.get(&format!("{}[{index}].{field}", l.id)).cloned();
    let primary = read(&l.summary.primary).unwrap_or_else(|| format!("(unnamed {})", l.singular));
    match l.summary.secondary.as_ref().and_then(|s| read(s)) {
        Some(extra) if !extra.is_empty() => format!("{primary} — {}", truncate(&extra, 48)),
        _ => primary,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
}

fn clear_entry(path: &str, index: usize, answers: &mut Answers) {
    let prefix = format!("{path}[{index}].");
    let doomed: Vec<String> = answers
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect();
    for k in doomed {
        answers.remove(&k);
    }
}

/// Remove one entry and close the gap, so indices stay contiguous.
fn remove_entry(path: &str, index: usize, count: usize, answers: &mut Answers) {
    clear_entry(path, index, answers);
    for i in index + 1..count {
        let from = format!("{path}[{i}].");
        let to = format!("{path}[{}].", i - 1);
        let moving: Vec<(String, String)> = answers
            .iter()
            .filter(|(k, _)| k.starts_with(&from))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (k, v) in moving {
            answers.remove(&k);
            answers.insert(k.replacen(&from, &to, 1), v);
        }
    }
}

/// The providers step: pick from what is on this machine.
fn providers(io: &mut Io, p: &Providers, answers: &mut Answers) -> Result<(), Nav> {
    io.heading(&p.title);
    for line in p.hint.iter() {
        io.note(line);
    }
    io.note("Ones already on this machine are marked ✓ and come pre-filled.");

    let found = detect::installed();
    if !found.iter().any(|f| f.present) {
        io.note("None of the known CLIs were found on PATH — you can still add one by hand.");
    }

    loop {
        let count = answers::entry_count(&p.id, answers);
        for i in 0..count {
            let id = answers
                .get(&format!("{}[{i}].id", p.id))
                .cloned()
                .unwrap_or_default();
            let command = answers
                .get(&format!("{}[{i}].command", p.id))
                .cloned()
                .unwrap_or_default();
            io.println(&format!("  {}. {id} ({command})", i + 1));
        }
        if count == 0 {
            io.note("No providers yet. A loop needs at least one.");
        }

        let mut choices = vec![Choice::new("add", "Add a provider")];
        if count > 0 {
            choices.push(Choice::new("remove", "Remove a provider"));
            choices.push(Choice::new("done", "This part is done"));
        }
        match io.ask_select("What next?", &[], &choices, Some(0))?.as_str() {
            "add" => add_provider(io, p, &found, count, answers)?,
            "remove" => {
                let names: Vec<Choice> = (0..count)
                    .map(|i| {
                        let id = answers
                            .get(&format!("{}[{i}].id", p.id))
                            .cloned()
                            .unwrap_or_default();
                        Choice::new(i.to_string(), id)
                    })
                    .collect();
                let pick = io.ask_select("Remove which one?", &[], &names, Some(0))?;
                if let Ok(i) = pick.parse::<usize>() {
                    remove_entry(&p.id, i, count, answers);
                }
            }
            _ => {
                cascade(&p.id, answers);
                return Ok(());
            }
        }
    }
}

fn add_provider(
    io: &mut Io,
    p: &Providers,
    found: &[Found],
    index: usize,
    answers: &mut Answers,
) -> Result<(), Nav> {
    let mut choices: Vec<Choice> = found
        .iter()
        .map(|f| {
            let mark = match &f.path {
                Some(path) => format!("✓ {}", path.display()),
                None => "not found".to_string(),
            };
            Choice::new(f.known.id, f.known.label).noted(mark)
        })
        .collect();
    choices.push(Choice::new("__byok__", "Something else (enter the command yourself)"));

    let pick = io.ask_select(
        "Which provider?",
        &["Pick a CLI to pre-fill, or the last option to type a custom command."],
        &choices,
        Some(0),
    )?;
    let set = |answers: &mut Answers, field: &str, value: String| {
        if !value.is_empty() {
            answers.insert(format!("{}[{index}].{field}", p.id), value);
        }
    };

    if pick == "__byok__" {
        let id = io.ask_text("Provider id", &[], None, &nonempty)?;
        let command = io.ask_text(
            "Command to run",
            &["The binary itself, not a shell line."],
            None,
            &nonempty,
        )?;
        let args = io.ask_text(
            "Arguments",
            &["Split on spaces. Use {{prompt}} where the prompt goes."],
            Some("{{prompt}}"),
            &|_| Ok(()),
        )?;
        set(answers, "id", id);
        set(answers, "kind", "byok".into());
        set(answers, "command", command);
        set(answers, "args", args);
        return Ok(());
    }

    let known = crate::catalog::find(&pick).expect("the menu only offers catalog ids");
    if !found.iter().any(|f| f.known.id == pick && f.present) {
        io.note(&format!(
            "{} was not found on PATH. Its argv is a template — confirm it before a long run.",
            known.label
        ));
    }
    let model = model_for(io, known)?;
    let id = io.ask_text(
        "Provider id (how nodes refer to it)",
        &["A short handle used in the config. The CLI name is a fine default."],
        Some(known.id),
        &nonempty,
    )?;

    set(answers, "id", id);
    set(answers, "kind", known.kind.to_string());
    set(answers, "command", known.bin.to_string());
    set(answers, "args", known.args.join(" "));
    set(answers, "tiers", known.tiers.join(", "));
    set(answers, "requires_env", known.requires_env.join(", "));
    set(answers, "prompt_on_stdin", known.prompt_on_stdin.to_string());
    if let Some(m) = model {
        set(answers, "model", m);
    }
    if let Some(cost) = known.cost_per_1k {
        set(answers, "cost_per_1k_tokens", cost.to_string());
    }
    Ok(())
}

fn model_for(io: &mut Io, known: &'static Known) -> Result<Option<String>, Nav> {
    if known.models.is_empty() {
        if known.discovers_models {
            io.note("This CLI reports its own models; leaving the model blank uses its default.");
        }
        return Ok(None);
    }
    let mut choices: Vec<Choice> = known.models.iter().map(|m| Choice::new(*m, *m)).collect();
    choices.push(Choice::new("__custom__", "Type a different model id"));
    let chosen = io.ask_select(&format!("Model for {}", known.label), &[], &choices, Some(0))?;
    if chosen == "__custom__" {
        return Ok(Some(io.ask_text("Model id", &[], None, &nonempty)?));
    }
    Ok(Some(chosen))
}

/// Route every tier through the providers that were picked, in order. Without
/// a cascade a tier resolves to "every provider that admits to serving it",
/// which is right until two of them do.
fn cascade(path: &str, answers: &mut Answers) {
    let ids: Vec<String> = (0..answers::entry_count(path, answers))
        .filter_map(|i| answers.get(&format!("{path}[{i}].id")).cloned())
        .collect();
    if ids.is_empty() {
        return;
    }
    let root = path
        .rsplit_once('.')
        .map(|(head, _)| format!("{head}.cascade"))
        .unwrap_or_else(|| "cascade".into());
    for tier in ["cheap", "standard", "strong"] {
        answers.insert(format!("{root}.{tier}"), ids.join(", "));
    }
}

fn nonempty(v: &str) -> Result<(), String> {
    if v.trim().is_empty() {
        Err("this one is needed".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers_with(pairs: &[(&str, &str)]) -> Answers {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn removing_an_entry_closes_the_gap_behind_it() {
        // Indices have to stay contiguous: the converter counts entries by the
        // highest index it sees, so a hole would resurrect a removed goal as
        // an empty one.
        let mut a = answers_with(&[
            ("intent.goals[0].name", "one"),
            ("intent.goals[1].name", "two"),
            ("intent.goals[2].name", "three"),
        ]);
        remove_entry("intent.goals", 1, 3, &mut a);
        assert_eq!(answers::entry_count("intent.goals", &a), 2);
        assert_eq!(a.get("intent.goals[0].name").unwrap(), "one");
        assert_eq!(a.get("intent.goals[1].name").unwrap(), "three");
        assert!(a.keys().all(|k| !k.starts_with("intent.goals[2]")));
    }

    #[test]
    fn an_abandoned_entry_leaves_nothing_behind() {
        let mut a = answers_with(&[
            ("intent.goals[0].name", "one"),
            ("intent.goals[1].name", "half-typed"),
        ]);
        clear_entry("intent.goals", 1, &mut a);
        assert_eq!(answers::entry_count("intent.goals", &a), 1);
    }

    #[test]
    fn the_cascade_routes_every_tier_through_what_was_picked() {
        let mut a = answers_with(&[
            ("execution.providers.providers[0].id", "claude"),
            ("execution.providers.providers[1].id", "ollama"),
        ]);
        cascade("execution.providers.providers", &mut a);
        for tier in ["cheap", "standard", "strong"] {
            assert_eq!(
                a.get(&format!("execution.providers.cascade.{tier}")).unwrap(),
                "claude, ollama"
            );
        }
    }
}
