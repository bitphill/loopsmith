<div align="center">
  <img src="assets/loopsmith-logo-512.png" alt="loopsmith logo" width="200" />
  <h1>loopsmith</h1>
  <h2>Detailed reference</h2>
  <p><em>The long version. Start at <a href="README.md">README.md</a> if you have not run a loop yet.</em></p>
  <p>
    <img alt="rust" src="https://img.shields.io/badge/rust-1.75%2B-C1272D?logo=rust&logoColor=white" />
    <img alt="tests" src="https://img.shields.io/badge/tests-240%20passing-2A5A8A" />
    <img alt="license" src="https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222" />
    <img alt="platforms" src="https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A" />
  </p>
  <p>
    <a href="#quickstart">Quickstart</a> ·
    <a href="#architecture">Architecture</a> ·
    <a href="#the-aj-model">The A–J model</a> ·
    <a href="#providers">Providers</a> ·
    <a href="#why-the-gate-is-rust">Why the gate is Rust</a>
  </p>
</div>

---

**loopsmith** builds agent loops that can be trusted to run unattended. You describe a purpose in a config — goals, how each is checked, what counts as success, when to stop, what the loop may never do — and the runtime handles scheduling, provider routing, shared memory, verification, and termination.

The design rests on one finding, which the whole architecture exists to enforce:

> A model must not be the thing that certifies its own completion.

So `goal_satisfied` is written by a deterministic Rust gate and by nothing else. No prompt, no confident summary, and no model that likes its own work can set it. The gate can also **revoke** — delete a required artifact and a previously satisfied goal flips back. A system that can only promote is a burndown chart with extra steps.

---

## Quickstart

```bash
# build (rustup users: PATH is often missing; rustup writes to ~/.profile,
# which zsh never reads)
export PATH="$HOME/.cargo/bin:$PATH"
cd runtime && cargo build --release
cp target/release/loopsmith /usr/local/bin/

# create a purpose-specific loop — --path is mandatory
loopsmith new --path ./loops/nightly-refactor --purpose "keep the module simple"

# the order that works
loopsmith validate ./loops/nightly-refactor/loop.yaml
loopsmith plan     ./loops/nightly-refactor/loop.yaml
loopsmith permissions ./loops/nightly-refactor/loop.yaml --write .claude/settings.local.json
loopsmith run      ./loops/nightly-refactor/loop.yaml
```

`validate` **fails on purpose** until you mark the `pre_execution` steps done:

```
error  pre_execution: 2 step(s) not marked done: Run this task manually end to
       end at least once; Write down what 'done' means in checkable terms.
       Automating before understanding produces fast, confident garbage
```

That refusal is the most valuable thing the tool does. A loop wrapped around a process nobody has performed by hand produces confident garbage at scale.

---

## Architecture

```
INVOCATION      loopsmith new --path <dir>
                  └─ permission preflight (one grant) → hands-off
                       │
CONTROL PLANE   loopsmith (Rust)                  ← owns truth
                  ├─ core      A–J config model and validation
                  ├─ graph     DAG, waves, critical path, Amdahl sizing
                  ├─ memory    sled: episodes, goal state, ledger, checkpoints
                  ├─ gate      deterministic verdicts — the ONLY writer of
                  │            goal_satisfied, and able to revoke it
                  ├─ provider  command-template routing to any CLI or API
                  └─ mcp       stdio server: plan, ledger, gate, scratchpad
                       │
EXECUTION       Any provider                       ← owns judgment
                  Claude Code · Ollama · Grok CLI · Grok Build · OpenAI ·
                  Gemini · Hermes · any BYOK command · any MCP server
```

The orchestrator is a binary rather than a chat session because a loop has to survive a crash, a schedule boundary, and a budget ceiling. Sessions are ephemeral and have no resume; a sled ledger does. Coordination is also a solved deterministic problem — spending model tokens on scheduling is the same mistake as spending frontier reasoning on entity extraction.

### Commands

