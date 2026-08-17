# Changelog

All notable changes to loopsmith. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is
[semver](https://semver.org/spec/v2.0.0.html).

## [0.1.3] — 2026-08-17

The release where Windows stopped being a badge and started being a platform.

### Fixed

- **`which()` found nothing at all on Windows.** It joined the bare command name
  onto each `PATH` entry, and executables there are `git.exe` — so nothing ever
  matched and it returned `None` for every command on the machine. Every caller
  then faithfully reported the falsehood it was handed: `doctor` listed every tool
  as absent, `Platform::detect` found no scheduler, and worktree isolation
  degraded to the shared directory with "git not on PATH" on a runner where
  `actions/checkout` had just used `C:\Program Files\Git\bin\git.exe`. The
  `schtasks` support added in 0.1.0 was dead on arrival for the same reason,
  because `which("schtasks")` could never succeed. Roughly a third of the Windows
  claim was decoration.

  Each `PATHEXT` suffix is now tried as well as the bare name, honouring the
  variable rather than hardcoding a list — it is how a machine says `.ps1` counts
  as a command. The suffix is *appended*, not set as the extension, so `check.sh`
  can become `check.sh.exe` where `set_extension` would have produced `check.exe`.

  `Command::new("git")` was never affected: Windows' `CreateProcess` appends the
  extension itself. Only loopsmith's own lookups were broken.

- **The generated `.cmd` launchers reported success when they had failed.**
  `setlocal` saves the current errorlevel and the implicit `endlocal` at the end
  of a batch file restores it, so a bare `exit /b 127` inside a `setlocal` scope
  exits **0**. A loop whose pinned binary had moved printed "loopsmith is not at
  … and not on PATH" and then exited successfully — the precise silent failure
  the exit code exists to prevent, and one a scheduled job would never surface.
  `resume.cmd`'s "no run id given" exit 2 had the same defect, as did the export
  launcher.

  `endlocal & exit /b <code>` fixes it on a top-level line but **not** inside a
  nested `if ( … )` block, which is where the broken one lived — so each launcher
  now has exactly one `exit /b`, on its last line, reached by every path via
  `goto`. They also enable delayed expansion and capture with `!ERRORLEVEL!`,
  because a parenthesised block is parsed before it runs and `%ERRORLEVEL%` inside
  one reads the value from *before* the command. A test asserts the single exit,
  the label, and the absence of parse-time capture.
- `loopsmith new` closed by telling every user to run `run.sh`, including on
  Windows where `cmd.exe` cannot execute it and `run.cmd` was sitting beside it.
  It now names the launcher the host can run, and prints `set` rather than
  `export` for the API-key line there.

### Changed

- **Every published package now carries a README that stands on its own.** The
  crates.io, npm, and PyPI pages previously said little and pointed at the
  repository; someone arriving from a registry had to leave to learn what the tool
  was. Each now explains the design, shows a real config, lists the subcommands,
  and covers install, providers, scheduling, and platform behaviour in the idiom
  of that registry. The eight library crates had **no** README at all and rendered
  as blank pages; each now explains its own role and where it sits.
- CI no longer downloads a third-party toolchain or cache action. `rustup` is
  preinstalled on GitHub-hosted runners, and eight consecutive runs lost a leg to
  codeload 429/502/503 while fetching an action — before a line of loopsmith
  compiled. `continue-on-error` could not save it either: actions are downloaded
  during "Set up job", before the step that was allowed to fail ever runs. A cold
  build takes about two minutes, so the cache was saving less than the flakiness
  cost.
- Two tests that invoked `./run.sh` and `./resume.sh` directly now pick the
  launcher the host can execute. Gating them with `#[cfg(unix)]` would have made
  the suite green while leaving the `.cmd` launchers unexercised on the only
  platform that runs them.

## [0.1.2] — 2026-08-17

### Fixed

- **The PyPI launcher exec-looped instead of running.** `ensure_binary()` had a
  "reuse a `loopsmith` already on PATH" shortcut, and pip installs *this* package's
  console script as `loopsmith` on PATH — so `shutil.which("loopsmith")` found the
  very script that was running and `execv` re-entered it forever. The symptom was
  the worst kind: no output, no error, no traceback, just a command that never
  returns. The shortcut is gone, because telling our own console script apart from
  a cargo-installed binary means comparing argv[0], the interpreter's script
  directory, and the symlinks between — all to save one download that happens once
  per version. `__main__` also refuses outright if the binary it resolved is the
  launcher itself, since that failure mode has no useful symptom to debug.

Only the PyPI wrapper was affected. `loopsmith-cli` 0.1.1 on PyPI is unusable;
0.1.2 is the version to install. The npm wrapper resolves a fixed path beside
itself and never had this bug.

Everything else is byte-for-byte 0.1.1. The version is bumped across all four
registries rather than only on PyPI, so one number means one thing everywhere.

## [0.1.1] — 2026-08-17

The first release the CI matrix ever ran against, and it found something on its
first attempt — which is the entire argument for having added it.

### Fixed

- **`schedule --install` no longer lies on Linux and Windows.** The flag was
  threaded to the launchd branch and dropped by the other two, so
  `loopsmith schedule loop.yaml --install` on Linux did exactly what it did
  without the flag, silently, leaving the user believing a schedule had been
  registered. It now explains why there is nothing to write: only launchd has a
  per-job file loopsmith can add without touching entries it does not own. A
  crontab is one file per user with no drop-in directory, and Task Scheduler
  keeps its jobs in a database reached only through `schtasks`. In both cases the
  next step was always "run the command above yourself".
- A test asserted the launchd wording on every host, so it had been passing by
  describing macOS. It now asserts that whichever scheduler path ran names the
  next step, and a new test covers the no-op `--install` from both sides.

Everything else in 0.1.0 is unchanged. 0.1.0's binaries remain published; this is
the version to install.

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
  On macOS it does write the plist, because a LaunchAgents directory holds one
  file per job and adding loopsmith's cannot disturb anyone else's. On Linux and
  Windows it writes nothing and says why: a crontab is a single per-user file with
  no drop-in directory, and Task Scheduler keeps its jobs in a database reachable
  only through `schtasks`. The flag used to be accepted and silently ignored
  there, which left the user believing a schedule had been registered.
- **Cron expressions are evaluated in UTC.** Deriving a correct local offset in
  a multithreaded process is unsound on Unix without care, and a scheduler that
  is quietly an hour off twice a year is worse than one that is honestly in UTC.
  `schedule` says so in its output. For a cadence that does not care, use an
  `interval` trigger.

And one default that is deliberately impatient: the starter `ollama` provider
times out at 120 seconds. `ollama run` pulls a missing model, a pull is
indistinguishable from slow generation from outside the process, and the point of
a cheap tier is to be abandoned quickly. Run `ollama pull <model>` first.

[0.1.3]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.3
[0.1.2]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.2
[0.1.1]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.1
[0.1.0]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.0
