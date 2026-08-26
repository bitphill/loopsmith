<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/v0.2.0/assets/loopsmith-logo-256.png" alt="loopsmith" width="180" />
  <h1>loopsmith</h1>
  <p><em>Self-evolving agent loops. The gate is code, so "done" cannot be argued.</em></p>
</div>

[![PyPI](https://img.shields.io/pypi/v/loopsmith-cli?logo=python&logoColor=white&label=PyPI&color=3775a9)](https://pypi.org/project/loopsmith-cli/)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![platforms](https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A)
![python](https://img.shields.io/badge/python-%E2%89%A53.8-3775a9?logo=python&logoColor=white)

```bash
pip install loopsmith-cli
loopsmith doctor
```

> **This distribution is a Rust binary, not a Python library.** There is nothing to
> `import`. It installs a `loopsmith` command. To drive loops from Python, use
> `subprocess` — its exit codes are its API.

## TL;DR:

You have a job you redo every week and are fussy about — a competitor roundup, a
lead list, a landing page, a research brief. Write down **what you want** and
**how anyone would tell it's good**, in one plain text file. loopsmith puts an AI
to work on it, checks the result, sends it back when it falls short, and stops
when it passes. It can run on a schedule for weeks without you.

The rule that makes it safe to walk away: **the AI never gets to say "done".** A
deterministic checker reads the actual files and decides — and it can revoke, so
a goal that stops being true stops being satisfied.

**Not a developer?** Marketing, sales, research, ops — if you can edit a text
file, you can run a loop.

### ➜ [START-HERE — README-FOR-DUMMIES.md](https://github.com/bitphill/loopsmith/blob/v0.2.0/README-FOR-DUMMIES.md)

A plain-English guide: one install line, thirteen ready-made loops to copy, the
six settings you actually edit, and how to leave it running on a schedule.

There is also a generated [code wiki](https://bitphill.github.io/loopsmith/wiki/#overview) mapping the crates,
the execution engine, the gate, and the provider layer.

## What it is

You describe a purpose in a config — goals, how each is checked, what counts as
success, when to stop, what the loop may never do. loopsmith handles scheduling,
provider routing, memory, verification, and termination, and can run for weeks
without you.

One rule holds the whole design up:

> **A model must not be the thing that certifies its own completion.**

`goal_satisfied` is written by a deterministic Rust gate and by nothing else, and
the gate can **revoke**: delete a required artifact and a satisfied goal flips
back. A system that can only promote is a burndown chart with extra steps.

## Five minutes

```bash
pip install loopsmith-cli

# --path must be outside any repo you care about: a loop edits files and writes
# state, so it does not get pointed at the tool that runs it.
loopsmith new --path ~/loops/nightly-refactor --purpose "keep the module simple"

cd ~/loops/nightly-refactor
$EDITOR loop.yaml           # your goals, and how each one is checked
loopsmith validate loop.yaml
loopsmith plan     loop.yaml
./run.sh                    # run.cmd on Windows
```

### `validate` fails on purpose

```
error  pre_execution: 2 step(s) not marked done: Run this task manually end to
       end at least once; Write down what 'done' means in checkable terms.
       Automating before understanding produces fast, confident garbage
```

That refusal is the most valuable thing the tool does. Do the task by hand once —
the manual run *is* the spec. Mark each `pre_execution` step `done: true` once you
actually have.

## What a config looks like

Ten sections, **A** to **J**: information, the manual work list, goals,
validations, success criteria, stop gates, schedules, constraints, execution
guidelines, default skills. YAML or Markdown — the same model either way.

```yaml
name: nightly-refactor
description: keep the payments module simple without breaking it

pre_execution:
  - step: Ran the refactor by hand on one file and kept the diff
    done: true

goals:
  - name: simpler
    description: Cyclomatic complexity down, behaviour unchanged.

validations:
  - target: simpler
    name: tests-still-pass
    mode: objective
    statement: The suite exits clean.
    detector:
      type: script
      command: ./scripts/check-tests.sh
      expect_exit: 0
    blocking: true

success:
  - target: overall
    name: all-blocking-pass
    mode: percentage
    statement: Every blocking validation passes.
    threshold: 1.0

stop_gates:
  max_iterations: 8
  max_revisions_per_node: 3
  max_cost_usd: 5.0
  no_progress_iterations: 3

graph:
  nodes:
    - id: refactor
      role: builder
      instruction: Simplify one function. State any assumption you had to make.
      goals: [simpler]
      isolated: true          # its own git worktree
    - id: review
      role: judge
      instruction: Check the diff against the brief. Pass or fail per check, with evidence.
      depends_on: [refactor]
      goals: [simpler]
      tier: strong
```

Detectors are `file_exists`, `regex`, `script`, and composites. Only a detector
can satisfy a goal — a model's opinion of its own work never does.

## While it runs

| Question | Command |
|---|---|
| What does the gate say? | `loopsmith status <config> <run-id>` |
| What happened? | `loopsmith ledger <config> <run-id>` |
| Why did it stop? | the last line of `logs/<run-id>.log` |
| What does it want changed about itself? | `loopsmith proposals <config> <run-id>` |
| Which providers can it reach? | `loopsmith providers <config>` |
| Will this machine get in the way? | `loopsmith doctor <config>` |

**The loop never edits its own config.** Changes to goals, validations, success
criteria, and sub-agent adoption are written as *proposals* for a human to apply.

## Providers and BYOK

Every provider is a command template, which is what makes bring-your-own-key
free: Claude Code, Ollama, a Grok CLI, an OpenAI-compatible endpoint driven by
`curl`, an MCP server over stdio — all of them are "a program you run with a
prompt". Adding one is a config edit, never a rebuild.

`requires_env` names variables that must **exist**. loopsmith never reads their
values, so a key cannot reach a prompt, a log, or the ledger.

> ⚠ Never paste an API key into a chat window, a config file, or an issue. If one
> ends up somewhere it should not be, rotate it — deleting the message is not
> enough.

## How this distribution installs the binary

The binary is fetched **on first run**, not during `pip install`, and cached under
`~/.loopsmith/bin/<version>/` (override with `LOOPSMITH_HOME`).

That is deliberate. A wheel that downloads at install time breaks in every
environment that installs without a network and runs with one — CI images, Docker
build stages, locked-down build hosts — and the failure surfaces as an install
error for a package the user has not tried to use yet.

Every download is **verified against the release's published `SHA256SUMS` before it
is executed**. Fetching a binary and running it unverified is a supply-chain hole
with a progress bar.

Prebuilt for:

| Platform | Target |
|---|---|
| Linux x86_64 (glibc) | `x86_64-unknown-linux-gnu` |
| Linux x86_64 (musl, auto-detected) | `x86_64-unknown-linux-musl` |
| Linux arm64 | `aarch64-unknown-linux-gnu` |
| macOS Intel | `x86_64-apple-darwin` |
| macOS Apple silicon | `aarch64-apple-darwin` |
| Windows x86_64 | `x86_64-pc-windows-msvc` |

Anywhere else, build from source — it is the same program:

```bash
cargo install loopsmith
```

## The distribution name

`loopsmith` on PyPI was already registered by an unrelated project, so this
distribution is `loopsmith-cli`. The installed command is `loopsmith` either way.
Elsewhere: [`loopsmith`](https://crates.io/crates/loopsmith) on crates.io,
[`@bitphill/loopsmith`](https://www.npmjs.com/package/@bitphill/loopsmith) on npm,
and the `bitphill/loopsmith` Homebrew tap.

## Documentation

- [Full README](https://github.com/bitphill/loopsmith#readme)
- [Section-by-section config reference](https://github.com/bitphill/loopsmith/blob/main/HOW-TO-USE.md)
- [Architecture and the reasoning behind it](https://github.com/bitphill/loopsmith/blob/main/README-DETAIL.md)
- [Thirteen worked examples](https://github.com/bitphill/loopsmith/tree/main/config/examples)
- [Changelog](https://github.com/bitphill/loopsmith/blob/main/CHANGELOG.md)

MIT licensed. © bitphill
