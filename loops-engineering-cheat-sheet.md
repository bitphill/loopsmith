# Loops Engineering Cheat Sheet

Reference distillation of every source in `planning/docs/loops-engineering/`, written so that any future loop-building task — this one or another — can be started without re-reading the corpus.

**Corpus:** 33 files, 20 unique sources. Fourteen files are PDF print-outs of a markdown article already present; only the markdown was read. `build-claude-skills.pdf` is the sole PDF with no markdown twin and is the highest-quality source in the set.

**Source quality warning that colours everything below.** Eighteen of twenty sources are self-published X threads or Medium posts. Several carry uncorroborated figures (repo star counts, benchmark numbers, before/after latencies). Where a claim drives a design decision, this document marks it `[unverified]`. The only authoritative source is the official Claude Code skills documentation.

---

## 1. Per-Source Distillation

| # | Source | Load-bearing idea | Used in `loopsmith` |
|---|---|---|---|
| 1 | Khairallah AL-Awady — *Loop Engineering: The 20-Step Roadmap* | The unit of work is a loop, not a prompt. Five parts: measurable done, verifier, layered exits, state, human checkpoint. Four gates decide whether a loop is even worth building. | Config sections C–H; `loopsmith-core` validation refuses a config missing a measurable definition of done |
| 2 | Anatoli Kopadze — *Loops explained* | `DISCOVER → PLAN → EXECUTE → VERIFY → ITERATE`. Five building blocks: automation, skill, sub-agents, connectors, verifier. Build order: manual run → skill → loop → schedule. Cost compounds because context is re-sent and grows. | Iteration state machine in `loopsmith-cli`; build-order enforced by section B being mandatory; `cost_per_accepted_change` metric in the ledger |
| 3 | CyrilXBT — *Self-Correcting AI Loop* | Builder / Judge / Manager with structured handoffs. Judge needs ground truth outside the Builder's reasoning. Four stress tests. Per-check verdicts, per-check routing. | Node execution contract in `loopsmith-provider`; `NodeVerdict` carries per-check results; scope-mismatch escalates rather than loops |
| 4 | Granite — *The Map* | Two layers: graph coordinates the fleet, loop makes one node trustworthy. Ten repos, each with a trap. The grading test: **can your system take "done" back?** | The whole two-plane architecture. `loopsmith-gate` is the only writer of `goal_satisfied` and can also revoke it |
| 5 | Argona — *What graph engineering is* | Node / edge / graph. The "and then" test finds false edges. Critical path is the floor, serial fraction is the cap. Fresh-context verifier. Worktree isolation + frozen git rules. | `loopsmith-graph`: DAG build, cycle detect, wave scheduling, critical path, Amdahl estimate driving auto-concurrency |
| 6 | Ajay — *Graph memory has one killer cost* | Separate extraction (high volume, low judgment, cheap+cached) from traversal (low volume, high judgment, expensive). Validate before writing — bad data compounds. Temporal edges. Graph edge is evidence; model assumption is not. | Provider tier routing in `loopsmith-provider` (cheap tier for extraction, strong tier for judgment); `loopsmith-memory` validates before write and stamps `valid_from` |
| 7 | Rahul — *Problem with how most people use AI* | Agents as a staffed team with persistent identity and lanes. Reviewer and devil's-advocate roles fire without being asked. Role descriptions must be tight to be useful. | Node `role` field; the adversarial-reviewer node type; tight role text enforced by schema `minLength` |
| 8 | Nick Spisak — *Paperclip* | Org chart, goal cascade, scheduled heartbeats, per-agent budget caps with hard stops, ticket trail, versioned config with rollback. | Per-node and global budget caps in section H; heartbeat schedules in section G; the sled ledger is the ticket trail |
| 9 | Greg Isenberg — *agency-agents* | Structure agents like a company of specialists rather than one generalist. | Role catalog seeding for the skill-acquisition step |
| 10 | Cobus Greyling — *Claude Code Agent Teams* | Subagents report to a lead and never talk to each other; Agent Teams removes that relay. Teammates start fresh with only the spawn prompt. Ephemeral vs durable trade-off. | Fresh-spawn context is exactly the verifier-independence property; durable side is why the orchestrator lives in Rust rather than in a session |
| 11 | **Claude Code docs — *Extend Claude with skills*** | `SKILL.md` frontmatter fields, resolution order, progressive disclosure, `allowed-tools`, `context: fork`, dynamic context injection, the six-field portability subset, 1,536-char description cap, keep body under 500 lines. | Both emitted skills conform exactly. The portability subset governs what the generated skill may put in frontmatter |
| 12 | Bober_smart — *10 folders with the best skills* | Commit to a direction before generating. Context is the failure mode, not intelligence. | Skill-acquisition candidate list; the "constrain before generating" rule shapes node prompts |
| 13 | Jaynit — *Musk's Algorithm* | Question → delete → simplify → accelerate → **automate last**. Requirements carry a person's name. Delete until you must add ~10% back. Automation locks a process in. | Section B (pre-execution work list) exists because of this; `loopsmith plan` reports deleted/false edges before any run |
| 14 | Jaynit — *First principles thinking* | Reasoning by analogy has a ceiling. Bezos Type 1 (irreversible, deliberate) vs Type 2 (reversible, fast). | Type 1/Type 2 is the `human_checkpoint` rule: irreversible node actions always stop for a human |
| 15 | Cobus Greyling — *Hierarchical Chunking in RAG* | Navigate a hierarchy rather than a flat index; a scratchpad carries reasoning between depths. | Memory ledger keeps a per-goal scratchpad readable by the next iteration |
| 16 | Vipra Singh — *Build an Agent from Scratch* | Minimal agent = model + tools + toolbox + system prompt, with `think`/`work`. No observe step means it is a router, not a loop. | Baseline for the provider adapter interface — what every provider must expose at minimum |
| 17 | *Why Japanese Developers Write Code Differently* | Kaizen, jidoka (stop the line), JIT (build only what is needed today), seven wastes. | Jidoka is the no-progress stop gate; JIT is why the schema rejects unused optional blocks |
| 18 | *He Rewrote Everything in Rust* | A rewrite redistributes who is load-bearing; the team that ignores it loses. | Cautionary only — informs the "propose, don't apply" bound on self-evolution |
| 19 | Ai With Piyas — *9 Opus design prompts* | `Act as [named role]. Produce [artifact]. Include: [enumerated constraints].` Critique prompts name an external standard (Nielsen, WCAG). | Node prompt template shape; naming an external standard is how subjective validations get a detector |
| 20 | *Forget ChatGPT & Gemini* | Interest has moved from tools to agents that automate workflows; existing automation platforms have a learning-curve barrier. | Motivation only; no design impact |

