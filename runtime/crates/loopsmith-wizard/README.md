<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/v1.0.0/assets/loopsmith-logo-256.png" alt="loopsmith" width="140" />
  <h1>loopsmith-wizard</h1>
  <p><em>The guided wizard — the questions both loopsmith front ends ask.</em></p>
</div>

[![crates.io](https://img.shields.io/crates/v/loopsmith-wizard?logo=rust&logoColor=white&label=crates.io&color=e6522c)](https://crates.io/crates/loopsmith-wizard)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![rust](https://img.shields.io/badge/rust-1.85%2B-C1272D?logo=rust&logoColor=white)

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

`loopsmith loop guided` in a terminal and `loopsmith web` in a browser build the same
config by asking the same questions. This crate is what they share:

- **`catalog`** — the agent CLIs loopsmith knows how to drive, each with the
  argv that works, the environment it needs, and the models it accepts. Adding
  a CLI is a data change.
- **`detect`** — what is actually installed here. The synchronous half (which
  CLIs are on `PATH`, which API keys are set, which MCP servers and skills are
  configured) is instant and needs no runtime. The `probe` feature adds the
  half that runs subprocesses: `--version` with a timeout, `ollama list`, and
  the opt-in handshake that proves a provider answers.
- **`interview`** — the terminal wizard, one call from start to a rendered
  config, with `:back`, `:quit`, and a draft save that never loses an answer.

### Features

| Feature | Default | Adds |
|---|---|---|
| `probe` | off | the async half of `detect` (pulls in `tokio`) |

## Where it sits

```
loopsmith  (the CLI binary: arguments, dispatch, scaffolding)
├── loopsmith-web ─────── loopsmith-wizard ──┐
├── loopsmith-run ──┬──── loopsmith-gate ────┤
│   (the engine)    ├──── loopsmith-skills ──┤
│                   ├──── loopsmith-provider ┼── loopsmith-core ── loopsmith-util
│                   ├──── loopsmith-graph ───┤      (config)        (primitives)
│                   └──── loopsmith-memory ──┘
└── loopsmith-mcp  (gate, memory, and graph over stdio)
```

| Crate | Purpose |
|---|---|
| [`loopsmith`](https://crates.io/crates/loopsmith) | the CLI binary |
| [`loopsmith-util`](https://crates.io/crates/loopsmith-util) | PATH lookup, wall clock, runtime platform detection |
| [`loopsmith-core`](https://crates.io/crates/loopsmith-core) | the config model — intent, execution, safety, evolution — and its validation |
| [`loopsmith-memory`](https://crates.io/crates/loopsmith-memory) | `sled`-backed episodes, goal state, ledger, checkpoints |
| [`loopsmith-graph`](https://crates.io/crates/loopsmith-graph) | DAG scheduling, critical path, Amdahl-driven concurrency |
| [`loopsmith-gate`](https://crates.io/crates/loopsmith-gate) | the deterministic verification gate |
| [`loopsmith-provider`](https://crates.io/crates/loopsmith-provider) | provider routing and the tier cascade |
| [`loopsmith-skills`](https://crates.io/crates/loopsmith-skills) | sub-agent acquisition, quarantine, outcome ranking |
| [`loopsmith-mcp`](https://crates.io/crates/loopsmith-mcp) | local stdio MCP server over memory, gate, and graph |
| [`loopsmith-run`](https://crates.io/crates/loopsmith-run) | the run engine: iteration state machine, dispatch, isolation, recovery, triggers |
| [`loopsmith-wizard`](https://crates.io/crates/loopsmith-wizard) | the guided wizard: known-CLI catalog, machine detection, the terminal interview |
| [`loopsmith-web`](https://crates.io/crates/loopsmith-web) | the loopback-only browser UI |

## The one rule the whole design rests on

> A model must not be the thing that certifies its own completion.

`goal_satisfied` is written by [`loopsmith-gate`](https://crates.io/crates/loopsmith-gate)
and by nothing else, and the gate can **revoke**: delete a required artifact and a
satisfied goal flips back.

Full documentation: <https://github.com/bitphill/loopsmith#readme>

MIT licensed. © bitphill
