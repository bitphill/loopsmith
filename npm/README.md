<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/v0.2.2/assets/loopsmith-logo-256.png" alt="loopsmith" width="180" />
  <h1>loopsmith</h1>
  <p><em>Self-evolving agent loops. The gate is code, so "done" cannot be argued.</em></p>
</div>

[![npm](https://img.shields.io/npm/v/%40bitphill%2Floopsmith?logo=npm&logoColor=white&label=npm&color=cb3837)](https://www.npmjs.com/package/@bitphill/loopsmith)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![platforms](https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A)
![node](https://img.shields.io/badge/node-%E2%89%A518-339933?logo=nodedotjs&logoColor=white)

```bash
npm install -g @bitphill/loopsmith
loopsmith doctor
```

Or without installing anything permanently:

```bash
npx @bitphill/loopsmith doctor
```

> **This package is a Rust binary, not a JavaScript library.** There is nothing to
> `require()` or `import`. It installs a `loopsmith` command. If you want to drive
> loops from Node, spawn the CLI — its exit codes are its API.

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
file, you can run a loop. And if you would rather not open a text file at all,
`loopsmith --web` builds one for you in a browser.

### ➜ [START-HERE — README-FOR-DUMMIES.md](https://github.com/bitphill/loopsmith/blob/v0.2.2/README-FOR-DUMMIES.md)

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

## The browser UI

```bash
loopsmith --web        # or: loopsmith web
```

Serves `http://127.0.0.1:3000` and opens a tab. Everything the CLI does, done by
clicking — with every field explained in place, for people who would rather not
learn a schema before they learn whether the tool is useful.

Six steps rather than one long form — Place, Power, Intent, Proof, Work, Ship —
carrying only the actions that make sense on each, and `⌘K` to reach any step,
section, action, or example directly.

It probes the machine first, so nothing has to be typed from memory: agent CLIs
on `PATH`, the Ollama models actually pulled, MCP servers already configured by
your editor, which API keys are set, and which sub-agents are installed. Found
CLIs become one-click provider cards prefilling a working argv, and a **Test**
button puts one real prompt through a provider so a wrong flag surfaces now
rather than in iteration four of an unattended run. The folder button opens your
operating system's own folder chooser, so no path has to be typed at all.

The right-hand panel re-checks the draft on every keystroke — the same validator,
planner, and permission derivation the CLI uses, in-process: every problem with
the field it belongs to, what a run could cost at the ceilings currently set (or
**unbounded** if none is), the wave schedule and the speedup ceiling no worker
count beats, and parallel builders that would overwrite each other.

All thirteen examples are compiled into the binary and load with one click. The
buttons spawn the real `loopsmith` binary and stream its output live, so the
browser can never drift from the CLI and can never do anything `loopsmith --help`
does not list. A run belongs to the server, not the page: close the tab and it
keeps going, reopen and the log picks up from the start.

Binds loopback only, and refuses any request not addressed to it. API keys go to
your shell profile or your OS secret store — only the variable *name* is ever
written into a config.

## Five minutes

```bash
npm install -g @bitphill/loopsmith

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

## How this package installs the binary

`postinstall` downloads the prebuilt binary for your platform from the matching
[GitHub release](https://github.com/bitphill/loopsmith/releases) and **verifies it
against the release's published `SHA256SUMS` before installing it**. A postinstall
script that runs an unverified download is a supply-chain hole with a progress bar.

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

If the download fails, `npm install` does **not** hard-fail (a flaky network
should not break an install of twenty other things); the launcher reports a clear
error at run time instead.

## The package name

`loopsmith` on npm was already registered by an unrelated project, so this package
is scoped `@bitphill/loopsmith`. The installed command is `loopsmith` either way.
Elsewhere: [`loopsmith`](https://crates.io/crates/loopsmith) on crates.io,
[`loopsmith-cli`](https://pypi.org/project/loopsmith-cli/) on PyPI, and the
`bitphill/loopsmith` Homebrew tap.

## Documentation

- [Full README](https://github.com/bitphill/loopsmith#readme)
- [Section-by-section config reference](https://github.com/bitphill/loopsmith/blob/main/HOW-TO-USE.md)
- [Architecture and the reasoning behind it](https://github.com/bitphill/loopsmith/blob/main/README-DETAIL.md)
- [Thirteen worked examples](https://github.com/bitphill/loopsmith/tree/main/config/examples)
- [Changelog](https://github.com/bitphill/loopsmith/blob/main/CHANGELOG.md)

MIT licensed. © bitphill
