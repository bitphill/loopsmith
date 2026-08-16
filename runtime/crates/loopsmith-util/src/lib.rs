//! Primitives that three or more loopsmith crates need and none of them owns.
//!
//! This crate has no dependencies, not even `serde`. That is deliberate: every
//! other crate in the workspace depends on it, so anything pulled in here is
//! pulled in everywhere.
//!
//! What lives here earned its place by having been written more than once:
//!
//! - [`which`] existed three times, in three different states of correctness.
//!   Only one of them checked the executable bit, so a non-executable file
//!   named `curl` on `PATH` read as "curl is available" to two callers.
//! - [`now_ms`] is the single clock. Timestamps are stored as numbers so the
//!   ledger stays sortable without a date parser.
//! - [`testing::temp_dir`] existed six times in four shapes. Two of those
//!   shapes omitted the atomic counter, which is the part that prevents the
//!   sled lock collision described on that function.

use std::path::{Path, PathBuf};

/// Resolve a command the way a shell would: absolute paths (and anything
/// containing a separator) are checked directly, bare names are resolved
/// against `PATH`.
///
/// A candidate counts only if it is executable. Checking `is_file()` alone —
/// which two of the three previous copies of this function did — reports a
/// non-executable file on `PATH` as an available command, and the failure then
/// surfaces much later as a confusing spawn error.
pub fn which(cmd: &str) -> Option<PathBuf> {
    let p = Path::new(cmd);
    if p.is_absolute() || cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') {
        return is_executable(p).then(|| p.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(cmd);
        is_executable(&candidate).then_some(candidate)
    })
}

/// Whether a path is a file the current process could execute.
///
/// Off unix there is no permission bit to consult, so this degrades to a file
/// check rather than pretending to know more than it does.
pub fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Milliseconds since the Unix epoch.
///
/// A clock that runs backwards yields 0 rather than panicking: a wrong
/// timestamp on a ledger entry is a nuisance, and a panic in the middle of an
/// unattended run is not.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(feature = "testing")]
pub mod testing {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A unique path under the system temp directory. **Not created.**
    ///
    /// The counter is the load-bearing part. Tests run in parallel threads
    /// within one process, so pid and millisecond timestamp are not enough to
    /// separate two directories created in the same millisecond — and when two
    /// tests share a directory, sled reports a lock error that reads like a
    /// backend bug rather than a test collision.
    pub fn temp_path(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "loopsmith-{tag}-{}-{}-{n}",
            std::process::id(),
            super::now_ms()
        ))
    }

    /// A unique directory under the system temp directory, created.
    pub fn temp_dir(tag: &str) -> PathBuf {
        let p = temp_path(tag);
        std::fs::create_dir_all(&p).expect("create temp dir");
        p
    }

    /// Best-effort teardown. A leaked temp directory must never fail a test.
    pub fn cleanup(p: &Path) {
        let _ = std::fs::remove_dir_all(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_on_path_resolves() {
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-real-binary-xyz").is_none());
    }

    #[test]
    fn an_absolute_path_is_checked_directly_not_joined_onto_path() {
        // The previous PATH-only copies of `which` joined an absolute path onto
        // every PATH entry and always returned None.
        let sh = which("sh").expect("sh on PATH");
        assert_eq!(which(&sh.to_string_lossy()).as_deref(), Some(sh.as_path()));
    }

    #[test]
    fn a_non_executable_file_is_not_a_command() {
        // Asserted through `is_executable` and the absolute-path branch of
        // `which` rather than by swapping PATH: `set_var` races every other
        // test thread that reads the environment, which is the same class of
        // parallel-test collision the temp-dir counter exists to prevent.
        let dir = std::env::temp_dir().join(format!("loopsmith-which-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bait = dir.join("loopsmith-not-really-a-binary");
        std::fs::write(&bait, "not executable").unwrap();

        assert!(!is_executable(&bait), "a plain file is not executable");
        assert!(
            which(&bait.to_string_lossy()).is_none(),
            "a non-executable file is not a command"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_clock_is_after_2020() {
        assert!(now_ms() > 1_577_836_800_000);
    }
}
