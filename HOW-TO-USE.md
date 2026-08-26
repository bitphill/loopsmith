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

---

## 1. Architecture

Three planes. The split exists because the corpus this is built on is unanimous
on one point: a model must not be the thing that certifies its own completion.

```
INVOCATION      /loopsmith  or  loopsmith new --path <dir>
                  └─ permission preflight (one grant) → hands-off
                       │
CONTROL PLANE   loopsmith (Rust)                  ← owns truth
                  ├─ core      A–H config model and validation
                  ├─ graph     DAG, waves, critical path, Amdahl sizing
                  ├─ memory    sled: episodes, goal state, ledger, checkpoints
                  ├─ gate      deterministic verdicts — the ONLY writer of
                  │            goal_satisfied, and able to revoke it
                  ├─ provider  command-template routing, token/cost accounting
                  ├─ skills    acquire, trial, rank, propose
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
loopsmith new --path ./loops/nightly-refactor --purpose "keep the module simple"
```

`--path` / `-p` is mandatory. A loop owns a ledger, checkpoints, outputs, and a
quarantine directory; without its own home you get several half-finished loops
writing into each other's state.

What lands:

```
nightly-refactor/
├── loop.yaml            the A–H config
├── README.md            how to run this specific loop
├── .gitignore           state/, out/, generated-skills/ are not source
├── state/               sled: episodes, goal state, ledger, checkpoints
├── out/                 deliverables
├── proposals/           changes the loop wants to make to its own goals
└── generated-skills/    auto-acquired sub-agents awaiting promotion
```

The scaffolded config ships with `pre_execution` steps set to `done: false`, so
`loopsmith validate` **fails on purpose** until you have done the manual run.

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
`runtime/crates/loopsmith-cli/templates/examples/`, and a test fails if the two
have drifted — so a stale copy is caught by `cargo test`, not by a user.

A user's own `~/.loopsmith/examples/*.yaml` take priority, and a checkout's
`config/examples/` is read live so edits show up without a rebuild.

### Building it

The frontend is React 19 + Vite + Tailwind v4, emitted to fixed filenames
(`index.html`, `app.js`, `app.css`) because a content hash cannot be chased by an
`include_str!` literal.

```bash
npm --prefix runtime/crates/loopsmith-cli/web install
npm --prefix runtime/crates/loopsmith-cli/web run build   # writes src/web/dist/
cargo build -p loopsmith --release
```

`npm run dev` serves on 5173 and proxies `/api` to a `loopsmith web --no-open`,
so the UI can be iterated on without a Rust rebuild. `npm run test:e2e` drives
the real binary through Playwright.

The whole thing sits behind a default-on `web` feature. To drop the async
dependency tree entirely: `cargo install loopsmith --no-default-features`.

---

## 5. Configuration reference, section by section

Validated against `config/loop.schema.json`. Cross-field rules the schema
cannot express are enforced by `loopsmith validate`.

### A · `information`
Static facts every node receives. Nodes start fresh with only their spawn
prompt, so anything not here gets rediscovered badly by each of them.
Fields: `key`, `value`, optional `note`.

### B · `pre_execution`
The manual work list. Fields: `step`, `done`, optional `evidence`. **Every step
must be `done: true` or validation fails.** This is the only place the tool
refuses on process rather than syntax, and it is deliberate: you cannot
automate what you cannot describe.

### C · `goals`
Fields: `name` (never `overall`, which is reserved), `description`, optional
`depends_on`, optional `priority`. Subjective phrasing is fine here — the
validation is what must be checkable.

### D · `validations`
Fields: `target` (goal name or `overall`), `name`, `mode`
(`subjective|objective|percentage`), `statement`, `blocking` (default true),
`detector`.

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

**Every goal needs at least one blocking validation**, or the config is
rejected — a goal that cannot be checked can never be honestly finished.

`regex_match` reads the files your `file_exists` detectors name, under either the
full path or the file's stem. So `detector: { type: file_exists, path: out/notes.md }`
makes `artifact: notes` and `artifact: out/notes.md` both work, and a regex
naming anything else is rejected by `validate` rather than failing closed for the
life of the loop.

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

### E · `success`
Fields: `target`, `name`, `mode`, `statement`, `threshold` (required for
`percentage`, 0.0–1.0 of blocking validations that must pass).

### F · `stop_gates`
`max_iterations`, `max_revisions_per_node`, `max_wall_clock_seconds`,
`max_tokens`, `max_cost_usd`, `no_progress_iterations` (0 disables),
`stop_on_overall_success`. Declare at least one budget ceiling; validation
warns if you declare none.