| Command | Does |
|---|---|
| `new --path <dir>` | Scaffold a purpose-specific loop. `--path` is required |
| `validate <config>` | Check the A–J model; fails on unfinished manual work |
| `plan <config>` | Waves, critical path, parallel fraction, predicted speedup |
| `run <config>` | Execute once. `--dry-run` plans without spending anything |
| **`watch <config>`** | **Stay resident and run whenever a trigger fires — this is what makes a loop live for weeks** |
| `schedule <config> [--install]` | Hand the schedule to launchd or cron so it survives a reboot |
| `resume <config> <run-id>` | Continue from the last checkpoint |
| `status <config> <run-id>` | Gate rulings per goal |
| `ledger <config> <run-id>` | Everything that happened, including every stop-gate trigger |
| `gate <config> --target <goal>` | Ask the gate now, without a provider call |
| `skills search <terms...>` | Search claudemarketplaces.com and skills.sh |
| `skills acquire <config> <name>` | Install a sub-agent into quarantine |
| `skills list <config> [--all]` | Sub-agents this loop can see |
| `skills scores <config>` | Rank sub-agents by the gate outcomes that followed their use |
| `proposals <config> <run-id>` | What the loop wants changed about itself |
| `providers <config>` | Which providers are usable, and why not |
| `permissions <config> [--write f]` | Derive and merge the narrowest grant |
| `prune <config>` | Remove the git worktrees this loop created |
| `mcp --state <dir>` | Serve the control plane over stdio MCP |

---

## The A–J model

The whole config, in ten sections. Full reference in [`HOW-TO-USE.md`](HOW-TO-USE.md); fill-in template in [`LOOP-TEMPLATE.md`](LOOP-TEMPLATE.md).

| | Section | What it is |
|---|---|---|
| **A** | `information` | Static facts every node receives |
| **B** | `pre_execution` | The manual work list. All steps must be done before the loop runs |
| **C** | `goals` | Named objectives in natural language |
| **D** | `validations` | How each goal is checked — per goal or `overall` |
| **E** | `success` | What counts as success, including percentage thresholds |
| **F** | `stop_gates` | Four layered exits |
| **G** | `schedules` | Time or event triggers |
| **H** | `constraints` | Limits per node or global, including human checkpoints |
| **I** | `execution_guidelines` | Named phases, each with a standing instruction and a place in an ordering |
| **J** | `default_skills` | Sub-agents installed before the loop starts, from a marketplace or a GitHub repo |

Two more sections exist that are not part of the lettered model, because they
configure the runtime rather than the work: `graph` (nodes and concurrency),
`providers` (routing), `skills` (acquisition policy), and `context` (how much of
the previous iterations each prompt carries).

### I — execution guidelines

Section G's `graph` edges mean one thing only: *this node reads that node's
output*. Section I expresses the other kind of ordering, the one about method
rather than data — gather before you draft, land the tests before you refactor.
Keeping them apart is what stops the critical path from becoming meaningless.

```yaml
execution_guidelines:
  items:
    - name: gather
      guideline: Collect sources. Write nothing yet.
    - name: draft
      guideline: Write only from what gather collected.
  dependency:
    - gather -> draft -> review

graph:
  nodes:
    - id: search
      stage: gather        # not dispatched until the phase is active
```

A phase opens when every phase before it is complete, and completes when its own
nodes have run and the gate has satisfied the goals they advance. Guidelines with
no arrow between them run in parallel. A cycle is a validation error, caught
before anything is dispatched.

### Markdown configs

Every config can be written as Markdown instead of YAML, with the same model:

```markdown
# my-loop

## C. Goals

### ship-it
- description: the thing is shipped and the suite is green
```

Headings are sections, `###` headings are entries, bullets are fields, and prose
at the left margin is documentation the parser ignores — so a config can explain
itself in place. `loopsmith convert <config>` translates in either direction, and
a round-trip test keeps the two grammars honest against every shipped example.

### Validation is where loops live or die

Detectors, strongest first:

| Type | Decides by |
|---|---|
| `script` | Exit code — prefer this |
| `file_exists` | Path present, optionally non-empty |
| `regex_match` | Pattern against a named artifact |
| `threshold` | Reported metric versus a number |
| `judge` | A model verdict against a **named external standard** |

`judge` is the weakest rung and the runtime treats it accordingly: a verdict produced by the same provider as the work it judges is **refused**, not discounted.

```
[FAIL] prose — judgment refused: judge and builder both ran on `claude`;
       a shared provider shares its blind spots
```

**Every goal needs at least one blocking validation**, or the config is rejected. A goal you cannot check is a goal the loop can never honestly finish.

### The four stop gates

Verifier satisfied · iteration cap · budget ceiling (tokens, cost, wall-clock) · no measurable progress. All evaluated every iteration, all hard logic. "Stop when it's good enough" inside a prompt is a suggestion a model will eventually talk itself past.

Every trigger is written to the ledger, not just successes — a node that hits its ceiling constantly is telling you its judge is miscalibrated, and that signal is invisible if you only record completions.

---

## Concurrency you can justify

`loopsmith plan` derives the parallel fraction from the graph itself and sizes the fleet by arithmetic instead of optimism:

