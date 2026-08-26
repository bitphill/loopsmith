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

use crate::web::catalog::{self, Known, ENV_KEYS};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a `--version` probe may take before it is written off.
///
/// Six seconds, and the number was measured rather than picked. Probes run
/// concurrently, so the whole scan costs one timeout, not ten — warm, ten
/// agent CLIs all answer inside half a second. The budget exists for the cold
/// case: on the very first scan after a reboot, a Node-based CLI whose module
/// cache is cold can take several seconds to print its own version, while a
/// native binary like `ollama` answers immediately.
///
/// Two seconds was the first guess and it was wrong in the worst possible
/// place: the first scan a new user ever sees reported the Node CLIs as
/// present but version-less, which reads like a broken install of the very
/// tool they were about to configure.
const PROBE_TIMEOUT: Duration = Duration::from_secs(6);

/// A full handshake gets longer: it is a real model round trip.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60);

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

/// Run everything. Probes fan out concurrently: ten sequential two-second
/// timeouts is twenty seconds of a blank page, and the probes do not depend on
/// each other.
pub async fn scan(deep: bool) -> Detection {
    let mut notes = Vec::new();

    let agent_futures = catalog::KNOWN.iter().map(|k| probe_agent(k, deep));
    let agents: Vec<Agent> = futures_join_all(agent_futures)
        .await
        .into_iter()
        .flatten()
        .collect();

    let ollama_models = if agents.iter().any(|a| a.id == "ollama") {
        match ollama_list().await {
            Ok(m) => m,
            Err(e) => {
                notes.push(format!(
                    "ollama is installed but `ollama list` failed ({e}). \
                     The daemon may not be running: try `ollama serve`."
                ));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let (mcp_servers, mcp_notes) = mcp_servers();
    notes.extend(mcp_notes);

    let env_keys = env_keys();
    let skills = skills();
    let git = git_facts().await;

    let p = loopsmith_util::platform::Platform::detect();
    let platform = PlatformFacts {
        os: p.os.as_str().to_string(),
        userland: p.userland.as_str().to_string(),
        bash: p.bash.as_ref().map(|b| format!("{}.{}", b.major, b.minor)),
        scheduler: p.scheduler().map(str::to_string),
        home: home_dir().map(|h| h.display().to_string()),
    };

    if agents.is_empty() {
        notes.push(
            "No agent CLI was found on PATH. A loop needs at least one provider to \
             call. `ollama` is the shortest route to a working loop that costs \
             nothing; `claude`, `gemini`, and `codex` are the hosted ones."
                .into(),
        );
    }
    if platform.scheduler.is_none() {
        notes.push(
            "No scheduler (launchd or cron) is installed, so a schedule cannot be \
             handed to the operating system. `Watch` still works while this \
             machine stays awake."
                .into(),
        );
    }

    Detection {
        agents,
        ollama_models,
        mcp_servers,
        env_keys,
        skills,
        git,
        platform,
        notes,
        scanned_at_ms: now_ms(),
    }
}

/// `futures::future::join_all` without the `futures` dependency.
///
/// Ten probes is a small enough set that collecting handles and awaiting them
/// in order is exactly as parallel as the real thing, and it keeps the crate
/// tree one dependency shorter.
async fn futures_join_all<F, T>(futs: impl Iterator<Item = F>) -> Vec<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let handles: Vec<_> = futs.map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(v) = h.await {
            out.push(v);
        }
    }
    out
}

async fn probe_agent(k: &'static Known, deep: bool) -> Option<Agent> {
    let path = loopsmith_util::which(k.bin)?;

    let version = capture(k.bin, &[k.version_arg], PROBE_TIMEOUT)
        .await
        .map(|out| first_line(&out))
        .filter(|s| !s.is_empty());

    // A deep probe is the caller's explicit choice, and it is the only path
    // here that can cost money. It never runs unless `deep` was asked for.
    let version = if deep && version.is_none() {
        capture(k.bin, &["--help"], PROBE_TIMEOUT)
            .await
            .map(|_| "responds to --help".to_string())
    } else {
        version
    };

    let missing_env: Vec<String> = k
        .requires_env
        .iter()
        .filter(|e| std::env::var_os(e).is_none())
        .map(|e| e.to_string())
        .collect();

    Some(Agent {
        id: k.id.into(),
        label: k.label.into(),
        kind: k.kind.into(),
        path: path.display().to_string(),
        version,
        command: k.bin.into(),
        args: k.args.iter().map(|s| s.to_string()).collect(),
        prompt_on_stdin: k.prompt_on_stdin,
        requires_env: k.requires_env.iter().map(|s| s.to_string()).collect(),
        env_ready: missing_env.is_empty(),
        missing_env,
        tiers: k.tiers.iter().map(|s| s.to_string()).collect(),
        models: k.models.iter().map(|s| s.to_string()).collect(),
        cost_per_1k: k.cost_per_1k,
        note: k.note.into(),
        confidence: k.confidence,
    })
}

/// The real handshake, behind the UI's per-provider "Test" button.
///
/// This spends tokens. It exists so a user can prove a provider works before
/// committing to an overnight run, which is a far better place to discover a
/// wrong flag than iteration four.
pub async fn handshake(command: &str, args: &[String], prompt_on_stdin: bool) -> HandshakeResult {
    const PROMPT: &str = "Reply with the single word: ready";

    let substituted: Vec<String> = args
        .iter()
        .map(|a| {
            a.replace("{prompt}", PROMPT)
                .replace("{system}", "Answer in one word.")
                .replace("{tier}", "cheap")
                .replace("{node}", "handshake")
        })
        .collect();

    let started = std::time::Instant::now();
    let out = capture_with_stdin(
        command,
        &substituted.iter().map(String::as_str).collect::<Vec<_>>(),
        if prompt_on_stdin { Some(PROMPT) } else { None },
        HANDSHAKE_TIMEOUT,
    )
    .await;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    match out {
        Ok(text) => {
            let trimmed = text.trim();
            HandshakeResult {
                ok: !trimmed.is_empty(),
                elapsed_ms,
                // Bounded on purpose: a CLI that decides to print its banner,
                // a changelog, and an ASCII logo should not flood the page.
                output: truncate(trimmed, 800),
                error: if trimmed.is_empty() {
                    Some(
                        "the command ran but produced no output. \
                         The prompt flag is probably wrong for this CLI."
                            .into(),
                    )
                } else {
                    None
                },
            }
        }
        Err(e) => HandshakeResult {
            ok: false,
            elapsed_ms,
            output: String::new(),
            error: Some(e),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HandshakeResult {
    pub ok: bool,
    pub elapsed_ms: u64,
    pub output: String,
    pub error: Option<String>,
}

async fn ollama_list() -> Result<Vec<OllamaModel>, String> {
    let out = capture("ollama", &["list"], Duration::from_secs(5))
        .await
        .ok_or_else(|| "no response".to_string())?;
    Ok(out
        .lines()
        .skip(1) // header row
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let name = cols.next()?.to_string();
            // NAME  ID  SIZE_NUMBER SIZE_UNIT  MODIFIED…
            let size = cols
                .nth(1)
                .map(|n| {
                    let unit = line
                        .split_whitespace()
                        .nth(3)
                        .filter(|u| u.len() <= 2)
                        .unwrap_or("");
                    format!("{n} {unit}").trim().to_string()
                })
                .unwrap_or_default();
            Some(OllamaModel { name, size })
        })
        .collect())
}

/// Every place an MCP server definition is likely to be, parsed as JSON.
///
/// Codex keeps its servers in TOML, which would mean a TOML parser for one
/// file. That trade is not worth it, so the file is named in a note instead of
/// being read: telling someone where to look beats a dependency.
fn mcp_servers() -> (Vec<McpServer>, Vec<String>) {
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

fn env_keys() -> Vec<EnvKey> {
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

/// Skills already visible to this machine, for section J's picker.
fn skills() -> Vec<SkillEntry> {
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

async fn git_facts() -> GitFacts {
    match loopsmith_util::which("git") {
        Some(p) => GitFacts {
            installed: true,
            path: Some(p.display().to_string()),
            version: capture("git", &["--version"], PROBE_TIMEOUT)
                .await
                .map(|o| first_line(&o)),
        },
        None => GitFacts {
            installed: false,
            path: None,
            version: None,
        },
    }
}

/// Run a command and return its stdout, or `None` if it failed or timed out.
///
/// Failure and timeout collapse to the same answer on purpose: for a probe,
/// "did not tell me its version" is one outcome, and distinguishing the ways
/// it can happen would produce a report nobody acts on.
pub async fn capture(cmd: &str, args: &[&str], timeout: Duration) -> Option<String> {
    capture_with_stdin(cmd, args, None, timeout).await.ok()
}

pub async fn capture_with_stdin(
    cmd: &str,
    args: &[&str],
    stdin_text: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let mut c = Command::new(cmd);
    c.args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(if stdin_text.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        });

    let mut child = c.spawn().map_err(|e| format!("could not start `{cmd}`: {e}"))?;

    if let Some(text) = stdin_text {
        if let Some(mut si) = child.stdin.take() {
            let _ = si.write_all(text.as_bytes()).await;
            // Dropping the handle closes the pipe. A CLI reading to EOF waits
            // forever without this, which is exactly the hang a hands-off loop
            // must never have.
            drop(si);
        }
    }

    let out = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(format!("`{cmd}` failed: {e}")),
        Err(_) => {
            return Err(format!(
                "`{cmd}` did not answer within {}s",
                timeout.as_secs()
            ))
        }
    };

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() || !stdout.trim().is_empty() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(format!(
        "`{cmd}` exited {}: {}",
        out.status.code().unwrap_or(-1),
        truncate(stderr.trim(), 300)
    ))
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

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or_default().trim().to_string()
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