---

## 2. The Cross-Cutting Findings

### 2.1 The two-plane thesis
Five sources independently decompose agent systems the same way:

> The graph decides who runs and when. The loop decides whether you can trust what comes back.

Build the graph out of loops you can trust, or you have built a faster way to ship bugs across a fleet. A graph without loops is fast and wrong; loops without a graph are correct and serialized.

### 2.2 The verifier-independence ladder
The single strongest signal in the corpus. Each source arrives at "the checker must not be the maker" from a different direction, and they form an escalation:

1. **Separate prompt** — weakest. Same context, same blind spots.
2. **Separate context** — a fresh-context verifier that never saw the work.
3. **Separate model** — a different model family; avoids shared blind spots.
4. **Separate mechanism** — deterministic code decides what survives. Strongest.

Trust rises with each step away from whatever produced the work. Supporting measurements `[unverified, secondhand]`: GPT-4 recognises its own writing 73.5% of the time and prefers it causally (Panickssery, NeurIPS 2024); self-grading inflation ~10% GPT-4, ~25% Claude (Zheng, NeurIPS 2023).

**Design consequence:** `loopsmith-gate` sits at rung 4. It is plain Rust, it is the only writer of `goal_satisfied`, and no prompt can talk it out of a verdict.

### 2.3 The four stop gates, layered
Any one alone is insufficient:

| Gate | Trigger | Meaning when it fires |
|---|---|---|
| Verifier satisfied | All validations pass | Success |
| Iteration cap | `max_iterations` reached | Escalate with full history |
| Budget ceiling | Token / time / cost limit | Escalate; task may be unsolvable at this price |
| No-progress | Last N iterations changed nothing measurable | Jidoka — stop the line, the loop is spinning |

Written as **hard logic, not prompt text**. "Stop when it's good enough" is a suggestion a model will eventually talk itself past. Loops fail quietly — the "Ralph Wiggum loop" declares victory early and keeps billing while producing nothing.

