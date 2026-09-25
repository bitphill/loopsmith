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
> `REPLACE ME`, and put the config below in `loop.yaml` beside it. This body
> stays thin on purpose: the config is data the runtime validates, and data
> in a config file can be checked, diffed, and scheduled. Prose in a skill
> body cannot.
>
> Faster path: `loopsmith loop new --path <dir>` writes both files for you,
> then edit them. Faster still: `loopsmith --guided` asks for each field in
> turn and explains it as it goes.

## What this loop is for

REPLACE ME — one paragraph. What outcome, for whom, and why a loop rather than
a single prompt.

## Mandatory argument

`--path` / `-p` is required. A loop owns durable state — a sled ledger,
checkpoints, quarantined sub-agents — and needs a directory of its own.
Without it you get several half-finished loops writing into each other's
state.

```bash
loopsmith loop new --path ./loops/my-purpose --purpose "what it is for"
```

## Run it

```bash
loopsmith loop validate    <path>/loop.yaml   # complete and consistent
loopsmith loop plan        <path>/loop.yaml   # waves, critical path, real speedup
loopsmith loop permissions <path>/loop.yaml --write .claude/settings.local.json
loopsmith run start        <path>/loop.yaml   # hands-off from here
```

`loop validate` fails while any `intent.prerequisites` step is unfinished. That
is the gate, not a nag: a loop built around a process you have not performed by
hand produces fast, confident garbage.

---

# The model

Everything below lives in `loop.yaml`. Eight top-level keys: the loop's
identity, where it is running, what it is allowed to do at all, and four
bundles grouping the sections by what they are *for*.

The runtime validates it against `config/loop.schema.json`, which is generated
from the Rust types. Anything a schema cannot express — every goal having a
blocking check, targets resolving, cycles, a judge sharing its builder's
provider — is checked by `loopsmith loop validate`.

Coming from 0.3? Every lettered key still parses, and
[the migration page](wiki/Migration-0-3-To-1-0.md) is the mapping.

## Identity and blast radius

```yaml
name: my-purpose-loop
version: 1.0.0
description: One sentence. Shown on the loop's card and given to every node.
environment: dev              # dev | staging | prod

features:
  self_evolution: false       # may the loop propose changes to itself
  marketplace_skills: false   # may it acquire sub-agents it did not ship with
  external_side_effects: false # may a node reach outside the loop's directory
  parallel_execution: true    # run waves wide
  human_approval: true        # checkpoints actually stop. `prod` refuses false
```

*Why `features` exists:* these are capabilities, not settings. The two that are
off decide what the loop can *become*; the two that are on decide whether its
limits are real. `environment: prod` refuses `human_approval: false` outright,
because a checkpoint you can switch off is not a checkpoint.

---

# `intent` — what the loop is for

## `intent.background` · Information

*Why it exists:* every node starts fresh with only its spawn prompt. Whatever
is not here has to be rediscovered, badly, by each of them.

```yaml
intent:
  background:
    - key: output_path
      value: out/result.md
      note: Optional. Why this fact matters.
```

Put durable facts here — paths, conventions, the one number everyone needs.
Not instructions; those belong to a node. Not anything that changes during a
run; that belongs in the run's own memory.

## `intent.prerequisites` · Pre-execution work

*Why it exists:* this is Musk's "question and delete" and the loop roadmap's
"do it manually first", which are the same instruction. The manual runs **are**
the spec.

```yaml
  prerequisites:
    - step: Ran this task by hand end to end and kept the transcript
      done: false
      evidence: "link it"
```

Every step must be `done: true` before the loop will run. This is the only
place the tool refuses to proceed on a matter of process rather than syntax.

## `intent.goals` · Goals

*Why it exists:* named, natural-language objectives that nodes and checks both
point at.

```yaml
  goals:
    - name: draft                 # `overall` is reserved
      description: Produce the brief with every claim traced to a source.
      depends_on: [gather]        # optional
      priority: 1                 # optional
```

Subjective phrasing is fine here. The **check** is what has to be decidable.

## `intent.success` · Success scenarios

*Why it exists:* checks say what is verified; success says how much of it has
to hold.

```yaml
  success:
    - target: overall
      name: complete-and-cited
      mode: percentage            # subjective | objective | percentage
      statement: Every blocking check passes.
      threshold: 1.0              # required when mode is percentage
```

---

# `execution` — how the work gets done

## `execution.graph` · Nodes and dependencies

