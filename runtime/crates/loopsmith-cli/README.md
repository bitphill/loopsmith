<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/v0.1.3/assets/loopsmith-logo-256.png" alt="loopsmith" width="180" />
  <h1>loopsmith</h1>
  <p><em>Self-evolving agent loops. The gate is code, so "done" cannot be argued.</em></p>
</div>

[![crates.io](https://img.shields.io/crates/v/loopsmith?logo=rust&logoColor=white&label=crates.io&color=e6522c)](https://crates.io/crates/loopsmith)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![rust](https://img.shields.io/badge/rust-1.75%2B-C1272D?logo=rust&logoColor=white)
![platforms](https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A)
[![npm](https://img.shields.io/npm/v/%40bitphill%2Floopsmith?logo=npm&logoColor=white&label=npm&color=cb3837)](https://www.npmjs.com/package/@bitphill/loopsmith)
[![PyPI](https://img.shields.io/pypi/v/loopsmith-cli?logo=python&logoColor=white&label=PyPI&color=3775a9)](https://pypi.org/project/loopsmith-cli/)

```bash
cargo install loopsmith
```

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
cargo install loopsmith

# --path must be outside any repo you care about: a loop edits files and writes
# state, so it does not get pointed at the tool that runs it.
loopsmith new --path ~/loops/nightly-refactor --purpose "keep the module simple"

cd ~/loops/nightly-refactor
$EDITOR loop.yaml           # your goals, and how each one is checked
loopsmith validate loop.yaml
loopsmith plan     loop.yaml
./run.sh                    # run.cmd on Windows
```

`new` writes the config, the directories, an MCP server definition, a permission
grant, `scripts/compat.sh`, and `run.sh` / `resume.sh` / `run.cmd` /
`resume.cmd` with absolute paths filled in.

### `validate` fails on purpose

```
error  pre_execution: 2 step(s) not marked done: Run this task manually end to
       end at least once; Write down what 'done' means in checkable terms.
       Automating before understanding produces fast, confident garbage
```

That refusal is the most valuable thing the tool does. Do the task by hand once —
the manual run *is* the spec. Mark each `pre_execution` step `done: true` when you
have actually done it.

## What a config looks like

Ten sections, **A** to **J**: information, the manual work list, goals,
validations, success criteria, stop gates, schedules, constraints, execution
guidelines, default skills. YAML or Markdown — the same model either way, and
`loopsmith convert` translates between them.

```yaml
name: nightly-refactor
description: keep the payments module simple without breaking it

pre_execution:
  - step: Ran the refactor by hand on one file and kept the diff
    done: true
  - step: Confirmed the suite is green before the loop starts
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

  - target: simpler
    name: complexity-report-exists
    mode: objective
    statement: The report was produced and is non-empty.
    detector:
      type: file_exists
      path: out/complexity.md
      non_empty: true
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
  max_wall_clock_seconds: 3600
  max_cost_usd: 5.0
  no_progress_iterations: 3

graph:
  nodes:
    - id: refactor
      role: builder
      instruction: Simplify one function. State any assumption you had to make.
      goals: [simpler]
      tier: standard
      isolated: true          # its own git worktree
    - id: review
      role: judge
      instruction: Check the diff against the brief. Pass or fail per check, with evidence.
      depends_on: [refactor]
      goals: [simpled]
      tier: strong
  concurrency:
    type: auto
    cap: 16
```

Detectors are `file_exists`, `regex`, `script`, and composites. Only a detector
can satisfy a goal — a model's opinion of its own work never does.

## What it does while it runs

| Question | Command |
|---|---|
| What does the gate say? | `loopsmith status <config> <run-id>` |
| What happened? | `loopsmith ledger <config> <run-id>` |
| Why did it stop? | the last line of `logs/<run-id>.log` |
| What does it want changed about itself? | `loopsmith proposals <config> <run-id>` |
| Which providers can it reach? | `loopsmith providers <config>` |
| Will this machine get in the way? | `loopsmith doctor <config>` |
| Ask the gate right now | `loopsmith gate <config> --target <goal>` |

**The loop never edits its own config.** Goals, validations, success criteria, and
sub-agent adoption are written as *proposals* for a human to apply.

Four stop gates are evaluated every iteration — iterations, per-node revisions,
wall clock, and no-progress. Their accounting lives in the checkpoint, so a resume
cannot refund a spent revision budget.

If a run stops early, `./resume.sh <run-id>` continues from the last checkpoint.
When one meets its bar, loopsmith writes `<name>-success/` beside the config: the
configuration that converged, the gate's evidence, and the artifacts.

## Providers and BYOK

Every provider is a command template, which is what makes bring-your-own-key
free: Claude Code, Ollama, a Grok CLI, an OpenAI-compatible endpoint driven by
`curl`, an MCP server over stdio — all of them are "a program you run with a
prompt". Adding one is a config edit, never a rebuild.

```yaml
providers:
  providers:
    - id: openai
      kind: openai
      tiers: [strong]
      command: curl
      args: ["-sS", "https://api.openai.com/v1/chat/completions",
             "-H", "Authorization: Bearer $OPENAI_API_KEY", "-d", "@-"]
      requires_env: [OPENAI_API_KEY]
      prompt_on_stdin: true
  cascade:
    cheap:    [ollama, claude]
    standard: [claude, gemini]
    strong:   [openai, claude]
  enforce_judge_independence: true
```

`requires_env` names variables that must **exist**. loopsmith never reads their
values, so a key cannot reach a prompt, a log, or the ledger — let the command
expand them itself, as `curl` does above.

> ⚠ Never paste an API key into a chat window, a config file, or an issue. If one
> ends up somewhere it should not be, rotate it — deleting the message is not
> enough.

Using `ollama`? Pull the model first (`ollama pull llama3`). `ollama run`
downloads a missing model, which is indistinguishable from slow generation from
outside the process, so the starter provider times out at 120s and lets the
cascade move on.

## Scheduling

A loop that runs once is a script. Two ways to make one live:

```bash
loopsmith watch    ~/loops/nightly-refactor/loop.yaml            # stay resident
loopsmith schedule ~/loops/nightly-refactor/loop.yaml --install  # hand it to the OS
```

Triggers are declared in section **G**: `cron`, `interval`, `file_change`,
`goal_satisfied`, or `manual`. Cron is evaluated in **UTC** — deriving a local
offset in a multithreaded process is unsound on Unix, and a scheduler quietly an
hour off twice a year is worse than one honestly in UTC. For a cadence that does
not care, use `interval`.

`schedule` uses whichever scheduler the machine actually has, not whichever its
OS is famous for: `launchctl` then `crontab` on macOS, `crontab` then `systemctl`
on Linux, `schtasks` then `crontab` on Windows. It **writes the definition and
stops** — registering a scheduled job is a persistent change to your machine, so
that step stays yours.

## Platforms

Linux, macOS, and Windows, checked by a CI matrix rather than asserted.

Nothing about the host is decided at build time. The userland is probed by asking
`sed` for a version — Homebrew's coreutils puts GNU tools ahead of BSD ones on a
Mac, so the operating system does not imply the answer — and the scheduler is
whichever candidate is actually on `PATH`.

`loopsmith doctor` reports what the host is and what that stops you doing. Every
new loop ships `scripts/compat.sh` with `sed_i`, `stat_size`, `stat_mtime`,
`readlink_f`, `sha256`, `require`, and `need_bash`, so a detector does not have to
branch by hand. Every `.sh` generated is POSIX, because macOS still ships bash
3.2; a `.cmd` sibling is written alongside on every platform, because a loop
directory outlives the machine that made it.

## The rest of the workspace

This crate is the binary. The libraries behind it are published separately and
compile automatically as its dependencies:

| Crate | Purpose |
|---|---|
| [`loopsmith-util`](https://crates.io/crates/loopsmith-util) | PATH lookup, wall clock, runtime platform detection |
| [`loopsmith-core`](https://crates.io/crates/loopsmith-core) | The A–J config model and its validation |
| [`loopsmith-memory`](https://crates.io/crates/loopsmith-memory) | `sled`-backed episodes, goal state, ledger, checkpoints |
| [`loopsmith-graph`](https://crates.io/crates/loopsmith-graph) | DAG scheduling, critical path, Amdahl-driven concurrency |
| [`loopsmith-gate`](https://crates.io/crates/loopsmith-gate) | The deterministic verification gate |
| [`loopsmith-provider`](https://crates.io/crates/loopsmith-provider) | Provider routing and the tier cascade |
| [`loopsmith-skills`](https://crates.io/crates/loopsmith-skills) | Sub-agent acquisition, quarantine, outcome ranking |
| [`loopsmith-mcp`](https://crates.io/crates/loopsmith-mcp) | Local stdio MCP server over memory, gate, and graph |

## Other ways to install

```bash
cargo install loopsmith                    # this crate — builds from source
npm  install -g @bitphill/loopsmith        # prebuilt binary
pip  install loopsmith-cli                 # prebuilt binary, fetched on first run
brew tap bitphill/loopsmith && brew install loopsmith
```

The npm and PyPI names differ from the command because `loopsmith` was already
registered on both by unrelated projects. The installed command is always
`loopsmith`.

## Documentation

- [Full README](https://github.com/bitphill/loopsmith#readme)
- [Section-by-section config reference](https://github.com/bitphill/loopsmith/blob/main/HOW-TO-USE.md)
- [Architecture and the reasoning behind it](https://github.com/bitphill/loopsmith/blob/main/README-DETAIL.md)
- [Blank template](https://github.com/bitphill/loopsmith/blob/main/LOOP-TEMPLATE.md)
- [JSON schema](https://github.com/bitphill/loopsmith/blob/main/config/loop.schema.json)
- [Thirteen worked examples](https://github.com/bitphill/loopsmith/tree/main/config/examples)
- [Changelog](https://github.com/bitphill/loopsmith/blob/main/CHANGELOG.md)

MIT licensed. © bitphill
