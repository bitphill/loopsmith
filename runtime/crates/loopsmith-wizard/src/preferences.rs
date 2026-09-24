//! What the terminal remembers between wizard runs.
//!
//! One thing, for now: whether this person wants the guided walk or the
//! expert's editor. Asking it once and remembering it is what makes the choice
//! worth offering at all — a question asked on every run is a toll, not a
//! preference — and it mirrors what the browser already does with its first
//! screen.
//!
//! Stored in `~/.loopsmith/wizard.json`, beside the loop library the web UI
//! keeps. A machine where the home directory cannot be found or written is not
//! an error: the preference is simply not remembered, and the question is
//! asked again next time.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How much hand-holding the wizard gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Every question, one at a time, with its explanation.
    Novice,
    /// A filled-in config in `$EDITOR`, validated on the way out.
    Expert,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Novice => "novice",
            Level::Expert => "expert",
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    level: Option<Level>,
}

fn path() -> Option<PathBuf> {
    crate::detect::home_dir().map(|h| h.join(".loopsmith/wizard.json"))
}

/// The level this person picked last time, if they ever did.
pub fn level() -> Option<Level> {
    let text = std::fs::read_to_string(path()?).ok()?;
    serde_json::from_str::<Stored>(&text).ok()?.level
}

/// Remember a level. Silent on failure: a preference that cannot be written is
/// not a reason to stop.
pub fn remember(level: Level) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let stored = Stored { level: Some(level) };
    if let Ok(text) = serde_json::to_string_pretty(&stored) {
        let _ = std::fs::write(p, text);
    }
}

/// Forget it, so the question is asked again.
pub fn forget() {
    if let Some(p) = path() {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_survives_being_written_and_read() {
        // The real file is this machine's, so the round trip is tested at the
        // serialisation seam rather than by writing to a home directory a test
        // has no business touching.
        let stored = Stored {
            level: Some(Level::Expert),
        };
        let text = serde_json::to_string(&stored).unwrap();
        assert!(text.contains("expert"));
        let back: Stored = serde_json::from_str(&text).unwrap();
        assert_eq!(back.level, Some(Level::Expert));
    }

    #[test]
    fn a_file_from_a_later_version_does_not_lose_the_level() {
        let back: Stored = serde_json::from_str(r#"{"level":"novice","theme":"dark"}"#).unwrap();
        assert_eq!(back.level, Some(Level::Novice));
    }
}
