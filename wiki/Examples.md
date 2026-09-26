# The example loops

Worked loops in `config/examples/`, each shipped as a `.yaml` and an equivalent
`.md`. The two are the same model in two grammars, generated from one source by
`tools/sync-examples.sh`, and a round-trip test keeps them honest.

**All of them refuse to validate**, with exactly one error each, until you tick
their `intent.prerequisites`. That is not an oversight. An example config is a
description of a job nobody has done yet, and the tool's single most valuable
refusal is the one that says so.

```bash
loopsmith loop validate config/examples/research-loop.yaml
#   error  intent.prerequisites: 2 step(s) not marked done: …
```

Copy one, do the job by hand once, tick the steps with what you actually did,
and then run it.

| Loop | What it does |
|---|---|
| [`research-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/research-loop.yaml) | Research a question against primary sources, every claim cited |
| [`refactor-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/refactor-loop.yaml) | Behaviour-preserving refactor where the test suite is the gate |
| [`traffic-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/traffic-loop.yaml) | Post where an audience already gathers, measured in referred sessions |
| [`trend-radar-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/trend-radar-loop.yaml) | Track a category across X, Instagram, and TikTok with dated evidence |
| [`landing-page-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/landing-page-loop.yaml) | A static landing page gated on Lighthouse, page weight, and working CTAs |
| [`sales-leads-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/sales-leads-loop.yaml) | Build a lead list from permitted sources, with lawful basis recorded per record |
| [`marketing-automation-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/marketing-automation-loop.yaml) | Turn product docs into scheduled posts, published behind a human checkpoint |
| [`blogger-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/blogger-loop.yaml) | Write on a trending topic, gated on style measurements and an independent read |
| [`cold-outreach-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/cold-outreach-loop.yaml) | Personalised first contact, with suppression and opt-out enforced by the gate |
| [`x402-agent-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/x402-agent-loop.yaml) | An agent that pays for things, supervised or autonomous, under a hard cap |
| [`viral-game-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/viral-game-loop.yaml) | A small Godot game gated on build health and time-to-first-play |
| [`idea-radar-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/idea-radar-loop.yaml) | Product ideas traced to dated public complaints, checked against what already sells |
| [`account-watch-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/account-watch-loop.yaml) | Watch accounts for pre-viral topics, and score yesterday's predictions |
| [`container-refactor-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/container-refactor-loop.yaml) | Refactor three modules at once, each in its own container over its own worktree |
| [`self-tuning-loop`](https://github.com/bitphill/loopsmith/blob/main/config/examples/self-tuning-loop.yaml) | A weekly report that may propose changes to itself, measured against a recorded baseline |

## Which one to start from

- **Learning what a loop is** — `research-loop`. It is the smallest one whose
  checks are all deterministic.
- **You write software** — `refactor-loop`. The test suite is already the gate,
  so there is nothing to invent.
- **Anything that leaves the building** — `cold-outreach-loop` or
  `marketing-automation-loop`. Both show `human_checkpoint` doing real work.
- **Anything that spends money** — `x402-agent-loop`. It is the one built
  around a hard cap.
- **Wide parallelism** — `container-refactor-loop`. Three builders in one wave,
  each in its own container, with the test suite as the gate.
- **Self-evolution** — `self-tuning-loop`. It is mostly about the fence: the
  baseline a proposal is measured against and the components it may never
  touch.