### G · `schedules`
`manual`, `cron` (`expr`), `interval` (`seconds`), `file_change` (`path`),
`goal_satisfied` (`goal`). Schedule last, after the loop is reliable by hand.

**Cron is evaluated in UTC.** Deriving a correct local offset in a
multithreaded process is unsound on Unix without care, and a scheduler quietly
an hour off twice a year is worse than one honestly in UTC. For plain cadence,
`interval` avoids the question entirely.

`file_change` and `goal_satisfied` fire on the **edge**, not the level: a goal
that stays satisfied does not retrigger, and the watcher skips its own `state/`
directory so ledger writes cannot retrigger it.

### H · `constraints`
`global` plus `per_node` overrides. Merge semantics: **rules append, limits
override**. Fields: `rules`, `forbidden_paths`, `forbidden_commands`,
`max_tokens`, `max_seconds`, `human_checkpoint`.

`human_checkpoint` stops and waits regardless of any permission grant.

### F, continued · `no_progress_iterations_randomness`
Fires *before* `no_progress_iterations` and must be strictly less than it —
otherwise the loop halts before it ever tries something different, and validation
refuses the config.

When it fires, a cheap-tier agent is shown the failing checks and the recent
iteration summaries, and picks one of exactly four tactics: `reorder`,
`escalate`, `explore`, or `reframe`. The menu is fixed, so the agent can change
how the loop works and cannot change what counts as done. If no cheap provider is
reachable, or the answer is not on the menu, a seeded fallback picks instead. The
seed is derived from the run id and iteration and written to the ledger, so a run
that took a strange turn replays exactly.

### I · `execution_guidelines`
Named **phases**, each with a standing instruction and a place in an ordering.
Nodes join a phase with `stage:`, and a node is not dispatched until its phase
is active.

```yaml
execution_guidelines:
  items:
    - name: gather
      guideline: Collect sources. Write nothing yet.
    - name: draft
      guideline: Write only from what gather collected.
  dependency:
    - gather -> draft -> review     # chains are allowed
```

Use this for ordering that is about **method**; `graph.depends_on` is only for a
node that genuinely reads another node's output. Overloading `depends_on` with
both makes the critical path meaningless.

A phase opens when everything before it is complete, and completes when its own
nodes have run and the gate has satisfied the goals they advance. A phase with no
nodes gates nothing and completes on sight. Guidelines with no arrow between them
run in parallel. Cycles and unknown names are validation errors, caught before
anything is dispatched.

### J · `default_skills`
Sub-agents installed before the loop starts. Idempotent, so it runs at the start
of every run and a loop directory can be rebuilt from its config alone.

```yaml
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

### `context`
How much of the previous iterations each prompt carries. Each iteration is
compressed to a summary; only the last few are sent forward, so prompt size stops
growing with the run.

`carry_summaries` (default 2, `0` disables), `summary_provider` (optional — the
deterministic facts are always written, this only buys prose), `max_summary_chars`
(default 1200).

### `graph`
Nodes: `id`, `role` (`builder|judge|manager|adversary|researcher`),
`instruction` (min 16 chars), `depends_on`, `goals`, `tier`, `provider`,
`stage`, `skills`, `weight`, `isolated`.

Only list a dependency whose output the node actually reads.

Concurrency: `sequential`, `fixed` (`max_parallel`), or `auto` (`cap`,
`min_marginal_gain`). `auto` derives the parallel fraction from the graph and
adds workers only while the next buys `min_marginal_gain` of Amdahl speedup.

### `skills`
`acquisition_order`, `quarantine_dir`, `min_marketplace_stars`,
`require_human_promotion`.

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
loopsmith permissions loop.yaml                                  # show
loopsmith permissions loop.yaml --write .claude/settings.local.json
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
loopsmith status loop.yaml <run-id>
loopsmith ledger loop.yaml <run-id> --limit 50
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
loopsmith proposals loop.yaml <run-id>  # what it wants changed
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
loopsmith watch loop.yaml                 # until interrupted
loopsmith watch loop.yaml --check         # list triggers, run nothing
loopsmith watch loop.yaml --max-runs 5    # bounded, useful for a first soak
loopsmith schedule loop.yaml              # print the launchd agent / crontab line
loopsmith schedule loop.yaml --install    # write it (loading it stays your call)
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
| Leftover state | `loopsmith prune` removes the worktrees |

Worktrees are reused across iterations rather than recreated, so a node's
in-progress work survives the next pass. Outside a git repository, isolation
degrades to the shared directory and says so in the ledger rather than failing.
