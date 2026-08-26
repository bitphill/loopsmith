<div align="center">
  <img src="assets/loopsmith-logo-512.png" alt="loopsmith logo" width="200" />
  <h1>loopsmith</h1>
  <p><em>Self-evolving agent loops. The gate is code, so "done" cannot be argued.</em></p>
  <p>
    <a href="https://github.com/bitphill/loopsmith/releases"><img alt="release" src="https://img.shields.io/github/v/release/bitphill/loopsmith?include_prereleases&color=C1272D" /></a>
    <a href="https://github.com/bitphill/loopsmith/stargazers"><img alt="stars" src="https://img.shields.io/github/stars/bitphill/loopsmith?color=2A5A8A" /></a>
    <a href="LICENSE"><img alt="license" src="https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222" /></a>
    <img alt="rust" src="https://img.shields.io/badge/rust-1.75%2B-C1272D?logo=rust&logoColor=white" />
    <img alt="platforms" src="https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A" />
    <img alt="tests" src="https://img.shields.io/badge/tests-402%20passing-2A5A8A" />
  </p>
  <p>
    <a href="https://crates.io/crates/loopsmith"><img alt="crates.io" src="https://img.shields.io/crates/v/loopsmith?logo=rust&logoColor=white&label=crates.io&color=e6522c" /></a>
    <a href="https://www.npmjs.com/package/@bitphill/loopsmith"><img alt="npm" src="https://img.shields.io/npm/v/%40bitphill%2Floopsmith?logo=npm&logoColor=white&label=npm&color=cb3837" /></a>
    <a href="https://pypi.org/project/loopsmith-cli/"><img alt="PyPI" src="https://img.shields.io/pypi/v/loopsmith-cli?logo=python&logoColor=white&label=PyPI&color=3775a9" /></a>
    <a href="https://github.com/bitphill/homebrew-loopsmith"><img alt="Homebrew" src="https://img.shields.io/badge/Homebrew-tap-FBB040?logo=homebrew&logoColor=white" /></a>
  </p>
  <p><a href="#install">Install</a> · <a href="#five-minutes">Five minutes</a> · <a href="#examples">Examples</a> · <a href="#scheduling">Scheduling</a> · <a href="#portability">Portability</a> · <a href="README-DETAIL.md">Full reference</a> · <a href="https://bitphill.github.io/loopsmith/wiki/#overview">Code wiki</a></p>
</div>

---

## TL;DR:

You have a job you redo every week and are fussy about — a competitor roundup, a
lead list, a landing page, a research brief. Write down **what you want** and
**how anyone would tell it's good**, in one plain text file. loopsmith puts an AI
to work on it, checks the result, sends it back when it falls short, and stops
when it passes. It can run on a schedule for weeks without you.

The rule that makes it safe to walk away: **the AI never gets to say "done".** A
deterministic checker reads the actual files and decides — and it can revoke, so
a goal that stops being true stops being satisfied.

**Not a developer?** You don't need to be. Marketing, sales, research, ops — if
you can edit a text file, you can run a loop. And if you would rather not open a
text file at all, `loopsmith --web` builds one for you in a browser.

### ➜ [START-HERE — README-FOR-DUMMIES.md](README-FOR-DUMMIES.md)

A plain-English guide: one install line, thirteen ready-made loops to copy, the
six settings you actually edit, and how to leave it running on a schedule.

---

You describe a purpose in a config — goals, how each is checked, what counts as
success, when to stop, what the loop may never do. loopsmith handles scheduling,
provider routing, memory, verification, and termination, and can run for weeks
without you. One rule holds the whole design up:

> A model must not be the thing that certifies its own completion.

`goal_satisfied` is written by a deterministic Rust gate and by nothing else, and
the gate can **revoke**: delete a required artifact and a satisfied goal flips
back. A system that can only promote is a burndown chart with extra steps.

---

## Install

Pick whichever package manager you already have. All of them put a `loopsmith`
on `PATH`.

```bash
# crates.io — builds from source
cargo install loopsmith

# npm — downloads a prebuilt binary
npm install -g @bitphill/loopsmith

# PyPI — downloads a prebuilt binary on first run
pip install loopsmith-cli

# Homebrew tap — builds from source
brew tap bitphill/loopsmith
brew install loopsmith
```

Or in one line without tapping, using the fully qualified name:

```bash
brew install bitphill/loopsmith/loopsmith
```

Those are the only two forms Homebrew accepts. A two-component
`brew install bitphill/loopsmith` is not a formula reference — Homebrew reads it
as a core formula called `loopsmith` owned by nobody and answers `No available
formula with the name "loopsmith"`.

The npm and PyPI names differ from the command because `loopsmith` was already
registered on both by unrelated projects. crates.io and the binary are both
plain `loopsmith`.

Homebrew 6 added a trust gate on third-party taps, so either form stops with
`Refusing to load formula … from untrusted tap` until you acknowledge it once:

```bash
brew trust bitphill/loopsmith
```

It is per tap, not per formula, and it is Homebrew asking whether you have decided
to run someone else's build instructions. Older Homebrew does not need it.