```
Waves (3 total):
   1. survey
   2. refactor-a, refactor-b
   3. review

Critical path (5.0 cost): survey -> refactor-a -> review
Parallel fraction p: 0.375
Concurrency chosen:  2
Predicted speedup:   1.23x  (ceiling 1.60x at infinite workers)
```

Amdahl's law is the cap and the critical path is the floor. At p=0.95, sixteen workers buy ×9.14 — not ×16. `auto` mode adds workers only while the next one still buys a configurable slice of additional speedup, then stops.

**The question that builds a graph:** on every "and then", does the next step actually *read* the previous step's output? Yes is a real edge. No was never an edge — run them together, and cut a false edge rather than adding a worker.

---

## Providers

Every provider is a **command template**, which is what makes BYOK free: if you can run it from a shell, loopsmith can route to it. Adding one is a config edit, never a rebuild.

```yaml
providers:
  providers:
    - id: ollama
      kind: ollama
      tiers: [cheap]
      command: ollama
      args: ["run", "{model}"]
      model: llama3
      prompt_on_stdin: true

    - id: openai
      kind: openai
      tiers: [strong]
      command: curl
      args: ["-sS", "https://api.openai.com/v1/chat/completions",
             "-H", "Authorization: Bearer $OPENAI_API_KEY", "-d", "@-"]
      requires_env: [OPENAI_API_KEY]
      prompt_on_stdin: true

  cascade:
    cheap:    [ollama, claude]
    standard: [claude, gemini]
    strong:   [openai, claude]
```

Supported kinds: `claude_code`, `ollama`, `grok_cli`, `grok_build`, `hermes`, `openai`, `gemini`, `byok`, `mcp`. Common aliases (`claude`, `grok`, `open_ai`, `google_gemini`, `custom`) are accepted, because a config that rejects `openai` in favour of `open_ai` wastes your afternoon.

**Secrets never enter the process.** `requires_env` names keys that must be present; values are never read, substituted into arguments, or written to the ledger. Let the command expand them itself, as `curl` does above.

```bash
$ loopsmith providers loop.yaml
claude       available    claude
ollama       available    ollama
openai       unavailable  missing env: OPENAI_API_KEY
gemini       unavailable  command not found on PATH; missing env: GEMINI_API_KEY
```

Cheap tiers carry mechanical, high-volume work; strong tiers carry judgment. Spending frontier reasoning on extraction is where loop budgets die.

**Spend accounting.** Set `usage_regex` to pull a real token count out of a provider's output and `cost_per_1k_tokens` to price it. Without a regex, usage is estimated at roughly four characters per token and every report says so:

```
spend:       263 tokens (estimated: no provider reported usage), $0.0000
```

An approximate ceiling that fires beats an exact one that never does, which is what an unaccounted budget gate amounts to.

---

## Why the gate is Rust

Five independently written sources on loop engineering converge on the same rule from different directions, and they form an escalation of trust:

1. **Separate prompt** — weakest. Same context, same blind spots.
2. **Separate context** — a verifier that never saw the work being made.
3. **Separate model family** — avoids characteristic blind spots.
4. **Separate mechanism** — deterministic code decides. Strongest.

`loopsmith-gate` sits at rung 4. The reasoning is in [`loops-engineering-cheat-sheet.md`](loops-engineering-cheat-sheet.md), which distils all twenty sources and records what was borrowed, what was rejected, and why.

The MCP server makes the same point by omission: it exposes the plan, the ledger, the gate's verdict, and the scratchpad — and has **no tool for marking a goal satisfied**. There is a test asserting that absence, so removing the guarantee cannot happen quietly.

---

## Running for weeks

`run` executes once. `watch` is the process that keeps a loop alive:

```bash
loopsmith watch loop.yaml              # until interrupted
loopsmith watch loop.yaml --check      # show triggers, run nothing
loopsmith schedule loop.yaml --install # survive a reboot (launchd / cron)
```

Triggers: `cron` (five fields, **evaluated in UTC**), `interval` (timezone-independent, preferred for plain cadence), `file_change`, and `goal_satisfied`. File and goal triggers fire on the *edge*, not the level, so a satisfied goal does not retrigger forever, and the watcher ignores its own `state/` directory so the ledger's writes cannot retrigger it.

A failed run logs and the watcher continues. That difference — a crash ending one run instead of the whole schedule — is what separates a scheduler from a one-shot.

## Self-evolution, bounded

The loop **discovers** which sub-agents help rather than being told:

```yaml
skills:
  explore: true                                  # off by default; it spends money
  explore_candidates: [table-formatter, chart-maker]
  min_trials: 3
```

