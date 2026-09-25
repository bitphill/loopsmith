# How to use the loop template

`LOOP-TEMPLATE.md` is the authoring surface. This document explains what it
produces, how the pieces fit, and what every configuration field is for.

**Contents**

1. [Architecture](#1-architecture)
2. [The two skills](#2-the-two-skills)
3. [Title and summary conventions](#3-title-and-summary-conventions)
4. [Making a purpose-specific loop](#4-making-a-purpose-specific-loop)
5. [Configuration reference, section by section](#5-configuration-reference-section-by-section)
6. [Providers and BYOK](#6-providers-and-byok)
7. [The permission preflight](#7-the-permission-preflight)
8. [Sub-agent acquisition](#8-sub-agent-acquisition)
9. [Reading the memory ledger](#9-reading-the-memory-ledger)
10. [Failure playbook](#10-failure-playbook)
11. [Self-evolution and the proposals directory](#11-self-evolution-and-the-proposals-directory)
12. [Promotion path](#12-promotion-path)
13. [Running for weeks](#13-running-for-weeks)
14. [Where the design came from](#14-where-the-design-came-from)

---

## 1. Architecture

Three planes. The split exists because the corpus this is built on is unanimous
on one point: a model must not be the thing that certifies its own completion.

```
INVOCATION      /loopsmith  or  loopsmith loop new --path <dir>
                  └─ permission preflight (one grant) → hands-off
                       │
CONTROL PLANE   loopsmith (Rust)                  ← owns truth
                  ├─ core      the four-bundle config model and validation
                  ├─ graph     DAG, waves, critical path, Amdahl sizing
                  ├─ memory    sled: episodes, goal state, ledger, checkpoints
                  ├─ gate      deterministic verdicts — the ONLY writer of
                  │            goal_satisfied, and able to revoke it
                  ├─ provider  command-template routing, token/cost accounting
                  ├─ skills    acquire, trial, rank, propose
                  ├─ run       the run lifecycle: validate, plan, dispatch, close
                  ├─ wizard    the interview, as data both front ends render
                  ├─ web       the browser UI, which spawns this same binary
                  └─ mcp       stdio server exposing plan, ledger, gate, pad
                       │
EXECUTION       Any provider                       ← owns judgment
                  Claude Code · Ollama · Grok · OpenAI · Gemini · Hermes ·
                  any BYOK command · any MCP server
```

**Why the orchestrator is Rust and not a session.** A loop must survive a
crash, a schedule boundary, and a budget ceiling. Sessions are ephemeral and
have no `/resume`; a sled ledger does. Coordination is also a solved
deterministic problem — spending model tokens on scheduling is the same mistake
as spending frontier reasoning on entity extraction.

**What the gate being code buys you.** `goal_satisfied` cannot be set by a
prompt, a confident summary, or a model that likes its own work. It is written
by `loopsmith-gate` after running detectors, and re-running the gate on fresh
evidence can flip a satisfied target back. That is the whole trust model.

---

## 2. The two skills

Both live in `skills/`. They are split by invocation, which is a real
architectural choice rather than tidiness:

| Skill | Invocation | Context cost | Why |
|---|---|---|---|
| `loopsmith` | User-invoked (`disable-model-invocation: true`) | Zero | Starting a loop spends real money. It should be a decision, not an inference. |
| `loopsmith-reference` | Model-invoked (carries a description) | Always-loaded description | The design principles are useful whenever anyone builds *any* iterative agent system, so the agent should be able to reach them unprompted. |

Install by copying either directory into `~/.claude/skills/` (all projects) or
`.claude/skills/` (this project only).

---

## 3. Title and summary conventions

The `description` field is the entire triggering mechanism — it is the only
part always in context. Rules that matter:

- **Say what it does *and* when to use it.** All "when to use" information goes
  in the description, never in the body, because the body is not loaded until
  after the decision to load it has been made.
- **Cap: 1,536 characters** for `description` plus `when_to_use` combined.
  Anything past that is truncated in the skill listing.
- **Model-invoked skills undertrigger.** Be concrete about the situations, and
  include cases where the user would not name the skill.
- **User-invoked skills strip the description** to a human-facing one-liner —
  nothing but you can reach them, so trigger lists are wasted words.
- **Keep the body under 500 lines.** Once loaded it stays in context across
  turns, so every line is a recurring cost. Push detail into sibling files and
  point at them.

---

## 4. Making a purpose-specific loop

```bash
loopsmith loop new --path ./loops/nightly-refactor --purpose "keep the module simple"
```

`--path` / `-p` is mandatory. A loop owns a ledger, checkpoints, outputs, and a
quarantine directory; without its own home you get several half-finished loops
writing into each other's state.

What lands:

```
nightly-refactor/
├── loop.yaml            the config: four bundles, eight top-level keys
├── README.md            how to run this specific loop
├── .gitignore           state/, out/, generated-skills/ are not source
├── state/               sled: episodes, goal state, ledger, checkpoints
├── out/                 deliverables
├── proposals/           changes the loop wants to make to its own goals
└── generated-skills/    auto-acquired sub-agents awaiting promotion
```

The scaffolded config ships with `pre_execution` steps set to `done: false`, so
`loopsmith loop validate` **fails on purpose** until you have done the manual run.

---

## 4b. The browser UI

```bash
loopsmith --web        # identical to: loopsmith web
```

Both spellings exist and neither is the real one. `--web` is what people reach
for; a subcommand is what the rest of this grammar looks like. `--web` combined
with any subcommand is refused rather than silently resolved.

Serves `http://127.0.0.1:3000`, stepping up a port at a time if that is busy, and
opens a browser tab. `--no-open` prints the URL instead; `--port` picks a
starting port.

### Two ways in

Before anything else it asks which kind of smith you are, and remembers the
answer:

- **experienced** — straight to the six-step editor (Place, Power, Intent, Proof,
  Work, Ship), every section reachable at once. Within a step the cards are
  grouped under the bundle they write — `intent`, `execution`, `safety`,
  `evolution` — and each card says the dotted path it edits, so the form and the
  YAML are navigable by the same names.
- **new** — the explanation, then the shipped examples to start from or an empty
  config, then the **same walk-through `--guided` runs in a terminal**: one field
  per card, in the order `loopsmith-wizard` publishes, each optional section
  behind its own opt-in card, and a repeating section collecting entries behind a
  `+` until you say "This part is done". Both front ends render the one spec the
  wizard crate serves at `/api/wizard/spec`, so neither can ask a question the
  other does not.
- **show me one running** — a third door that builds a loop in a temporary
  directory and dry-runs it. Real scheduling, real gate, real ledger, and no
  provider called. Nothing of yours is touched and nothing is spent.

The two are views of one draft, not two drafts. **Expert editor** on any card
hands what is filled in so far to the six-step form, `⌘K` switches back, and the
review rail runs the real validator across both. Create stays gated on that
validator reporting no errors, which is the same gate the terminal wizard applies
before it writes a file.

### What it is, structurally

Three properties hold, and each is load-bearing:

| Property | Why |
|---|---|
| Binds `127.0.0.1` only | It spawns commands as this user. An interface bind would hand that to the network. |
| Every action spawns `current_exe()` | The browser cannot drift from the CLI, and cannot do anything `loopsmith --help` does not list. |
| The frontend is compiled in | Someone who installed from a registry has no checkout; a UI that only works beside its own source is one most users never see. |

The browser names a **verb** from a closed list and its parameters. It never
names a program to run. That is the difference between a control panel and a
remote shell.

### What it computes in-process

The right-hand rail re-runs on every edit, calling the same crates the CLI does:

- `loopsmith_core::validate` — every issue, with its dotted field path
- `loopsmith_graph::plan` — waves, critical path, Amdahl ceiling, chosen concurrency
- `loopsmith_graph::unisolated_parallel_writers` — builders that would clobber each other
- the permission derivation from §7
- an upper-bound cost from iterations × nodes × the priciest reachable provider

None of it spawns a process, so it answers in under a millisecond and can run on
every keystroke.

### Detection

Probing is free by default: `which` plus a `--version` bounded at six seconds,
run concurrently. The budget is generous because a Node-based CLI with a cold
module cache can take several seconds to print its own version on the first scan
after a reboot — and that first scan is the one a new user sees.

Detected: agent CLIs on `PATH`, Ollama models via `ollama list`, MCP servers from
`~/.claude.json`, `~/.claude/settings.json`, Claude Desktop, `~/.cursor/mcp.json`,
VS Code and `./.mcp.json`, which API keys are present (presence only — values are
never read), installed sub-agents, git, and the platform facts `doctor` reports.

Codex keeps its MCP servers in TOML. That file is named in a note rather than
parsed: a TOML dependency for one file is not a trade worth making.

A **Test** button per provider performs a real handshake — one prompt, one round
trip. It is a button rather than part of detection because a page load is not
consent to spend money.

### Secrets

Two stores, and the trade is stated rather than hidden:

- **Shell profile** — a real environment variable every tool on the machine sees.
  Plaintext on disk, mode `0600`, inside a fenced block that is rewritten in
  place. The file is chosen from `$SHELL`, so a zsh login gets `.zshrc` and not
  `.profile`, which zsh never reads.
- **OS secret store** — Keychain, Credential Manager, or libsecret. Nothing in a
  dotfile; only loopsmith-started runs see the value.

Either way the config records the key **name** only, in `requires_env`, which is
the rule that section already had.

### The example library

All thirteen `config/examples/*.yaml` are compiled in with `include_str!`, since
`include_str!` cannot reach above the package root and `config/` is excluded from
the published tarball. `tools/sync-examples.sh` copies them into
`runtime/crates/loopsmith-web/templates/examples/`, and a test fails if the two
have drifted — so a stale copy is caught by `cargo test`, not by a user.

A user's own `~/.loopsmith/examples/*.yaml` take priority, and a checkout's
`config/examples/` is read live so edits show up without a rebuild.

### Building it

The frontend is React 19 + Vite + Tailwind v4, emitted to fixed filenames
(`index.html`, `app.js`, `app.css`) because a content hash cannot be chased by an
`include_str!` literal.

```bash
npm --prefix runtime/crates/loopsmith-web/web install
npm --prefix runtime/crates/loopsmith-web/web run build   # writes src/dist/
cargo build -p loopsmith --release
```

`npm run dev` serves on 5173 and proxies `/api` to a `loopsmith web --no-open`,
so the UI can be iterated on without a Rust rebuild. `npm run test:e2e` drives
the real binary through Playwright.

The whole thing sits behind a default-on `web` feature. To drop the async
dependency tree entirely: `cargo install loopsmith --no-default-features`.

---

## 5. Configuration reference, section by section

Validated against `config/loop.schema.json`, which is generated from the Rust
types rather than written by hand. Cross-field rules a JSON Schema cannot
express — a goal with no blocking check, a judge that would grade its own
provider, a randomness threshold above the no-progress cap — are enforced by
`loopsmith loop validate`.

**The shape.** Eight top-level keys. Four of them are bundles that group the
sections by what they are *for*, and the order below is the order of the model
itself: what the loop is for, how the work gets done, what must not happen, and
how the loop may change itself.

```yaml
name: nightly-refactor
version: 1.0.0
description: Keep the hot modules under the complexity budget.
environment: dev            # dev | staging | prod
features: {...}             # what this loop may do at all

intent:                     # what the loop is for
  background: []            #   facts every node is given
  prerequisites: []         #   the manual work you did first
  goals: []                 #   what you want
  success: []               #   how much of it has to pass

execution:                  # how the work gets done
  graph: {...}              #   nodes, edges, concurrency, join
  providers: {...}          #   which models, and the cascade
  phases: {...}             #   named stages and their order
  default_skills: []        #   sub-agents installed up front
  skills: {...}             #   where a new sub-agent may come from
  memory: {...}             #   what each prompt carries forward
  triggers: {...}           #   what makes this loop start

safety:                     # what must not happen, and when to stop
  checks: []                #   how each goal is verified
  gates: {...}              #   stop / entry / approval / rollback
  limits: {...}             #   rules, forbidden paths, ceilings
  recovery: {...}           #   failure class → action
  protected: {...}          #   what self-evolution may never touch
  alerts: []                #   numbers worth being told about

evolution: {...}            # how the loop improves itself
```

**Every 0.3 config still parses.** The lettered keys (`information`,
`pre_execution`, `goals`, `validations`, `success`, `stop_gates`, `schedules`,
`constraints`, `execution_guidelines`, `default_skills`) are relocated into the
bundles by one table as the file is read, with a warning naming each key that
moved. `loopsmith loop migrate --write` rewrites a file using that same table,
so the migrator cannot disagree with the parser. See
[Migration 0.3 → 1.0](wiki/Migration-0-3-To-1-0.md) for the full mapping.

---

### `intent.background` · Information

Static facts every node receives. Nodes start fresh with only their spawn
prompt, so anything not here gets rediscovered badly by each of them.

Fields: `key`, `value`, optional `note`.

Keep it to things that stay true. Anything that changes during a run belongs in
the run's own memory, not here.

### `intent.prerequisites` · Pre-execution work

The manual work list. Fields: `step`, `done`, optional `evidence`. **Every step
must be `done: true` or validation fails.** This is the only place the tool
refuses on process rather than syntax, and it is deliberate: automating a
process you have never performed does not save you the work, it produces the
wrong result faster and at a scale that is harder to undo.

### `intent.goals` · Goals

Fields: `name` (never `overall`, which is reserved for the loop as a whole),
`description`, optional `depends_on`, optional `priority`. Subjective phrasing
is fine here — the check is what has to be decidable.

### `intent.success` · Success scenarios

Fields: `target`, `name`, `mode` (`subjective` | `objective` | `percentage`),
`statement`, `threshold` (required for `percentage`: the fraction of blocking
checks that must pass, 0.0–1.0).

Checks say what is verified; success says how much of it has to pass. Without a
success scenario the loop runs to its iteration ceiling even after it has
already done the job.

---

### `execution.graph` · Nodes and dependencies

Nodes: `id`, `role` (`builder` | `judge` | `manager` | `adversary` |
`researcher`), `instruction` (minimum 16 characters), `depends_on`, `goals`,
`tier`, `provider`, `stage`, `skills`, `weight`, `isolation`.

Only list a dependency whose output the node actually reads. An "and then" that
is not read is not an edge, and every false edge makes the loop slower for
nothing — loopsmith derives the parallel schedule from these edges.

**Concurrency** is `sequential`, `fixed` (`max_parallel`), or `auto` (`cap`,
`min_marginal_gain`). `auto` derives the parallel fraction from the graph and
adds workers only while the next one buys `min_marginal_gain` of additional
Amdahl speedup.

**Join** decides what it takes for a wave to count as finished:

| `join.strategy` | Releases the wave when |
|---|---|
| `wait_for_all` | every node in it has finished (the default) |
| `quorum` | `count` nodes have succeeded; the rest are left to finish |
| `first_success` | any one node succeeds |

Use a quorum when several nodes attack the same question and the run does not
need all the answers to proceed.

**Isolation** is per node, `isolation.mode`:

| Mode | What it gets |
|---|---|
| `none` | the loop directory itself — correct for a single writer or a read-only node |
| `worktree` | its own git worktree, published back on success — required for parallel writers |
| `container` | a container over its own worktree, with `image` and `network` (off by default) |

`graph.container_image` sets the default image for container nodes that name
none. A machine with no Docker degrades to `worktree` with a warning rather
than failing: `loopsmith doctor` reports which of the three this host can
actually give you.

Two builders writing the same files in the same wave, neither isolated, will
overwrite each other. `loopsmith loop plan` flags exactly that.

### `execution.providers` · Providers

`providers` is the list, `cascade` orders them per tier, and
`enforce_judge_independence` (on by default) refuses a judge that would run on
the same provider as the work it grades.

Per provider: `id`, `kind`, `tiers`, `command`, `args`, `model`, `requires_env`,
`timeout_seconds`, `prompt_on_stdin`, `usage_regex`.

Every provider is a command template, which is why any CLI on this machine can
serve a loop without a code change. See §6.

### `execution.phases` · Execution guidelines

Named **phases**, each with a standing instruction and a place in an ordering.
Nodes join a phase with `stage:`, and a node is not dispatched until its phase
is active.

```yaml
execution:
  phases:
    items:
      - name: gather
        guideline: Collect sources. Write nothing yet.
      - name: draft
        guideline: Write only from what gather collected.
    dependency:
      - gather -> draft -> review     # chains are allowed
```

Use this for ordering that is about **method**; `depends_on` is only for a node
that genuinely reads another node's output. Overloading `depends_on` with both
makes the critical path meaningless.

A phase opens when everything before it is complete, and completes when its own
nodes have run and the gate has satisfied the goals they advance. A phase with
no nodes gates nothing and completes on sight. Guidelines with no arrow between
them run in parallel. Cycles and unknown names are validation errors, caught
before anything is dispatched.

### `execution.default_skills` · Sub-agents

Sub-agents installed before the loop starts. Idempotent, so it runs at the start
of every run and a loop directory can be rebuilt from its config alone.

```yaml
execution:
  default_skills:
    - name: agent-reach
      source: github                  # marketplace | github | local
      url: https://github.com/Panniantong/agent-reach
      init_command: npm install       # ARGV, not a shell line
```

`github` clones an **https** repo into the quarantine directory — `git://`,
`ssh://` and `file://` are refused. `init_command` is split on whitespace and
executed directly, so `&&`, `|` and `$(…)` are literal arguments rather than
shell syntax. `loopsmith skills install <config>` runs this without starting a
run.

### `execution.skills` · How sub-agents are acquired

What happens when a node wants a specialist this machine does not have.

`acquisition_order` (`installed` | `marketplace` | `generate`, in the order to
try), `quarantine_dir`, `min_marketplace_stars`, `require_human_promotion`,
`min_trust_level`, `require_checksum`, `allow_external_side_effects`,
`explore`, `explore_candidates`, `min_trials`.

Anything acquired at run time lands in the quarantine directory and stays there
until a person promotes it. Marketplace acquisition with no star floor and no
human promotion is an unreviewed dependency running with your credentials.

### `execution.memory` · Carried context

How much of the previous iterations each prompt carries, and what the loop is
allowed to remember across runs.

`carry_summaries` (default 2, `0` disables), `summary_provider` (optional — the
deterministic facts are always written, this only buys prose),
`max_summary_chars` (default 1200), `max_retrieved`.

**Namespaces.** Four of them, each with its own `enabled`, `retention_days`,
`promotion`, `min_confidence` and `require_provenance`:

| Namespace | Holds | Written by |
|---|---|---|
| `episodic` | what happened in this run | the engine |
| `semantic` | facts learned about the domain | agents, via MCP `loopsmith_remember` |
| `procedural` | procedures that worked | the engine |
| `failure` | what went wrong and how it was recovered | the engine |

`promotion` is `never`, `automatic`, or `human_approval` — whether a memory in
that namespace is allowed to graduate into the next run's context.
`loopsmith memory list | promote | forget` is the surface for all of it.

### `execution.triggers` · Schedules

`triggers` is the list; each entry is a `trigger` plus an optional
`idempotency_key` and an `enabled` flag. `max_depth` caps how deep one run may
trigger another, and `dedup_window_seconds` is how long an idempotency key
suppresses a repeat.

Trigger kinds: `manual`, `cron` (`expr`), `interval` (`seconds`), `file_change`
(`path`), `goal_satisfied` (`goal`). Schedule last, after the loop is reliable
by hand.

**Cron is evaluated in UTC.** Deriving a correct local offset in a
multithreaded process is unsound on Unix without care, and a scheduler quietly
an hour off twice a year is worse than one honestly in UTC. For plain cadence,
`interval` avoids the question entirely.

`file_change` and `goal_satisfied` fire on the **edge**, not the level: a goal
that stays satisfied does not retrigger, and the watcher skips its own `state/`
directory so ledger writes cannot retrigger it.

`loopsmith run watch` keeps loopsmith resident and fires these while it runs;
`loopsmith run schedule` hands the job to launchd or cron so it survives a
reboot.

---

### `safety.checks` · Validations

Fields: `target` (goal name or `overall`), `name`, `mode`
(`subjective` | `objective` | `percentage`), `statement`, `blocking` (default
true), `detector`.

Detectors, strongest first:

| Type | Passes when | Notes |
|---|---|---|
| `script` | Command exits with `expect_exit` (default 0) | Prefer this |
| `file_exists` | Path exists, optionally non-empty | |
| `regex_match` | Pattern matches a named artifact | |
| `threshold` | Reported metric satisfies `op` vs `value` | Missing metric fails closed |
| `judge` | Model verdict against a **named** `standard` | Weakest rung |

A `judge` verdict from the same provider that produced the work is **refused**,
not discounted, when `enforce_judge_independence` is on. A missing judgment
fails closed rather than passing by default.

**Every goal needs at least one blocking check**, or the config is rejected — a
goal that cannot be verified can never be honestly finished. That refusal is the
feature.

`regex_match` reads the files your `file_exists` detectors name, under either the
full path or the file's stem. So `detector: { type: file_exists, path: out/notes.md }`
makes `artifact: notes` and `artifact: out/notes.md` both work, and a regex
naming anything else is rejected by `loopsmith loop validate` rather than failing
closed for the life of the loop.

#### Writing a detector that runs on more than one machine

A detector runs with **no shell**: `command` is argv[0] and `args` are literal,
so `&&`, `|`, and `$(…)` are arguments rather than syntax. Write a real file with
a real shebang.

Three differences break detectors when a loop directory moves, and every new loop
ships `scripts/compat.sh` to absorb them:

```sh
#!/bin/sh
. ./scripts/compat.sh

require jq                       # exit 2 when a tool is missing
sed_i 's/draft/final/' out/x.md  # `sed -i` vs `sed -i ''`
[ "$(stat_size out/x.md)" -gt 0 ] || exit 1
```

| Helper | Absorbs |
|---|---|
| `sed_i` | GNU `sed -i` takes no argument; BSD requires one |
| `stat_size`, `stat_mtime` | `-c%s` on GNU, `-f%z` on BSD |
| `readlink_f` | `readlink -f` is absent from BSD before macOS 12 |
| `sha256` | `sha256sum` on Linux, `shasum -a 256` on macOS |
| `require` | Names the missing command instead of failing obscurely |
| `need_bash 4` | macOS ships bash 3.2, so `${x,,}`, arrays, and `mapfile` are absent |

`require` and `need_bash` exit **2**, not 1. A detector's exit code is its
verdict, and "this machine cannot run the check" is a different fact from "the
check failed" — a gate that cannot tell them apart reports missing tooling as
unfinished work. Run `loopsmith doctor <config>` to see what this machine is and
which of your detectors it cannot run.

### `safety.gates.stop` · Stop gates

Every way this loop is allowed to end. All checked every iteration, any one of
which halts the run.

`max_iterations`, `max_revisions_per_node`, `max_wall_clock_seconds`,
`max_tokens`, `max_cost_usd`, `no_progress_iterations` (0 disables),
`no_progress_iterations_randomness`, `stop_on_overall_success`.

Declare at least one budget ceiling; validation warns if you declare none. A
loop with only an iteration cap and an expensive provider is an unbounded bill
waiting for a slow night.

**`no_progress_iterations_randomness`** fires *before* `no_progress_iterations`
and must be strictly less than it — otherwise the loop halts before it ever
tries something different, and validation refuses the config.

When it fires, a cheap-tier agent is shown the failing checks and the recent
iteration summaries, and picks one of exactly four tactics: `reorder`,
`escalate`, `explore`, or `reframe`. The menu is fixed, so the agent can change
how the loop works and cannot change what counts as done. If no cheap provider is
reachable, or the answer is not on the menu, a seeded fallback picks instead. The
seed is derived from the run id and iteration and written to the ledger, so a run
that took a strange turn replays exactly.

### `safety.gates.entry` · Entry gates

What must already be true before the first node is dispatched. Checked once,
while the run is still validating and before a single provider is called.

Each rule: `id`, `statement`, `detector`, `on_fail` (`stop` | `escalate` |
`pause` | `rollback` | `warn`).

The clean branch, the key that answers, the disk with room on it — the
conditions that make the whole run pointless if they are false. A failing entry
gate stops the run at the cheapest place a run can stop.

### `safety.gates.approval` · Approval gates

What has to be signed off before the loop is allowed to start working. Same
rule shape as entry gates, checked after planning and before the first dispatch
— so the plan is on the table when the decision is made.

These are for work whose cost of being wrong is external: money moving, mail
leaving, something published. The run waits rather than guessing, and says so.
An approval gate whose detector can satisfy itself is not an approval, it is a
delay.

### `safety.gates.rollback` · Rollback gates

What, if it becomes true mid-run, means the last iteration should be undone.
Same rule shape, checked every iteration like a stop gate but with a different
answer: a rollback discards the work of the iteration that tripped it and
records why.

Spend is not refunded — nothing can refund that — so these are about not
building on top of a bad iteration rather than about saving money.

### `safety.limits` · Constraints

`global` plus `per_node` overrides. Merge semantics: **rules append, limits
override**. Fields: `rules`, `forbidden_paths`, `forbidden_commands`,
`max_tokens`, `max_seconds`, `human_checkpoint`.

`human_checkpoint` stops and waits regardless of any permission grant. Anything
irreversible — sending mail, spending money, publishing, deleting — belongs
there. A hands-off loop with no checkpoints is not hands-off, it is
unsupervised.

### `safety.recovery` · Recovery

What the loop does about each kind of failure, decided before it happens. Seven
named classes, each mapped to one action:

| Failure class | Default action |
|---|---|
| `transient_error` | `retry` with backoff |
| `invalid_output` | `revise` |
| `tool_unavailable` | `fallback` |
| `repeated_failure` | `escalate` |
| `safety_violation` | `stop` |
| `resource_exhaustion` | `pause` |
| `corrupted_state` | `restore_checkpoint` |

Actions are `retry` (with `max_attempts` and `backoff`: `fixed` | `linear` |
`exponential`), `revise`, `fallback`, `escalate`, `pause`, `restore_checkpoint`,
`stop`. The defaults are deliberately unequal: a transient error is retried, a
safety violation never is. One policy for every failure either retries a safety
violation or gives up on a flaky network.

### `safety.alerts` · Alerts

Numbers worth being told about while the run is still going. Each alert: `id`,
`metric`, `above` and/or `below`, optional `message`.

Metrics: `iterations`, `tokens_used`, `cost_usd`, `wall_clock_seconds`,
`failed_dispatches`, `retries`, `stale_iterations`, `validation_pass_rate`.

An alert stops nothing; that is what stop gates are for. It is the thing that
says a run is going wrong an hour before the ceiling would have said it.

### `safety.protected` · Protected components

What self-evolution may never touch, whatever it proposes. `components` names
them; `extra_paths` adds files by path.

Components: `gates`, `limits`, `recovery`, `protected`, `approvals`,
`credentials`, `audit`, `baselines`, `retention`, `environment`.

A proposal that would rewrite one of these is rejected before it is evaluated
rather than after. A loop allowed to edit its own limits does not have limits.

---

### `evolution` · Self-evolution

Off by default. With `enabled: true` the loop may propose changes of the kinds
in `allowed_kinds` and no others, each measured against `baseline`.

`enabled`, `baseline`, `max_regression`, `allowed_kinds`, `require_sandbox`,
`require_approval`, `keep_rollback`.

`baseline` records `completion_rate`, `validation_pass_rate`, `cost_usd`,
`latency_seconds`, `iterations_to_success` and `measured_at`. A proposal that
regresses any of them by more than `max_regression` is refused by the gate
rather than by a reviewer's patience. An evolution with no baseline has nothing
to be better than, so every proposal looks like an improvement.

`allowed_kinds`: `new_skill`, `skill_update`, `prompt_change`, `graph_change`,
`provider_routing`, `validation_change`, `success_criteria`.

Proposals stay proposals. loopsmith will not apply one on its own — see §11.

---

### `features` · What this loop may do at all

Five switches, above the bundles rather than inside one, because they are
capabilities rather than settings.

| Switch | Default | What turning it on means |
|---|---|---|
| `self_evolution` | off | the loop may propose changes to itself |
| `marketplace_skills` | off | it may acquire sub-agents it did not ship with |
| `external_side_effects` | off | a node may reach outside the loop's own directory |
| `parallel_execution` | on | waves run wide; turning it off is the first thing to try when a run behaves differently under load |
| `human_approval` | on | checkpoints actually stop — **refused outright** when `environment: prod` |

Turning off `human_approval` makes every checkpoint in the config decorative,
including the ones guarding something irreversible. That is why production
refuses it.

### `environment` · Where this is running

`dev` (the default), `staging`, or `prod`. It is not decoration: `prod` refuses
`features.human_approval: false`, and it is one of the protected components, so
self-evolution cannot move a loop out of production to get around a rule.

---

## 6. Providers and BYOK

Every provider is a command template. That single decision is what makes BYOK
free: Claude Code, Ollama, a Grok CLI, an OpenAI-compatible endpoint driven by
`curl`, an MCP server over stdio — all of them are "a program you run with a
prompt". Adding one is a config edit, never a rebuild.

```yaml
- id: openai
  kind: openai              # aliases accepted: open_ai, OpenAI
  tiers: [strong]
  command: curl
  args: ["-sS", "https://api.openai.com/v1/chat/completions",
         "-H", "Authorization: Bearer $OPENAI_API_KEY", "-d", "@-"]
  requires_env: [OPENAI_API_KEY]
  prompt_on_stdin: true
```

Placeholders: `{prompt}` `{system}` `{model}` `{tier}` `{node}`.

**If you use `ollama`, pull the model first.**

```bash
ollama pull llama3
```

`ollama run <model>` downloads a missing model, and from outside the process
that is indistinguishable from a slow generation. The starter `ollama` provider
sits at `timeout_seconds: 120` so a cascade abandons it and tries the next
provider rather than spending its whole budget on a download — which is exactly
what one observed run did before this was lowered.

**Secrets never enter the process.** `requires_env` names keys that must exist;
values are never read, substituted, or logged. Let the command expand them
itself, as `curl` does above.

**Cascade.** Each tier resolves to an ordered list; the first provider whose
binary exists and whose environment is complete serves the call, and skipped
providers are recorded in the ledger with the reason.

```bash
loopsmith providers loop.yaml
# claude   available    claude
# openai   unavailable  missing env: OPENAI_API_KEY
```

**Tier discipline.** Cheap tiers carry mechanical, high-volume work; strong
tiers carry judgment. Spending frontier reasoning on extraction is where loop
budgets die.

**Pin your judges.** Node routing follows the cascade unless you set
`provider:`. A judge left on the default cascade can end up on the same
provider as its builder — legal, but it wastes the independence the gate is
trying to give you. Pin judge nodes to a different family.

---

## 7. The permission preflight

```bash
loopsmith loop permissions loop.yaml                                  # show
loopsmith loop permissions loop.yaml --write .claude/settings.local.json
```

The grant is **derived from the config**, not guessed: one `Bash(...)` rule per
declared provider command, one per script detector, marketplace access only if
the acquisition policy actually uses it, plus the file tools. A loop that never
reaches the marketplace never asks for network access.

Merging preserves existing rules and unrelated settings, and is idempotent.

Two layers work together: `allowed-tools` in the skill frontmatter pre-approves
the invoking turn, and the settings file persists the rest across sessions.
Neither overrides `human_checkpoint`.

---

## 8. Sub-agent acquisition

Order: **installed → marketplace → generate**.

1. **Installed** — already in `~/.claude/skills/` or the project.
2. **Marketplace** — `claudemarketplaces.com/api/marketplaces` (a flat JSON
   array of ~2,600 plugin-marketplace repos with `repo`, `slug`, `description`,
   `categories`, `pluginKeywords`, `stars`, `pluginCount`), plus `npx skills`
   for single skills. Trust floors in `runtime/crates/loopsmith-cli/templates/marketplaces.json`: minimum stars
   and installs, with an owner allowlist that bypasses them.
3. **Generate** — author a new skill from the requirement.

Everything acquired lands in `generated-skills/`. An auto-acquired sub-agent is
a proposal, not a decision — it runs with whatever your permission grant
allowed, so promotion stays a human act.

---

## 9. Reading the memory ledger

sled, behind a `Store` trait so the backend can be swapped (sled is shipped but
effectively frozen upstream; the trait means callers never learn that).

| Record | Holds |
|---|---|
| Episode | What one node produced, which provider served it, prompt digest, timing |
| Goal state | The gate's ruling per target: satisfied, passed/failed counts, reason, iteration |
| Ledger | Append-only: dispatches, cascade skips, gate verdicts, **every stop-gate trigger**, proposals |
| Checkpoint | Where to resume: iteration, completed nodes, spend |
| Scratchpad | Per-goal reasoning carried between iterations |

```bash
loopsmith run status loop.yaml <run-id>
loopsmith run ledger loop.yaml <run-id> --limit 50
```

Writes are validated before they land. Bad data compounds — one wrong record
becomes a retrieved "fact", then reasoning, then another record — so malformed
episodes are rejected rather than stored.

---

## 10. Failure playbook

| Stop reason | Diagnosis | Fix |
|---|---|---|
| **all overall success scenarios met** | Success | — |
| **iteration cap reached** | Ran out of attempts while still changing something | Read the ledger. Usually the instruction is vague or the verifier checks the wrong thing. Raising the cap is rarely the fix |
| **no measurable change for N iterations** | The loop cannot affect what it is judged on | The detector may target an artifact no node writes, or the builder may lack a required tool |
| **token / cost / wall-clock exhausted** | Too expensive | Move mechanical nodes to `tier: cheap` before raising ceilings |
| `NodeFailed: no provider available` | Cascade exhausted | `loopsmith providers` — usually a missing binary or env key |
| `judgment refused: judge and builder both ran on X` | Judge was not independent | Pin the judge to another provider |
| `no blocking validation targets X` | Target can never be satisfied | Add a blocking validation |
| Validation error on `pre_execution` | Manual run not done | Do it. This is the point |

A run that stops without success exits non-zero and leaves the full history in
the ledger.

---

## 11. Self-evolution and the proposals directory

The loop finds out which sub-agents help by trying them and watching the gate.

```yaml
skills:
  explore: true                                  # off by default; it spends money
  explore_candidates: [table-formatter, chart-maker]
  min_trials: 3
```

Each iteration attaches one under-trialled candidate to a **builder** node —
judges and adversaries keep a fixed toolset, so the check does not drift while
the work does. After the gate rules, every skill used is paired with the
outcome for the goals that node advances, and stored as a trial.

```bash
loopsmith skills scores loop.yaml       # ranked by satisfaction rate
loopsmith run proposals loop.yaml <run-id>  # what it wants changed
```

| The loop does, on its own | The loop only proposes |
|---|---|
| Acquire, install, or generate sub-agents (quarantined) | Goals |
| Trial candidates and score them against gate outcomes | Validations |
| Write scratchpad notes between iterations | Success scenarios |
| | Which skills the config uses |

A candidate below `min_trials` is recorded and ignored — one lucky run is not
evidence. A skill already in the config is never re-proposed; a configured
skill that consistently fails is proposed for removal.

The loop cannot move its own goalposts, and cannot silently adopt a tool.
Apply a proposal by editing the config yourself.

---

## 12. Promotion path

```
generated-skills/<name>/     auto-acquired, quarantined, runs nowhere yet
        │  human review: read it, check what it can reach
        ▼
.claude/skills/<name>/       this project
        │  proved useful across projects
        ▼
~/.claude/skills/<name>/     everywhere
```

Read the whole `SKILL.md` before promoting, including `allowed-tools` and any
bundled scripts. Promotion grants it your permissions.

---

## 13. Running for weeks

`run` executes once and exits. `watch` is what keeps a loop alive.

```bash
loopsmith run watch loop.yaml                 # until interrupted
loopsmith run watch loop.yaml --check         # list triggers, run nothing
loopsmith run watch loop.yaml --max-runs 5    # bounded, useful for a first soak
loopsmith run schedule loop.yaml              # print the launchd agent / crontab line
loopsmith run schedule loop.yaml --install    # write it (loading it stays your call)
```

`watch` refuses to start on a manual-only config rather than sleeping forever.
Poll interval is derived from the trigger set: 5s when a file is watched, a
quarter of the shortest interval, 20s when cron is involved, 30s otherwise.

**A failed run does not stop the watcher.** It logs and waits for the next
trigger — the difference between a scheduler and a one-shot.

`schedule --install` writes the launchd plist but does not load it. Loading is
a persistent change to your machine, so the `launchctl load -w` command is
printed for you to run.

### What a long run actually needs

| Concern | What handles it |
|---|---|
| Surviving a crash | Checkpoint after every iteration; `resume` continues rather than restarting |
| Not spending forever | Token, cost, and wall-clock ceilings, all evaluated every iteration |
| Not spinning | `no_progress_iterations` halts when verdicts stop changing |
| Knowing what happened | Append-only ledger, including every stop-gate trigger |
| Parallel writers colliding | `isolated: true` puts the node in its own git worktree |
| Leftover state | `loopsmith run prune` removes the worktrees |

Worktrees are reused across iterations rather than recreated, so a node's
in-progress work survives the next pass. Outside a git repository, isolation
degrades to the shared directory and says so in the ledger rather than failing.

---

## 14. Where the design came from

Every rule in this document was taken from somewhere. This section is the
provenance: twenty sources distilled, what each contributed, what was rejected,
and why. It was a separate file until 1.0 — a design rationale that lives beside
the design it explains is one that gets read.

Reference distillation of every source in `docs/`, written so that a future loop-building task — this one or another — can be started without re-reading the corpus.

**Corpus:** 33 files, 20 unique sources. Fourteen files are PDF print-outs of a markdown article already present; only the markdown was read. `build-claude-skills.pdf` is the sole PDF with no markdown twin and is the highest-quality source in the set.

**Source quality warning that colours everything below.** Eighteen of twenty sources are self-published X threads or Medium posts. Several carry uncorroborated figures (repo star counts, benchmark numbers, before/after latencies). Where a claim drives a design decision, this document marks it `[unverified]`. The only authoritative source is the official Claude Code skills documentation.

---

### 14.1 Per-Source Distillation

| # | Source | Load-bearing idea | Used in `loopsmith` |
|---|---|---|---|
| 1 | Khairallah AL-Awady — *Loop Engineering: The 20-Step Roadmap* | The unit of work is a loop, not a prompt. Five parts: measurable done, verifier, layered exits, state, human checkpoint. Four gates decide whether a loop is even worth building. | `intent.goals` through `safety.limits`; `loopsmith-core` validation refuses a config missing a measurable definition of done |
| 2 | Anatoli Kopadze — *Loops explained* | `DISCOVER → PLAN → EXECUTE → VERIFY → ITERATE`. Five building blocks: automation, skill, sub-agents, connectors, verifier. Build order: manual run → skill → loop → schedule. Cost compounds because context is re-sent and grows. | The run state machine in `loopsmith-run`; build order enforced by `intent.prerequisites` being mandatory; `cost_per_accepted_change` metric in the ledger |
| 3 | CyrilXBT — *Self-Correcting AI Loop* | Builder / Judge / Manager with structured handoffs. Judge needs ground truth outside the Builder's reasoning. Four stress tests. Per-check verdicts, per-check routing. | Node execution contract in `loopsmith-provider`; `NodeVerdict` carries per-check results; scope-mismatch escalates rather than loops |
| 4 | Granite — *The Map* | Two layers: graph coordinates the fleet, loop makes one node trustworthy. Ten repos, each with a trap. The grading test: **can your system take "done" back?** | The whole two-plane architecture. `loopsmith-gate` is the only writer of `goal_satisfied` and can also revoke it |
| 5 | Argona — *What graph engineering is* | Node / edge / graph. The "and then" test finds false edges. Critical path is the floor, serial fraction is the cap. Fresh-context verifier. Worktree isolation + frozen git rules. | `loopsmith-graph`: DAG build, cycle detect, wave scheduling, critical path, Amdahl estimate driving auto-concurrency |
| 6 | Ajay — *Graph memory has one killer cost* | Separate extraction (high volume, low judgment, cheap+cached) from traversal (low volume, high judgment, expensive). Validate before writing — bad data compounds. Temporal edges. Graph edge is evidence; model assumption is not. | Provider tier routing in `loopsmith-provider` (cheap tier for extraction, strong tier for judgment); `loopsmith-memory` validates before write and stamps `valid_from` |
| 7 | Rahul — *Problem with how most people use AI* | Agents as a staffed team with persistent identity and lanes. Reviewer and devil's-advocate roles fire without being asked. Role descriptions must be tight to be useful. | Node `role` field; the adversarial-reviewer node type; tight role text enforced by schema `minLength` |
| 8 | Nick Spisak — *Paperclip* | Org chart, goal cascade, scheduled heartbeats, per-agent budget caps with hard stops, ticket trail, versioned config with rollback. | Per-node and global budget caps in `safety.limits`; heartbeat schedules in `execution.triggers`; the sled ledger is the ticket trail |
| 9 | Greg Isenberg — *agency-agents* | Structure agents like a company of specialists rather than one generalist. | Role catalog seeding for the skill-acquisition step |
| 10 | Cobus Greyling — *Claude Code Agent Teams* | Subagents report to a lead and never talk to each other; Agent Teams removes that relay. Teammates start fresh with only the spawn prompt. Ephemeral vs durable trade-off. | Fresh-spawn context is exactly the verifier-independence property; durable side is why the orchestrator lives in Rust rather than in a session |
| 11 | **Claude Code docs — *Extend Claude with skills*** | `SKILL.md` frontmatter fields, resolution order, progressive disclosure, `allowed-tools`, `context: fork`, dynamic context injection, the six-field portability subset, 1,536-char description cap, keep body under 500 lines. | Both emitted skills conform exactly. The portability subset governs what the generated skill may put in frontmatter |
| 12 | Bober_smart — *10 folders with the best skills* | Commit to a direction before generating. Context is the failure mode, not intelligence. | Skill-acquisition candidate list; the "constrain before generating" rule shapes node prompts |
| 13 | Jaynit — *Musk's Algorithm* | Question → delete → simplify → accelerate → **automate last**. Requirements carry a person's name. Delete until you must add ~10% back. Automation locks a process in. | `intent.prerequisites` exists because of this; `loopsmith loop plan` reports deleted and false edges before any run |
| 14 | Jaynit — *First principles thinking* | Reasoning by analogy has a ceiling. Bezos Type 1 (irreversible, deliberate) vs Type 2 (reversible, fast). | Type 1/Type 2 is the `human_checkpoint` rule: irreversible node actions always stop for a human |
| 15 | Cobus Greyling — *Hierarchical Chunking in RAG* | Navigate a hierarchy rather than a flat index; a scratchpad carries reasoning between depths. | Memory ledger keeps a per-goal scratchpad readable by the next iteration |
| 16 | Vipra Singh — *Build an Agent from Scratch* | Minimal agent = model + tools + toolbox + system prompt, with `think`/`work`. No observe step means it is a router, not a loop. | Baseline for the provider adapter interface — what every provider must expose at minimum |
| 17 | *Why Japanese Developers Write Code Differently* | Kaizen, jidoka (stop the line), JIT (build only what is needed today), seven wastes. | Jidoka is the no-progress stop gate; JIT is why the schema rejects unused optional blocks |
| 18 | *He Rewrote Everything in Rust* | A rewrite redistributes who is load-bearing; the team that ignores it loses. | Cautionary only — informs the "propose, don't apply" bound on self-evolution |
| 19 | Ai With Piyas — *9 Opus design prompts* | `Act as [named role]. Produce [artifact]. Include: [enumerated constraints].` Critique prompts name an external standard (Nielsen, WCAG). | Node prompt template shape; naming an external standard is how subjective validations get a detector |
| 20 | *Forget ChatGPT & Gemini* | Interest has moved from tools to agents that automate workflows; existing automation platforms have a learning-curve barrier. | Motivation only; no design impact |

---

### 14.2 The Cross-Cutting Findings

#### 14.2.1 The two-plane thesis
Five sources independently decompose agent systems the same way:

> The graph decides who runs and when. The loop decides whether you can trust what comes back.

Build the graph out of loops you can trust, or you have built a faster way to ship bugs across a fleet. A graph without loops is fast and wrong; loops without a graph are correct and serialized.

#### 14.2.2 The verifier-independence ladder
The single strongest signal in the corpus. Each source arrives at "the checker must not be the maker" from a different direction, and they form an escalation:

1. **Separate prompt** — weakest. Same context, same blind spots.
2. **Separate context** — a fresh-context verifier that never saw the work.
3. **Separate model** — a different model family; avoids shared blind spots.
4. **Separate mechanism** — deterministic code decides what survives. Strongest.

Trust rises with each step away from whatever produced the work. Supporting measurements `[unverified, secondhand]`: GPT-4 recognises its own writing 73.5% of the time and prefers it causally (Panickssery, NeurIPS 2024); self-grading inflation ~10% GPT-4, ~25% Claude (Zheng, NeurIPS 2023).

**Design consequence:** `loopsmith-gate` sits at rung 4. It is plain Rust, it is the only writer of `goal_satisfied`, and no prompt can talk it out of a verdict.

#### 14.2.3 The four stop gates, layered
Any one alone is insufficient:

| Gate | Trigger | Meaning when it fires |
|---|---|---|
| Verifier satisfied | All validations pass | Success |
| Iteration cap | `max_iterations` reached | Escalate with full history |
| Budget ceiling | Token / time / cost limit | Escalate; task may be unsolvable at this price |
| No-progress | Last N iterations changed nothing measurable | Jidoka — stop the line, the loop is spinning |

Written as **hard logic, not prompt text**. "Stop when it's good enough" is a suggestion a model will eventually talk itself past. Loops fail quietly — the "Ralph Wiggum loop" declares victory early and keeps billing while producing nothing.

**Log every stop-gate trigger, not just successes.** One node hitting its ceiling constantly while others rarely do means its judge is miscalibrated or checking the wrong ground truth. That pattern is invisible if you only track completions.

#### 14.2.4 Amdahl sizing — know the speedup before deploying
$$ S = \frac{1}{(1-p) + \frac{p}{N}} $$

| p | N | Speedup |
|---|---|---|
| 0.95 | 16 | ×9.14 |
| 0.70 | 16 | ×2.91 |
| 0.95 | 256 | ×18.6 |
| any | ∞ | `1/(1-p)` |

Estimate `p` with the "and then" test: for every sequential step, ask whether the next step actually *reads* the previous step's output. Yes is a real edge; no was never an edge. Cut a false edge rather than adding an agent — cost scales with `N` while speedup flattens.

#### 14.2.5 Musk's ordering applied to loop construction
Question → delete → simplify → accelerate → **automate last**. A loop is an analogy engine at machine speed: point it at an unexamined process and it executes that process faithfully, tirelessly, and at scale. Automating before questioning locks in the wrong process.

This is the same instruction as the loop roadmap's "do it manually first" and "the manual runs are the spec", arrived at from manufacturing rather than from agents.

#### 14.2.6 Cost discipline
Loop cost compounds because context is re-sent and grows each pass; a ten-iteration loop is ten prompts that each get bigger, and a maker+checker split doubles it. The metric that matters is **cost per accepted change** — below a 50% accept rate the loop costs more than it returns. Levers: route each step to the cheapest capable model, cache stable prefixes, batch non-time-sensitive work, cap iterations and budget as hard logic, track cost in aggregate rather than per loop.

#### 14.2.7 Isolation has two halves
Parallel nodes need **separate file state** (a git worktree per crew, not per agent — four worktrees of sixteen, not sixty-four checkouts) *and* **separate context** (or they agree with each other). Both, or neither works. The frozen rule set that made a 64-agent run safe:

```
Never git stash. Never git reset.
No git command except committing a specific file.
No slow commands before the test phase.
```

---

### 14.3 What `loopsmith` Takes From This

| Corpus idea | Component | How it is enforced |
|---|---|---|
| Measurable definition of done | `loopsmith-core` | Config fails validation if a goal has no validation entry |
| Verifier at rung 4 | `loopsmith-gate` | Sole writer of `goal_satisfied`; deterministic; can revoke |
| Four layered stop gates | `loopsmith-run` | All four evaluated every iteration; any one halts |
| "Can it take done back?" | `loopsmith-gate` | `revoke` command and automatic revocation on re-validation failure |
| And-then test / critical path | `loopsmith-graph` | `plan` reports real edges, false edges, critical path, predicted speedup |
| Amdahl-driven fan-out | `loopsmith-graph` | Auto-concurrency picks `N` where marginal speedup still exceeds marginal cost |
| Cheap extraction / costly judgment | `loopsmith-provider` | Per-node `tier: cheap \| standard \| strong` routed across providers |
| Builder / Judge / Manager | `loopsmith-provider` + run loop | Node roles with structured verdicts, per-check routing |
| Persistent state, resumable | `loopsmith-memory` | sled ledger behind a `Store` trait; `loopsmith run resume` continues from the last checkpoint |
| Validate before write | `loopsmith-memory` | Schema check on every episode write; bad data never enters the ledger |
| Jidoka | Stop gates | No-progress window halts the line rather than continuing |
| JIT / delete step | Schema | Unused optional blocks are rejected, not ignored |
| Type 1 vs Type 2 | `safety.limits` | Irreversible actions always require `human_checkpoint` |
| Worktree isolation + frozen git rules | `execution.graph` isolation | `worktree` per parallel writer, `container` where the host has Docker |
| Progressive disclosure, frontmatter rules | Emitted skills | Both skills conform to the official spec, including the six-field portability subset |
| Tight role descriptions | Schema | `role` has a minimum length and must name a standard for subjective checks |

---

### 14.4 Rejected, and Why

| Idea from corpus | Rejected because |
|---|---|
| Ephemeral agent teams as the orchestration substrate | Teams vanish with the session and have no `/resume`. A loop that must survive a crash, a schedule, and a budget ceiling needs durable state, so the orchestrator lives in Rust and sessions become disposable workers |
| Model-in-the-coordination-loop | Coordination is a solved deterministic problem (DAG + waves). Spending model tokens on scheduling is the exact "frontier intelligence on mechanical work" mistake source 6 warns about |
| "Zero token coordination" claim | Half true and misleading. Coordination is free; every worker underneath is billed. The ledger reports real cost rather than repeating the claim |
| Self-grading with a rubric prompt | Rung 1 of the independence ladder. Retained only as a *pre-filter* before the real gate, never as the gate |
| Existence-only review gates | A gate that checks a review file exists lets the agent disagree and skip findings. Verdicts must be parsed, not counted |
| Fully autonomous self-modification | The loop may acquire skills, tune descriptions, and reshape its graph, but changes to goals, validations, and success criteria go to `proposals/`. A system that can move its own goalposts cannot certify that it met them |
| Star counts / benchmark figures as evidence | Uncorroborated single-source numbers. Recorded as context, never as a basis for a default |
| Kaizen "never rewrite" as a global rule | Directly contradicted elsewhere in the corpus by two step-change rewrites. Adopted only as jidoka (stop the line) and JIT (build what is needed today) |
| One tool/skill per capability | Context is the failure mode. Skills are acquired on demand and unloaded, not staffed "just in case" |

---

### 14.5 Quick Reference Card

**Before building any loop:**
1. Does the task repeat at least weekly?
2. Can something automatically reject bad output?
3. Can the agent do it end to end?
4. Is "done" objective?

Miss one — keep it a manual prompt.

**Build order:** one reliable manual run → save as skill → wrap in loop with gate and stop condition → *then* schedule.

**Every loop needs:** measurable done · externally grounded verifier · four layered exits · persistent state · a human checkpoint before anything irreversible.

**The test that grades the whole system:** can it take "done" back?
