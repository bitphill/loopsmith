//! Markdown-native config: the same A–J model, written as a document.
//!
//! A `.md` config is not YAML wearing a markdown hat. Headings are sections,
//! `###` headings are entries, bullets are fields, and any prose at column 0 is
//! documentation that the parser ignores. That means a loop config can explain
//! itself in place — the reason a goal exists sits next to the goal.
//!
//! # Shape
//!
//! ```markdown
//! # my-loop
//!
//! - version: 0.1.0
//! - description: what this loop is for
//!
//! Prose at the left margin is ignored. Put the reasoning here.
//!
//! ## C. Goals
//!
//! ### ship-it
//! - description: the thing is shipped and the suite is green
//! - priority: 1
//!
//! ## F. Stop gates
//! - max_iterations: 12
//! - max_cost_usd: 10.0
//! ```
//!
//! # How it works
//!
//! The parser does **not** know about `Goal` or `StopGates`. It turns the
//! document into a `serde_yaml::Value` and hands that to the same `Deserialize`
//! impls the YAML path uses. Every default, alias, and `deny_unknown_fields`
//! rule therefore applies identically, and a new config field needs no parser
//! change at all.
//!
//! The renderer is the exact inverse, over `serde_yaml::to_value`. Round-trip
//! is a property test rather than a hope.

mod parse;
mod render;

pub use parse::{parse_md, parse_md_reporting};
pub use render::render_md;

/// Where a `###` heading's text goes, per section.
///
/// This is the only place the markdown layer knows anything section-specific,
/// and it exists because `### ship-it` has to become `name: ship-it` for a goal
/// but `id: ship-it` for a node. Sections absent from this table take no `###`
/// entries — they are plain field bags like `stop_gates`.
pub(crate) struct SectionShape {
    /// Field inside the section that holds the list, or `None` when the
    /// section *is* the list.
    pub list_field: Option<&'static str>,
    /// Field a `###` heading fills in. May be dotted: a trigger's heading is
    /// its kind, which sits at `on.type` now that the trigger is nested inside
    /// the spec that carries its idempotency key.
    pub key_field: &'static str,
}

/// Read a possibly-dotted key out of a mapping.
pub(crate) fn nested_get<'a>(
    m: &'a serde_yaml::Mapping,
    path: &str,
) -> Option<&'a serde_yaml::Value> {
    let mut parts = path.split('.');
    let mut cur = m.get(serde_yaml::Value::from(parts.next()?))?;
    for part in parts {
        cur = cur.get(part)?;
    }
    Some(cur)
}

