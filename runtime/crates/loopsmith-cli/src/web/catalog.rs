//! What loopsmith knows about the agent CLIs it can route to.
//!
//! Every provider in a config is a command template, which is the whole reason
//! BYOK needs no Rust change. The cost of that generality is that a newcomer
//! faces an empty `command:` field and no idea what belongs in it. This table
//! is the answer: for each CLI that might be on the machine, the argv that
//! actually works, the environment it needs, and the models it accepts.
//!
//! Two grades of entry, and the difference is stated rather than hidden:
//!
//! - [`Confidence::Verified`] — the argv is known to work.
//! - [`Confidence::Template`] — the CLI exists and is worth offering, but the
//!   argv is a starting point the user should confirm. The UI says so on the
//!   card instead of letting a wrong flag surface later as a spawn failure.
//!
//! Adding a CLI here is a data change, never a code change.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// The argv below is known-good.
    Verified,
    /// The argv is a plausible starting point. Confirm before a long run.
    Template,
}

/// One known agent CLI, and everything the UI needs to prefill a provider.
#[derive(Debug, Clone, Serialize)]
pub struct Known {
    /// Stable id, and the default provider id written into the config.
    pub id: &'static str,
    /// What a person calls it.
    pub label: &'static str,
    /// The binary to look for on PATH.
    pub bin: &'static str,
    /// `ProviderKind` wire name. Anything without a family of its own is
    /// `byok` — the config model's own escape hatch, not a second-class slot.
    pub kind: &'static str,
    /// Argument template. `{prompt}` `{system}` `{model}` `{tier}` `{node}`
    /// are substituted at spawn time by `loopsmith-provider`.
    pub args: &'static [&'static str],
    /// Send the prompt on stdin instead of substituting it into argv. Right
    /// for anything that reads a document, and the only safe choice for a
    /// prompt long enough to hit the platform's argv ceiling.
    pub prompt_on_stdin: bool,
    /// Environment variable names this CLI needs. Names only — loopsmith never
    /// reads the values, and neither does this table.
    pub requires_env: &'static [&'static str],
    /// Tiers this provider is a sensible default for.
    pub tiers: &'static [&'static str],
    /// Model identifiers offered in the dropdown. The field stays free text,
    /// because a list in a binary goes stale and a text box never does.
    pub models: &'static [&'static str],
    /// This CLI reports its own models at run time, so `models` above is empty
    /// on purpose and the UI fills the list from the machine. Only Ollama does
    /// this today; without the distinction, "no models listed" and "models
    /// discovered elsewhere" look identical to anything checking the table.
    pub discovers_models: bool,
    /// Rough price per 1000 tokens, for the cost ceiling. `None` where the
    /// answer is "nothing, it runs on your machine" or genuinely unknown.
    pub cost_per_1k: Option<f64>,
    /// The flag that makes the CLI report its version without doing any work.
    pub version_arg: &'static str,
    /// One line the UI shows under the card. Written for someone who has not
    /// used this CLI.
    pub note: &'static str,
    pub confidence: Confidence,
}

