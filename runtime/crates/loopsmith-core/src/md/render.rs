//! `LoopConfig` → markdown document.
//!
//! The exact inverse of [`super::parse_md`], written against
//! `serde_yaml::Value` for the same reason: it stays correct when the config
//! model grows.
//!
//! One thing markdown cannot carry is trailing whitespace inside a value — a
//! bullet ends where the line ends. Values are therefore emitted `trim_end`ed.
//! Nothing else is lost.

use super::{section_shape, SECTION_PATHS};
use crate::LoopConfig;
use serde_yaml::Value;

/// Top-level keys that are not sections; they render as preamble bullets.
const PREAMBLE: &[&str] = &["version", "description", "environment", "features"];

/// Resolve a dotted path against a serialised config.
fn at<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

/// Every leaf of the config must be reachable from [`SECTION_PATHS`] or
/// [`PREAMBLE`]. A leaf in neither is silently dropped on render, which is how
/// the `context` section went missing until a round-trip test caught it.
#[cfg(test)]
fn covered_paths() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = SECTION_PATHS.iter().map(|(_, p, _)| *p).collect();
    v.extend_from_slice(PREAMBLE);
    v.push("name");
    v
}

pub fn render_md(cfg: &LoopConfig) -> String {
    let Ok(root) = serde_yaml::to_value(cfg) else {
        // `LoopConfig` always serializes to a mapping; this arm exists so the
        // function has no panic in it.
        return String::new();
    };
    if !root.is_mapping() {
        return String::new();
    }

    let mut out = String::new();
    let name = root
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed-loop");
    out.push_str(&format!("# {name}\n\n"));

    for key in PREAMBLE {
        if let Some(v) = root.get(*key) {
            if !is_blank(v) {
                push_field(&mut out, key, v, 0);
            }
        }
    }
    if out.lines().count() > 2 {
        out.push('\n');
    }

    for (_, path, heading) in SECTION_PATHS {
        let Some(value) = at(&root, path) else {
            continue;
        };
        if is_blank(value) {
            continue;
        }
        out.push_str(&format!("## {heading}\n\n"));
        render_section(&mut out, path, value);
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
    let heading = super::nested_get(m, key_field)
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed");
    out.push_str(&format!("### {heading}\n"));

    // Render everything except the field the heading already carries. Removing
    // it from a clone rather than skipping it inline is what makes a dotted
    // key work: `on.type` has to come out of the nested mapping, and the
    // now-empty `on` has to come out with it.
    let mut rest = m.clone();
    super::nested_remove(&mut rest, key_field);
    for (k, v) in &rest {
        if is_blank(v) {
            continue;
        }
        push_field(out, k.as_str().unwrap_or_default(), v, 0);
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
    fn the_renderer_knows_about_every_config_section() {
        // Serialise a config and check that every section it produces has
        // somewhere to go. Sections now sit one level inside a bundle, so this
        // walks two levels: a top-level key is covered if it is itself listed
        // (`name`, the preamble, a bundle that *is* a section like `evolution`)
        // or if every key beneath it is.
        //
        // Without this, adding a section to a bundle and forgetting
        // SECTION_PATHS loses it silently on the markdown path — which is
        // exactly what happened to `context` once already.
        let cfg = crate::parse_str(
            r#"
name: t
intent:
  goals: [{ name: g1, description: a sufficiently long goal description }]
safety:
  checks:
    - target: g1
      name: v
      mode: objective
      statement: it exists
      detector: { type: file_exists, path: out.txt }
"#,
            "test",
        )
        .expect("parses");

        let root = serde_yaml::to_value(&cfg).expect("a config serialises");
        let covered = covered_paths();
        let mut missing: Vec<String> = Vec::new();

        for (key, value) in root.as_mapping().expect("a mapping") {
            let Some(key) = key.as_str() else { continue };
            if covered.contains(&key) {
                continue;
            }
            // A bundle: every section inside it must be listed by full path.
            let Some(inner) = value.as_mapping() else {
                missing.push(key.to_string());
                continue;
            };
            for sub in inner.keys().filter_map(|k| k.as_str()) {
                let path = format!("{key}.{sub}");
                if !covered.contains(&path.as_str()) {
                    missing.push(path);
                }
            }
        }

        assert!(
            missing.is_empty(),
            "these sections would be dropped by render_md; add them to \
             SECTION_PATHS or PREAMBLE: {missing:?}"
        );
    }
}
