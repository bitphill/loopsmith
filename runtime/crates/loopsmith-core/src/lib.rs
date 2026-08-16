//! Config model and validation for loopsmith.
//!
//! A loop config is the A–H model from the template:
//!
//! - **A** `information`      — static context handed to every node
//! - **B** `pre_execution`    — the "do it manually first" work list
//! - **C** `goals`            — named objectives in natural language
//! - **D** `validations`      — how a goal is checked, per goal or `overall`
//! - **E** `success`          — what counts as success, per goal or `overall`
//! - **F** `stop_gates`       — the four layered exits
//! - **G** `schedules`        — time or event triggers
//! - **H** `constraints`      — limits applied per node or globally
//!
//! Validation exists to make the corpus rule enforceable: a goal without a
//! machine-checkable validation is the single most common way loops fail, so
//! the config is rejected rather than run.

pub mod config;
pub mod validate;

pub use config::*;
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

/// Load a config from YAML or JSON, chosen by extension with a fallback that
/// tries both. Does not validate — call [`validate`] separately so callers can
/// decide whether warnings are fatal.
pub fn load(path: impl AsRef<Path>) -> Result<LoopConfig, CoreError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| CoreError::Io {
        path: path.display().to_string(),
        source,
    })?;
    parse_str(&text, &path.display().to_string())
}

/// Parse config text, trying YAML first (a superset of JSON in practice) and
/// falling back to strict JSON so both error messages survive to the caller.
pub fn parse_str(text: &str, origin: &str) -> Result<LoopConfig, CoreError> {
    let yaml_err = match serde_yaml::from_str::<LoopConfig>(text) {
        Ok(cfg) => return Ok(cfg),
        Err(e) => e.to_string(),
    };
    let json_err = match serde_json::from_str::<LoopConfig>(text) {
        Ok(cfg) => return Ok(cfg),
        Err(e) => e.to_string(),
    };
    Err(CoreError::Parse {
        path: origin.to_string(),
        yaml: yaml_err,
        json: json_err,
    })
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
