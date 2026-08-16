<div align="center">
  <img src="assets/loopsmith-logo-512.png" alt="loopsmith logo" width="200" />
  <h1>loopsmith</h1>
  <p><em>Self-evolving agent loops. The gate is code, so "done" cannot be argued.</em></p>
  <p>
    <img alt="rust" src="https://img.shields.io/badge/rust-1.75%2B-C1272D?logo=rust&logoColor=white" />
    <img alt="tests" src="https://img.shields.io/badge/tests-240%20passing-2A5A8A" />
    <img alt="license" src="https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222" />
    <img alt="platforms" src="https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A" />
  </p>
  <p>
    <a href="#five-minutes">Five minutes</a> ·
    <a href="#examples">Examples</a> ·
    <a href="#scheduling">Scheduling</a> ·
    <a href="README-DETAIL.md">Full reference</a>
  </p>
</div>

---

You describe a purpose in a config — goals, how each is checked, what counts as
success, when to stop, what the loop may never do. loopsmith handles scheduling,
provider routing, memory, verification, and termination, and can run for weeks
without you.

One rule holds the whole design up:

> A model must not be the thing that certifies its own completion.

`goal_satisfied` is written by a deterministic Rust gate and by nothing else. The
gate can also **revoke** — delete a required artifact and a satisfied goal flips
back. A system that can only promote is a burndown chart with extra steps.

---

## Five minutes

**1. Build and install.**

```bash
export PATH="$HOME/.cargo/bin:$PATH"     # rustup writes to ~/.profile, which zsh never reads
cd runtime && cargo build --release
cp target/release/loopsmith /usr/local/bin/
```

**2. Create a loop.** `--path` is required and must be outside this repository —
a loop edits files and writes state, so it does not get pointed at the tool that
runs it.

```bash
loopsmith new --path ~/loops/nightly-refactor --name nightly-refactor \
  --purpose "keep the module simple without changing behaviour"
```

That writes the config, the directories, an MCP definition, a permission grant,
and `run.sh` / `resume.sh` with absolute paths already filled in.

**3. Configure.** Open `~/loops/nightly-refactor/loop.yaml` and set your goals
and validations. Then export the keys your providers name — loopsmith checks only
that these variables *exist* and never reads their values, so a key cannot reach
a prompt, a log, or the ledger:

```bash
export OPENAI_API_KEY=...
```

> ⚠ Never paste an API key into a chat window, a config file, or an issue. If one
> ends up somewhere it should not be, rotate it — deleting the message is not
> enough.

**4. Check, then run.**

```bash
loopsmith validate ~/loops/nightly-refactor/loop.yaml
loopsmith plan     ~/loops/nightly-refactor/loop.yaml
~/loops/nightly-refactor/run.sh
```

`validate` **fails on purpose** until every `pre_execution` step says
`done: true`:

```
error  pre_execution: 2 step(s) not marked done: Run this task manually end to
       end at least once; Write down what 'done' means in checkable terms.
       Automating before understanding produces fast, confident garbage
```

That refusal is the most valuable thing the tool does. Do the task by hand once —
the manual run *is* the spec.

**5. If it stops early**, resume from the last checkpoint. The run id is printed
at the end of every run and names the file in `logs/`:

```bash
~/loops/nightly-refactor/resume.sh run-1786838400000
```

When a run meets its bar, loopsmith writes `<name>-success/` next to the config:
the configuration that converged, the gate's evidence for it, and the artifacts.

---

## Configs

A config is ten sections, **A** to **J** — information, the manual work list,
goals, validations, success, stop gates, schedules, constraints, execution
guidelines, default skills. Write it as YAML or as Markdown; they are the same
model, and `loopsmith convert` translates either way.

- Section-by-section reference: [`HOW-TO-USE.md`](HOW-TO-USE.md)
- Fill-in template: [`LOOP-TEMPLATE.md`](LOOP-TEMPLATE.md)
- Canonical schema: [`config/loop.schema.json`](config/loop.schema.json)
- Architecture, providers, self-evolution, the reasoning:
  [`README-DETAIL.md`](README-DETAIL.md)

---

## Examples

Thirteen worked loops in [`config/examples/`](config/examples/), each as a
`.yaml` and an equivalent `.md`. All ship with `pre_execution` unfinished, so
`validate` refuses them until you have done the task by hand once.

| Loop | What it does |
|---|---|
| [`research-loop`](config/examples/research-loop.yaml) | Research a question against primary sources, every claim cited |
| [`refactor-loop`](config/examples/refactor-loop.yaml) | Behaviour-preserving refactor where the test suite is the gate |
| [`traffic-loop`](config/examples/traffic-loop.yaml) | Post where an audience already gathers, measured in referred sessions |
| [`trend-radar-loop`](config/examples/trend-radar-loop.yaml) | Track a category across X, Instagram, and TikTok with dated evidence |
| [`landing-page-loop`](config/examples/landing-page-loop.yaml) | A static landing page gated on Lighthouse, page weight, and working CTAs |
| [`sales-leads-loop`](config/examples/sales-leads-loop.yaml) | Build a lead list from permitted sources, with lawful basis recorded per record |
| [`marketing-automation-loop`](config/examples/marketing-automation-loop.yaml) | Turn product docs into scheduled posts, published behind a human checkpoint |
| [`blogger-loop`](config/examples/blogger-loop.yaml) | Write on a trending topic, gated on style measurements and an independent read |
| [`cold-outreach-loop`](config/examples/cold-outreach-loop.yaml) | Personalised first contact, with suppression and opt-out enforced by the gate |
| [`x402-agent-loop`](config/examples/x402-agent-loop.yaml) | An agent that pays for things, supervised or autonomous, under a hard cap |
| [`viral-game-loop`](config/examples/viral-game-loop.yaml) | A small Godot game gated on build health and time-to-first-play |
| [`idea-radar-loop`](config/examples/idea-radar-loop.yaml) | Product ideas traced to dated public complaints, checked against what already sells |
| [`account-watch-loop`](config/examples/account-watch-loop.yaml) | Watch accounts for pre-viral topics, and score yesterday's predictions |

Start one from an example rather than from the blank template:

```bash
loopsmith new --path ~/loops/my-research --name my-research \
  --config-file config/examples/research-loop.yaml
```

---

## Scheduling

A loop that runs once is a script. These are the two ways to make one live:

```bash
# stay resident and run whenever a trigger fires
loopsmith watch ~/loops/nightly-refactor/loop.yaml

# or hand the schedule to the OS so it survives a reboot
loopsmith schedule ~/loops/nightly-refactor/loop.yaml --install
```

Triggers are declared in section **G**: `cron`, `interval`, `file_change`,
`goal_satisfied`, or `manual`. Cron is evaluated in UTC.

Every run writes a plain-text log to `logs/run-<id>.log` alongside the queryable
ledger, so `tail -f` works and `loopsmith ledger` still answers questions.

---

## While it runs

| Question | Command |
|---|---|
| What does the gate say? | `loopsmith status <config> <run-id>` |
| What happened? | `loopsmith ledger <config> <run-id>` |
| Why did it stop? | the last line of `logs/<run-id>.log` |
| What does it want changed about itself? | `loopsmith proposals <config> <run-id>` |
| Which providers can it reach? | `loopsmith providers <config>` |
| Ask the gate right now | `loopsmith gate <config> --target <goal>` |

The loop never edits its own config. Goals, validations, success criteria, and
sub-agent adoption are written as proposals for you to apply.

---

## Licence

MIT. See [`LICENSE`](LICENSE).
