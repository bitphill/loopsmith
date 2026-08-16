//! Provider plane.
//!
//! Every provider is a **command template**. That single decision is what
//! makes BYOK support free: Claude Code, Ollama, a Grok CLI, an OpenAI-
//! compatible endpoint driven by `curl`, an MCP server over stdio — all of
//! them are "a program you can run with a prompt". Adding a provider is a
//! config edit, never a Rust change and never a rebuild.
//!
//! Two behaviours matter for correctness rather than convenience:
//!
//! - **Cascade with availability checks.** A tier resolves to an ordered list
//!   of providers; the first one whose binary exists and whose required
//!   environment is present serves the call. Cheap tiers carry the mechanical
//!   work, strong tiers carry judgment.
//! - **Secrets stay out of the record.** `requires_env` names keys that must
//!   exist. Values are never read, never substituted into a logged command
//!   line, and never written to the ledger.

use loopsmith_core::{LoopConfig, ProviderKind, ProviderSpec, Tier};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("no provider available for tier {tier:?}; tried: {tried}")]
    NoneAvailable { tier: Tier, tried: String },
    #[error("provider `{id}` failed to start: {source}")]
    Spawn {
        id: String,
        #[source]
        source: std::io::Error,
    },
    #[error("provider `{id}` timed out after {seconds}s")]
    Timeout { id: String, seconds: u64 },
    #[error("provider `{id}` exited {code}: {stderr}")]
    Failed {
        id: String,
        code: i32,
        stderr: String,
    },
}

#[derive(Debug, Clone)]
pub struct InvokeRequest {
    /// Node the call is for; used only for logging and digests.
    pub node_id: String,
    pub system: String,
    pub prompt: String,
    pub tier: Tier,
    pub workdir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResponse {
    pub provider_id: String,
    pub output: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    #[serde(default)]
    pub stderr_tail: Option<String>,
}

/// Cheap, dependency-free digest for prompt provenance. Not cryptographic —
/// its only job is to let the ledger say "this is the same prompt as before"
/// without storing the prompt twice.
pub fn digest(s: &str) -> String {
    // FNV-1a, 64-bit.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Is this provider usable right now?
pub fn availability(spec: &ProviderSpec) -> Availability {
    let missing_env: Vec<String> = spec
        .requires_env
        .iter()
        .filter(|k| std::env::var_os(k).is_none())
        .cloned()
        .collect();
    let on_path = which(&spec.command).is_some();
    Availability {
        on_path,
        missing_env,
    }
}

#[derive(Debug, Clone)]
pub struct Availability {
    pub on_path: bool,
    /// Names only. Values are never read.
    pub missing_env: Vec<String>,
}

impl Availability {
    pub fn ok(&self) -> bool {
        self.on_path && self.missing_env.is_empty()
    }
    pub fn why_not(&self) -> String {
        let mut parts = Vec::new();
        if !self.on_path {
            parts.push("command not found on PATH".to_string());
        }
        if !self.missing_env.is_empty() {
            parts.push(format!("missing env: {}", self.missing_env.join(", ")));
        }
        parts.join("; ")
    }
}

/// Minimal `which`: absolute paths are checked directly, bare names are
/// resolved against PATH.
pub fn which(cmd: &str) -> Option<PathBuf> {
    let p = Path::new(cmd);
    if p.is_absolute() || cmd.contains('/') {
        return is_executable(p).then(|| p.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(cmd);
        is_executable(&candidate).then_some(candidate)
    })
}

fn is_executable(p: &Path) -> bool {
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

/// Substitute the supported placeholders into a template.
pub fn render(template: &str, vars: &BTreeMap<&str, &str>) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

fn tier_name(t: Tier) -> &'static str {
    match t {
        Tier::Cheap => "cheap",
        Tier::Standard => "standard",
        Tier::Strong => "strong",
    }
}

/// Invoke one specific provider.
pub fn invoke(spec: &ProviderSpec, req: &InvokeRequest) -> Result<InvokeResponse, ProviderError> {
    let model = spec.model.clone().unwrap_or_default();
    let tier = tier_name(req.tier);
    let vars: BTreeMap<&str, &str> = [
        ("prompt", req.prompt.as_str()),
        ("system", req.system.as_str()),
        ("model", model.as_str()),
        ("tier", tier),
        ("node", req.node_id.as_str()),
    ]
    .into_iter()
    .collect();

    let args: Vec<String> = spec.args.iter().map(|a| render(a, &vars)).collect();

    let mut cmd = Command::new(&spec.command);
    cmd.args(&args)
        .current_dir(&req.workdir)
        .stdin(if spec.prompt_on_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let started = Instant::now();
    let mut child = cmd.spawn().map_err(|source| ProviderError::Spawn {
        id: spec.id.clone(),
        source,
    })?;

    if spec.prompt_on_stdin {
        if let Some(mut sin) = child.stdin.take() {
            // A broken pipe here means the child exited early; the exit code
            // path below reports that more usefully than an io error would.
            let _ = sin.write_all(req.prompt.as_bytes());
        }
    }

    // std::process has no timeout, so poll. The alternative is an async
    // runtime, which is a heavy dependency for one feature.
    let timeout = spec.timeout_seconds.map(Duration::from_secs);
    let poll = Duration::from_millis(50);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if let Some(limit) = timeout {
                    if started.elapsed() >= limit {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(ProviderError::Timeout {
                            id: spec.id.clone(),
                            seconds: limit.as_secs(),
                        });
                    }
                }
                std::thread::sleep(poll);
            }
            Err(source) => {
                return Err(ProviderError::Spawn {
                    id: spec.id.clone(),
                    source,
                })
            }
        }
    }