/// Every CLI worth offering, in the order the UI should show them.
///
/// Local-first ordering is deliberate: `ollama` costs nothing and cannot leak,
/// so it heads the list even though it is rarely the strongest option.
pub const KNOWN: &[Known] = &[
    Known {
        id: "claude",
        label: "Claude Code",
        bin: "claude",
        kind: "claude_code",
        // `--model` matters: without it the `model` field below is set, shown
        // in the UI, and then silently ignored, so picking "opus" got you
        // whatever the CLI defaults to.
        args: &["-p", "{prompt}", "--append-system-prompt", "{system}", "--model", "{model}"],
        prompt_on_stdin: false,
        requires_env: &[],
        tiers: &["standard", "strong"],
        discovers_models: false,
        models: &["opus", "sonnet", "haiku"],
        cost_per_1k: Some(0.015),
        version_arg: "--version",
        note: "Anthropic's agentic CLI. Reads and writes files in the directory it \
               is started in, so the loop path is what it can touch. Authenticated \
               by `claude login`, not by an API key.",
        confidence: Confidence::Verified,
    },
    Known {
        id: "ollama",
        label: "Ollama",
        bin: "ollama",
        kind: "ollama",
        args: &["run", "{model}"],
        prompt_on_stdin: true,
        requires_env: &[],
        tiers: &["cheap"],
        discovers_models: true,
        models: &[],
        cost_per_1k: Some(0.0),
        version_arg: "--version",
        note: "Runs models on this machine. Free, private, and slower than a \
               hosted model. The model list below is what is actually pulled \
               locally — `ollama pull <name>` adds more.",
        confidence: Confidence::Verified,
    },
    Known {
        id: "gemini",
        label: "Gemini CLI",
        bin: "gemini",
        kind: "gemini",
        args: &["-p", "{prompt}", "-m", "{model}"],
        prompt_on_stdin: false,
        requires_env: &["GEMINI_API_KEY"],
        tiers: &["cheap", "standard"],
        discovers_models: false,
        models: &["gemini-2.5-pro", "gemini-2.5-flash", "gemini-2.0-flash"],
        cost_per_1k: Some(0.002),
        version_arg: "--version",
        note: "Google's agentic CLI. A generous free tier makes it a good judge \
               to pair against a Claude builder, since a judge must not run on \
               the same family as the work it grades. Not installed on the \
               machine this table was checked against — confirm the argv with \
               `gemini --help` before a long run.",
        confidence: Confidence::Template,
    },
    Known {
        id: "codex",
        label: "Codex CLI",
        bin: "codex",
        kind: "openai",
        args: &["exec", "{prompt}", "--model", "{model}"],
        prompt_on_stdin: false,
        requires_env: &["OPENAI_API_KEY"],
        tiers: &["standard", "strong"],
        discovers_models: false,
        models: &["gpt-5-codex", "gpt-5", "o4-mini"],
        cost_per_1k: Some(0.01),
        version_arg: "--version",
        note: "OpenAI's agentic CLI. `exec` is the non-interactive mode — the \
               interactive one would hang a hands-off loop forever. Not installed \
               on the machine this table was checked against — confirm the argv \
               with `codex --help` before a long run.",
        confidence: Confidence::Template,
    },
    Known {
        id: "grok",
        label: "Grok CLI",
        bin: "grok",
        kind: "grok_cli",
        args: &["-p", "{prompt}", "-m", "{model}"],
        prompt_on_stdin: false,
        requires_env: &["XAI_API_KEY"],
        tiers: &["standard"],
        discovers_models: false,
        models: &["grok-4", "grok-3", "grok-code-fast-1"],
        cost_per_1k: Some(0.005),
        version_arg: "--version",
        note: "`-p` is the short form of `--single`: one prompt, printed to stdout, \
               then exit. A run that seems to hang is usually Grok waiting to \
               approve a tool call — `--always-approve` removes that pause, which \
               is worth understanding before adding it.",
        confidence: Confidence::Verified,
    },
    Known {
        id: "opencode",
        label: "OpenCode",
        bin: "opencode",
        kind: "byok",
        args: &["run", "{prompt}"],
        prompt_on_stdin: false,
        requires_env: &[],
        tiers: &["standard"],
        discovers_models: false,
        models: &[],
        cost_per_1k: None,
        version_arg: "--version",
        note: "Open-source agentic CLI that fronts many providers. Whichever \
               model it is configured for is the one this loop will spend. Not \
               installed on the machine this table was checked against — confirm \
               the argv with `opencode --help` before a long run.",
        confidence: Confidence::Template,
    },
    Known {
        id: "cursor-agent",
        label: "Cursor Agent",
        bin: "cursor-agent",
        kind: "byok",
        args: &["-p", "{prompt}"],
        prompt_on_stdin: false,
        requires_env: &[],
        tiers: &["standard"],
        discovers_models: false,
        models: &[],
        cost_per_1k: None,
        version_arg: "--version",
        note: "Cursor's headless agent. Uses your Cursor subscription rather \
               than a key in the environment. Not installed on the machine this \
               table was checked against — confirm the argv with \
               `cursor-agent --help` before a long run.",
        confidence: Confidence::Template,
    },
    Known {
        id: "aider",
        label: "Aider",
        bin: "aider",
        kind: "byok",
        args: &["--message", "{prompt}", "--yes", "--no-auto-commits"],
        prompt_on_stdin: false,
        requires_env: &[],
        tiers: &["standard"],
        discovers_models: false,
        models: &[],
        cost_per_1k: None,
        version_arg: "--version",
        note: "Pair-programming CLI. `--yes` and `--no-auto-commits` matter for a \
               hands-off loop: the first stops it waiting on a prompt, the second \
               keeps it out of your git history. Not installed on the machine this \
               table was checked against — confirm the argv with `aider --help` \
               before a long run.",
        confidence: Confidence::Template,
    },
    Known {
        id: "hermes",
        label: "Hermes",
        bin: "hermes",
        kind: "hermes",
        // `-z` / `--oneshot`, not `-p`, which this CLI does not have at all.
        // Its own description is exactly what a loop wants: send one prompt,
        // print only the final response, no banner and no spinner.
        args: &["-z", "{prompt}"],
        prompt_on_stdin: false,
        requires_env: &[],
        tiers: &["standard"],
        discovers_models: false,
        models: &[],
        cost_per_1k: None,
        version_arg: "--version",
        note: "One-shot mode prints just the answer, which is what a loop needs. \
               A run that seems to hang is usually Hermes waiting on a command \
               approval — `--yolo` bypasses those, which is worth understanding \
               before adding it.",
        confidence: Confidence::Verified,
    },
    Known {
        id: "llm",
        label: "llm (Datasette)",
        bin: "llm",
        kind: "byok",
        // No `-m {model}`: this entry offers no model list, so the placeholder
        // would render as an empty string and the CLI would be handed `-m ""`.
        // Bare `llm` reads the prompt from stdin and uses its own configured
        // default, which is the right behaviour when nothing was chosen.
        args: &[],
        prompt_on_stdin: true,
        requires_env: &[],
        tiers: &["cheap", "standard"],
        discovers_models: false,
        models: &[],
        cost_per_1k: None,
        version_arg: "--version",
        note: "Simon Willison's `llm`. Talks to almost any provider through its \
               own plugins, and keeps its keys in its own store rather than the \
               environment. Not installed on the machine this table was checked \
               against — confirm the argv with `llm --help` before a long run.",
        confidence: Confidence::Template,
    },
];