**Log every stop-gate trigger, not just successes.** One node hitting its ceiling constantly while others rarely do means its judge is miscalibrated or checking the wrong ground truth. That pattern is invisible if you only track completions.

### 2.4 Amdahl sizing — know the speedup before deploying
$$ S = \frac{1}{(1-p) + \frac{p}{N}} $$

| p | N | Speedup |
|---|---|---|
| 0.95 | 16 | ×9.14 |
| 0.70 | 16 | ×2.91 |
| 0.95 | 256 | ×18.6 |
| any | ∞ | `1/(1-p)` |

Estimate `p` with the "and then" test: for every sequential step, ask whether the next step actually *reads* the previous step's output. Yes is a real edge; no was never an edge. Cut a false edge rather than adding an agent — cost scales with `N` while speedup flattens.

### 2.5 Musk's ordering applied to loop construction
Question → delete → simplify → accelerate → **automate last**. A loop is an analogy engine at machine speed: point it at an unexamined process and it executes that process faithfully, tirelessly, and at scale. Automating before questioning locks in the wrong process.

This is the same instruction as the loop roadmap's "do it manually first" and "the manual runs are the spec", arrived at from manufacturing rather than from agents.

### 2.6 Cost discipline
Loop cost compounds because context is re-sent and grows each pass; a ten-iteration loop is ten prompts that each get bigger, and a maker+checker split doubles it. The metric that matters is **cost per accepted change** — below a 50% accept rate the loop costs more than it returns. Levers: route each step to the cheapest capable model, cache stable prefixes, batch non-time-sensitive work, cap iterations and budget as hard logic, track cost in aggregate rather than per loop.

### 2.7 Isolation has two halves
Parallel nodes need **separate file state** (a git worktree per crew, not per agent — four worktrees of sixteen, not sixty-four checkouts) *and* **separate context** (or they agree with each other). Both, or neither works. The frozen rule set that made a 64-agent run safe:

```
Never git stash. Never git reset.
No git command except committing a specific file.
No slow commands before the test phase.
```

---

## 3. What `loopsmith` Takes From This

| Corpus idea | Component | How it is enforced |
|---|---|---|
| Measurable definition of done | `loopsmith-core` | Config fails validation if a goal has no validation entry |
| Verifier at rung 4 | `loopsmith-gate` | Sole writer of `goal_satisfied`; deterministic; can revoke |
| Four layered stop gates | `loopsmith-cli` run loop | All four evaluated every iteration; any one halts |
| "Can it take done back?" | `loopsmith-gate` | `revoke` command and automatic revocation on re-validation failure |
| And-then test / critical path | `loopsmith-graph` | `plan` reports real edges, false edges, critical path, predicted speedup |
| Amdahl-driven fan-out | `loopsmith-graph` | Auto-concurrency picks `N` where marginal speedup still exceeds marginal cost |
| Cheap extraction / costly judgment | `loopsmith-provider` | Per-node `tier: cheap \| standard \| strong` routed across providers |
| Builder / Judge / Manager | `loopsmith-provider` + run loop | Node roles with structured verdicts, per-check routing |
| Persistent state, resumable | `loopsmith-memory` | sled ledger behind a `Store` trait; `resume` continues from last checkpoint |
| Validate before write | `loopsmith-memory` | Schema check on every episode write; bad data never enters the ledger |
| Jidoka | Stop gates | No-progress window halts the line rather than continuing |
| JIT / delete step | Schema | Unused optional blocks are rejected, not ignored |
| Type 1 vs Type 2 | Section H | Irreversible actions always require the human checkpoint |
| Worktree isolation + frozen git rules | Section H defaults | Emitted into every parallel node's constraint block |
| Progressive disclosure, frontmatter rules | Emitted skills | Both skills conform to the official spec, including the six-field portability subset |
| Tight role descriptions | Schema | `role` has a minimum length and must name a standard for subjective checks |

---

## 4. Rejected, and Why

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

## 5. Quick Reference Card

**Before building any loop:**
1. Does the task repeat at least weekly?
2. Can something automatically reject bad output?
3. Can the agent do it end to end?
4. Is "done" objective?

Miss one — keep it a manual prompt.

**Build order:** one reliable manual run → save as skill → wrap in loop with gate and stop condition → *then* schedule.

**Every loop needs:** measurable done · externally grounded verifier · four layered exits · persistent state · a human checkpoint before anything irreversible.

**The test that grades the whole system:** can it take "done" back?