    let out = child
        .wait_with_output()
        .map_err(|source| ProviderError::Spawn {
            id: spec.id.clone(),
            source,
        })?;
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if code != 0 {
        return Err(ProviderError::Failed {
            id: spec.id.clone(),
            code,
            stderr: stderr.lines().last().unwrap_or("").to_string(),
        });
    }

    Ok(InvokeResponse {
        provider_id: spec.id.clone(),
        output: String::from_utf8_lossy(&out.stdout).to_string(),
        exit_code: code,
        duration_ms: started.elapsed().as_millis() as u64,
        stderr_tail: stderr.lines().last().map(|s| s.to_string()),
    })
}

/// Walk the cascade for a tier and invoke the first provider that is both
/// available and succeeds. Returns the response plus the ids that were skipped
/// so the ledger can record why.
pub fn dispatch(
    cfg: &LoopConfig,
    req: &InvokeRequest,
    pinned: Option<&str>,
) -> Result<(InvokeResponse, Vec<String>), ProviderError> {
    let candidates: Vec<&ProviderSpec> = match pinned {
        Some(id) => cfg.provider(id).into_iter().collect(),
        None => cfg.cascade_for(req.tier),
    };

    let mut skipped = Vec::new();
    for spec in &candidates {
        let av = availability(spec);
        if !av.ok() {
            skipped.push(format!("{} ({})", spec.id, av.why_not()));
            continue;
        }
        match invoke(spec, req) {
            Ok(resp) => return Ok((resp, skipped)),
            Err(e) => skipped.push(format!("{}: {e}", spec.id)),
        }
    }

    Err(ProviderError::NoneAvailable {
        tier: req.tier,
        tried: if skipped.is_empty() {
            "none declared".to_string()
        } else {
            skipped.join("; ")
        },
    })
}

