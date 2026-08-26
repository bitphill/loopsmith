# Changelog

All notable changes to loopsmith. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is
[semver](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2026-08-26

Adds a browser UI. No change to the config model, the gate, or any existing
command's behaviour.

### Added

- **`loopsmith --web` / `loopsmith web`** — a local browser UI for building,
  checking, creating, and running loops. Both spellings resolve to one command;
  `--web` combined with a subcommand is refused rather than silently resolved.
  Serves `127.0.0.1:3000`, steps up a port if that one is busy, and opens a tab.

  It exists for the reader `README-FOR-DUMMIES.md` was written for — the one who
  bounced off a schema reference. Every field carries a permanent one-line hint
  and an info control explaining *why the field exists and what goes wrong
  without it*, and a dismissible five-panel tour explains the one idea the rest
  depends on: that a model never certifies its own completion.

  Three properties hold the design up. It binds loopback only, because it spawns
  commands as this user. Every action spawns `current_exe()` rather than calling
  the crates, so the browser cannot drift from the CLI and cannot do anything
  `loopsmith --help` does not list — the browser names a verb from a closed list
  and never names a program. And the frontend is compiled into the binary, so an
  install from any registry has a working UI with nothing else to fetch.

- **Machine detection.** Agent CLIs on `PATH` with their versions, Ollama models
  via `ollama list`, MCP servers read from `~/.claude.json`,
  `~/.claude/settings.json`, Claude Desktop, `~/.cursor/mcp.json`, VS Code and
  `./.mcp.json`, which API keys are present (presence only — values are never
  read), installed sub-agents, git, and the platform facts `doctor` reports.
  Found CLIs become one-click provider cards prefilling a known-good argv;
  entries whose argv is a starting point rather than verified say so on the card
  instead of failing later as a spawn error.

  A per-provider **Test** button performs a real handshake. It is a button and
  not part of detection because a page load is not consent to spend money.

- **Live review**, recomputed in-process on every edit: `loopsmith_core::validate`
  issues with clickable field paths, the wave schedule and Amdahl ceiling from
  `loopsmith_graph::plan`, parallel builders that would overwrite each other for
  want of a worktree, the derived permission grant, and an upper-bound cost — or
  the word **unbounded** where no ceiling is set.

- **Secrets panel.** Writes to the shell profile (a real environment variable,
  `0600`, inside a fenced block rewritten in place) or to the OS secret store —
  Keychain, Credential Manager, libsecret. The profile file is chosen from
  `$SHELL`, so a zsh login gets `.zshrc` rather than the `.profile` zsh never
  reads. Only the key *name* ever reaches a config, via `requires_env`.

- **The thirteen examples ship inside the binary** and load in one click.
  `tools/sync-examples.sh` copies `config/examples/*.yaml` into the crate, since
  `include_str!` cannot reach above the package root and `config/` is excluded
  from the published tarball. A test fails when the copies have drifted, so a
  stale example is caught by `cargo test` rather than by a user.

- **The logo in the UI** — header, first-run tour, and favicon, all served from
  the binary. `tools/sync-logo.sh` regenerates the crate's copy from
  `assets/loopsmith-logo-256.png`, keying the flat background to alpha: the
  published logo is RGB with no alpha, which is right for a README on GitHub and
  renders as a white tile on the dark theme. The keying is stdlib Python —
  flood-filled inward from the corners so the figure's own white highlights
  survive, which a lightness threshold would punch holes in.

- **A six-step flow instead of one long form.** The first cut put sixteen
  config sections in a single scroll, three columns wide, with nine buttons live
  at the bottom: everything reachable, nothing findable. It is now one step at a
  time — Place, Power, Intent, Proof, Work, Ship — in the order the problem is
  actually thought about, with only the actions that make sense on the current
  step. The step bar shows a red dot behind any step holding an error, so hiding
  a section never hides a problem.

  ⌘K is what makes that reasonable rather than obstructive: a palette over every
  step, section, action, and example, matched as a subsequence so a rough guess
  still lands. A status pill in the header morphs between idle, scanning, and
  running, replacing a console column that was empty most of the time.

  Motion throughout is for orientation, not decoration — the panel morphs height
  and slides in the direction you moved, so going back reads as a return rather
  than a fresh page. All of it degrades to instant under `prefers-reduced-motion`.

- **The OS folder chooser**, on the folder button beside both path fields. The
  browser cannot help here — `showDirectoryPicker()` hands back a handle with no
  filesystem path, deliberately — but the server is a local process on the same
  machine, so it opens the real dialog: `osascript` on macOS,
  `FolderBrowserDialog` on Windows, `zenity` or `kdialog` on Linux. A machine
  with none says so rather than leaving a button that does nothing, and the text
  box has always worked and still does.

- **`web` cargo feature**, on by default. `cargo install loopsmith
  --no-default-features` drops the whole async dependency tree.

### Fixed

- `path_facts` reported an unwritable target whenever the parent directory did
  not exist yet — the ordinary first-loop case, since `loopsmith new` creates the
  whole chain. It now walks up to the first existing ancestor. This blocked the
  Create button for exactly the newcomer the UI is for.

### Notes

- 175 tests in the CLI crate and 387 across the workspace, clippy clean, plus a
  twelve-case Playwright suite driving the real binary.
- New dependencies, both behind the `web` feature: `axum` and `tokio`. The
  WebSocket transport is `axum::extract::ws` — the same RFC6455 the browser
  speaks. The `websocket` crate was considered and rejected: it is published as
  `[deprecated]`, last updated 2024-03, and being synchronous it would need a
  second listener and a blocking thread pool alongside the axum server.
- The frontend adds `motion` for the spring and layout transitions. It is the
  only runtime dependency the UI has beyond React itself.

## [0.1.4] — 2026-08-18

Documentation only. No behaviour, no API, no config-model change.

### Added

- **`README-FOR-DUMMIES.md`** — a start-to-finish guide for the people loopsmith
  is actually most useful to and who bounced off every existing page: marketers,
  salespeople, researchers, analysts. It assumes no shell fluency and no
  programming, and it stops at the point where a loop is running on a schedule.
  Everything about how the tool works internally is deliberately absent.

  It routes those readers down the **Markdown** path rather than the YAML one,
  and by seeding from a shipped example rather than from the blank starter —
  `loopsmith new --config-file <example>.md` writes `loop.md` *and* points
  `run.sh` at it, so the reader never has a second config file to be confused by.
  Scheduling is taught as `interval` seconds; cron is never mentioned, because a
  five-field expression is a wall for this reader and the trigger that avoids it
  already exists.

  It names six sections — **B**, **C**, **D**, **F**, **G**, **H** — as the whole
  editable surface, and reduces detectors to the two that need no programmer:
  `file_exists` and `judge`. It also tells the reader to **delete every
  `type: script` check**, which is the one edit standing between a copied example
  and a config that validates: the examples reference detector scripts the
  repository deliberately does not ship, and a non-programmer has no way to
  discover that from the error.

- A **TL;DR** section at the top of `README.md` and of the crates.io, npm, and
  PyPI READMEs, stating the purpose in plain language and linking to the above.
  On the registry pages that link is pinned to the release tag, for the same
  reason the logo already was: a published README cannot be edited, and a
  `main`-relative link in one will eventually point at something else.

### Changed

- `tools/sync-version.sh` now also rewrites the tag-pinned `README-FOR-DUMMIES.md`
  link in every published README, so the release checklist stays one edit.

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

[0.1.4]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.4
[0.1.3]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.3
[0.1.2]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.2
[0.1.1]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.1
[0.1.0]: https://github.com/bitphill/loopsmith/releases/tag/v0.1.0