```yaml
execution:
  graph:
    nodes:
      - id: write
        role: builder           # builder | judge | manager | adversary | researcher
        instruction: Draft the brief. Every claim carries its citation inline.
        depends_on: [search]    # ONLY if this node reads that node's output
        goals: [draft]
        tier: standard          # cheap | standard | strong
        provider: openai        # optional pin; pin judges to a different family
        stage: draft            # optional; the phase this node belongs to
        weight: 3.0             # relative cost, drives the critical path
        isolation:
          mode: worktree        # none | worktree | container
      - id: review
        role: judge
        instruction: Check every claim against the source it cites. Do not fix anything.
        depends_on: [write]
        goals: [draft]
        tier: strong
        provider: claude        # a different family from `write`, or it is refused
    concurrency:
      mode: auto                # sequential | fixed | auto
      cap: 16
      min_marginal_gain: 0.05
    join:
      strategy: wait_for_all    # wait_for_all | quorum | first_success
    container_image: null       # default image for `container` nodes
```

**The one question that builds a graph:** for every "and then", does the next
step actually *read* the previous step's output? Yes is a real edge. No was
never an edge — run them together.

`auto` derives the parallel fraction from the graph and adds workers only while
the next one still buys `min_marginal_gain` of Amdahl speedup.
`loopsmith loop plan` shows the arithmetic before you spend anything.

**Isolation** is per node. Parallel writers need `worktree` or they overwrite
each other; `container` adds a container over that worktree, with the network
off unless you ask for it. A host with no Docker degrades to `worktree` with a
warning — `loopsmith doctor` says which of the three this machine can give you.

**Join** decides what releases a wave. `quorum` is for several nodes attacking
the same question where the run does not need all the answers.

## `execution.providers` · Providers

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
      - id: claude
        kind: claude_code
        tiers: [standard, strong]
        command: claude
        args: ["-p", "{prompt}"]
      - id: openai
        kind: openai
        tiers: [standard, strong]
        command: curl
        args: ["-sS", "https://api.openai.com/v1/chat/completions"]
        requires_env: [OPENAI_API_KEY]
        prompt_on_stdin: true
    cascade:
      cheap:    [ollama, claude]
      standard: [claude, openai]
      strong:   [openai, claude]
    enforce_judge_independence: true
```

Every id a node pins or a cascade names has to be in `providers`. Two
different families is the minimum for judge independence to be satisfiable —
with one provider, every judge is refused and nothing can ever pass.

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

## `execution.phases` · Execution guidelines

*Why it exists:* ordering that is about **method** rather than about data.

```yaml
  phases:
    items:
      - name: gather
        guideline: Collect sources. Write nothing yet.
      - name: draft
        guideline: Write only from what gather collected.
    dependency:
      - gather -> draft -> review
```

A node joins a phase with `stage:` and is not dispatched until that phase is
active. Use `depends_on` only for a node that genuinely reads another's output
— overloading it with method makes the critical path meaningless.

## `execution.default_skills` · Sub-agents installed up front

```yaml
  default_skills:
    - name: agent-reach
      source: github            # marketplace | github | local
      url: https://github.com/Panniantong/agent-reach
      init_command: npm install # ARGV, not a shell line
```

Idempotent, so it runs at the start of every run and a loop directory can be
rebuilt from its config alone. `github` clones an **https** repo into the
quarantine directory; `git://`, `ssh://` and `file://` are refused.

## `execution.skills` · How a new sub-agent is acquired

```yaml
  skills:
    acquisition_order: [installed, marketplace, generate]
    quarantine_dir: generated-skills
    min_marketplace_stars: 100
    require_human_promotion: true
    min_trust_level: reviewed
    require_checksum: true
    allow_external_side_effects: false
    explore: false              # on = try things you did not configure
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
loopsmith skills scores loop.yaml           # ranked by satisfaction rate
loopsmith run proposals loop.yaml <run-id>  # adopt / drop suggestions
```

It is off by default because exploration spends real money, and below
`min_trials` a result is recorded and ignored — one lucky run is not evidence.

## `execution.memory` · What each prompt carries

```yaml
  memory:
    carry_summaries: 2          # 0 disables
    max_summary_chars: 1200
    summary_provider: null      # optional; the facts are written either way
    max_retrieved: 10
    namespaces:
      episodic:
        enabled: true
        retention_days: 30
        promotion: { rule: never }
      semantic:
        enabled: true
        promotion: { rule: human_approval }   # or `repeated_validation`, `times: 3`
        min_confidence: 0.7
        require_provenance: true
      procedural:
        enabled: true
        promotion: { rule: automatic }
      failure:
        enabled: true
        promotion: { rule: automatic }
```

