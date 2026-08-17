<div align="center">
  <img src="assets/loopsmith-logo-512.png" alt="loopsmith logo" width="200" />
  <h1>loopsmith</h1>
  <p><em>Self-evolving agent loops. The gate is code, so "done" cannot be argued.</em></p>
  <p><img alt="rust" src="https://img.shields.io/badge/rust-1.75%2B-C1272D?logo=rust&logoColor=white" /> <img alt="tests" src="https://img.shields.io/badge/tests-315%20passing-2A5A8A" /> <img alt="license" src="https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222" /> <img alt="platforms" src="https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A" /></p>
  <p><a href="#five-minutes">Five minutes</a> · <a href="#examples">Examples</a> · <a href="#scheduling">Scheduling</a> · <a href="#portability">Portability</a> · <a href="README-DETAIL.md">Full reference</a></p>
</div>

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

## Five minutes

```bash
export PATH="$HOME/.cargo/bin:$PATH"     # rustup writes to ~/.profile, which zsh never reads
cd runtime && cargo build --release && cp target/release/loopsmith /usr/local/bin/

# --path must be outside this repository: a loop edits files and writes state,
# so it does not get pointed at the tool that runs it.
loopsmith new --path ~/loops/nightly-refactor --purpose "keep the module simple"

cd ~/loops/nightly-refactor
$EDITOR loop.yaml            # your goals, and how each one is checked
loopsmith validate loop.yaml && loopsmith plan loop.yaml && ./run.sh
```

`new` writes the config, the directories, an MCP definition, a permission grant,
`scripts/compat.sh`, and `run.sh` / `resume.sh` with absolute paths filled in.

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
[Schema](config/loop.schema.json) · [Architecture and reasoning](README-DETAIL.md)

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
branching by hand. Everything loopsmith generates is POSIX `sh`, because macOS
still ships bash 3.2.

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
