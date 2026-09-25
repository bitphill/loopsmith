<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/main/assets/loopsmith-logo-256.png" alt="loopsmith" width="180" />
  <h1>loopsmith</h1>
  <p><em>Hand a repeating job to an AI. A checker you wrote — not the AI — decides when it is done.</em></p>
</div>

[![PyPI](https://img.shields.io/pypi/v/loopsmith-cli?logo=pypi&logoColor=white&label=PyPI&color=3775A9)](https://pypi.org/project/loopsmith-cli/)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![platforms](https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A)
![python](https://img.shields.io/badge/python-%E2%89%A53.8-3776AB?logo=python&logoColor=white)

```bash
pipx install loopsmith-cli      # or: pip install loopsmith-cli
loopsmith doctor
```

> **The distribution is `loopsmith-cli`; the command is `loopsmith`.** The
> shorter name was taken on PyPI. This package is a Rust binary with a thin
> Python launcher — there is nothing to `import`. The binary for your platform
> is downloaded on first run, so there is no Rust toolchain involved.

---

## What it is

You have a job you redo every week and are fussy about — a competitor roundup,
a lead list, a landing page, a research brief. You describe it once. loopsmith
runs it over and over on its own, and stops when a check **you** wrote says it
is done, or a limit **you** set says enough.

The part that makes it worth running unattended: whether a goal is done is
decided by compiled Rust, never by a model. The model does the work; a gate
that cannot be argued with decides whether the work counts. That gate can also
take "done" back — delete a required file and a satisfied goal flips straight
back to unsatisfied.

## Sixty seconds

```bash
loopsmith doctor                    # what this machine can and cannot do
loopsmith --web                     # build a loop in your browser
```

Not a browser person? `loopsmith --guided` asks the same questions in the
terminal. Either way, nothing runs and nothing is spent while you answer.

To go straight to a file instead:

```bash
loopsmith loop new --path ~/loops/my-loop --purpose "keep the roundup current"
loopsmith loop validate ~/loops/my-loop/loop.yaml     # refuses, on purpose
loopsmith loop plan     ~/loops/my-loop/loop.yaml     # what it would do, and how fast
loopsmith run start     ~/loops/my-loop/loop.yaml
```

**That refusal is the point.** A fresh loop will not run until you tick the
steps saying you have done the job by hand at least once. Automating a process
nobody has performed produces the wrong answer faster, and at a scale that is
harder to undo.

## Fifteen worked loops to start from

Research, refactoring, traffic, trend tracking, landing pages, lead lists,
marketing, blogging, cold outreach, agent payments, a small game, idea
discovery, account watching, a container-isolated refactor, and one that
proposes improvements to itself. All fifteen are compiled into the binary and
load with one click in `--web`, or copy one from
[`config/examples/`](https://github.com/bitphill/loopsmith/tree/main/config/examples).

## Bring your own model

Every provider is a command template, so anything you can run from a shell can
serve a loop: Claude Code, Ollama, a Grok CLI, an OpenAI-compatible endpoint
driven by `curl`, an MCP server over stdio. Adding one is a config edit, never
a rebuild. Keys are named, never read — `requires_env` says a key must be
present, and the value goes from your environment to the command without
passing through loopsmith or its ledger.

Cheap models carry the mechanical work; strong models carry judgment. And a
judge is refused outright if it would run on the same provider as the work it
is grading, because a model marking its own homework is not a check.

## Keeping it alive

```bash
loopsmith run watch     loop.yaml            # stay resident, run on every trigger
loopsmith run schedule  loop.yaml --install  # hand it to launchd, cron, or Task Scheduler
loopsmith run status    loop.yaml <run-id>   # what the gate has ruled so far
loopsmith run ledger    loop.yaml <run-id>   # everything that happened, including why it stopped
```

A run survives a crash: state is a real store on disk, and `loopsmith run
resume` picks up from the last checkpoint.

## Where the rest of it is

- [**README-FOR-DUMMIES.md**](https://github.com/bitphill/loopsmith/blob/main/README-FOR-DUMMIES.md)
  — the same thing with no jargon, if this page assumed too much
- [**README.md**](https://github.com/bitphill/loopsmith/blob/main/README.md)
  — the full version
- [**HOW-TO-USE.md**](https://github.com/bitphill/loopsmith/blob/main/HOW-TO-USE.md)
  — every config section, one at a time, with what goes wrong if you skip it
- [**LOOP-TEMPLATE.md**](https://github.com/bitphill/loopsmith/blob/main/LOOP-TEMPLATE.md)
  — a blank loop with a note on every field
- [**Architecture**](https://github.com/bitphill/loopsmith/wiki/Architecture)
  · [**Commands**](https://github.com/bitphill/loopsmith/wiki/Commands)
  · [**Migration 0.3 → 1.0**](https://github.com/bitphill/loopsmith/wiki/Migration-0-3-To-1-0)
- [**Code wiki**](https://bitphill.github.io/loopsmith/wiki/#overview)
  — a page per subsystem, generated from the code

Runs on Linux, macOS, and Windows. MIT licensed.
