---
name: REPLACE-ME-loop
description: >
  REPLACE ME. What this loop does and when to reach for it. This field is the
  only thing always in context, so it is the whole triggering mechanism — say
  what the loop produces AND the situations that should invoke it. Keep the
  combined description under 1,536 characters.
argument-hint: "[--path <dir>] [--run-id <id>] [--dry-run]"
arguments: path run_id
allowed-tools: Bash Read Write Edit Glob Grep
disable-model-invocation: true
---

# REPLACE-ME-loop

> **How to use this file.** Copy it to `<your-loop>/SKILL.md`, fill every
> `REPLACE ME`, and put the A–H content in `loop.yaml` beside it. This body
> stays thin on purpose: the config is data the runtime validates, and data
> in a config file can be checked, diffed, and scheduled. Prose in a skill
> body cannot.
>
> Faster path: `loopsmith new --path <dir>` writes both files for you, then
> edit them.

## What this loop is for

REPLACE ME — one paragraph. What outcome, for whom, and why a loop rather than
a single prompt.

## Mandatory argument

`--path` / `-p` is required. A loop owns durable state — a sled ledger,
checkpoints, quarantined sub-agents — and needs a directory of its own.
Without it you get several half-finished loops writing into each other's
state.

```bash
loopsmith new --path ./loops/my-purpose --purpose "what it is for"
```

## Run it

```bash
loopsmith validate <path>/loop.yaml      # A–H model complete and consistent
loopsmith plan     <path>/loop.yaml      # waves, critical path, real speedup
loopsmith permissions <path>/loop.yaml --write .claude/settings.local.json
loopsmith run      <path>/loop.yaml      # hands-off from here
```

`validate` fails while any `pre_execution` step is unfinished. That is the
gate, not a nag: a loop built around a process you have not performed by hand
produces fast, confident garbage.

---

# The A–H model

Everything below lives in `loop.yaml`. The runtime validates it against
`config/loop.schema.json`; anything the schema cannot express (every goal
having a blocking validation, targets resolving, cycles) is checked by
`loopsmith validate`.

## A · Information

*Why it exists:* every node starts fresh with only its spawn prompt. Whatever
is not here has to be rediscovered, badly, by each of them.

```yaml
information:
  - key: output_path
    value: out/result.md
    note: Optional. Why this fact matters.
```

Put durable facts here — paths, conventions, the one number everyone needs.
Not instructions; those belong to a node.

## B · Pre-execution work list

*Why it exists:* this is Musk's "question and delete" and the loop roadmap's
"do it manually first", which are the same instruction. The manual runs **are**
the spec.

```yaml
pre_execution:
  - step: Ran this task by hand end to end and kept the transcript
    done: false
    evidence: "link it"
```

Every step must be `done: true` before the loop will run. This is the only
place the tool refuses to proceed on a matter of process rather than syntax.

## C · Goals

*Why it exists:* named, natural-language objectives that nodes and validations
both point at.

```yaml
goals:
  - name: draft                 # `overall` is reserved
    description: Produce the brief with every claim traced to a source.
    depends_on: [gather]        # optional
    priority: 1                 # optional
```

Subjective phrasing is fine here. The **validation** is what has to be
checkable.

## D · Validation list

*Why it exists:* the gate. Everything else in a loop is plumbing; this decides
whether it helped or just spent money.

```yaml
validations:
  - target: draft               # a goal name, or `overall`
    name: every-claim-cited
    mode: objective             # subjective | objective | percentage
    statement: The citation checker finds no uncited claim.
    blocking: true              # default; false records without holding the gate
    detector: { type: script, command: scripts/check-citations.sh }
```

Detectors, strongest first:

| Detector | Decides by | Use when |
|---|---|---|
| `script` | Exit code | Anything a command can settle. Prefer this. |
| `file_exists` | Path present, optionally non-empty | A deliverable must exist |
| `regex_match` | Pattern against a named artifact | A required phrase or format |
| `threshold` | Reported metric vs a number | Coverage, counts, ratios |
| `judge` | A model verdict against a **named standard** | Genuinely subjective quality |

`judge` is the weakest rung and the runtime treats it that way: a verdict from
the same provider that produced the work is **refused**, not discounted. Name
the external standard — Nielsen's heuristics, WCAG 2.2 AA, your own style
guide. An unnamed standard is an opinion.

**Every goal needs at least one blocking validation.** A goal you cannot check
is a goal the loop can never honestly finish, so the config is rejected.

## E · Success scenarios

*Why it exists:* validations say what is checked; success says how much of it
has to hold.

```yaml
success:
  - target: overall
    name: complete-and-cited
    mode: percentage
    statement: Every blocking validation passes.
    threshold: 1.0              # required when mode is percentage
```

## F · Stop gates

*Why it exists:* a loop with no exit runs until it succeeds, breaks, or drains
the account. Loops fail quietly — they do not crash, they bill you in silence.

```yaml
stop_gates:
  max_iterations: 8
  max_revisions_per_node: 3
  max_wall_clock_seconds: 3600
  max_tokens: 2000000
  max_cost_usd: 5.0
  no_progress_iterations: 3     # jidoka: stop the line. 0 disables
  stop_on_overall_success: true
```

All four exits are evaluated every iteration and all are hard logic. Declare at
least one budget ceiling; without one, an unsolvable task bills until someone
notices.

Every trigger is written to the ledger, not just successes. A node that hits
its ceiling constantly is telling you its judge is miscalibrated.

## G · Schedule triggers

```yaml
schedules:
  - type: manual
  - type: cron
    expr: "0 2 * * *"       # five fields, evaluated in UTC
  - type: interval
    seconds: 3600           # timezone-independent; prefer this for cadence
  - type: file_change
    path: src/
  - type: goal_satisfied
    goal: gather
```