Each iteration attaches one under-trialled candidate to a builder, records the gate outcome that followed, and ranks by satisfaction rate. What correlates with satisfied goals becomes a proposal:

```
$ loopsmith skills scores loop.yaml
skill                         trials satisfied   mean pass  source
chart-maker                        3      100%       1.00  generated
table-formatter                    3       33%       0.42  generated

$ loopsmith proposals loop.yaml run-1786861783335
[AdoptSkill] chart-maker (iteration 3)
  goals were satisfied in 100% of 3 trials using `chart-maker`; it is not in the config
  suggested: skills: [chart-maker]

Apply these by editing the config yourself. The loop cannot.
```

| The loop does, on its own | The loop only proposes |
|---|---|
| Acquire, install, or generate sub-agents (quarantined) | Goals |
| Trial candidates and score them against gate outcomes | Validations |
| Write scratchpad notes between iterations | Success scenarios |
| | Which skills the config uses |

The loop cannot move its own goalposts, and cannot silently adopt a tool. A system that rewrites the criteria it is judged against cannot certify that it met them.

**One lucky run is not evidence.** Below `min_trials`, a candidate is recorded and ignored.

Sub-agents are sourced **installed → marketplace → generate**, with trust floors in [`config/marketplaces.json`](config/marketplaces.json). Names matching credential-shaped patterns are never auto-installed regardless of star count — popularity is not trust. Everything acquired lands in `generated-skills/` until a human promotes it, because an acquired sub-agent runs with whatever your permission grant allowed.

---

## Repository layout

```
loops/
├── loops-engineering-cheat-sheet.md   distillation of all 20 sources + decisions
├── LOOP-TEMPLATE.md                   the fill-in authoring template
├── HOW-TO-USE.md                      section-by-section reference
├── assets/                            logo
├── skills/
│   ├── loopsmith/                     user-invoked runner
│   └── loopsmith-reference/           model-invoked design reference
├── config/
│   ├── loop.schema.json               JSON Schema for the A–J model
│   ├── marketplaces.json              acquisition sources + trust floors
│   ├── permissions.template.json      shape of the consolidated grant
│   ├── mcp.template.json              MCP registration
│   └── examples/                      research-loop.yaml, refactor-loop.yaml
└── runtime/
    └── crates/
        ├── loopsmith-core             config model, A–H validation
        ├── loopsmith-memory           Store trait + sled backend
        ├── loopsmith-graph            DAG, waves, critical path, Amdahl
        ├── loopsmith-gate             deterministic verdicts
        ├── loopsmith-provider         command-template routing + usage accounting
        ├── loopsmith-skills           acquisition, marketplace, outcome ranking
        ├── loopsmith-mcp              stdio MCP server
        └── loopsmith-cli              the binary, scheduler, judge parsing
```

`sled` is shipped but sits behind a `Store` trait — it is effectively frozen upstream, and the trait means a swap to `redb` never reaches callers.

---

## Tests

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd runtime && cargo test --workspace
```

148 tests, no warnings. The ones worth knowing about:

- `the_gate_can_take_done_back` — satisfied flips to unsatisfied when the artifact disappears
- `judge_on_the_builders_provider_is_refused` — self-judgment cannot satisfy the gate
- `a_missing_judgment_fails_closed` / `a_missing_metric_fails_rather_than_passes_by_default`
- `there_is_no_tool_for_declaring_a_goal_satisfied` — guards the MCP surface
- `amdahl_matches_the_published_table` — the sizing arithmetic
- `checkpoint_survives_reopen` — resume after a crash
- `an_unsatisfiable_loop_stops_on_no_progress_not_on_success`
- `a_judge_verdict_now_reaches_the_gate` / `a_judge_on_the_builders_provider_still_cannot_satisfy_the_gate`
- `a_bare_pass_without_evidence_is_demoted_to_fail` — a verdict with no evidence is an assertion
- `tokens_are_now_accounted_so_the_budget_gate_can_fire`
- `independent_nodes_in_a_wave_run_concurrently` — timed, not asserted by inspection
- `two_nodes_get_separate_directories` — worktree isolation actually isolates
- `one_lucky_run_is_not_evidence` — a single trial cannot drive a config change
- `a_high_star_credential_grabber_is_still_excluded` — popularity is not trust
- `the_live_payload_shape_parses` — guards the field-type bug that made search silently return nothing
- `a_cron_trigger_fires_once_per_minute_not_once_per_poll`
- `the_watcher_ignores_its_own_state_directory`

---

## License

MIT — see [LICENSE](LICENSE).
