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

pub use parse::parse_md;
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
    /// Field a `###` heading fills in.
    pub key_field: &'static str,
}

pub(crate) fn section_shape(section: &str) -> Option<SectionShape> {
    let (list_field, key_field) = match section {
        "information" => (None, "key"),
        "pre_execution" => (None, "step"),
        "goals" => (None, "name"),
        "validations" => (None, "name"),
        "success" => (None, "name"),
        "schedules" => (None, "type"),
        "execution_guidelines" => (Some("items"), "name"),
        "default_skills" => (None, "name"),
        "graph" => (Some("nodes"), "id"),
        "providers" => (Some("providers"), "id"),
        _ => return None,
    };
    Some(SectionShape {
        list_field,
        key_field,
    })
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
