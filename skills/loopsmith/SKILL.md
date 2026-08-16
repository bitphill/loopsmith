---
name: loopsmith
description: Create and run self-evolving agent loops. Use when the user wants to build a loop, scaffold a new purpose-specific loop, run or resume one, check why a loop stopped, or inspect its gate rulings and ledger.
argument-hint: "new --path <dir> | run <config> | plan <config> | status <config> <run-id>"
allowed-tools: Bash Read Write Edit Glob Grep
disable-model-invocation: true
---

# loopsmith

The runner. You type it; it does not fire on its own — a loop is a durable,
budget-spending thing and starting one should be a decision, not an inference.

For the concepts behind any of this, see the `loopsmith-reference` skill.

## Create a loop

`--path` / `-p` is **mandatory**. Every loop owns a sled ledger, checkpoints,
and a quarantine directory, and needs a home of its own.

```bash
loopsmith new --path ./loops/<purpose> --purpose "one line on what it is for"
```

That writes `loop.yaml`, `.gitignore`, `README.md`, and the `state/`, `out/`,
`proposals/`, `generated-skills/` directories.

Then edit `loop.yaml`. The sections are A–H; `LOOP-TEMPLATE.md` documents each
one with an example and the reason it exists.

## The order that works

```bash
loopsmith validate <path>/loop.yaml
loopsmith plan     <path>/loop.yaml
loopsmith permissions <path>/loop.yaml --write .claude/settings.local.json
loopsmith run      <path>/loop.yaml
```

`validate` refuses while any `pre_execution` step is unfinished. Do the task by
hand first and record what you learned — that refusal is the most valuable
thing this tool does, because a loop wrapped around a process nobody has
performed produces confident garbage at scale.

`plan` prints waves, the critical path, the parallel fraction, the chosen
concurrency, and the predicted speedup with its ceiling. Read it before
spending anything: sixteen workers at p=0.95 buy ×9.14, not ×16.

`permissions` derives the narrowest grant the config actually needs and merges
it once. After that the run is hands-off, except for anything the config marks
as a human checkpoint — those stop regardless of the grant.

## Keep it running

`run` executes once. `watch` is what makes a loop live for weeks:

```bash
loopsmith watch <path>/loop.yaml --check      # list triggers, run nothing
loopsmith watch <path>/loop.yaml              # until interrupted
loopsmith schedule <path>/loop.yaml --install # survive a reboot
```

Triggers: `cron` (UTC), `interval`, `file_change`, `goal_satisfied`. A failed
run logs and the watcher continues — that is the difference between a
scheduler and a one-shot. `watch` refuses a manual-only config rather than
sleeping forever.

## Let it find what works

```bash
loopsmith skills search <terms...>            # claudemarketplaces.com + skills.sh
loopsmith skills acquire <config> <name>      # into quarantine
loopsmith skills list <config>                # what this loop can see
loopsmith skills scores <config>              # ranked by gate outcomes
loopsmith proposals <config> <run-id>         # what it wants changed
```

Set `skills.explore: true` with `explore_candidates` and the loop trials
sub-agents it was not told to use, then proposes the ones that correlate with
satisfied goals. It cannot adopt them itself — apply a proposal by editing the
config.

## When a run ends

```bash
loopsmith status <path>/loop.yaml <run-id>    # gate rulings per goal
loopsmith ledger <path>/loop.yaml <run-id>    # everything that happened
loopsmith resume <path>/loop.yaml <run-id>    # continue from the checkpoint
```

A non-zero exit means the run did not meet the bar. The stop reason says which
gate fired:

| Stop reason | What it means | What to do |
|---|---|---|
| all overall success scenarios met | Success | Nothing |
| iteration cap reached | Ran out of attempts | Read the ledger; usually the verifier or the instruction is wrong, not the cap |
| no measurable change for N iterations | Spinning | The loop cannot affect what it is being judged on |
| token / cost / wall-clock budget exhausted | Too expensive | Route mechanical nodes to a cheaper tier before raising the ceiling |

Raising a ceiling to make a run pass is almost always the wrong fix. The gate
is the thing telling you the truth.

## Check the gate on its own

```bash
loopsmith gate <path>/loop.yaml --target <goal|overall> --workdir <dir>
```

Useful mid-development: it answers "would this pass right now?" without
spending a provider call. It also revokes — delete a required artifact and the
same command flips to NOT SATISFIED. A system that can only promote is a
burndown chart with extra steps.

## Which providers are live

```bash
loopsmith providers <path>/loop.yaml
```

Reports each provider as available or not, and why not — missing binary,
missing environment key. Key **names** only; values are never read.

## Expose the control plane over MCP

```bash
claude mcp add loopsmith -- loopsmith mcp --state ./state
```

Gives an agent the plan, the ledger, the gate's verdict, and the scratchpad.
There is deliberately no tool for marking a goal satisfied.

## Sanity rules worth keeping

- Pin judge nodes to a different provider family than the builder they judge.
  A shared model shares its blind spots, and the gate refuses a self-judgment
  outright.
- Mark parallel builders `isolated: true` or they clobber each other's files.
- Set a budget ceiling. Every one of them.
- Read `proposals/` after a run. That is where the loop asks to change its own
  goals, and it cannot apply those itself.