`loopsmith watch <config>` stays resident and runs the loop whenever one of
these fires; `loopsmith schedule <config> --install` hands the job to launchd
or cron so it survives a reboot. File and goal triggers fire on the *edge*, so
a goal that stays satisfied does not retrigger.

Schedule last. Scheduling something you have not made reliable by hand is how
loops blow up overnight.

## H · Constraints

*Why it exists:* the loop's leash. Applied globally, then merged per node —
rules append, limits override.

```yaml
constraints:
  global:
    rules:
      - Never git stash. Never git reset.
      - No git command except committing a specific file.
      - No slow commands before the test phase.
    forbidden_paths: [".git/", "state/"]
    forbidden_commands: ["rm -rf", "git push"]
    max_seconds: 900
    human_checkpoint:
      - publishing anything
      - sending a message
      - deleting data
  per_node:
    critic:
      rules:
        - You are not here to approve.
```

`human_checkpoint` stops and waits **regardless of any permission grant**.
Bezos Type 1: irreversible decisions do not get made at machine speed.

Those three git rules are the frozen set that let a 64-agent run share four
checkouts without clobbering. Keep them whenever builders can run in parallel.

---

# The graph

```yaml
graph:
  nodes:
    - id: write
      role: builder           # builder | judge | manager | adversary | researcher
      instruction: Draft the brief. Every claim carries its citation inline.
      depends_on: [search]    # ONLY if this node reads that node's output
      goals: [draft]
      tier: standard          # cheap | standard | strong
      provider: openai        # optional pin; pin judges to a different family
      weight: 3.0             # relative cost, drives the critical path
      isolated: true          # own git worktree; required for parallel builders
  concurrency:
    mode: auto                # sequential | fixed | auto
    cap: 16
    min_marginal_gain: 0.05
```

**The one question that builds a graph:** for every "and then", does the next
step actually *read* the previous step's output? Yes is a real edge. No was
never an edge — run them together.

`auto` derives the parallel fraction from the graph and adds workers only while
the next one still buys `min_marginal_gain` of Amdahl speedup. `loopsmith plan`
shows the arithmetic before you spend anything.

---

# Providers

Every provider is a **command template**, so any CLI or HTTP endpoint you can
run from a shell works with no code change.

```yaml
providers:
  providers:
    - id: ollama
      kind: ollama            # claude_code | ollama | grok_cli | grok_build
      tiers: [cheap]          # | hermes | openai | gemini | byok | mcp
      command: ollama
      args: ["run", "{model}"]
      model: llama3
      prompt_on_stdin: true
      timeout_seconds: 600
  cascade:
    cheap:    [ollama, claude]
    standard: [claude, gemini]
    strong:   [openai, claude]
  enforce_judge_independence: true
```

Placeholders: `{prompt}` `{system}` `{model}` `{tier}` `{node}`.

`requires_env` names keys that must be present. Values are never read,
substituted, or logged — pass secrets through the command itself (`curl`
expanding `$OPENAI_API_KEY`) so they never enter the ledger.

**Spend accounting.** Add `usage_regex` to pull a real token count out of the
provider's output, and `cost_per_1k_tokens` to price it:

```yaml
  usage_regex: '"total_tokens"\s*:\s*(\d+)'
  cost_per_1k_tokens: 0.0006
```

Without a regex, usage is estimated at roughly four characters per token and
every report says so. An approximate ceiling that fires beats an exact one that
never does.

Cheap tiers carry mechanical work; strong tiers carry judgment. Spending
frontier reasoning on extraction is where loop budgets die.

---

# Sub-agent acquisition

```yaml
skills:
  acquisition_order: [installed, marketplace, generate]
  quarantine_dir: generated-skills
  min_marketplace_stars: 100
  require_human_promotion: true
  explore: false                                 # on = try things you did not configure
  explore_candidates: [table-formatter, chart-maker]
  min_trials: 3
```

Installed first, then the marketplace, then generate a new one. Anything
acquired lands in quarantine — an auto-acquired sub-agent is a proposal, not a
decision, and promotion into `~/.claude/skills/` stays a human act.

**Exploration** is how the loop discovers what helps rather than only
confirming what you told it. With `explore: true`, each iteration attaches one
under-trialled candidate to a builder node, and the gate outcome that follows
is recorded against that skill. After `min_trials`, what correlates with
satisfied goals becomes a proposal:

```bash
loopsmith skills scores loop.yaml       # ranked by satisfaction rate
loopsmith proposals loop.yaml <run-id>  # adopt / drop suggestions
```

It is off by default because exploration spends real money, and below
`min_trials` a result is recorded and ignored — one lucky run is not evidence.

---

# What the loop may change about itself

| Does on its own | Only proposes |
|---|---|
| Acquire, install, or generate sub-agents (quarantined) | Goals |
| Trial candidates and score them against gate outcomes | Validations |
| Write scratchpad notes between iterations | Success scenarios |
| | Which skills the config uses |

The loop cannot move its own goalposts. A system that can rewrite the criteria
it is judged against cannot certify that it met them.

---

# Checklist before first run

- [ ] Every `pre_execution` step actually done, not just marked done
- [ ] Every goal has a blocking validation
- [ ] At least one budget ceiling set
- [ ] Judge nodes pinned to a different provider family than their builder
- [ ] Parallel builders marked `isolated: true`
- [ ] `human_checkpoint` covers everything irreversible in your domain
- [ ] `loopsmith plan` speedup looks like the work you expect
- [ ] Permission grant reviewed and written once
- [ ] A budget ceiling that can actually fire — set `usage_regex` or accept the estimate
- [ ] For a long-lived loop: a non-manual trigger, or `watch` refuses to start
