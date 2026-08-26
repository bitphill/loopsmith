<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/v0.2.1/assets/loopsmith-logo-256.png" alt="loopsmith" width="140" />
  <h1>loopsmith-gate</h1>
  <p><em>The deterministic verification gate — the only component that may mark a goal satisfied.</em></p>
</div>

[![crates.io](https://img.shields.io/crates/v/loopsmith-gate?logo=rust&logoColor=white&label=crates.io&color=e6522c)](https://crates.io/crates/loopsmith-gate)
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

The single most important crate in the workspace, because of what it forbids.

`goal_satisfied` is written **here and nowhere else**. No model, no judge node,
and no prompt can set it. Detectors are `file_exists`, `regex`, `script`, and
composites over those, and each one is a mechanical check with an exit code or a
match — never an opinion.

The gate also **revokes**. Delete a required artifact and a satisfied goal flips
back to unsatisfied. A gate that can only promote is a burndown chart with extra
steps.

A detector's exit code is its verdict, and "cannot run the check" is a different
fact from "the check failed" — a gate that conflates them reports missing tooling
as unfinished work.

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
