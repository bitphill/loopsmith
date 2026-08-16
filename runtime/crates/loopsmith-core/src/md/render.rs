//! `LoopConfig` → markdown document.
//!
//! The exact inverse of [`super::parse_md`], written against
//! `serde_yaml::Value` for the same reason: it stays correct when the config
//! model grows.
//!
//! One thing markdown cannot carry is trailing whitespace inside a value — a
//! bullet ends where the line ends. Values are therefore emitted `trim_end`ed.
//! Nothing else is lost.

use super::section_shape;
use crate::LoopConfig;
use serde_yaml::Value;

/// Section order and headings, matching the template so a rendered config and
/// `LOOP-TEMPLATE.md` read the same way. Absent sections are skipped.
const SECTIONS: &[(&str, &str)] = &[
    ("information", "A. Information"),
    ("pre_execution", "B. Pre-execution"),
    ("goals", "C. Goals"),
    ("validations", "D. Validations"),
    ("success", "E. Success"),
    ("stop_gates", "F. Stop gates"),
    ("schedules", "G. Schedules"),
    ("constraints", "H. Constraints"),
    ("execution_guidelines", "I. Execution guidelines"),
    ("default_skills", "J. Default skills"),
    ("graph", "Graph"),
    ("providers", "Providers"),
    ("skills", "Skills"),
    ("context", "Context"),
];

/// Top-level keys that are not sections; they render as preamble bullets.
const PREAMBLE: &[&str] = &["version", "description"];

/// Every top-level key must be either a section, a preamble field, or `name`.
/// A key in none of those lists is silently dropped on render, which is how the
/// `context` section went missing until a round-trip test caught it.
#[cfg(test)]
fn covered_keys() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = SECTIONS.iter().map(|(k, _)| *k).collect();
    v.extend_from_slice(PREAMBLE);
    v.push("name");
    v
}

