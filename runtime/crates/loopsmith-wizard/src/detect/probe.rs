//! The half of detection that runs subprocesses, and so needs a runtime.
//!
//! Everything here is `async` because a probe waits on a child process with a
//! timeout, and a scan fans ten of them out at once.

use super::*;
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

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or_default().trim().to_string()
}