Or the universal installer, which detects the host and needs no package manager:

```bash
./install.sh      # Linux, macOS, BSD — calls installers/deps.sh
install.bat       # Windows — calls installers/deps.ps1
```

It installs `rustup` and `cargo` if they are missing, ensures `git`, builds in
release mode, puts the binary in `~/.loopsmith/bin/loopsmith`, and symlinks
`/usr/local/bin` if it can write there. Re-runnable, idempotent, logs to
`~/.loopsmith/install.log`.

### Packages

| Registry | Package | Ships |
|---|---|---|
| [crates.io](https://crates.io/crates/loopsmith) | `loopsmith` | source |
| [npm](https://www.npmjs.com/package/@bitphill/loopsmith) | `@bitphill/loopsmith` | prebuilt binary |
| [PyPI](https://pypi.org/project/loopsmith-cli/) | `loopsmith-cli` | prebuilt binary |
| [Homebrew](https://github.com/bitphill/homebrew-loopsmith) | `bitphill/loopsmith/loopsmith` | source |

The eight libraries behind the binary — `loopsmith-util`, `-core`, `-memory`,
`-graph`, `-gate`, `-provider`, `-skills`, `-mcp` — are published on crates.io
too. They compile automatically as dependencies and need no separate install.

---

## The browser UI

```bash
loopsmith --web        # or: loopsmith web
```

Starts a local server on `http://127.0.0.1:3000` and opens a tab. Everything the
CLI does, done by clicking — with every field explained in place, for people who
would rather not learn a schema before they learn whether the tool is useful.

Six steps rather than one long form — Place, Power, Intent, Proof, Work, Ship —
with only the actions that make sense on the step you are on, and `⌘K` to reach
any step, section, action, or example directly.

It probes the machine first, so nothing has to be typed from memory: agent CLIs
on `PATH`, the Ollama models actually pulled, MCP servers already configured by
your editor or desktop app, which API keys are set, and which sub-agents are
installed. The folder button beside either path field opens your operating
system's own folder chooser, so no absolute path has to be typed at all. Found providers become one-click cards that prefill a working argv,
and a **Test** button puts one real prompt through a provider so a wrong flag
surfaces now rather than in iteration four of an unattended run.

The right-hand panel re-checks the draft on every keystroke — the same validator,
planner, and permission derivation the CLI uses, in-process:

- every problem, with the field it belongs to, clickable
- what a run could cost at the ceilings currently set, or **unbounded** if none is
- the wave schedule, the longest chain, and the Amdahl ceiling no worker count beats
- the exact permission grant the loop will need
- parallel builders that would overwrite each other for want of a worktree

All thirteen examples are compiled into the binary and load with one click, which
is the fastest way to read a working config with the explanations attached. The
buttons — check, plan, create, dry run, run, watch, install schedule, grant
permissions — spawn the real `loopsmith` binary and stream its output live, so the
browser can never drift from the CLI and can never do anything `loopsmith --help`
does not list.

API keys go to your shell profile or your operating system's secret store,
whichever you pick. Only the *name* is ever written into a config, which is the
same rule `requires_env` has always had.

| | |
|---|---|
| Binds to | `127.0.0.1` only, never a network interface |
| Port | 3000, stepping up if it is busy — `--port` to choose |
| Runs | by spawning this same binary, never by reimplementing it |
| Needs | nothing installed; the whole UI is compiled in |

Build without it — dropping the async dependency tree entirely — with
`cargo install loopsmith --no-default-features`.

---

## Five minutes

```bash
export PATH="$HOME/.cargo/bin:$PATH"     # rustup writes to ~/.profile, which zsh never reads
cargo install loopsmith                  # or: cd runtime && cargo build --release

# --path must be outside this repository: a loop edits files and writes state,
# so it does not get pointed at the tool that runs it.
loopsmith new --path ~/loops/nightly-refactor --purpose "keep the module simple"

cd ~/loops/nightly-refactor
$EDITOR loop.yaml            # your goals, and how each one is checked
loopsmith validate loop.yaml && loopsmith plan loop.yaml && ./run.sh   # run.cmd on Windows
```

`new` writes the config, the directories, an MCP definition, a permission grant,
`scripts/compat.sh`, and `run.sh` / `resume.sh` / `run.cmd` / `resume.cmd` with
absolute paths filled in. Both script flavours are written on every platform, so
the directory still starts after it moves to a different kind of machine.

`validate` **fails on purpose** until every `pre_execution` step says
`done: true`:

```
error  pre_execution: 2 step(s) not marked done: Run this task manually end to
       end at least once; Write down what 'done' means in checkable terms.
       Automating before understanding produces fast, confident garbage
```

That refusal is the most valuable thing the tool does. Do the task by hand once —
the manual run *is* the spec.

Export the keys your providers name under `requires_env`. loopsmith checks only
that these variables *exist* and never reads their values, so a key cannot reach
a prompt, a log, or the ledger.

> ⚠ Never paste an API key into a chat window, a config file, or an issue. If one
> ends up somewhere it should not be, rotate it — deleting the message is not enough.

If a run stops early, `./resume.sh <run-id>` continues from the last checkpoint.
When one meets its bar, loopsmith writes `<name>-success/` beside the config: the
configuration that converged, the gate's evidence, and the artifacts.

---

## Configs

Ten sections, **A** to **J** — information, the manual work list, goals,
validations, success, stop gates, schedules, constraints, execution guidelines,
default skills. Write YAML or Markdown; they are the same model, and
`loopsmith convert` translates either way.

[Section reference](HOW-TO-USE.md) · [Template](LOOP-TEMPLATE.md) ·
[Schema](config/loop.schema.json) · [Architecture and reasoning](README-DETAIL.md) ·
[Code wiki](https://bitphill.github.io/loopsmith/wiki/#overview)

---

## Examples

Thirteen worked loops in [`config/examples/`](config/examples/), each as a `.yaml`
and an equivalent `.md`, all shipping with `pre_execution` unfinished. Annotated
index in [`README-DETAIL.md`](README-DETAIL.md#the-examples).

**Build** [`refactor`](config/examples/refactor-loop.yaml) ·
[`landing-page`](config/examples/landing-page-loop.yaml) ·
[`viral-game`](config/examples/viral-game-loop.yaml) —
**Find out** [`research`](config/examples/research-loop.yaml) ·
[`trend-radar`](config/examples/trend-radar-loop.yaml) ·
[`idea-radar`](config/examples/idea-radar-loop.yaml) ·
[`account-watch`](config/examples/account-watch-loop.yaml) —
**Reach people** [`traffic`](config/examples/traffic-loop.yaml) ·
[`blogger`](config/examples/blogger-loop.yaml) ·
[`cold-outreach`](config/examples/cold-outreach-loop.yaml) ·
[`sales-leads`](config/examples/sales-leads-loop.yaml) ·
[`marketing-automation`](config/examples/marketing-automation-loop.yaml) —
**Spend money** [`x402-agent`](config/examples/x402-agent-loop.yaml)

Start from one with `loopsmith new --path … --config-file <example>` rather than
from the blank template.

---

## Scheduling

A loop that runs once is a script. Two ways to make one live:

```bash
loopsmith watch    ~/loops/nightly-refactor/loop.yaml            # stay resident
loopsmith schedule ~/loops/nightly-refactor/loop.yaml --install  # hand it to the OS
```

Triggers are declared in section **G**: `cron`, `interval`, `file_change`,
`goal_satisfied`, or `manual`. Cron is evaluated in UTC. `schedule` uses whichever
scheduler this machine actually has, not whichever its OS is famous for. Every run
writes a plain-text log to `logs/run-<id>.log` beside the queryable ledger, so
`tail -f` works and `loopsmith ledger` still answers questions.

---

## Portability

Detector scripts get written on one machine and run on another. `loopsmith doctor`
reports what this one is and what that stops you doing — which bash, GNU or BSD
userland, which scheduler, which detectors it cannot run.

Every new loop ships `scripts/compat.sh`: source it in a detector and use `sed_i`,
`stat_size`, `readlink_f`, `sha256`, `require`, and `need_bash` rather than
branching by hand. Every `.sh` loopsmith generates is POSIX, because macOS still
ships bash 3.2.

| | Linux | macOS | Windows |
|---|---|---|---|
| Launchers a new loop gets | `run.sh`, `resume.sh` (**and** the `.cmd` pair) | same | `run.cmd`, `resume.cmd` (**and** the `.sh` pair, under Git Bash or WSL) |
| Scheduler `schedule` hands the loop to | `crontab`, else `systemctl` | `launchctl`, else `crontab` | `schtasks`, else `crontab` |
| Home directory | `HOME` | `HOME` | `HOME`, else `USERPROFILE`, else `HOMEDRIVE`+`HOMEPATH` |
| `compat.sh` helpers | native | native | under Git Bash, WSL, or MSYS |

Nothing is decided at build time. The userland is probed by asking `sed` for a
version — Homebrew's coreutils puts GNU tools ahead of BSD ones on a Mac, so the
operating system does not imply the answer — and the scheduler is whichever
candidate is actually on `PATH`, not whichever one the OS is famous for.

CI runs the full suite on `ubuntu-latest`, `macos-latest`, and `windows-latest`,
so the three-platform claim above is something that gets checked rather than
something written down.

---

## While it runs

| Question | Command |
|---|---|
| What does the gate say? | `loopsmith status <config> <run-id>` |
| What happened? | `loopsmith ledger <config> <run-id>` |
| Why did it stop? | the last line of `logs/<run-id>.log` |
| What does it want changed about itself? | `loopsmith proposals <config> <run-id>` |
| Which providers can it reach? | `loopsmith providers <config>` |
| Will this machine get in the way? | `loopsmith doctor <config>` |
| Ask the gate right now | `loopsmith gate <config> --target <goal>` |

The loop never edits its own config. Goals, validations, success criteria, and
sub-agent adoption are written as proposals for you to apply.

MIT licensed. See [`LICENSE`](LICENSE).
