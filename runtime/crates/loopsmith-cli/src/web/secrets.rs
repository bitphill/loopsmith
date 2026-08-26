//! Where an API key goes when someone types one into the browser.
//!
//! Two stores, because the two honest answers pull in opposite directions:
//!
//! - [`Store::Profile`] writes `export NAME="…"` into the shell startup file.
//!   Every tool on the machine then sees it, which is what "set it as an
//!   environment variable" means and what most CLIs actually require. The cost
//!   is a plaintext secret on disk, readable by anything running as this user.
//! - [`Store::Keychain`] hands it to the operating system's own secret store —
//!   Keychain, Credential Manager, libsecret. Nothing lands in a dotfile. The
//!   cost is that other tools do not see it automatically, so loopsmith exports
//!   it into the environment of the runs it starts and nowhere else.
//!
//! Three rules hold across both, and the tests below are what enforce them:
//!
//! 1. A value is never written to a log, a ledger, an error string, or a config.
//!    Only key *names* reach `requires_env`, which is the config model's own
//!    rule and not something invented here.
//! 2. Writing is idempotent. The marked block is rewritten in place, so setting
//!    the same key four times leaves one line, not four.
//! 3. Nothing outside the marked block is touched. A profile is somebody's
//!    accumulated shell setup, and clobbering it to save a key is not a trade
//!    anyone agreed to.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Fence around the lines loopsmith owns. Everything between these two markers
/// is rewritten wholesale; everything outside is preserved byte for byte.
const BEGIN: &str = "# >>> loopsmith secrets >>>";
const END: &str = "# <<< loopsmith secrets <<<";

/// Service name under which keys are filed in the OS store.
const SERVICE: &str = "loopsmith";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    /// Shell startup file. Visible to every tool, plaintext on disk.
    Profile,
    /// OS secure store. Invisible to other tools, encrypted at rest.
    Keychain,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretStatus {
    pub name: String,
    /// Set in this process's environment right now.
    pub in_env: bool,
    /// Present in the shell profile's loopsmith block.
    pub in_profile: bool,
    /// Present in the OS secure store.
    pub in_keychain: bool,
    /// Which secure store this platform actually has, or `None`.
    pub keychain_kind: Option<String>,
}

/// The shell startup file to write to.
///
/// zsh does not read `~/.profile`, which is exactly the trap that has cost
/// this project time before: a `PATH` export landed in `.profile` on a machine
/// whose login shell was zsh, and nothing ever read it. So the file is chosen
/// by `$SHELL`, and `.profile` is only the fallback for shells that do read it.
pub fn profile_path() -> Option<PathBuf> {
    let home = crate::web::detect::home_dir()?;
    let shell = std::env::var("SHELL").unwrap_or_default();
    let name = if shell.ends_with("zsh") {
        ".zshrc"
    } else if shell.ends_with("bash") {
        // On macOS an interactive bash reads .bash_profile, not .bashrc.
        if cfg!(target_os = "macos") && home.join(".bash_profile").exists() {
            ".bash_profile"
        } else {
            ".bashrc"
        }
    } else if shell.ends_with("fish") {
        // fish uses its own syntax; handled in `render_block`.
        return Some(home.join(".config/fish/conf.d/loopsmith.fish"));
    } else {
        ".profile"
    };
    Some(home.join(name))
}

fn is_fish(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("fish")
}

/// Read the key/value pairs loopsmith owns in the profile.
pub fn read_profile() -> Vec<(String, String)> {
    let Some(path) = profile_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Some(block) = extract_block(&text) else {
        return Vec::new();
    };
    let fish = is_fish(&path);
    block
        .lines()
        .filter_map(|line| parse_export(line.trim(), fish))
        .collect()
}

fn parse_export(line: &str, fish: bool) -> Option<(String, String)> {
    let rest = if fish {
        line.strip_prefix("set -gx ")?
    } else {
        line.strip_prefix("export ")?
    };
    let (name, value) = if fish {
        let mut parts = rest.splitn(2, ' ');
        (parts.next()?, parts.next()?)
    } else {
        rest.split_once('=')?
    };
    Some((name.trim().to_string(), unquote(value.trim())))
}

fn extract_block(text: &str) -> Option<&str> {
    let start = text.find(BEGIN)? + BEGIN.len();
    let end = text[start..].find(END)? + start;
    Some(&text[start..end])
}

