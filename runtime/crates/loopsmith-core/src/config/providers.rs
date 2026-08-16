//! Provider routing. Every provider is a command template.

use super::graph::Tier;
use super::yes;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every provider is a command template. `{prompt}`, `{system}`, `{model}`
/// and `{tier}` are substituted before spawn. This keeps BYOK support out of
/// the Rust build entirely: if you can invoke it from a shell, loopsmith can
/// route to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSpec {
    pub id: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub tiers: Vec<Tier>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Environment variables that must be present. Values are never read by
    /// loopsmith, only checked for presence, so keys stay out of the ledger.
    #[serde(default)]
    pub requires_env: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    /// Send the prompt on stdin instead of substituting into args.
    #[serde(default)]
    pub prompt_on_stdin: bool,
    /// Regex with one capture group pulling a token count out of the
    /// provider's own output. Without it, usage is estimated from character
    /// count, which is enough to make a budget ceiling real but is not exact.
    #[serde(default)]
    pub usage_regex: Option<String>,
    /// Price per thousand tokens, for the cost ceiling.
    #[serde(default)]
    pub cost_per_1k_tokens: Option<f64>,
}

/// Provider families. Each variant accepts the spellings people actually
/// write, because a config that rejects `openai` in favour of `open_ai` is a
/// config that wastes the author's afternoon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[serde(
        rename = "claude_code",
        alias = "claude-code",
        alias = "claude",
        alias = "claudecode"
    )]
    ClaudeCode,
    #[serde(alias = "Ollama")]
    Ollama,
    #[serde(rename = "grok_cli", alias = "grok-cli", alias = "grok")]
    GrokCli,
    #[serde(rename = "grok_build", alias = "grok-build")]
    GrokBuild,
    #[serde(alias = "Hermes")]
    Hermes,
    #[serde(
        rename = "openai",
        alias = "open_ai",
        alias = "open-ai",
        alias = "OpenAI"
    )]
    OpenAi,
    #[serde(alias = "google_gemini", alias = "google-gemini", alias = "Gemini")]
    Gemini,
    /// Any OpenAI-compatible or bespoke endpoint driven by a command.
    #[serde(alias = "BYOK", alias = "custom")]
    Byok,
    /// An MCP server spoken to over stdio.
    #[serde(alias = "MCP")]
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProviderRouting {
    #[serde(default)]
    pub providers: Vec<ProviderSpec>,
    /// Ordered fallback chain per tier. First reachable provider wins.
    #[serde(default)]
    pub cascade: BTreeMap<String, Vec<String>>,
    /// A judge must not run on the provider that produced the work.
    #[serde(default = "yes")]
    pub enforce_judge_independence: bool,
}
