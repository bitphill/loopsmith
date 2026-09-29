//! What is actually installed on this machine.
//!
//! The empty-form problem is the reason this module exists. A newcomer opening
//! a provider section has no way to know whether `claude` is on their PATH,
//! which Ollama models they pulled six months ago, or which MCP servers some
//! editor configured on their behalf. Asking them to type those from memory is
//! how a config ends up naming a binary that is not there, which surfaces much
//! later as a spawn failure in iteration four of an unattended run.
//!
//! So: probe first, offer what was found, and let everything stay editable.
//!
//! **Probing is free by default.** `which` plus a `--version` that returns in
//! two seconds costs nothing and reaches no network. A real handshake — one
//! that puts a prompt through the CLI and waits for a token — is behind an
//! explicit per-provider button in the UI, because doing it on every page load
//! would quietly bill the user for the privilege of opening a form.

//!
//! Two halves, split by what they cost:
//!
//! - This module is synchronous and instant: which known CLIs are on `PATH`
//!   ([`installed`]), which API keys are set, which MCP servers and skills are
//!   already configured, and whether a directory is writable. The terminal
//!   wizard uses only this half, which is why it works in a build with no
//!   async runtime at all.
//! - [`probe`] (behind the `probe` feature) runs subprocesses: `--version`
//!   with a timeout, `ollama list`, git facts, and the paid handshake. The
//!   browser UI uses both halves.
//!
//! Before 1.0 these were two modules in two front ends, each with its own
//! `PATH` walk, and the terminal one imported from the browser one.

use crate::catalog::{self, Known, ENV_KEYS};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[cfg(feature = "probe")]
mod probe;
#[cfg(feature = "probe")]
pub use probe::{capture, capture_with_stdin, handshake, scan, HandshakeResult};