/// Write (or clear) a key in the chosen store.
///
/// `value` of `None` removes the key. Removing is as important as setting: a
/// rotated key left behind in a profile is a key that will be used by accident.
pub fn set(name: &str, value: Option<&str>, store: Store) -> Result<(), String> {
    validate_name(name)?;
    match store {
        Store::Profile => set_profile(name, value),
        Store::Keychain => set_keychain(name, value),
    }?;

    // Make it usable by anything this process starts from here on, so a run
    // launched thirty seconds from now sees the key without the user having to
    // restart loopsmith. Racy in principle against other threads reading the
    // environment; in practice the alternative is a UI that says "saved" and
    // then fails the very next run for want of a shell restart.
    match value {
        Some(v) => std::env::set_var(name, v),
        None => std::env::remove_var(name),
    }
    Ok(())
}

/// Variables that change how programs are found or loaded.
///
/// Writing one of these into a shell profile is not storing a secret, it is
/// arranging for code to run at the next login. `PATH` alone is enough: point
/// it at a directory the attacker controls and every command the user types
/// afterwards is theirs.
///
/// Nothing here is a credential, so refusing them costs a legitimate user
/// nothing. This is defence in depth behind [`crate::web::guard`] — that stops
/// a hostile page reaching this code at all, and this stops the damage if some
/// other path ever does.
const NEVER_WRITABLE: &[&str] = &[
    "PATH",
    "HOME",
    "SHELL",
    "IFS",
    "PS1",
    "PROMPT_COMMAND",
    "BASH_ENV",
    "ENV",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "PYTHONPATH",
    "PYTHONSTARTUP",
    "NODE_OPTIONS",
    "PERL5OPT",
    "RUBYOPT",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
    "PAGER",
    "EDITOR",
    "VISUAL",
];

/// Names are `A-Z`, `0-9`, `_`. Not a style preference: a name containing a
/// quote, a newline, or a `$` would break out of the generated `export` line
/// and turn a saved secret into an arbitrary shell command at next login.
fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("an environment variable needs a name".into());
    }
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err(format!("`{name}` starts with a digit, which no shell accepts"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(format!(
            "`{name}` is not a usable variable name. Use capitals, digits, and \
             underscores only — that is what every shell agrees on."
        ));
    }
    if NEVER_WRITABLE.contains(&name) {
        return Err(format!(
            "`{name}` is not a credential — it changes how your machine finds and \
             loads programs, so writing it to a shell profile would run code at \
             your next login rather than store a key. loopsmith will not set it. \
             If you genuinely need to change it, edit your shell profile yourself."
        ));
    }
    Ok(())
}

fn set_profile(name: &str, value: Option<&str>) -> Result<(), String> {
    let path = profile_path().ok_or("could not find a home directory to write into")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let fish = is_fish(&path);

    let mut pairs: Vec<(String, String)> = extract_block(&existing)
        .map(|b| b.lines().filter_map(|l| parse_export(l.trim(), fish)).collect())
        .unwrap_or_default();

    pairs.retain(|(k, _)| k != name);
    if let Some(v) = value {
        pairs.push((name.to_string(), v.to_string()));
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let block = render_block(&pairs, fish);
    let updated = splice_block(&existing, &block);

    // Written to a sibling and renamed: a half-written shell profile is a
    // machine that will not open a terminal, and `write` truncates before it
    // writes. Rename is atomic on every platform that matters here.
    let tmp = path.with_extension("loopsmith-tmp");
    std::fs::write(&tmp, updated).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("could not replace {}: {e}", path.display()))?;
    restrict(&path);
    Ok(())
}

fn render_block(pairs: &[(String, String)], fish: bool) -> String {
    let mut s = String::from(BEGIN);
    s.push_str("\n# Written by `loopsmith web`. Edit the block or delete it; loopsmith\n");
    s.push_str("# rewrites only what is between these two markers.\n");
    for (k, v) in pairs {
        if fish {
            s.push_str(&format!("set -gx {k} {}\n", shell_quote(v)));
        } else {
            s.push_str(&format!("export {k}={}\n", shell_quote(v)));
        }
    }
    s.push_str(END);
    s.push('\n');
    s
}

/// Single quotes, with the one escape single quoting needs. Everything else —
/// `$`, backticks, `"`, newlines, spaces — is literal inside single quotes,
/// which is precisely why they are used instead of double quotes.
fn shell_quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', r"'\''"))
}

fn unquote(v: &str) -> String {
    let t = v.trim();
    if t.len() >= 2 && ((t.starts_with('\'') && t.ends_with('\'')) || (t.starts_with('"') && t.ends_with('"'))) {
        return t[1..t.len() - 1].replace(r"'\''", "'");
    }
    t.to_string()
}

