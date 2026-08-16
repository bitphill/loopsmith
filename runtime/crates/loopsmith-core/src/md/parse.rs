//! Markdown document → `serde_yaml::Value` → `LoopConfig`.
//!
//! The parser is deliberately ignorant of the config model. It produces a
//! generic value tree and lets serde do the typing, so a new section needs an
//! entry in [`super::section_shape`] at most, and usually nothing at all.

use super::{heading_to_key, section_shape};
use crate::{CoreError, LoopConfig};
use serde_yaml::{Mapping, Value};

#[derive(Debug)]
enum Tok {
    H1(String),
    H2(String),
    H3(String),
    Bullet { indent: usize, text: String },
}

/// Parse a markdown config.
pub fn parse_md(text: &str, origin: &str) -> Result<LoopConfig, CoreError> {
    let toks = tokenize(text);
    let value = build_document(&toks).map_err(|e| CoreError::Parse {
        path: origin.to_string(),
        yaml: e,
        json: "not attempted: the file was read as markdown".into(),
    })?;
    serde_yaml::from_value::<LoopConfig>(value).map_err(|e| CoreError::Parse {
        path: origin.to_string(),
        yaml: e.to_string(),
        json: "not attempted: the file was read as markdown".into(),
    })
}

/// Split the document into headings and bullets, folding indented
/// continuation lines into the bullet above them.
///
/// Anything else — paragraphs, tables, fenced blocks at the left margin — is
/// documentation and is dropped. That is the feature: the config explains
/// itself in the same file.
fn tokenize(text: &str) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    let mut fenced = false;
    let mut last_bullet_indent: Option<usize> = None;
    let mut after_blank = false;

    for raw in text.lines() {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();

        if trimmed.starts_with("```") && indent == 0 {
            fenced = !fenced;
            last_bullet_indent = None;
            continue;
        }
        if fenced {
            continue;
        }
        if trimmed.is_empty() {
            after_blank = true;
            continue;
        }

        if indent == 0 {
            if let Some(rest) = trimmed.strip_prefix("### ") {
                out.push(Tok::H3(rest.trim().to_string()));
                last_bullet_indent = None;
                after_blank = false;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("## ") {
                out.push(Tok::H2(rest.trim().to_string()));
                last_bullet_indent = None;
                after_blank = false;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("# ") {
                out.push(Tok::H1(rest.trim().to_string()));
                last_bullet_indent = None;
                after_blank = false;
                continue;
            }
        }

        if let Some(rest) = trimmed.strip_prefix("- ") {
            out.push(Tok::Bullet {
                indent,
                text: rest.trim_end().to_string(),
            });
            last_bullet_indent = Some(indent);
            after_blank = false;
            continue;
        }

        // A non-bullet line indented past the bullet above it, with no blank
        // line in between, continues that bullet's value. This is how a long
        // `instruction` spans several lines without becoming prose.
        if let Some(bi) = last_bullet_indent {
            if !after_blank && indent > bi {
                if let Some(Tok::Bullet { text, .. }) = out.last_mut() {
                    text.push('\n');
                    text.push_str(trimmed.trim_end());
                    continue;
                }
            }
        }

        // Anything else is prose.
        last_bullet_indent = None;
        after_blank = false;
    }
    out
}

/// Assemble the token stream into the config mapping.
fn build_document(toks: &[Tok]) -> Result<Value, String> {
    let mut root = Mapping::new();
    // The section currently open, and the entry currently open inside it.
    let mut section: Option<String> = None;
    let mut entry: Option<Mapping> = None;

    let mut i = 0usize;
    while i < toks.len() {
        match &toks[i] {
            Tok::H1(name) => {
                flush_entry(&mut root, &section, &mut entry)?;
                root.insert(Value::from("name"), Value::from(name.clone()));
                i += 1;
            }
            Tok::H2(heading) => {
                flush_entry(&mut root, &section, &mut entry)?;
                section = Some(heading_to_key(heading));
                i += 1;
            }
            Tok::H3(heading) => {
                flush_entry(&mut root, &section, &mut entry)?;
                let Some(sec) = section.as_deref() else {
                    return Err(format!(
                        "`### {heading}` appears before any `##` section heading"
                    ));
                };
                let shape = section_shape(sec).ok_or_else(|| {
                    format!("section `{sec}` does not take `###` entries; use bullets")
                })?;
                let mut m = Mapping::new();
                // A heading is always a string, never re-interpreted as YAML.
                // `### Recorded the baseline: test count, coverage` would
                // otherwise parse as a one-entry mapping and land on a field
                // that wanted text.
                m.insert(
                    Value::from(shape.key_field),
                    Value::from(heading.to_string()),
                );
                entry = Some(m);
                i += 1;
            }
            Tok::Bullet { indent, .. } => {
                let base = *indent;
                let end = toks[i..]
                    .iter()
                    .position(|t| !matches!(t, Tok::Bullet { .. }))
                    .map(|p| i + p)
                    .unwrap_or(toks.len());
                let block: Vec<(usize, &str)> = toks[i..end]
                    .iter()
                    .map(|t| match t {
                        Tok::Bullet { indent, text } => (*indent, text.as_str()),
                        _ => unreachable!("filtered above"),
                    })
                    .collect();
                let (value, _) = build_block(&block, 0, base)?;

                match (&mut entry, section.as_deref()) {
                    // Bullets inside a `###` entry are that entry's fields.
                    (Some(m), _) => merge_into(m, value)?,
                    // Bullets directly under a `##` section are the section.
                    (None, Some(sec)) => {
                        let slot = root
                            .entry(Value::from(sec.to_string()))
                            .or_insert(Value::Mapping(Mapping::new()));
                        match slot {
                            Value::Mapping(m) => merge_into(m, value)?,
                            _ => return Err(format!("section `{sec}` already holds a list")),
                        }
                    }
                    // Bullets before any section are top-level fields.
                    (None, None) => merge_into(&mut root, value)?,
                }
                i = end;
            }
        }
    }
    flush_entry(&mut root, &section, &mut entry)?;
    Ok(Value::Mapping(root))
}

/// Append a finished `###` entry to its section's list.
fn flush_entry(
    root: &mut Mapping,
    section: &Option<String>,
    entry: &mut Option<Mapping>,
) -> Result<(), String> {
    let Some(m) = entry.take() else {
        return Ok(());
    };
    let sec = section
        .as_deref()
        .ok_or_else(|| "an entry was written outside any section".to_string())?;
    let shape = section_shape(sec).ok_or_else(|| format!("section `{sec}` takes no entries"))?;

    let target = match shape.list_field {
        // e.g. `graph` holds its entries under `graph.nodes`.
        Some(field) => {
            let slot = root
                .entry(Value::from(sec.to_string()))
                .or_insert(Value::Mapping(Mapping::new()));
            let Value::Mapping(section_map) = slot else {
                return Err(format!("section `{sec}` should be a mapping"));
            };
            section_map
                .entry(Value::from(field))
                .or_insert(Value::Sequence(vec![]))
        }
        // e.g. `goals` *is* the list.
        None => root
            .entry(Value::from(sec.to_string()))
            .or_insert(Value::Sequence(vec![])),
    };
    match target {
        Value::Sequence(seq) => seq.push(Value::Mapping(m)),
        _ => return Err(format!("section `{sec}` already holds a mapping")),
    }
    Ok(())
}

fn merge_into(target: &mut Mapping, value: Value) -> Result<(), String> {
    match value {
        Value::Mapping(m) => {
            for (k, v) in m {
                target.insert(k, v);
            }
            Ok(())
        }
        _ => Err("expected `- key: value` bullets here, found a bare list".into()),
    }
}

/// Turn one indentation level of bullets into a mapping or a sequence.
///
/// Returns the built value and the index just past the block it consumed.
fn build_block(
    lines: &[(usize, &str)],
    start: usize,
    indent: usize,
) -> Result<(Value, usize), String> {
    let mut map = Mapping::new();
    let mut seq: Vec<Value> = Vec::new();
    let mut i = start;

    while i < lines.len() {
        let (ind, text) = lines[i];
        if ind < indent {
            break;
        }
        if ind > indent {
            return Err(format!("unexpected extra indentation before `- {text}`"));
        }

        match split_field(text) {
            Some((key, "")) => {
                // A key with no value owns the deeper bullets below it.
                let child_indent = lines.get(i + 1).map(|(n, _)| *n).unwrap_or(indent);
                if child_indent > indent {
                    let (child, next) = build_block(lines, i + 1, child_indent)?;
                    map.insert(scalar(key), child);
                    i = next;
                } else {
                    // Nothing below: an explicitly empty value.
                    map.insert(scalar(key), Value::Null);
                    i += 1;
                }
            }
            Some((key, rest)) => {
                map.insert(scalar(key), scalar(rest));
                i += 1;
            }
            None => {
                seq.push(scalar(text));
                i += 1;
            }
        }
    }

    if !map.is_empty() && !seq.is_empty() {
        return Err("a bullet list mixes `key: value` entries with bare items".into());
    }
    if map.is_empty() && !seq.is_empty() {
        return Ok((Value::Sequence(seq), i));
    }
    Ok((Value::Mapping(map), i))
}

/// Split `key: value` when the text really is a field rather than prose.
///
/// The guard on spaces is what keeps `- Never git stash. Never git reset.`
/// from being read as a field named `Never git stash. Never git reset.`.
fn split_field(text: &str) -> Option<(&str, &str)> {
    let (key, rest) = match text.split_once(": ") {
        Some((k, v)) => (k, v.trim()),
        None => (text.strip_suffix(':')?, ""),
    };
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    Some((key, rest))
}

/// Interpret a scalar the way YAML would, so `12`, `true`, and `[a, b]` arrive
/// as the types they look like. Multi-line values stay verbatim strings —
/// prose with a colon in it is not a mapping.
fn scalar(text: &str) -> Value {
    if text.contains('\n') {
        return Value::from(text.to_string());
    }
    serde_yaml::from_str::<Value>(text).unwrap_or_else(|_| Value::from(text.to_string()))
}
