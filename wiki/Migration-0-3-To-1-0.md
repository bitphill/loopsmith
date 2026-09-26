# Migration 0.3 → 1.0

**Nothing you have stops working.** Every 0.3 config parses in 1.0, and every
0.3 command spelling still runs. Both print a notice naming what moved, and
both go away at 2.0.

## Why anything moved

0.3 had ten lettered sections — `A` through `J` — that an author had to
memorise. `F` was stop gates. Nothing about the letter said so, and nothing
ever would. 1.0 replaces the letters with four bundles named for what the
sections in them are *for*:

| Bundle | Holds |
|---|---|
| `intent` | what the loop is for |
| `execution` | how the work gets done |
| `safety` | what must not happen, and when to stop |
| `evolution` | how the loop may change itself |

## The relocation table

This is the whole of it. One table in `loopsmith-core` applies it as a file is
read, and `loopsmith loop migrate` rewrites a file using that same table — so
the migrator cannot disagree with the parser.

| 0.3 key | 1.0 path |
|---|---|
| `information` | `intent.background` |
| `pre_execution` | `intent.prerequisites` |
| `goals` | `intent.goals` |
| `success` | `intent.success` |
| `validations` | `safety.checks` |
| `stop_gates` | `safety.gates.stop` |
| `constraints` | `safety.limits` |
| `graph` | `execution.graph` |
| `providers` | `execution.providers` |
| `execution_guidelines` | `execution.phases` |
| `default_skills` | `execution.default_skills` |
| `skills` | `execution.skills` |
| `context` | `execution.memory` |
| `schedules` | `execution.triggers.triggers` |

Everything else in 1.0 is new and has no 0.3 spelling: `environment`,
`features`, `safety.gates.entry`, `safety.gates.approval`,
`safety.gates.rollback`, `safety.recovery`, `safety.protected`,
`safety.alerts`, `execution.memory.namespaces`, `execution.triggers.max_depth`,
`execution.triggers.dedup_window_seconds`, `execution.graph.join`,
`execution.graph.container_image`, per-node `isolation`, and the whole
`evolution` bundle.

## Migrating a file

```bash
loopsmith loop migrate loop.yaml --check    # say what would change, write nothing
loopsmith loop migrate loop.yaml --write    # rewrite it in place
```

`--check` exits non-zero when a file would change, which is what makes it
usable in CI. Both grammars work: a Markdown config is rewritten as Markdown
and a YAML config as YAML, because a migration that silently changes the format
is a migration nobody trusts twice.

## The commands that moved

1.0 groups twenty-two flat verbs under four nouns. Every old spelling still
works, prints one line, and goes at 2.0.

| 0.3 | 1.0 |
|---|---|
| `loopsmith new` | `loopsmith loop new` |
| `loopsmith guided` | `loopsmith loop guided` |
| `loopsmith validate` | `loopsmith loop validate` |
| `loopsmith plan` | `loopsmith loop plan` |
| `loopsmith convert` | `loopsmith loop convert` |
| `loopsmith migrate` | `loopsmith loop migrate` |
| `loopsmith permissions` | `loopsmith loop permissions` |
| `loopsmith run <config>` | `loopsmith run start <config>` |
| `loopsmith resume` | `loopsmith run resume` |
| `loopsmith status` | `loopsmith run status` |
| `loopsmith ledger` | `loopsmith run ledger` |
| `loopsmith gate` | `loopsmith run gate` |
| `loopsmith watch` | `loopsmith run watch` |
| `loopsmith schedule` | `loopsmith run schedule` |
| `loopsmith prune` | `loopsmith run prune` |
| `loopsmith proposals` | `loopsmith run proposals` |

`doctor`, `providers`, `web`, `mcp` and `skills` did not move.

## What to do about a loop that is already running

Nothing, until you want to. A 0.3 config resumes from its own checkpoints under
1.0, and its launchers — `run.sh`, `resume.sh`, `run.cmd`, `resume.cmd` — keep
working because the 0.3 spellings they contain are rewritten before clap sees
them. Migrate when you next edit the config, not because a version number
changed.
