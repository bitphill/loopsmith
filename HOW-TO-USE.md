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
                  ├─ provider  command-template routing across any CLI or API
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

### E · `success`
Fields: `target`, `name`, `mode`, `statement`, `threshold` (required for
`percentage`, 0.0–1.0 of blocking validations that must pass).

### F · `stop_gates`
`max_iterations`, `max_revisions_per_node`, `max_wall_clock_seconds`,
`max_tokens`, `max_cost_usd`, `no_progress_iterations` (0 disables),
`stop_on_overall_success`. Declare at least one budget ceiling; validation
warns if you declare none.

### G · `schedules`
`manual`, `cron` (`expr`), `file_change` (`path`), `goal_satisfied` (`goal`).
Schedule last, after the loop is reliable by hand.

### H · `constraints`
`global` plus `per_node` overrides. Merge semantics: **rules append, limits
override**. Fields: `rules`, `forbidden_paths`, `forbidden_commands`,
`max_tokens`, `max_seconds`, `human_checkpoint`.

`human_checkpoint` stops and waits regardless of any permission grant.

### `graph`
Nodes: `id`, `role` (`builder|judge|manager|adversary|researcher`),
`instruction` (min 16 chars), `depends_on`, `goals`, `tier`, `provider`,
`skills`, `weight`, `isolated`.

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
   for single skills. Trust floors in `config/marketplaces.json`: minimum stars
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

| The loop may, on its own | The loop must propose |
|---|---|
| Acquire or generate sub-agents (quarantined) | Goals |
| Tune skill descriptions for triggering | Validations |
| Reshape the graph after repeated node failure | Success scenarios |
| Write scratchpad notes between iterations | Stop gates |

Everything in the right column is written to `proposals/` for review. The loop
cannot move its own goalposts — a system that rewrites the criteria it is
judged against cannot certify that it met them.

Review `proposals/` after every run that produced one.

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