pub fn find(id: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.id == id)
}

/// API keys worth reporting on. Presence only: a value is never read here, and
/// [`crate::web::secrets`] is the only module that writes one.
pub const ENV_KEYS: &[(&str, &str)] = &[
    ("ANTHROPIC_API_KEY", "Claude, when used through the API rather than `claude login`"),
    ("OPENAI_API_KEY", "OpenAI and Codex CLI"),
    ("XAI_API_KEY", "xAI / Grok"),
    ("GEMINI_API_KEY", "Google Gemini"),
    ("GOOGLE_API_KEY", "Google, older variable name still read by some tools"),
    ("GROQ_API_KEY", "Groq"),
    ("OPENROUTER_API_KEY", "OpenRouter, which fronts many models on one key"),
    ("DEEPSEEK_API_KEY", "DeepSeek"),
    ("MISTRAL_API_KEY", "Mistral"),
    ("PERPLEXITY_API_KEY", "Perplexity"),
    ("TOGETHER_API_KEY", "Together AI"),
    ("HF_TOKEN", "Hugging Face"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_entry_asks_for_a_model_it_cannot_offer() {
        // `spec.model.unwrap_or_default()` turns an unset model into an empty
        // string, so `["-m", "{model}"]` on an entry with nothing to prefill
        // hands the CLI a bare `-m ""`. The web UI prefills `models[0]`, so an
        // entry is safe exactly when it has a model to prefill.
        for k in KNOWN {
            let wants_model = k.args.iter().any(|a| a.contains("{model}"));
            assert!(
                !wants_model || !k.models.is_empty() || k.discovers_models,
                "`{}` substitutes {{model}} but offers no models, so it would be \
                 invoked with an empty one",
                k.id
            );
        }
    }

    #[test]
    fn every_entry_passes_the_prompt_exactly_once() {
        // A provider that never receives the prompt is the most confusing
        // possible failure: it runs, it succeeds, and it answers a question
        // nobody asked.
        for k in KNOWN {
            let in_args = k.args.iter().filter(|a| a.contains("{prompt}")).count();
            if k.prompt_on_stdin {
                assert_eq!(in_args, 0, "`{}` sends the prompt on stdin and in argv", k.id);
            } else {
                assert_eq!(in_args, 1, "`{}` must pass {{prompt}} exactly once", k.id);
            }
        }
    }

    #[test]
    fn a_model_that_is_offered_is_actually_used() {
        // The inverse of the first test, and a real bug it caught: the Claude
        // entry listed models, the UI showed them, the user picked one, and
        // the argv never mentioned {model} — so the choice was silently
        // discarded and the CLI used its own default.
        for k in KNOWN {
            if k.models.is_empty() && !k.discovers_models {
                continue;
            }
            assert!(
                k.args.iter().any(|a| a.contains("{model}")),
                "`{}` offers models but never passes one, so the choice is ignored",
                k.id
            );
        }
    }

    #[test]
    fn a_verified_entry_names_no_doubt_and_a_template_one_does() {
        // The grade is only worth carrying if it means something. Verified is
        // reserved for argv actually checked against the CLI's own help.
        for k in KNOWN {
            if k.confidence == Confidence::Template {
                assert!(
                    k.note.contains("confirm") || k.note.contains("Confirm"),
                    "`{}` is a template but its note does not say to confirm it",
                    k.id
                );
            }
        }
    }
}