*Why namespaces exist:* "what happened in this run" and "what we learned about
the domain" have different lifetimes and different rules about who may write
them. The engine writes failures and procedures; agents write facts through the
MCP `loopsmith_remember` tool, and a `human_approval` namespace promotes
nothing until you run `loopsmith memory promote`.

## `execution.triggers` · What starts a run

```yaml
  triggers:
    triggers:
      - on: { type: manual }
      - on: { type: cron, expr: "0 2 * * *" }   # five fields, UTC
      - on: { type: interval, seconds: 3600 }   # prefer this for cadence
      - on: { type: file_change, path: src/ }
      - on: { type: goal_satisfied, goal: draft }
        idempotency_key: nightly
    max_depth: 2                # how deep a run may trigger another
    dedup_window_seconds: 3600  # how long a key suppresses a repeat
```

`loopsmith run watch <config>` stays resident and runs the loop whenever one of
these fires; `loopsmith run schedule <config> --install` hands the job to
launchd, cron, or Task Scheduler so it survives a reboot. File and goal
triggers fire on the *edge*, so a goal that stays satisfied does not retrigger.

`max_depth` and `idempotency_key` are what stop a loop that triggers itself
from becoming a fork bomb with a billing account.

Schedule last. Scheduling something you have not made reliable by hand is how
loops blow up overnight.

---

# `safety` — what must not happen, and when to stop

## `safety.checks` · Validations

*Why it exists:* the gate. Everything else in a loop is plumbing; this decides
whether it helped or just spent money.

```yaml
safety:
  checks:
    - target: draft               # a goal name, or `overall`
      name: every-claim-cited
      mode: objective             # subjective | objective | percentage
      statement: The citation checker finds no uncited claim.
      blocking: true              # default; false records without holding the gate
      detector: { type: script, command: scripts/check-citations.sh }
    - target: overall
      name: brief-is-publishable
      mode: subjective
      statement: The brief reads as a finished piece against the house style guide.
      blocking: true
      detector: { type: judge, standard: docs/style-guide.md }
```

An `overall` check is what lets the loop finish as a whole rather than only
per goal. Without one, every goal can be satisfied and the run still has
nothing to say about whether the job is done.

Detectors, strongest first:

| Detector | Decides by | Use when |
|---|---|---|
| `script` | Exit code | Anything a command can settle. Prefer this. |
| `file_exists` | Path present, optionally non-empty | A deliverable must exist |
| `regex_match` | Pattern against a named artefact | A required phrase or format |
| `threshold` | Reported metric vs a number | Coverage, counts, ratios |
| `judge` | A model verdict against a **named standard** | Genuinely subjective quality |

`judge` is the weakest rung and the runtime treats it that way: a verdict from
the same provider that produced the work is **refused**, not discounted. Name
the external standard — Nielsen's heuristics, WCAG 2.2 AA, your own style
guide. An unnamed standard is an opinion.

**Every goal needs at least one blocking check.** A goal you cannot verify is a
goal the loop can never honestly finish, so the config is rejected.

## `safety.gates.stop` · Stop gates

*Why it exists:* a loop with no exit runs until it succeeds, breaks, or drains
the account. Loops fail quietly — they do not crash, they bill you in silence.

```yaml
  gates:
    stop:
      max_iterations: 8
      max_revisions_per_node: 3
      max_wall_clock_seconds: 3600
      max_tokens: 2000000
      max_cost_usd: 5.0
      no_progress_iterations: 3            # jidoka: stop the line. 0 disables
      no_progress_iterations_randomness: 2 # try something different first
      stop_on_overall_success: true
```

All of them are evaluated every iteration and all are hard logic. Declare at
least one budget ceiling; without one, an unsolvable task bills until someone
notices.

`no_progress_iterations_randomness` must be strictly less than
`no_progress_iterations`, or the loop halts before it ever tries something
different. When it fires, a cheap agent picks one of four fixed tactics —
`reorder`, `escalate`, `explore`, `reframe` — so it can change how the loop
works and cannot change what counts as done.

Every trigger is written to the ledger, not just successes. A node that hits
its ceiling constantly is telling you its judge is miscalibrated.

## `safety.gates.entry` · Before the first dispatch

```yaml
    entry:
      - id: clean-tree
        statement: The working tree has no uncommitted changes.
        detector: { type: script, command: scripts/clean-tree.sh }
        on_fail: stop           # stop | escalate | pause | rollback | warn
```

Checked once, while the run is still validating and before a single provider is
called. The clean branch, the key that answers, the disk with room on it — the
conditions that make the whole run pointless if they are false.

