<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/v1.0.0/assets/loopsmith-logo-256.png" alt="loopsmith" width="140" />
  <h1>loopsmith-run</h1>
  <p><em>The run engine — the iteration state machine behind every loopsmith run.</em></p>
</div>

[![crates.io](https://img.shields.io/crates/v/loopsmith-run?logo=rust&logoColor=white&label=crates.io&color=e6522c)](https://crates.io/crates/loopsmith-run)
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

Everything that happens between "start this loop" and "here is why it
stopped". Each iteration acquires any missing sub-agents, dispatches the
execution graph wave by wave (in parallel, in isolated git worktrees where a
node asks for one), collects evidence, asks the gate for a ruling, records what
each skill was worth, and then asks the stop gates whether to go on.

Two properties are structural rather than advisory:

- **The gate decides, the engine obeys.** The engine never writes
  `goal_satisfied`; it hands evidence to
  [`loopsmith-gate`](https://crates.io/crates/loopsmith-gate) and records the
  ruling.
- **The stop check is mechanical.** It runs after the gate and reads only
  counters and ceilings, so no amount of confident model output can extend a
  run past its budget.

The same crate holds the pieces a run leans on: worktree isolation, the
plain-text run log that mirrors the ledger, judge-verdict parsing, and the
trigger watcher behind `loopsmith run watch` and `loopsmith run schedule`.

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
