---
name: loopsmith-reference
description: >
  Reference for designing agent loops: when a loop is worth building at all,
  the four layered stop gates, why a verifier must be grounded outside the
  model that produced the work, graph-vs-chain decomposition, Amdahl sizing for
  agent fan-out, and cheap-vs-strong provider routing. Use this whenever
  designing, reviewing, or debugging any iterative agent system — a self-
  correcting loop, a multi-agent fan-out, a verification gate, a retry policy,
  or a run that will not converge — even when loopsmith itself is not involved.
---

# Loop design reference

The distilled version of what makes iterative agent systems work. Applies to
any loop; `loopsmith` is one implementation.

## Is a loop even warranted?

All four must hold. Miss one and a single good prompt beats a loop:

1. The task repeats, at least weekly.
2. Something can **automatically reject** bad output.
3. The agent can do the work end to end.
4. "Done" is objective, not a judgment call.

## Chain versus loop

A chain is a fixed sequence and works only when every step is known in advance.
A loop tries something, observes the result, and decides the next step from
that result. A chain cannot fix a failing test; a loop can. The three-beat
rhythm underneath is reason → act → observe.

An agent with no observe step is a tool router, not a loop.

## The parts, and what breaks without each

| Part | Missing it means |
|---|---|
| Machine-checkable definition of done | The loop never knows when to stop |
| Grounded verifier | The agent grades its own homework and approves junk |
| Layered exits | Runs forever, drains budget, or spins |
| Persistent state | Restarts from zero after any hiccup |
| Human checkpoint | Autonomy becomes recklessness |

## Verifier independence — the load-bearing idea

A model reviewing its own output, in the same context that produced it, defends
rather than scrutinises. From the inside a plausible wrong answer still looks
plausible, so "are you sure?" yields reassurance, not re-examination.

Trust rises with each step away from whatever produced the work:

1. Separate prompt — weakest; same context, same blind spots.
2. Separate context — a verifier that never saw the work being made.
3. Separate model family — avoids characteristic blind spots.
4. **Separate mechanism** — deterministic code decides. Strongest.

**Coherence versus correctness:** a judge that sees only the builder's output
can tell you it is internally consistent. It cannot tell you it is right. A
confidently wrong, well-formatted answer passes every time.

Ground truth by task type:

- **Code** — the test suite, actual execution output, lint, build status. Not
  "does this look right" but "did it pass when run".
- **Content** — the original source and the brief, side by side with the draft.
- **Research** — the actual sources, so every claim can be traced.

If you cannot say what the judge's ground truth is, you do not have a
self-correcting loop. You have a rephrasing loop.

**Name the standard.** A subjective check against "quality" is an opinion. The
same check against Nielsen's heuristics, WCAG 2.2 AA, or a written style guide
is a check.

## The four stop gates

Layer all of them; any one alone is insufficient:

- **Verifier satisfied** — the goal is genuinely met.
- **Iteration cap** — escalate with full history rather than trying again.
- **Budget ceiling** — tokens, cost, or wall-clock.
- **No progress** — halt when recent iterations change nothing measurable.

Write them as hard logic. "Stop when it's good enough" inside a prompt is a
suggestion the model will eventually talk itself past. Loops do not crash when
they fail; they bill you in silence.

**Log every trigger, not just successes.** One node hitting its ceiling
constantly while others rarely do is a miscalibrated judge — invisible if you
only record completions.

## The grading test for any agent system

**Can it take "done" back?** A gate that refuses a failing merge, a task that
flips back to not-ready, a review hook that un-finishes a session. A system
that can only promote is a burndown chart with extra steps.

## Graph decomposition

- **Node** — one unit of work: one input, one output.
- **Edge** — a real dependency: the next step reads the previous step's output.
- **Critical path** — the longest chain of real edges. The floor on wall-clock
  time; no amount of parallelism lowers it.

**The one question:** on every "and then", does the next step actually read the
previous one's output? Yes keeps the order. No was never an edge — run them
together. Cut a false edge rather than adding a worker.

## Amdahl sizing

$$S = \frac{1}{(1-p) + p/N}$$

| p | N | Speedup |
|---|---|---|
| 0.95 | 16 | ×9.14 |
| 0.70 | 16 | ×2.91 |
| 0.95 | 256 | ×18.6 |
| any | ∞ | 1/(1−p) |

Cost scales with N; speedup flattens. The critical path is the floor, the
serial fraction is the cap. Estimate p from the graph before deploying
anything.

## Isolation has two halves

Parallel workers need **separate file state** (a worktree per crew, not per
agent) *and* **separate context** (or they agree with each other). Both, or
neither works. The frozen git rule set that made large runs survivable:

```
Never git stash. Never git reset.
No git command except committing a specific file.
No slow commands before the test phase.
```

## Cost

Loop cost compounds: every iteration re-sends a context pile that grows, and a
maker-plus-checker split doubles it. The metric that matters is **cost per
accepted change** — below a 50% accept rate the loop costs more than it
returns.

Route mechanical, high-volume work (extraction, classification, formatting) to
cheap models and reserve strong models for judgment. Cache stable prefixes;
batch anything not time-sensitive.

## Build order

1. Get one manual run reliable by hand.
2. Save it as a skill.
3. Wrap the skill in a loop with a gate and a stop condition.
4. *Then* schedule it.

Automation goes last because it locks a process in and makes it invisible.
Automating before questioning makes a bad process permanent — and a loop is an
analogy engine at machine speed, so it will execute an unexamined process
faithfully, tirelessly, and at scale.

The loop has no taste. You supply it; the loop enforces it. The ceiling on any
loop is the quality of the judgment encoded in its verifier.
