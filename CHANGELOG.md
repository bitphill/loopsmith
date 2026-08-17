# Changelog

All notable changes to loopsmith. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is
[semver](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-08-17

First release. The design in one line: `goal_satisfied` is written by a
deterministic Rust gate and by nothing else, and the gate can revoke.

### The runtime

- **A–J config model** in YAML or Markdown, with `validate` refusing to run until
  every `pre_execution` step is marked done. That refusal is the feature: a
  process you cannot describe in checkable terms should not be automated.
- **Deterministic verification gate.** Detectors are `file_exists`, `regex`,
  `script`, and composites. The gate promotes *and* revokes — delete a required
  artifact and a satisfied goal flips back.
- **DAG scheduling** with critical path and Amdahl-derived concurrency, and
  phases that open one at a time.
- **Four layered stop gates** — iterations, per-node revisions, wall clock, and
  no-progress — all evaluated every iteration. Their accounting lives in the
  checkpoint, so a resume cannot refund a spent revision budget.
- **Shared memory** on `sled`: episodes, goal state, an append-only ledger,
  checkpoints, per-goal scratchpads, and skill trials.
- **Provider routing** for Claude Code, Ollama, Grok, OpenAI, Gemini, Hermes,
  MCP, and any BYOK command, with a per-tier cascade and enforced judge
  independence. `requires_env` names the variables a provider needs; loopsmith
  checks only that they exist and never reads their values, so a key cannot
  reach a prompt, a log, or the ledger.
- **Git worktree isolation** per builder, degrading to the shared directory —
  and saying so — outside a repository or without git.
- **A local stdio MCP server** exposing memory, gate, and graph to any MCP
  client.
- **Sub-agent acquisition** ordered installed → marketplace → generate, with a
  star floor, credential-shaped-name refusal, and quarantine until a human
  promotes.

### Cross-platform

- Linux, macOS, and Windows, verified by a CI matrix rather than asserted.
- **Nothing is decided at build time.** The userland is probed by asking `sed`
  for a version, because Homebrew's coreutils puts GNU tools ahead of BSD ones
  on a Mac and the operating system therefore does not imply the answer. The
  scheduler is whichever candidate is on `PATH`: `launchctl` then `crontab` on
  macOS, `crontab` then `systemctl` on Linux and BSD, `schtasks` then `crontab`
  on Windows.
- **Every new loop gets both launcher flavours** — `run.sh`/`resume.sh` and
  `run.cmd`/`resume.cmd` — on every host, because a loop directory outlives the
  machine that produced it. Every `.sh` is POSIX, since macOS ships bash 3.2.
- `scripts/compat.sh` travels with each loop: `sed_i`, `stat_size`,
  `stat_mtime`, `readlink_f`, `sha256`, `require`, `need_bash`. `require` and
  `need_bash` exit **2**, not 1 — a detector's exit code is its verdict, and
  "cannot run the check" is not "the check failed".
- `loopsmith doctor` reports the host and what it stops you doing.
- Home directory resolution honours `USERPROFILE` and `HOMEDRIVE`+`HOMEPATH`,
  not just `HOME`.
- Universal installers: `install.sh` for Linux/macOS/BSD, `install.bat` +
  `installers/deps.ps1` for Windows.

### Packaging

- crates.io: `loopsmith` plus the eight libraries behind it.
- npm: `@bitphill/loopsmith`. PyPI: `loopsmith-cli`. Both names differ from the
  command because `loopsmith` was already registered on those registries by
  unrelated projects; the installed binary is `loopsmith` either way.
- Homebrew: the `bitphill/loopsmith` tap.

### Decisions worth knowing about

Two things look unfinished and are not:

- **`--install` writes a scheduler definition and stops.** It does not run
  `launchctl load -w` or `schtasks /Create`. Registering a scheduled job is a
  persistent, user-visible change to someone's machine, so it stays their call.
- **Cron expressions are evaluated in UTC.** Deriving a correct local offset in
  a multithreaded process is unsound on Unix without care, and a scheduler that
  is quietly an hour off twice a year is worse than one that is honestly in UTC.
  `schedule` says so in its output. For a cadence that does not care, use an
  `interval` trigger.

And one default that is deliberately impatient: the starter `ollama` provider
times out at 120 seconds. `ollama run` pulls a missing model, a pull is
indistinguishable from slow generation from outside the process, and the point of
a cheap tier is to be abandoned quickly. Run `ollama pull <model>` first.

[0.1.0]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.0
