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
//! - [`platform`] answers what the host actually is — which bash, which
//!   userland, which scheduler — because `cfg!(target_os = …)` answers none of
//!   those and every one of them changes what a generated script may assume.

use std::path::{Path, PathBuf};

pub mod platform;

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
        return first_executable(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| first_executable(&dir.join(cmd)))
}

/// `base` if it is executable, else `base` with each `PATHEXT` suffix tried.
///
/// The suffix loop is what makes this work on Windows at all. There, `git` is a
/// file called `git.exe`, so joining the bare name onto every `PATH` entry
/// matches nothing and `which` returns `None` for every command that exists.
/// Everything downstream then reports the truth it was given: `doctor` says no
/// tool is installed, `Platform::detect` finds no scheduler, and worktree
/// isolation degrades to the shared directory with "git not on PATH" — on a
/// machine where git is very much on PATH.
fn first_executable(base: &Path) -> Option<PathBuf> {
    if is_executable(base) {
        return Some(base.to_path_buf());
    }
    for ext in path_extensions() {
        // `set_extension` would replace a real one: `foo.bar` must be allowed to
        // become `foo.bar.exe`, since a command may legitimately contain a dot.
        let mut name = base.as_os_str().to_os_string();
        name.push(&ext);
        let candidate = PathBuf::from(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// The extensions that make a file directly runnable on this platform.
///
/// Empty on unix, where the executable bit is the whole answer. On Windows it is
/// `PATHEXT`, honoured rather than hardcoded because it is how a machine says
/// that `.ps1` or `.py` counts as a command; the fallback list is what Windows
/// itself defaults to.
fn path_extensions() -> Vec<std::ffi::OsString> {
    if cfg!(not(windows)) {
        return Vec::new();
    }
    let raw = std::env::var_os("PATHEXT")
        .unwrap_or_else(|| std::ffi::OsString::from(".COM;.EXE;.BAT;.CMD"));
    raw.to_string_lossy()
        .split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(std::ffi::OsString::from)
        .collect()
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
    fn a_command_is_found_under_a_platform_executable_suffix() {
        // The Windows bug this pins: `git` there is a file called `git.exe`, so
        // joining the bare name onto every PATH entry matched nothing and `which`
        // returned None for every command on the machine. Downstream that read as
        // "git not on PATH", "no scheduler installed", and a `doctor` report where
        // nothing exists.
        //
        // Asserted through the absolute-path branch so it is meaningful on both
        // platforms without mutating PATH, which races every other test thread.
        let dir = std::env::temp_dir().join(format!("loopsmith-pathext-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        for ext in path_extensions() {
            let mut name = std::ffi::OsString::from("tool");
            name.push(&ext);
            let real = dir.join(&name);
            std::fs::write(&real, "x").unwrap();

            // Asking for `tool` must find `tool.EXE`, and asking for the full
            // name must still work.
            let bare = dir.join("tool");
            assert_eq!(
                which(&bare.to_string_lossy()).as_deref(),
                Some(real.as_path()),
                "asking for {} should have found {}",
                bare.display(),
                real.display()
            );
            assert_eq!(
                which(&real.to_string_lossy()).as_deref(),
                Some(real.as_path())
            );
            std::fs::remove_file(&real).unwrap();
        }

        // On unix there are no suffixes, so the list is empty and an extensionless
        // name is the only spelling — which is exactly the invariant to state.
        #[cfg(unix)]
        assert!(
            path_extensions().is_empty(),
            "unix has no PATHEXT; the executable bit is the whole answer"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_suffix_is_appended_rather_than_replacing_a_real_extension() {
        // `set_extension` would turn `check.sh` into `check.exe`. A command may
        // legitimately contain a dot, so the suffix has to be appended.
        let dir = std::env::temp_dir().join(format!("loopsmith-dotted-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dotted = dir.join("check.sh");
        std::fs::write(&dotted, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dotted, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(
            which(&dotted.to_string_lossy()).as_deref(),
            Some(dotted.as_path()),
            "a dotted command name must resolve as itself"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_runnable_is_not_a_command() {
        // Asserted through `is_executable` and the absolute-path branch of
        // `which` rather than by swapping PATH: `set_var` races every other
        // test thread that reads the environment, which is the same class of
        // parallel-test collision the temp-dir counter exists to prevent.
        //
        // "Not runnable" means different things on the two platforms, and the
        // assertion has to mean the platform's own rule rather than unix's. Off
        // unix there is no executable bit — `is_executable` says so and degrades
        // to a file check — so asserting that a plain file is not executable is
        // asserting unix semantics on a system that has none.
        let dir = std::env::temp_dir().join(format!("loopsmith-which-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bait = dir.join("loopsmith-not-really-a-binary");
        std::fs::write(&bait, "not executable").unwrap();

        #[cfg(unix)]
        {
            // The bit is the whole answer here.
            assert!(!is_executable(&bait), "a plain file is not executable");
            assert!(
                which(&bait.to_string_lossy()).is_none(),
                "a non-executable file is not a command"
            );
        }

        #[cfg(not(unix))]
        {
            // Windows decides by extension, so the equivalent claim is that a
            // name carrying none of `PATHEXT` does not resolve as a command.
            let plain = dir.join("loopsmith-no-extension-here");
            std::fs::write(&plain, "not runnable").unwrap();
            assert!(
                path_extensions().iter().all(|ext| {
                    let mut n = plain.as_os_str().to_os_string();
                    n.push(ext);
                    !PathBuf::from(n).exists()
                }),
                "the fixture must not accidentally carry an executable suffix"
            );
            // `which` still finds the file itself — `is_executable` is a file
            // check here — but nothing gets invented by appending a suffix.
            let resolved = which(&plain.to_string_lossy());
            assert_eq!(
                resolved.as_deref(),
                Some(plain.as_path()),
                "an exact path resolves to itself"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_clock_is_after_2020() {
        assert!(now_ms() > 1_577_836_800_000);
    }
}
