<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/v0.1.3/assets/loopsmith-logo-256.png" alt="loopsmith" width="140" />
  <h1>loopsmith-memory</h1>
  <p><em>Shared embedded memory for loopsmith: episodes, goal state, ledger, checkpoints.</em></p>
</div>

[![crates.io](https://img.shields.io/crates/v/loopsmith-memory?logo=rust&logoColor=white&label=crates.io&color=e6522c)](https://crates.io/crates/loopsmith-memory)
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

Everything that must survive a crash, a schedule boundary, or a context reset:
episodes (what a node did), goal state (what the gate has ruled), an append-only
ledger, checkpoints (where to resume), per-goal scratchpads, and skill trials.

Two rules shape it:

- **Validate before writing.** Bad data compounds — one wrong record becomes a
  retrieved "fact", which becomes reasoning, which becomes another record. Writes
  reject malformed input rather than storing it.
- **The store is a trait.** `sled` is the shipped backend but is effectively
  frozen upstream, so callers depend on `Store` and a different engine can be
  swapped in without touching them.

Checkpoints carry the stop gates' own accounting, so a loop that resumes often
cannot be handed a fresh revision budget every time.

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