#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub agents: Vec<Agent>,
    pub ollama_models: Vec<OllamaModel>,
    pub mcp_servers: Vec<McpServer>,
    pub env_keys: Vec<EnvKey>,
    pub skills: Vec<SkillEntry>,
    pub git: GitFacts,
    pub platform: PlatformFacts,
    /// Anything that did not work and the user should know about, in plain
    /// language. An empty list is the normal case.
    pub notes: Vec<String>,
    pub scanned_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Agent {
    pub id: String,
    pub label: String,
    pub kind: String,
    /// Absolute path the shell would resolve. Shown so a user with two copies
    /// installed can see which one wins.
    pub path: String,
    pub version: Option<String>,
    pub command: String,
    pub args: Vec<String>,
    pub prompt_on_stdin: bool,
    pub requires_env: Vec<String>,
    /// Which of `requires_env` are actually set right now.
    pub env_ready: bool,
    pub missing_env: Vec<String>,
    pub tiers: Vec<String>,
    pub models: Vec<String>,
    pub cost_per_1k: Option<f64>,
    pub note: String,
    pub confidence: catalog::Confidence,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServer {
    pub name: String,
    /// Where this definition was found, so a duplicate is explainable.
    pub origin: String,
    pub command: String,
    pub args: Vec<String>,
    /// Env var *names* declared by the server definition. Values are not read.
    pub env_keys: Vec<String>,
    /// An MCP server reached over HTTP rather than stdio.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvKey {
    pub name: String,
    pub purpose: String,
    pub present: bool,
    /// First four characters and last four, nothing between. Enough to tell
    /// two keys apart, useless to anyone who reads the page over a shoulder.
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillEntry {
    pub name: String,
    pub origin: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitFacts {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformFacts {
    pub os: String,
    pub userland: String,
    pub bash: Option<String>,
    pub scheduler: Option<String>,
    pub home: Option<String>,
}

/// Every place an MCP server definition is likely to be, parsed as JSON.
///
/// Codex keeps its servers in TOML, which would mean a TOML parser for one
/// file. That trade is not worth it, so the file is named in a note instead of
/// being read: telling someone where to look beats a dependency.
pub fn mcp_servers() -> (Vec<McpServer>, Vec<String>) {
    let mut found: Vec<McpServer> = Vec::new();
    let mut notes = Vec::new();
    let Some(home) = home_dir() else {
        return (found, vec!["could not determine a home directory".into()]);
    };

    // (path, json pointer to the server map, label)
    let sources: Vec<(PathBuf, &str, &str)> = vec![
        (home.join(".claude.json"), "mcpServers", "~/.claude.json"),
        (
            home.join(".claude/settings.json"),
            "mcpServers",
            "~/.claude/settings.json",
        ),
        (
            home.join("Library/Application Support/Claude/claude_desktop_config.json"),
            "mcpServers",
            "Claude Desktop",
        ),
        (home.join(".cursor/mcp.json"), "mcpServers", "~/.cursor/mcp.json"),
        (
            home.join(".config/Code/User/mcp.json"),
            "servers",
            "VS Code",
        ),
        (PathBuf::from(".mcp.json"), "mcpServers", "./.mcp.json"),
    ];

    for (path, key, label) in sources {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            notes.push(format!("{label} exists but is not valid JSON, so it was skipped"));
            continue;
        };
        let Some(map) = v.get(key).and_then(|m| m.as_object()) else {
            continue;
        };
        for (name, def) in map {
            // A name already found in a higher-priority file wins. Two editors
            // configuring the same server is normal, not an error.
            if found.iter().any(|s| s.name == *name) {
                continue;
            }
            found.push(McpServer {
                name: name.clone(),
                origin: label.to_string(),
                command: def
                    .get("command")
                    .and_then(|c| c.as_str())
                    .unwrap_or_default()
                    .to_string(),
                args: def
                    .get("args")
                    .and_then(|a| a.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                env_keys: def
                    .get("env")
                    .and_then(|e| e.as_object())
                    .map(|e| e.keys().cloned().collect())
                    .unwrap_or_default(),
                url: def
                    .get("url")
                    .and_then(|u| u.as_str())
                    .map(str::to_string),
            });
        }
    }

    if home.join(".codex/config.toml").exists() {
        notes.push(
            "Codex keeps its MCP servers in ~/.codex/config.toml, which is TOML \
             rather than JSON and is not read here. Copy a server across by hand \
             if you want the loop to use it."
                .into(),
        );
    }

    (found, notes)
}

pub fn env_keys() -> Vec<EnvKey> {
    ENV_KEYS
        .iter()
        .map(|(name, purpose)| {
            let val = std::env::var(name).ok().filter(|v| !v.trim().is_empty());
            EnvKey {
                name: (*name).to_string(),
                purpose: (*purpose).to_string(),
                present: val.is_some(),
                fingerprint: val.as_deref().map(fingerprint),
            }
        })
        .collect()
}

/// `sk-ab…9f21`. Enough to distinguish two keys, useless as a key.
fn fingerprint(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 10 {
        return "•".repeat(chars.len().max(4));
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{head}…{tail}")
}

/// Skills already visible to this machine, for the `default_skills` picker.
pub fn skills() -> Vec<SkillEntry> {
    let mut out = Vec::new();
    let mut seen = BTreeMap::new();

    let mut roots: Vec<(PathBuf, &str)> = Vec::new();
    if let Some(home) = home_dir() {
        roots.push((home.join(".claude/skills"), "user"));
        roots.push((home.join(".claude/plugins"), "plugin"));
    }
    roots.push((PathBuf::from(".claude/skills"), "project"));

    for (root, origin) in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.flatten() {
            let dir = e.path();
            if !dir.is_dir() {
                continue;
            }
            let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if seen.contains_key(name) {
                continue;
            }
            let description = std::fs::read_to_string(dir.join("SKILL.md"))
                .ok()
                .and_then(|t| frontmatter_field(&t, "description"))
                .unwrap_or_default();
            seen.insert(name.to_string(), ());
            out.push(SkillEntry {
                name: name.to_string(),
                origin: origin.to_string(),
                description: truncate(&description, 160),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Pull one field out of a YAML frontmatter block without a YAML parser.
/// Frontmatter here is flat, one `key: value` per line, so a parser would be
/// answering a question nobody asked.
fn frontmatter_field(text: &str, field: &str) -> Option<String> {
    let body = text.strip_prefix("---")?;
    let end = body.find("\n---")?;
    body[..end]
        .lines()
        .find_map(|l| l.trim().strip_prefix(&format!("{field}:")))
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
}

/// Does the given directory look reachable and writable to an agent CLI?
///
/// This is the check behind the UI's permission warning. It answers three
/// separate questions people conflate: does the path exist, can this process
/// write there, and is it inside a git repository (which decides whether
/// `isolated: true` nodes can have their own worktree).
#[derive(Debug, Clone, Serialize)]
pub struct PathFacts {
    pub path: String,
    pub exists: bool,
    pub is_dir: bool,
    pub writable: bool,
    pub empty: bool,
    pub in_git_repo: bool,
    pub git_root: Option<String>,
    pub has_claude_settings: bool,
    pub existing_loop: Option<String>,
}

pub fn path_facts(path: &Path) -> PathFacts {
    let exists = path.exists();
    let is_dir = path.is_dir();
    let empty = !is_dir
        || std::fs::read_dir(path)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);

    // Probing writability by writing is the only honest answer: permission
    // bits lie on network mounts, and on Windows they mean something else
    // entirely. The file is removed immediately.
    let writable = if is_dir {
        let probe = path.join(".loopsmith-write-probe");
        match std::fs::write(&probe, b"") {
            Ok(()) => {
                let _ = std::fs::remove_file(&probe);
                true
            }
            Err(_) => false,
        }
    } else {
        // Not there yet. The question is not whether the immediate parent
        // exists — `~/loops/first-loop` normally has no `~/loops` yet, and
        // `loopsmith new` creates the whole chain. So walk up to the first
        // ancestor that does exist and ask whether *that* will take a write.
        // Testing only the immediate parent reported the ordinary first-loop
        // case as unwritable and blocked the Create button.
        let mut ancestor = path.parent();
        loop {
            match ancestor {
                Some(dir) if dir.as_os_str().is_empty() => break false,
                Some(dir) if dir.is_dir() => {
                    let probe = dir.join(".loopsmith-write-probe");
                    break match std::fs::write(&probe, b"") {
                        Ok(()) => {
                            let _ = std::fs::remove_file(&probe);
                            true
                        }
                        Err(_) => false,
                    };
                }
                Some(dir) => ancestor = dir.parent(),
                None => break false,
            }
        }
    };

    let mut git_root = None;
    let mut probe = if is_dir { Some(path.to_path_buf()) } else { path.parent().map(Path::to_path_buf) };
    while let Some(dir) = probe {
        if dir.join(".git").exists() {
            git_root = Some(dir.display().to_string());
            break;
        }
        probe = dir.parent().map(Path::to_path_buf);
    }

    let existing_loop = ["loop.yaml", "loop.yml", "loop.md"]
        .iter()
        .find(|f| path.join(f).exists())
        .map(|f| (*f).to_string());

    PathFacts {
        path: path.display().to_string(),
        exists,
        is_dir,
        writable,
        empty,
        in_git_repo: git_root.is_some(),
        git_root,
        has_claude_settings: path.join(".claude/settings.local.json").exists(),
        existing_loop,
    }
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

/// Re-exported rather than redefined: the browser UI stamps jobs and library
/// entries with it, and one clock is easier to reason about than two.
pub use loopsmith_util::now_ms;

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
/// A plain `PATH` walk — no subprocess, instant — which is all a numbered menu
/// needs. `scan` (behind the `probe` feature) goes further and asks each present binary for its
/// version, which costs a process per CLI and a timeout.
///
/// Order is deliberately not "present first": the catalog is already ordered
/// local-first (Ollama leaks nothing), and reordering by presence would move a
/// provider's number between runs, which is exactly the kind of thing that makes
/// a numbered menu untrustworthy.
pub fn installed() -> Vec<Found> {
    catalog::KNOWN
        .iter()
        .map(|k| {
            let path = loopsmith_util::which(k.bin);
            Found {
                known: k,
                present: path.is_some(),
                path,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_covers_every_catalog_entry_once() {
        let found = installed();
        assert_eq!(found.len(), catalog::KNOWN.len());
        for (f, k) in found.iter().zip(catalog::KNOWN) {
            assert_eq!(f.known.id, k.id, "the scan must preserve catalog order");
        }
    }

    #[test]
    fn a_binary_that_cannot_exist_is_absent() {
        assert!(loopsmith_util::which("loopsmith-no-such-binary-xyzzy").is_none());
    }

    #[test]
    fn a_fingerprint_shows_the_ends_and_nothing_else() {
        let fp = fingerprint("sk-ant-api03-abcdefghijklmnop-9f21");
        assert!(fp.starts_with("sk-a"), "keeps the head: {fp}");
        assert!(fp.ends_with("9f21"), "keeps the tail: {fp}");
        assert!(!fp.contains("abcdefgh"), "must not leak the middle: {fp}");
    }

    #[test]
    fn a_short_secret_is_fingerprinted_as_dots_not_as_itself() {
        // The head-and-tail rule would print an eight character key almost in
        // full, which is worse than saying nothing.
        let fp = fingerprint("abc12345");
        assert!(fp.chars().all(|c| c == '•'), "got {fp}");
    }

    #[test]
    fn frontmatter_reads_a_flat_field() {
        let text = "---\nname: thing\ndescription: \"does a thing\"\n---\n# body\n";
        assert_eq!(
            frontmatter_field(text, "description").as_deref(),
            Some("does a thing")
        );
        assert_eq!(frontmatter_field(text, "missing"), None);
    }

    #[test]
    fn frontmatter_on_a_file_without_any_is_none_not_a_panic() {
        assert_eq!(frontmatter_field("# just a heading\n", "description"), None);
        assert_eq!(frontmatter_field("---\nunterminated: yes\n", "x"), None);
    }

    #[test]
    fn a_loop_directory_whose_parents_do_not_exist_yet_is_still_writable() {
        // The ordinary first-loop case: `~/loops/my-first` where `~/loops` has
        // never existed. `loopsmith new` creates the chain, so reporting this
        // as unwritable would block Create for exactly the newcomer the web UI
        // is for.
        let base = loopsmith_util::testing::temp_dir("web-deep-path");
        let deep = base.join("loops").join("nested").join("my-first");
        let f = path_facts(&deep);
        assert!(!f.exists, "nothing has been created");
        assert!(f.writable, "a creatable path under a writable root is writable");
    }

    #[test]
    fn path_facts_on_a_missing_directory_do_not_claim_it_exists() {
        let f = path_facts(Path::new("/definitely/not/here/loopsmith"));
        assert!(!f.exists);
        assert!(!f.is_dir);
        assert!(f.existing_loop.is_none());
    }

    #[test]
    fn path_facts_find_a_writable_temp_dir() {
        let dir = loopsmith_util::testing::temp_dir("web-path-facts");
        let f = path_facts(&dir);
        assert!(f.exists && f.is_dir, "temp dir should exist");
        assert!(f.writable, "temp dir should be writable");
        assert!(f.empty, "a fresh temp dir is empty");
    }

    #[test]
    fn truncation_is_by_character_not_by_byte() {
        // Slicing a multi-byte string by byte index panics. The é is the test.
        let s = "café ".repeat(50);
        let t = truncate(&s, 10);
        assert_eq!(t.chars().count(), 11, "10 chars plus the ellipsis");
    }
}
