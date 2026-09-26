# Concepts

The vocabulary, defined once, with the dotted path each thing lives at. If a
word in an error message or a browser card is unfamiliar, it is here.

[Architecture](Architecture.md) covers why the system is shaped this way, and
[Commands](Commands.md) covers how to drive it. This page is only the nouns.

## The file

A **loop** is one config file — YAML, or Markdown with the config in fenced
blocks. It is the whole definition: there is no hidden state beside it and no
server that remembers it. `name`, `version` and `description` describe the file.
`environment` says whether this is `dev`, `staging` or `prod`; `staging` and
`prod` refuse what `dev` merely warns about. `features` switches whole
capabilities on and off — self-evolution, marketplace sub-agents, side effects
outside the loop directory, parallel execution, human checkpoints — so a
capability a loop never uses cannot surprise it. The first three are off until
asked for.

Everything else is one of four bundles:

| Bundle | Question it answers |
| --- | --- |
| `intent` | What is this loop for? |
| `execution` | How does it run? |
| `safety` | What must not happen, and when does it stop? |
| `evolution` | What may it change about itself? |

A **run** is one execution of a loop. It proceeds in **iterations**, and each
iteration dispatches work, collects output, and asks the gate whether anything
is now satisfied. A run can be stopped, resumed from its last **checkpoint**,
and read back from its **ledger** — an append-only record of everything that
happened, which is the only account of a run that survives the process.

## What the loop is for — `intent`

A **goal** (`intent.goals`) is one outcome, named, with a description long
enough to check against. The name is what everything else refers to: a node
claims to work on it, a check verifies it, and the gate marks it satisfied or
not. Goals are the unit of progress, so a loop with one enormous goal has no
progress to report until it is finished.

**Background** (`intent.background`) is the key–value context every dispatch
carries: the target module, the test command, the rule that must not be broken.
**Prerequisites** (`intent.prerequisites`) is work that must happen before the
first iteration — usually a human's. **Success scenarios**
(`intent.success`) are the concrete stories that describe done.

## What must not happen — `safety`

A **check** (`safety.checks`) verifies one goal, or `overall`. It carries a
`statement` in plain language, a `mode`, and a **detector** that decides it:

- `mode: objective` — it either holds or it does not.
- `mode: subjective` — a judgement, which needs a judge.
- `mode: percentage` — a number against a threshold.

A **detector** is the mechanism, and there are five:

| Detector | What it does |
| --- | --- |
| `script` | Runs a command. Exit zero passes. |
| `file_exists` | An artefact is present at a path. |
| `regex_match` | An artefact matches a pattern. |
| `threshold` | A number in an artefact clears a bound. |
| `judge` | A model reads the output against a written standard. |

A **blocking** check holds the gate shut until it passes. A non-blocking one is
recorded and reported and holds nothing, which is the right setting for
something worth watching but not worth stopping for.

The **gate** is the deterministic Rust component that reads detector results and
writes `goal_satisfied`. Nothing else writes it — no prompt, no summary, no
model. It can also revoke: delete a required artefact and a satisfied goal flips
back, because a system that can only promote is a burndown chart with extra
steps.

**Gates** in the plural (`safety.gates`) are the four moments a run can be
stopped:

- `stop` — the ceilings that end a run: iterations, cost, iterations without
  progress.
- `entry` — checked once before the first iteration. A failing entry gate means
  the run never starts, which is the cheapest possible failure.
- `approval` — checked after each iteration; failing halts for a human rather
  than ending the run.
- `rollback` — checked after each iteration; failing restores the last good
  checkpoint, for when continuing is worse than undoing.

**Limits** (`safety.limits`) are the constraints that are not ceilings: paths
that may not be touched, commands that may not be run, the human checkpoints.
**Recovery** (`safety.recovery`) names an action for each failure class —
`transient_error`, `invalid_output`, `tool_unavailable`, `repeated_failure`,
`safety_violation`, `resource_exhaustion`, `corrupted_state` — and the action is
one of retry with backoff, revise, fall back to the next provider, escalate,
pause, restore a checkpoint, or stop. **Alerts** (`safety.alerts`) say who hears
about it.

## How it runs — `execution`

A **node** (`execution.graph.nodes`) is one unit of work: a role, an
instruction, the goals it serves, and what it depends on. Dependencies make the
graph; the graph is planned into **waves**, where every node in a wave can run
at the same time because none of them depends on another.

A **join** (`execution.graph.join`) is what it takes for a wave to count as
finished: every node (`wait_for_all`), a `quorum` of them, or the
`first_success`. **Concurrency** (`execution.graph.concurrency`) is how wide to
run: `sequential`, a `fixed` number, or `auto`, derived from the graph's own
widest wave and trimmed to the point where another worker stops paying for
itself.

**Isolation** (`execution.graph.nodes[].isolation`) is how far a node is kept
from everything else: `none` runs in the loop directory, `worktree` gives it its
own git worktree published back on success, and `container` runs it in a
container over that worktree. Parallel writers need at least a worktree.
Container isolation degrades to a worktree where Docker is absent, and says so,
because the same loop directory gets checked out on laptops, CI runners and
servers.

A **provider** (`execution.providers`) is a command that talks to a model —
Claude Code, Ollama, an OpenAI or Gemini CLI, anything that takes a prompt and
returns text. Each belongs to a **tier**: `cheap` for high volume and low
judgement, `standard`, `strong` for the final review and the multi-hop
reasoning. A node names a tier rather than a provider, and the **cascade** is
the order to try within it — which is what makes a fallback a configuration
choice instead of a code change.

**Memory** (`execution.memory`) is what outlives a run, kept in four
**namespaces**: `episodic` for what happened, `semantic` for what was learned,
`procedural` for how something is done, and `failure` for what went wrong. A
record moves from observed to
reusable by a **promotion** rule — `never`, `automatic`, after this many
independent corroborations (`repeated_validation`), or only once a human says so
(`human_approval`). Automatic promotion is only right where writing the record
is itself the evidence, such as a failure that was observed happening.

A **trigger** (`execution.triggers`) fires a run: `cron`, `interval`,
`file_change`, `goal_satisfied`, or `manual`. A trigger the loop's own output can
fire is capped by depth, so a loop cannot drive itself indefinitely by writing
the file it watches.

**Phases** (`execution.phases`) are the guidelines each stage of work carries,
and **skills** (`execution.skills`) are the sub-agents a node may acquire, trial
and rank.

## What it may change about itself — `evolution`

A **proposal** is a change the loop suggests to its own config. It is measured
against a **baseline**, and `max_regression` says how much one metric may worsen
and still count as an improvement overall — nonzero on purpose, because a change
that trades a hair of accuracy for half the cost is usually right, and a
zero-tolerance gate refuses every such trade while admitting any change that
touches nothing measured.

`allowed_kinds` limits what may be proposed at all. `require_sandbox` makes a
proposal prove itself in isolation first, `require_approval` makes a human
adopt it, and `keep_rollback` keeps the previous known-good config so an
adoption can be undone. What no proposal may touch is listed separately, under
`safety.protected` — it is a safety statement rather than an evolution setting,
because it has to hold whether or not evolution is on.

Evolution is off unless both `features.self_evolution` and `evolution.enabled`
say otherwise, and turning `require_approval` off is refused outright in `prod`.

## Where these came from

0.3 called these sections by letters — A through J. 1.0 calls each one by the
path it lives at, in every error message, every document and every card in the
browser. [Migration 0.3 → 1.0](Migration-0-3-To-1-0.md) has the table.