## `safety.gates.approval` · Before the loop is allowed to work

```yaml
    approval:
      - id: budget-signed-off
        statement: Someone has approved this run's plan and its ceiling.
        detector: { type: file_exists, path: .approved }
        on_fail: pause
```

Checked after planning, so the plan is on the table when the decision is made.
For work whose cost of being wrong is external: money moving, mail leaving,
something published. An approval gate whose detector can satisfy itself is not
an approval, it is a delay.

## `safety.gates.rollback` · Undo the last iteration

```yaml
    rollback:
      - id: suite-still-green
        statement: The test suite passes.
        detector: { type: script, command: scripts/test.sh }
        on_fail: rollback
```

Checked every iteration. A rollback discards the work of the iteration that
tripped it. Spend is not refunded — nothing can refund that — so this is about
not building on top of a bad iteration.

## `safety.limits` · Constraints

*Why it exists:* the loop's leash. Applied globally, then merged per node —
rules append, limits override.

```yaml
  limits:
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

## `safety.recovery` · What to do about each kind of failure

```yaml
  recovery:
    transient_error:      { action: retry, max_attempts: 3, backoff: exponential }
    invalid_output:       { action: revise, max_attempts: 2 }
    tool_unavailable:     { action: fallback }
    repeated_failure:     { action: escalate }
    safety_violation:     { action: stop }
    resource_exhaustion:  { action: pause }
    corrupted_state:      { action: restore_checkpoint }
```

Decided before it happens, and deliberately unequal: a transient error is
retried, a safety violation never is. One policy for every failure either
retries a safety violation or gives up on a flaky network. The values above are
the defaults — omit the section entirely to get them.

## `safety.alerts` · Told while it is still going

```yaml
  alerts:
    - id: costly
      metric: cost_usd          # iterations | tokens_used | cost_usd |
      above: 2.0                # wall_clock_seconds | failed_dispatches |
      message: Half the ceiling, and not done.  # retries | stale_iterations |
                                # validation_pass_rate
```

An alert stops nothing; that is what stop gates are for. It is the thing that
says a run is going wrong an hour before the ceiling would have said it.

## `safety.protected` · What self-evolution may never touch

```yaml
  protected:
    components: [gates, limits, recovery, protected, approvals,
                 credentials, audit, baselines, retention, environment]
    extra_paths: ["scripts/check-citations.sh"]
```

A proposal that would rewrite one of these is refused before it is evaluated
rather than after. A loop allowed to edit its own limits does not have limits.

---

# `evolution` — how the loop may improve itself

```yaml
evolution:
  enabled: false
  max_regression: 0.05
  allowed_kinds: [new_skill, skill_update, prompt_change]
  require_sandbox: true
  require_approval: true
  keep_rollback: true
  baseline:
    completion_rate: 0.8
    validation_pass_rate: 0.9
    cost_usd: 1.5
    latency_seconds: 600
    iterations_to_success: 4
    measured_at: "2026-01-01"
```

*Why the baseline exists:* an evolution with nothing to be better than makes
every proposal look like an improvement. A proposal that regresses any of these
by more than `max_regression` is refused by the gate rather than by a
reviewer's patience.

| Does on its own | Only proposes |
|---|---|
| Acquire, install, or generate sub-agents (quarantined) | Goals |
| Trial candidates and score them against gate outcomes | Checks |
| Write scratchpad notes between iterations | Success scenarios |
| | Which skills the config uses |

The loop cannot move its own goalposts. A system that can rewrite the criteria
it is judged against cannot certify that it met them.

---

# Checklist before first run

- [ ] Every `intent.prerequisites` step actually done, not just marked done
- [ ] Every goal has a blocking check in `safety.checks`
- [ ] At least one budget ceiling in `safety.gates.stop`
- [ ] `no_progress_iterations_randomness` strictly below `no_progress_iterations`
- [ ] Judge nodes pinned to a different provider family than their builder
- [ ] Parallel builders given `isolation: { mode: worktree }`
- [ ] `human_checkpoint` covers everything irreversible in your domain
- [ ] `features.human_approval` left on, and `environment` set honestly
- [ ] An entry gate for whatever makes the whole run pointless if it is false
- [ ] `safety.protected` reviewed if `features.self_evolution` is on
- [ ] `loopsmith loop plan` speedup looks like the work you expect
- [ ] Permission grant reviewed and written once
- [ ] A budget ceiling that can actually fire — set `usage_regex` or accept the estimate
- [ ] For a long-lived loop: a non-manual trigger, or `run watch` refuses to start