fn splice_block(existing: &str, block: &str) -> String {
    match (existing.find(BEGIN), existing.find(END)) {
        (Some(a), Some(b)) if b > a => {
            let tail = &existing[b + END.len()..];
            format!("{}{}{}", &existing[..a], block, tail.trim_start_matches('\n'))
        }
        _ => {
            let mut out = existing.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(block);
            out
        }
    }
}

/// Owner-only. A world-readable shell profile with a key in it is a key that
/// leaked to every other account on the machine.
#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}

/// Which secure store this platform has, if any.
pub fn keychain_kind() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        loopsmith_util::which("security").map(|_| "macOS Keychain")
    } else if cfg!(target_os = "windows") {
        loopsmith_util::which("powershell").map(|_| "Windows Credential Manager")
    } else {
        loopsmith_util::which("secret-tool").map(|_| "libsecret")
    }
}

fn set_keychain(name: &str, value: Option<&str>) -> Result<(), String> {
    use std::process::Command;
    let kind = keychain_kind().ok_or(
        "this machine has no secret store loopsmith can reach. On Linux install \
         `libsecret-tools`; otherwise save to the shell profile instead.",
    )?;

    // Values go through argv here, which means they are briefly visible to
    // `ps` on a multi-user machine. Every one of these tools takes the value
    // that way and offers no stdin path, so the exposure is inherent rather
    // than a shortcut — worth stating plainly rather than papering over.
    let ok = match kind {
        "macOS Keychain" => match value {
            Some(v) => Command::new("security")
                .args(["add-generic-password", "-U", "-s", SERVICE, "-a", name, "-w", v])
                .status(),
            None => Command::new("security")
                .args(["delete-generic-password", "-s", SERVICE, "-a", name])
                .status(),
        },
        "libsecret" => match value {
            Some(v) => {
                use std::io::Write;
                let mut child = Command::new("secret-tool")
                    .args(["store", "--label", SERVICE, "service", SERVICE, "account", name])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("secret-tool would not start: {e}"))?;
                if let Some(mut si) = child.stdin.take() {
                    let _ = si.write_all(v.as_bytes());
                }
                child.wait()
            }
            None => Command::new("secret-tool")
                .args(["clear", "service", SERVICE, "account", name])
                .status(),
        },
        _ => match value {
            Some(v) => Command::new("cmdkey")
                .args([
                    &format!("/generic:{SERVICE}:{name}"),
                    &format!("/user:{name}"),
                    &format!("/pass:{v}"),
                ])
                .status(),
            None => Command::new("cmdkey")
                .args([&format!("/delete:{SERVICE}:{name}")])
                .status(),
        },
    };

    match ok {
        // A delete that finds nothing is not a failure worth reporting: the
        // caller asked for the key to be gone, and it is gone.
        Ok(s) if s.success() || value.is_none() => Ok(()),
        Ok(s) => Err(format!("{kind} refused the write (exit {})", s.code().unwrap_or(-1))),
        Err(e) => Err(format!("could not reach {kind}: {e}")),
    }
}

/// Read a value back, for the UI's reveal control.
///
/// Reveal is a deliberate, per-key action taken by the person who typed the
/// key in the first place. It is not part of any listing, and no code path
/// other than this one returns a secret value.
pub fn reveal(name: &str, store: Store) -> Result<String, String> {
    validate_name(name)?;
    match store {
        Store::Profile => read_profile()
            .into_iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("{name} is not in the shell profile")),
        Store::Keychain => read_keychain(name),
    }
}