pub fn render_md(cfg: &LoopConfig) -> String {
    let Ok(Value::Mapping(root)) = serde_yaml::to_value(cfg) else {
        // `LoopConfig` always serializes to a mapping; this arm exists so the
        // function has no panic in it.
        return String::new();
    };

    let mut out = String::new();
    let name = root
        .get(Value::from("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed-loop");
    out.push_str(&format!("# {name}\n\n"));

    for key in PREAMBLE {
        if let Some(v) = root.get(Value::from(*key)) {
            if !is_blank(v) {
                push_field(&mut out, key, v, 0);
            }
        }
    }
    if out.lines().count() > 2 {
        out.push('\n');
    }

    for (key, heading) in SECTIONS {
        let Some(value) = root.get(Value::from(*key)) else {
            continue;
        };
        if is_blank(value) {
            continue;
        }
        out.push_str(&format!("## {heading}\n\n"));
        render_section(&mut out, key, value);
        out.push('\n');
    }
    out
}

fn render_section(out: &mut String, key: &str, value: &Value) {
    let shape = section_shape(key);

    match (value, shape) {
        // The section *is* the list: every element becomes a `###` entry.
        (Value::Sequence(items), Some(s)) => {
            for item in items {
                render_entry(out, item, s.key_field);
            }
        }
        // The section is a mapping that holds a list under a named field.
        (Value::Mapping(m), Some(s)) => {
            let list_field = s.list_field;
            for (k, v) in m {
                let k = k.as_str().unwrap_or_default();
                if Some(k) == list_field || is_blank(v) {
                    continue;
                }
                push_field(out, k, v, 0);
            }
            if let Some(field) = list_field {
                if let Some(Value::Sequence(items)) = m.get(Value::from(field)) {
                    if !items.is_empty() {
                        out.push('\n');
                    }
                    for item in items {
                        render_entry(out, item, s.key_field);
                    }
                }
            }
        }
        // A plain field bag: `stop_gates`, `constraints`, `skills`.
        (Value::Mapping(m), None) => {
            for (k, v) in m {
                if is_blank(v) {
                    continue;
                }
                push_field(out, k.as_str().unwrap_or_default(), v, 0);
            }
        }
        _ => push_value_inline(out, value, 0),
    }
}

/// One `###` entry: its key field becomes the heading, the rest become bullets.
fn render_entry(out: &mut String, item: &Value, key_field: &str) {
    let Value::Mapping(m) = item else {
        push_value_inline(out, item, 0);
        return;
    };
    let heading = m
        .get(Value::from(key_field))
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed");
    out.push_str(&format!("### {heading}\n"));
    for (k, v) in m {
        let k = k.as_str().unwrap_or_default();
        if k == key_field || is_blank(v) {
            continue;
        }
        push_field(out, k, v, 0);
    }
    out.push('\n');
}

fn push_field(out: &mut String, key: &str, value: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match value {
        Value::Mapping(m) => {
            out.push_str(&format!("{pad}- {key}:\n"));
            for (k, v) in m {
                if is_blank(v) {
                    continue;
                }
                push_field(out, k.as_str().unwrap_or_default(), v, indent + 2);
            }
        }
        Value::Sequence(items) if items.iter().all(is_scalar) => {
            out.push_str(&format!("{pad}- {key}: {}\n", flow(value)));
        }
        Value::Sequence(_) => {
            // A nested list of objects has no bullet form that survives a
            // round trip, so it is emitted as inline flow — still valid YAML,
            // which is exactly what the parser feeds a scalar to.
            out.push_str(&format!("{pad}- {key}: {}\n", flow(value)));
        }
        Value::String(s) => push_string(out, key, s, indent),
        other => out.push_str(&format!("{pad}- {key}: {}\n", flow(other))),
    }
}

/// A string that YAML would read back as something other than a string has to
/// be quoted; everything else is written bare so prose stays readable.
fn push_string(out: &mut String, key: &str, s: &str, indent: usize) {
    let pad = " ".repeat(indent);
    let trimmed = s.trim_end();

    if trimmed.contains('\n') {
        let cont = " ".repeat(indent + 4);
        let mut lines = trimmed.lines();
        out.push_str(&format!(
            "{pad}- {key}: {}\n",
            lines.next().unwrap_or_default()
        ));
        for line in lines {
            out.push_str(&format!("{cont}{}\n", line.trim()));
        }
        return;
    }

    let needs_quoting = trimmed.is_empty()
        || !matches!(
            serde_yaml::from_str::<Value>(trimmed),
            Ok(Value::String(ref got)) if got == trimmed
        );
    if needs_quoting {
        out.push_str(&format!("{pad}- {key}: {}\n", flow(&Value::from(trimmed))));
    } else {
        out.push_str(&format!("{pad}- {key}: {trimmed}\n"));
    }
}

fn push_value_inline(out: &mut String, value: &Value, indent: usize) {
    out.push_str(&format!("{}- {}\n", " ".repeat(indent), flow(value)));
}

/// YAML flow style, borrowed from JSON — JSON is a subset of YAML, so this
/// round-trips through the parser's scalar reader unchanged.
fn flow(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Mapping(_) | Value::Sequence(_))
}



/// Empty collections and nulls are omitted: a config full of `- skills: []` is
/// noise, and the defaults put them back on the way in.
///
/// An empty **string** is not blank. `value: ""` is a value the author chose,
/// and on a required field dropping it produces a document that no longer
/// parses — which is exactly what happened to `information[].value` before this
/// distinction existed.
fn is_blank(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Sequence(s) => s.is_empty(),
        Value::Mapping(m) => m.is_empty() || m.values().all(is_blank),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_renderer_knows_about_every_top_level_config_key() {
        // Serialise a default-ish config and check that every key it produces
        // has somewhere to go. Without this, adding a section to `LoopConfig`
        // and forgetting `SECTIONS` loses it silently on the markdown path.
        let cfg = crate::parse_str(
            r#"
name: t
goals: [{ name: g1, description: a sufficiently long goal description }]
validations:
  - target: g1
    name: v
    mode: objective
    statement: it exists
    detector: { type: file_exists, path: out.txt }
"#,
            "test",
        )
        .expect("parses");

        let Ok(Value::Mapping(root)) = serde_yaml::to_value(&cfg) else {
            panic!("a config serialises to a mapping");
        };
        let covered = covered_keys();
        let missing: Vec<String> = root
            .keys()
            .filter_map(|k| k.as_str())
            .filter(|k| !covered.contains(k))
            .map(str::to_string)
            .collect();
        assert!(
            missing.is_empty(),
            "these top-level keys would be dropped by render_md; add them to \
             SECTIONS or PREAMBLE: {missing:?}"
        );
    }
}