/// Sensible starting providers for a fresh config. Emitted by
/// `loopsmith init` so a new loop has a working cascade on day one; every one
/// of them is just a command, so unavailable ones are skipped rather than
/// fatal.
pub fn starter_providers() -> Vec<ProviderSpec> {
    vec![
        ProviderSpec {
            id: "claude".into(),
            kind: ProviderKind::ClaudeCode,
            tiers: vec![Tier::Standard, Tier::Strong],
            command: "claude".into(),
            args: vec!["-p".into(), "{prompt}".into()],
            model: None,
            requires_env: vec![],
            timeout_seconds: Some(900),
            prompt_on_stdin: false,
        },
        ProviderSpec {
            id: "ollama".into(),
            kind: ProviderKind::Ollama,
            tiers: vec![Tier::Cheap],
            command: "ollama".into(),
            args: vec!["run".into(), "{model}".into()],
            model: Some("llama3".into()),
            requires_env: vec![],
            timeout_seconds: Some(600),
            prompt_on_stdin: true,
        },
        ProviderSpec {
            id: "grok".into(),
            kind: ProviderKind::GrokCli,
            tiers: vec![Tier::Standard],
            command: "grok".into(),
            args: vec!["-p".into(), "{prompt}".into()],
            model: None,
            requires_env: vec!["XAI_API_KEY".into()],
            timeout_seconds: Some(600),
            prompt_on_stdin: false,
        },
        ProviderSpec {
            id: "openai".into(),
            kind: ProviderKind::OpenAi,
            tiers: vec![Tier::Strong],
            command: "curl".into(),
            args: vec![
                "-sS".into(),
                "https://api.openai.com/v1/chat/completions".into(),
                "-H".into(),
                "Content-Type: application/json".into(),
                "-H".into(),
                // curl expands the variable itself, so the key never enters
                // this process's memory or the ledger.
                "Authorization: Bearer $OPENAI_API_KEY".into(),
                "-d".into(),
                "@-".into(),
            ],
            model: Some("gpt-4o-mini".into()),
            requires_env: vec!["OPENAI_API_KEY".into()],
            timeout_seconds: Some(300),
            prompt_on_stdin: true,
        },
        ProviderSpec {
            id: "gemini".into(),
            kind: ProviderKind::Gemini,
            tiers: vec![Tier::Standard],
            command: "gemini".into(),
            args: vec!["-p".into(), "{prompt}".into()],
            model: None,
            requires_env: vec!["GEMINI_API_KEY".into()],
            timeout_seconds: Some(600),
            prompt_on_stdin: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, command: &str, args: &[&str]) -> ProviderSpec {
        ProviderSpec {
            id: id.into(),
            kind: ProviderKind::Byok,
            tiers: vec![Tier::Cheap, Tier::Standard, Tier::Strong],
            command: command.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            model: None,
            requires_env: vec![],
            timeout_seconds: Some(30),
            prompt_on_stdin: false,
        }
    }

    fn req() -> InvokeRequest {
        InvokeRequest {
            node_id: "n1".into(),
            system: "be terse".into(),
            prompt: "hello".into(),
            tier: Tier::Standard,
            workdir: std::env::temp_dir(),
        }
    }

    #[test]
    fn placeholders_are_substituted() {
        let vars: BTreeMap<&str, &str> =
            [("prompt", "hi"), ("model", "m1")].into_iter().collect();
        assert_eq!(render("say {prompt} via {model}", &vars), "say hi via m1");
    }

    #[test]
    fn unknown_placeholders_are_left_alone() {
        let vars: BTreeMap<&str, &str> = [("prompt", "hi")].into_iter().collect();
        assert_eq!(render("{prompt} {unknown}", &vars), "hi {unknown}");
    }

    #[test]
    fn digest_is_stable_and_differentiating() {
        assert_eq!(digest("abc"), digest("abc"));
        assert_ne!(digest("abc"), digest("abd"));
    }

    #[test]
    fn which_finds_a_real_binary_and_misses_a_fake_one() {
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-real-binary-xyz").is_none());
    }

    #[test]
    fn a_provider_with_a_missing_binary_is_unavailable() {
        let s = spec("ghost", "definitely-not-a-real-binary-xyz", &[]);
        let av = availability(&s);
        assert!(!av.ok());
        assert!(av.why_not().contains("not found on PATH"));
    }

    #[test]
    fn a_provider_with_missing_env_is_unavailable_and_names_only_the_key() {
        let mut s = spec("needs-key", "sh", &[]);
        s.requires_env = vec!["LOOPSMITH_TEST_ABSENT_KEY".into()];
        let av = availability(&s);
        assert!(!av.ok());
        let why = av.why_not();
        assert!(why.contains("LOOPSMITH_TEST_ABSENT_KEY"));
        // The value is never read, so nothing but the name can leak.
        assert!(!why.contains('='));
    }

    #[test]
    fn invoking_echo_returns_its_stdout() {
        let s = spec("echoer", "echo", &["{prompt}"]);
        let r = invoke(&s, &req()).expect("echo runs");
        assert_eq!(r.output.trim(), "hello");
        assert_eq!(r.exit_code, 0);
        assert_eq!(r.provider_id, "echoer");
    }

    #[test]
    fn stdin_mode_pipes_the_prompt() {
        let s = {
            let mut s = spec("catter", "cat", &[]);
            s.prompt_on_stdin = true;
            s
        };
        let r = invoke(&s, &req()).expect("cat runs");
        assert_eq!(r.output.trim(), "hello");
    }

    #[test]
    fn a_nonzero_exit_is_an_error_not_a_silent_pass() {
        let s = spec("failer", "false", &[]);
        let e = invoke(&s, &req()).unwrap_err();
        assert!(matches!(e, ProviderError::Failed { .. }));
    }

    #[test]
    fn a_hanging_provider_is_killed_at_the_timeout() {
        let mut s = spec("sleeper", "sleep", &["30"]);
        s.timeout_seconds = Some(1);
        let started = Instant::now();
        let e = invoke(&s, &req()).unwrap_err();
        assert!(matches!(e, ProviderError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(10), "kill was not prompt");
    }

    fn cfg_with(providers: Vec<ProviderSpec>, cascade: &[(&str, Vec<&str>)]) -> LoopConfig {
        let mut cfg = loopsmith_core::parse_str(
            r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
validations:
  - target: g1
    name: v
    mode: objective
    statement: s
    detector: { type: script, command: "true" }
"#,
            "test",
        )
        .unwrap();
        cfg.providers.providers = providers;
        cfg.providers.cascade = cascade
            .iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect();
        cfg
    }

    #[test]
    fn cascade_falls_past_an_unavailable_provider() {
        let cfg = cfg_with(
            vec![
                spec("missing", "definitely-not-a-real-binary-xyz", &[]),
                spec("works", "echo", &["{prompt}"]),
            ],
            &[("standard", vec!["missing", "works"])],
        );
        let (resp, skipped) = dispatch(&cfg, &req(), None).expect("falls through");
        assert_eq!(resp.provider_id, "works");
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].contains("missing"));
    }

    #[test]
    fn cascade_falls_past_a_provider_that_errors() {
        let cfg = cfg_with(
            vec![spec("boom", "false", &[]), spec("works", "echo", &["{prompt}"])],
            &[("standard", vec!["boom", "works"])],
        );
        let (resp, skipped) = dispatch(&cfg, &req(), None).unwrap();
        assert_eq!(resp.provider_id, "works");
        assert!(skipped[0].contains("boom"));
    }

    #[test]
    fn exhausting_the_cascade_reports_every_attempt() {
        let cfg = cfg_with(
            vec![spec("a", "false", &[]), spec("b", "false", &[])],
            &[("standard", vec!["a", "b"])],
        );
        let e = dispatch(&cfg, &req(), None).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains('a') && msg.contains('b'), "{msg}");
    }

    #[test]
    fn pinning_a_provider_bypasses_the_cascade() {
        let cfg = cfg_with(
            vec![spec("cheap", "false", &[]), spec("pinned", "echo", &["pinned-out"])],
            &[("standard", vec!["cheap"])],
        );
        let (resp, _) = dispatch(&cfg, &req(), Some("pinned")).unwrap();
        assert_eq!(resp.output.trim(), "pinned-out");
    }

    #[test]
    fn tiers_select_different_cascades() {
        let cfg = cfg_with(
            vec![
                spec("small", "echo", &["small"]),
                spec("big", "echo", &["big"]),
            ],
            &[("cheap", vec!["small"]), ("strong", vec!["big"])],
        );
        let mut r = req();
        r.tier = Tier::Cheap;
        assert_eq!(dispatch(&cfg, &r, None).unwrap().0.output.trim(), "small");
        r.tier = Tier::Strong;
        assert_eq!(dispatch(&cfg, &r, None).unwrap().0.output.trim(), "big");
    }

    #[test]
    fn starter_providers_cover_every_tier() {
        let ps = starter_providers();
        for tier in [Tier::Cheap, Tier::Standard, Tier::Strong] {
            assert!(
                ps.iter().any(|p| p.tiers.contains(&tier)),
                "no starter provider for {tier:?}"
            );
        }
    }
}