fn read_keychain(name: &str) -> Result<String, String> {
    use std::process::Command;
    let kind = keychain_kind().ok_or("no secret store on this machine")?;
    let out = match kind {
        "macOS Keychain" => Command::new("security")
            .args(["find-generic-password", "-s", SERVICE, "-a", name, "-w"])
            .output(),
        "libsecret" => Command::new("secret-tool")
            .args(["lookup", "service", SERVICE, "account", name])
            .output(),
        _ => return Err("Windows Credential Manager does not hand passwords back to a CLI".into()),
    }
    .map_err(|e| format!("could not reach {kind}: {e}"))?;

    if !out.status.success() {
        return Err(format!("{name} is not in {kind}"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string())
}

pub fn status(name: &str) -> SecretStatus {
    let kind = keychain_kind();
    SecretStatus {
        name: name.to_string(),
        in_env: std::env::var_os(name).is_some(),
        in_profile: read_profile().iter().any(|(k, _)| k == name),
        in_keychain: kind.is_some() && read_keychain(name).is_ok(),
        keychain_kind: kind.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variables_that_would_run_code_at_next_login_are_refused() {
        // Each of these is a perfectly valid variable name and none of them is
        // a credential. Setting PATH from a form is how a "save my API key"
        // feature becomes a persistence mechanism.
        for bad in [
            "PATH", "LD_PRELOAD", "DYLD_INSERT_LIBRARIES", "NODE_OPTIONS",
            "GIT_SSH_COMMAND", "BASH_ENV", "PYTHONSTARTUP", "IFS", "EDITOR",
        ] {
            let err = validate_name(bad).expect_err("`{bad}` must be refused");
            assert!(err.contains("not a credential"), "for {bad}: {err}");
        }
        // The ones people actually came here to set still work.
        for good in ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "XAI_API_KEY", "MY_OWN_TOKEN"] {
            assert!(validate_name(good).is_ok(), "`{good}` must be allowed");
        }
    }

    #[test]
    fn a_name_that_could_escape_the_export_line_is_refused() {
        // Each of these would end the quoted value and start a command.
        for bad in ["A=B", "A B", "A;rm -rf /", "A\nB", "A$X", "A'B", "1ABC", ""] {
            assert!(validate_name(bad).is_err(), "`{bad}` must be refused");
        }
        assert!(validate_name("ANTHROPIC_API_KEY").is_ok());
        assert!(validate_name("KEY_2").is_ok());
    }

    #[test]
    fn quoting_survives_every_character_a_key_might_contain() {
        for raw in ["plain", "with space", "$(whoami)", "`id`", "a\"b", "a'b", "a\nb", "sk-…$#!"] {
            let q = shell_quote(raw);
            assert_eq!(unquote(&q), raw, "round trip failed for {raw:?}");
            assert!(q.starts_with('\'') && q.ends_with('\''));
        }
    }

    #[test]
    fn splicing_replaces_the_block_and_leaves_the_rest_alone() {
        let before = "export PATH=/usr/bin\n\n# >>> loopsmith secrets >>>\nexport OLD='1'\n# <<< loopsmith secrets <<<\n\nalias ll='ls -l'\n";
        let block = render_block(&[("NEW".into(), "2".into())], false);
        let after = splice_block(before, &block);

        assert!(after.contains("export PATH=/usr/bin"), "kept what came before");
        assert!(after.contains("alias ll='ls -l'"), "kept what came after");
        assert!(after.contains("export NEW='2'"), "wrote the new key");
        assert!(!after.contains("OLD"), "dropped the old key");
        assert_eq!(after.matches(BEGIN).count(), 1, "exactly one block");
    }

    #[test]
    fn splicing_into_a_file_with_no_block_appends_one() {
        let block = render_block(&[("K".into(), "v".into())], false);
        let after = splice_block("export PATH=/usr/bin\n", &block);
        assert!(after.starts_with("export PATH=/usr/bin\n"));
        assert_eq!(after.matches(BEGIN).count(), 1);
    }

    #[test]
    fn splicing_into_an_empty_file_does_not_start_with_blank_lines() {
        let block = render_block(&[("K".into(), "v".into())], false);
        let after = splice_block("", &block);
        assert!(after.starts_with(BEGIN), "got: {after:?}");
    }

    #[test]
    fn writing_the_same_key_twice_leaves_one_line() {
        let mut pairs = vec![("K".to_string(), "1".to_string())];
        pairs.retain(|(k, _)| k != "K");
        pairs.push(("K".into(), "2".into()));
        let block = render_block(&pairs, false);
        assert_eq!(block.matches("export K=").count(), 1, "{block}");
        assert!(block.contains("export K='2'"));
    }

    #[test]
    fn fish_gets_fish_syntax_not_an_export() {
        let block = render_block(&[("K".into(), "v".into())], true);
        assert!(block.contains("set -gx K 'v'"), "{block}");
        assert!(!block.contains("export"), "fish has no export: {block}");
        assert_eq!(parse_export("set -gx K 'v'", true), Some(("K".into(), "v".into())));
    }

    #[test]
    fn parsing_ignores_lines_that_are_not_exports() {
        assert_eq!(parse_export("# a comment", false), None);
        assert_eq!(parse_export("", false), None);
        assert_eq!(parse_export("exporting is fun", false), None);
    }
}
