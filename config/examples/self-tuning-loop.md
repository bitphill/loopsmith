# self-tuning-loop

- version: 0.1.0
- description: A weekly report loop that is allowed to propose changes to itself, measured against the numbers it currently achieves, and forbidden from touching the parts that say no.
- environment: dev
- features:
  - self_evolution: true
  - marketplace_skills: false
  - external_side_effects: false
  - parallel_execution: true
  - human_approval: true

## Background

### report_path
- note: The deliverable. One file, overwritten each week.
- value: out/weekly.md

### audience
- value: The three people who run this product. They read on a phone, on Monday.

### length_rule
- value: Under 600 words. A report nobody finishes is a report nobody read.


## Prerequisites

### Ran this report by hand for three weeks and kept every draft
- evidence: link the three drafts
- done: false

### Recorded what each of those runs cost and how long it took
- done: false

### Wrote down what makes a report good enough to send, in checkable terms
- done: false


## Goals

### draft
- description: Produce the weekly report from this week's data, under the length rule.

### readable
- description: The report is something the audience will actually finish.
- depends_on: ["draft"]


## Success

### sendable
- target: overall
- threshold: 1.0
- mode: percentage
- statement: Every blocking check passes.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05
- join:
  - strategy: wait_for_all

### collect
- isolation:
  - mode: none
- role: researcher
- instruction: Gather this week's numbers from the sources named in the background. Record each with its source. Write no prose.
- goals: ["draft"]
- tier: cheap
- weight: 1.0

### write
- isolation:
  - mode: worktree
- role: builder
- instruction: Write the report from what collect gathered, under the length rule. Every number carries where it came from.
- depends_on: ["collect"]
- goals: ["draft"]
- tier: standard
- weight: 2.0

### read
- isolation:
  - mode: none
- role: judge
- instruction: Read it as the audience would, on a phone, on a Monday. Judge against the standard named in the check. Rewrite nothing.
- depends_on: ["write"]
- goals: ["readable"]
- tier: strong
- provider: openai
- weight: 1.0


## Providers

- cascade:
  - cheap: ["ollama","claude"]
  - standard: ["claude"]
  - strong: ["openai"]
- enforce_judge_independence: true

### ollama
- kind: ollama
- tiers: ["cheap"]
- command: ollama
- args: ["run","{model}"]
- model: llama3
- timeout_seconds: 120
- prompt_on_stdin: true

### claude
- kind: claude_code
- tiers: ["standard"]
- command: claude
- args: ["-p","{prompt}"]
- prompt_on_stdin: false

### openai
- kind: openai
- tiers: ["strong"]
- command: curl
- args: ["-sS","https://api.openai.com/v1/chat/completions"]
- requires_env: ["OPENAI_API_KEY"]
- prompt_on_stdin: true


## Skills

- acquisition_order: ["installed","marketplace","generate"]
- quarantine_dir: generated-skills
- min_marketplace_stars: 100
- require_human_promotion: true
- explore: false
- min_trials: 3
- min_trust_level: reviewed
- require_checksum: false
- allow_external_side_effects: false

## Memory

- carry_summaries: 3
- max_summary_chars: 1500
- namespaces:
  - episodic:
    - enabled: true
    - retention_days: 90
    - promotion:
      - rule: never
    - min_confidence: 0.75
    - require_provenance: true
  - semantic:
    - enabled: true
    - promotion:
      - rule: human_approval
    - min_confidence: 0.75
    - require_provenance: true
  - procedural:
    - enabled: true
    - promotion:
      - rule: automatic
    - min_confidence: 0.75
    - require_provenance: true
  - failure:
    - enabled: true
    - promotion:
      - rule: automatic
    - min_confidence: 0.75
    - require_provenance: true
- max_retrieved: 10

## Triggers

- max_depth: 1
- dedup_window_seconds: 86400

### cron
- on:
  - expr: 0 6 * * 1
- idempotency_key: weekly-report
- enabled: true


## Checks

### report-exists
- target: draft
- blocking: true
- mode: objective
- statement: The report exists and is not empty.
- detector:
  - type: file_exists
  - path: out/weekly.md
  - non_empty: true

### under-the-length-rule
- target: draft
- blocking: true
- mode: objective
- statement: The report is under 600 words.
- detector:
  - type: script
  - command: scripts/word-count.sh

### reads-well
- target: readable
- blocking: true
- mode: subjective
- statement: Every number carries its source, there is no filler paragraph, and the first sentence says the most important thing.
- detector:
  - type: judge
  - standard: docs/report-standard.md

### sendable-as-it-stands
- target: overall
- blocking: true
- mode: objective
- statement: The report exists, is under the length rule, and names its sources.
- detector:
  - type: script
  - command: scripts/sendable.sh


## Gates

- stop:
  - max_iterations: 6
  - max_revisions_per_node: 2
  - max_wall_clock_seconds: 1800
  - max_cost_usd: 2.0
  - no_progress_iterations: 3
  - no_progress_iterations_randomness: 2
  - stop_on_overall_success: true

## Limits

- global:
  - rules: ["Do not send anything. This loop writes a file and stops."]
  - forbidden_paths: [".git/","state/"]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 600
  - human_checkpoint: ["sending a message","publishing anything"]

## Recovery

- transient_error:
  - action: retry
  - max_attempts: 3
  - base_delay_seconds: 2
  - backoff: exponential
- invalid_output:
  - action: revise
  - max_attempts: 2
- tool_unavailable:
  - action: fallback
- repeated_failure:
  - action: escalate
- safety_violation:
  - action: stop
- resource_exhaustion:
  - action: pause
- corrupted_state:
  - action: restore_checkpoint

## Protected

- components: ["gates","limits","recovery","protected","approvals","credentials","audit","baselines","retention","environment"]
- extra_paths: ["scripts/word-count.sh","docs/report-standard.md"]

## Alerts

### drifting-up
- message: Half the ceiling for a 600-word report. Something is retrying.
- metric: cost_usd
- above: 1.0

### failing-more
- message: Most checks are failing. The proposals are probably making it worse.
- metric: validation_pass_rate
- below: 0.6


## Evolution

- enabled: true
- baseline:
  - completion_rate: 0.67
  - validation_pass_rate: 0.8
  - cost_usd: 0.45
  - latency_seconds: 420.0
  - iterations_to_success: 3.0
  - measured_at: 2026-01-12
- max_regression: 0.05
- allowed_kinds: ["prompt_change","graph_change","provider_routing","new_skill"]
- require_sandbox: true
- require_approval: true
- keep_rollback: true
