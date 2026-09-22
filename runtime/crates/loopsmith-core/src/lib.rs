//! Config model and validation for loopsmith.
//!
//! A loop config is four bundles, each answering one question:
//!
//! - `intent`    — what is this loop for, and how would we know it worked?
//! - `execution` — how does the work get done?
//! - `safety`    — what must not happen, and when does this stop?
//! - `evolution` — how is this allowed to change itself?
//!
//! Until 1.0 these were fourteen flat keys, ten of them known by a letter.
//! Every one of those spellings still loads: [`parse_str`] runs the document
//! through [`config::legacy`] before typing it, and reports what moved. See
//! [`config::bundles`] for why the grouping is what it is.
//!
//! Validation exists to make the corpus rule enforceable: a goal without a
//! machine-checkable validation is the single most common way loops fail, so
//! the config is rejected rather than run.

pub mod config;
pub mod md;
pub mod validate;

pub use config::*;
pub use md::{parse_md, render_md};
pub use validate::{validate, Issue, Severity, ValidationReport};

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse {path} as YAML or JSON:\n  yaml: {yaml}\n  json: {json}")]
    Parse {
        path: String,
        yaml: String,
        json: String,
    },
    #[error("config is invalid:\n{0}")]
    Invalid(String),
}

/// Load a config from Markdown, YAML, or JSON.
///
/// Markdown is chosen by extension, because a `.md` config is a different
/// grammar rather than a different serialization — guessing at it would mean
/// reporting a YAML parse error for a document that was never YAML.
/// Everything else falls through to [`parse_str`], which tries YAML then JSON.
pub fn load(path: impl AsRef<Path>) -> Result<LoopConfig, CoreError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| CoreError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let origin = path.display().to_string();
    if is_markdown(path) {
        return md::parse_md(&text, &origin);
    }
    parse_str(&text, &origin)
}

/// Whether a path should be read as a markdown config.
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}

/// Parse config text, trying YAML first (a superset of JSON in practice) and
/// falling back to strict JSON so both error messages survive to the caller.
///
/// Any 0.3 top-level key is relocated by [`config::legacy`] before typing, so
/// an old file loads unchanged. Use [`parse_str_reporting`] to find out whether
/// that happened.
pub fn parse_str(text: &str, origin: &str) -> Result<LoopConfig, CoreError> {
    parse_str_reporting(text, origin).map(|(cfg, _)| cfg)
}

/// [`parse_str`], additionally reporting which 0.3 keys were relocated.
///
/// The list is empty for a file already in the 1.0 shape. Callers that have
/// somewhere to put a deprecation notice — the CLI, the wizard, `migrate` —
/// use this; everything else uses [`parse_str`].
pub fn parse_str_reporting(
    text: &str,
    origin: &str,
) -> Result<(LoopConfig, Vec<config::legacy::Moved>), CoreError> {
    // Both formats are read into an untyped document first so the same
    // relocation runs for each. Typing directly and only falling back on
    // failure would mean a file mixing old and new keys parses as whichever
    // half the model happened to accept.
    let yaml_err = match serde_yaml::from_str::<serde_yaml::Value>(text) {
        Ok(doc) => match type_document(doc) {
            Ok(out) => return Ok(out),
            Err(e) => e.to_string(),
        },
        Err(e) => e.to_string(),
    };
    let json_err = match serde_json::from_str::<serde_yaml::Value>(text) {
        Ok(doc) => match type_document(doc) {
            Ok(out) => return Ok(out),
            Err(e) => e.to_string(),
        },
        Err(e) => e.to_string(),
    };
    Err(CoreError::Parse {
        path: origin.to_string(),
        yaml: yaml_err,
        json: json_err,
    })
}

fn type_document(
    doc: serde_yaml::Value,
) -> Result<(LoopConfig, Vec<config::legacy::Moved>), serde_yaml::Error> {
    let (doc, moved) = config::legacy::migrate(&doc);
    serde_yaml::from_value::<LoopConfig>(doc).map(|cfg| (cfg, moved))
}

/// The JSON Schema for a loop config, derived from the Rust model.
///
/// Generated rather than hand-written. The previous schema was 800 lines
/// nothing executed, and it had already drifted — `max_revisions_per_node` was
/// declared there, defaulted in Rust, documented twice, and read by no runtime
/// code at all. A generated schema cannot describe a field that does not exist
/// or miss one that does.
///
/// `config/loop.schema.json` is this value, written out. CI regenerates it and
/// fails if the committed copy differs.
pub fn json_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(LoopConfig);
    serde_json::to_value(schema).expect("a generated schema serialises")
}

/// Load and validate in one step, treating any error-severity issue as fatal.
pub fn load_validated(path: impl AsRef<Path>) -> Result<LoopConfig, CoreError> {
    let cfg = load(path)?;
    let report = validate(&cfg);
    if report.has_errors() {
        return Err(CoreError::Invalid(report.render()));
    }
    Ok(cfg)
}
