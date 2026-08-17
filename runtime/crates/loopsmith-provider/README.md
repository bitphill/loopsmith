<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/v0.1.3/assets/loopsmith-logo-256.png" alt="loopsmith" width="140" />
  <h1>loopsmith-provider</h1>
  <p><em>Provider routing for loopsmith: Claude Code, Ollama, Grok, OpenAI, Gemini, Hermes, MCP, and any BYOK command.</em></p>
</div>

[![crates.io](https://img.shields.io/crates/v/loopsmith-provider?logo=rust&logoColor=white&label=crates.io&color=e6522c)](https://crates.io/crates/loopsmith-provider)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![rust](https://img.shields.io/badge/rust-1.75%2B-C1272D?logo=rust&logoColor=white)

Part of **[loopsmith](https://github.com/bitphill/loopsmith)** — self-evolving
agent loops behind a deterministic verification gate.

> **You probably do not need to depend on this directly.** It is a component of the
> `loopsmith` binary and compiles automatically as one of its dependencies:
>
> ```bash
> cargo install loopsmith
> ```
>
> Depend on it directly only if you are building something else on loopsmith's
> internals. The API is not yet stable across minor versions.

## What this crate is

Every provider is a command template. That single decision is what makes
bring-your-own-key free: Claude Code, Ollama, a Grok CLI, an OpenAI-compatible
endpoint driven by `curl`, an MCP server over stdio — all of them are "a program
you run with a prompt". Adding one is a config edit, never a rebuild.

Nodes ask for a *tier* (`cheap`, `standard`, `strong`) and a cascade decides
which provider actually serves the call, falling through on failure or timeout.
Judge independence can be enforced, so a judge never runs on the same provider
as the builder whose work it is checking.

**Secrets never enter the process.** `requires_env` names variables that must
exist; their values are never read, substituted into arguments, or written to the
ledger. The command expands them itself, as `curl` does.

## Where it sits

```
loopsmith  (the CLI binary)
└── loopsmith-mcp ── loopsmith-gate ─┐
    loopsmith-skills ────────────────┤
    loopsmith-provider ──────────────┼── loopsmith-core ── loopsmith-util
    loopsmith-graph ─────────────────┤        (config)      (primitives)
    loopsmith-memory ────────────────┘
```

| Crate | Purpose |
|---|---|
| [`loopsmith`](https://crates.io/crates/loopsmith) | the CLI binary |
| [`loopsmith-util`](https://crates.io/crates/loopsmith-util) | PATH lookup, wall clock, runtime platform detection |
| [`loopsmith-core`](https://crates.io/crates/loopsmith-core) | the A–J config model and its validation |
| [`loopsmith-memory`](https://crates.io/crates/loopsmith-memory) | `sled`-backed episodes, goal state, ledger, checkpoints |
| [`loopsmith-graph`](https://crates.io/crates/loopsmith-graph) | DAG scheduling, critical path, Amdahl-driven concurrency |
| [`loopsmith-gate`](https://crates.io/crates/loopsmith-gate) | the deterministic verification gate |
| [`loopsmith-provider`](https://crates.io/crates/loopsmith-provider) | provider routing and the tier cascade |
| [`loopsmith-skills`](https://crates.io/crates/loopsmith-skills) | sub-agent acquisition, quarantine, outcome ranking |
| [`loopsmith-mcp`](https://crates.io/crates/loopsmith-mcp) | local stdio MCP server over memory, gate, and graph |

## The one rule the whole design rests on

> A model must not be the thing that certifies its own completion.

`goal_satisfied` is written by [`loopsmith-gate`](https://crates.io/crates/loopsmith-gate)
and by nothing else, and the gate can **revoke**: delete a required artifact and a
satisfied goal flips back.

Full documentation: <https://github.com/bitphill/loopsmith#readme>

MIT licensed. © bitphill