/// Write a possibly-dotted key into a mapping, creating parents.
pub(crate) fn nested_insert(m: &mut serde_yaml::Mapping, path: &str, value: serde_yaml::Value) {
    use serde_yaml::{Mapping, Value};
    let mut parts = path.split('.').peekable();
    let mut cursor = m;
    loop {
        let part = parts.next().expect("a path has at least one segment");
        let key = Value::from(part);
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

/// Remove a possibly-dotted key, dropping any parent it leaves empty.
///
/// The parent cleanup is what keeps a rendered trigger from carrying an empty
/// `- on:` bullet under the heading that already said which kind it is.
pub(crate) fn nested_remove(m: &mut serde_yaml::Mapping, path: &str) {
    use serde_yaml::Value;
    let Some((head, rest)) = path.split_once('.') else {
        m.remove(Value::from(path));
        return;
    };
    let key = Value::from(head);
    let Some(Value::Mapping(inner)) = m.get_mut(&key) else {
        return;
    };
    nested_remove(inner, rest);
    if inner.is_empty() {
        m.remove(&key);
    }
}

pub(crate) fn section_shape(section: &str) -> Option<SectionShape> {
    let (list_field, key_field) = match section {
        // 1.0 paths.
        "intent.background" => (None, "key"),
        "intent.prerequisites" => (None, "step"),
        "intent.goals" => (None, "name"),
        "intent.success" => (None, "name"),
        "safety.checks" => (None, "name"),
        "execution.triggers" => (Some("triggers"), "on.type"),
        "execution.phases" => (Some("items"), "name"),
        "execution.default_skills" => (None, "name"),
        "execution.graph" => (Some("nodes"), "id"),
        "execution.providers" => (Some("providers"), "id"),
        // 0.3 flat keys. A legacy document is parsed into the 0.3 shape and
        // then handed to `config::legacy::migrate`, so the md layer never
        // learns a second relocation table — and a `### cron` heading whose
        // fields were flat in 0.3 gets wrapped by the same `adapt` step that
        // handles the YAML path.
        "information" => (None, "key"),
        "pre_execution" => (None, "step"),
        "validations" => (None, "name"),
        "schedules" => (None, "type"),
        "execution_guidelines" => (Some("items"), "name"),
        _ => return None,
    };
    Some(SectionShape {
        list_field,
        key_field,
    })
}

/// Heading key → the dotted config path it writes to.
///
/// This is the markdown layer's half of the same bargain [`crate::config::legacy`]
/// makes for YAML: one table, read by the parser going in and the renderer
/// coming out, so a heading cannot mean one thing when written and another when
/// read back.
///
/// Order is the order sections are rendered, and it is the order a human would
/// write them: what the loop is for, how it runs, what stops it, how it learns.
pub(crate) const SECTION_PATHS: &[(&str, &str, &str)] = &[
    // (heading key, dotted path, rendered heading)
    ("background", "intent.background", "Background"),
    ("prerequisites", "intent.prerequisites", "Prerequisites"),
    ("goals", "intent.goals", "Goals"),
    ("success", "intent.success", "Success"),
    ("graph", "execution.graph", "Graph"),
    ("providers", "execution.providers", "Providers"),
    ("phases", "execution.phases", "Phases"),
    ("default_skills", "execution.default_skills", "Default skills"),
    ("skills", "execution.skills", "Skills"),
    ("memory", "execution.memory", "Memory"),
    ("triggers", "execution.triggers", "Triggers"),
    ("checks", "safety.checks", "Checks"),
    ("gates", "safety.gates", "Gates"),
    ("limits", "safety.limits", "Limits"),
    ("recovery", "safety.recovery", "Recovery"),
    ("protected", "safety.protected", "Protected"),
    ("evolution", "evolution", "Evolution"),
];

/// 0.3 heading keys that have no 1.0 spelling of their own.
///
/// A heading in this list is written into the document under its **0.3 flat
/// key**, and [`crate::config::legacy::migrate`] relocates it afterwards. That
/// is deliberate: there is then one description of where old keys go, shared
/// with the YAML path, instead of a second one here that could drift from it.
///
/// `goals`, `graph`, `providers` and `skills` are absent because they are also
/// 1.0 headings — [`SECTION_PATHS`] answers those first, and lands them in the
/// same place the migration would.
///
/// `context` is the reason `intent.background` is not called `intent.context`:
/// in a 0.3 document that heading meant the memory policy.
pub(crate) const LEGACY_SECTION_KEYS: &[&str] = &[
    "information",
    "pre_execution",
    "validations",
    "stop_gates",
    "constraints",
    "execution_guidelines",
    "schedules",
    "context",
];

/// Resolve a normalised heading key to the path it writes to.
///
/// Accepts the 1.0 heading, the 0.3 heading, and the dotted path written out in
/// full — the last because a config that says `## execution.graph` is being
/// unambiguous rather than wrong.
pub(crate) fn section_path(key: &str) -> Option<&'static str> {
    if let Some((_, path, _)) = SECTION_PATHS.iter().find(|(k, _, _)| *k == key) {
        return Some(path);
    }
    if let Some(legacy) = LEGACY_SECTION_KEYS.iter().find(|k| **k == key) {
        return Some(legacy);
    }
    SECTION_PATHS
        .iter()
        .find(|(_, path, _)| path.replace('.', "_") == key)
        .map(|(_, path, _)| *path)
}

/// Normalise a heading into a config key: `A. Pre-execution` → `pre_execution`.
///
/// Section letters are navigation aids for humans, not part of the grammar, so
/// they are stripped. Writing the raw key (`## pre_execution`) works too.
pub(crate) fn heading_to_key(heading: &str) -> String {
    let h = heading.trim();
    // Drop a leading section letter: "A.", "B)", "J -", "10." all count.
    let h = match h.find(['.', ')', '-']) {
        Some(i) if i <= 2 && h[..i].chars().all(|c| c.is_ascii_alphanumeric()) => &h[i + 1..],
        _ => h,
    };
    h.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
        .replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_normalise_to_config_keys() {
        for (heading, key) in [
            ("A. Information", "information"),
            ("B. Pre-execution", "pre_execution"),
            ("F. Stop gates", "stop_gates"),
            ("I. Execution guidelines", "execution_guidelines"),
            ("J. Default skills", "default_skills"),
            ("Providers", "providers"),
            ("stop_gates", "stop_gates"),
            ("  Graph  ", "graph"),
        ] {
            assert_eq!(heading_to_key(heading), key, "for heading `{heading}`");
        }
    }

    #[test]
    fn a_hyphenated_word_is_not_mistaken_for_a_section_letter() {
        // "Pre-execution" has a hyphen at index 3, past the letter window, so
        // the whole word survives.
        assert_eq!(heading_to_key("Pre-execution"), "pre_execution");
    }
}
