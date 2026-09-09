//! Which of the known agent CLIs are on this machine, decided without a runtime.
//!
//! The web UI answers the same question in [`crate::web::detect`], but that path
//! is async (it also does a paid model handshake behind a button) and lives
//! behind the `web` feature. The terminal wizard needs neither: it only wants to
//! know which binaries exist so it can offer them, and it is already blocking on
//! a human between every question. So this is a plain, synchronous `PATH` scan —
//! no `tokio`, no subprocess, instant, and available in a `--no-default-features`
//! build where the whole `web` module is compiled out.

use crate::catalog::{self, Known};
use std::path::{Path, PathBuf};

/// A known CLI paired with whether its binary was found on `PATH`.
pub struct Found {
    pub known: &'static Known,
    pub present: bool,
    /// The resolved binary path, when found. Shown so the user can confirm the
    /// wizard is about to offer the CLI they think it is.
    pub path: Option<PathBuf>,
}

/// Every catalog entry, each marked present or absent, catalog order preserved.
///
/// Order is deliberately not "present first": the catalog is already ordered
/// local-first (Ollama leaks nothing), and reordering by presence would move a
/// provider's number between runs, which is exactly the kind of thing that makes
/// a numbered menu untrustworthy.
pub fn scan() -> Vec<Found> {
    catalog::KNOWN
        .iter()
        .map(|k| {
            let path = which(k.bin);
            Found {
                known: k,
                present: path.is_some(),
                path,
            }
        })
        .collect()
}

/// Resolve a bare binary name against `PATH`, the way a shell would.
///
/// Hand-rolled rather than shelling out to `which`/`where` because those are
/// themselves not guaranteed present (Windows `where` yes, but spawning a
/// process per provider to answer a yes/no question is a lot of forks for a
/// menu). Honours `PATHEXT` on Windows so `claude` matches `claude.cmd`.
pub fn which(bin: &str) -> Option<PathBuf> {
    // An explicit path (rare here, but a user could rename an entry's bin) is
    // taken as-is rather than searched for.
    if bin.contains(std::path::MAIN_SEPARATOR) {
        let p = Path::new(bin);
        return is_executable_file(p).then(|| p.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    let exts = windows_pathext();
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        // Bare name first: on Unix this is the whole story; on Windows it lets a
        // real extensionless binary win over a same-named script.
        let bare = dir.join(bin);
        if is_executable_file(&bare) {
            return Some(bare);
        }
        for ext in &exts {
            let candidate = dir.join(format!("{bin}{ext}"));
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// The suffixes to try on Windows, from `PATHEXT`, lower-cased and each keeping
/// its leading dot. Empty on every other platform, where a binary has no
/// mandatory extension.
fn windows_pathext() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

/// A path that names a regular file the OS would run. On Unix that means the
/// execute bit is set for somebody; on Windows the extension already decided it.
fn is_executable_file(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_covers_every_catalog_entry_once() {
        let found = scan();
        assert_eq!(found.len(), catalog::KNOWN.len());
        for (f, k) in found.iter().zip(catalog::KNOWN) {
            assert_eq!(f.known.id, k.id, "scan must preserve catalog order");
        }
    }

    #[test]
    fn a_binary_that_cannot_exist_is_absent() {
        assert!(which("loopsmith-no-such-binary-xyzzy").is_none());
    }

    #[test]
    fn a_directory_on_path_is_not_mistaken_for_a_binary() {
        // `is_executable_file` rejects directories, so a directory named like a
        // binary sitting on PATH never reads as "installed".
        let dir = std::env::temp_dir();
        assert!(!is_executable_file(&dir));
    }
}
