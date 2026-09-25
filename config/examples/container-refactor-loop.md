# container-refactor-loop

- version: 0.1.0
- description: Refactor several modules at once, each in its own container over its own worktree, and let the test suite decide whether any of it survives.
- environment: dev
- features:
  - self_evolution: false
  - marketplace_skills: false
  - external_side_effects: false
  - parallel_execution: true
  - human_approval: true

## Background

### test_command
- note: The gate. If this does not pass, nothing the loop did counts.
- value: cargo test --workspace

### modules
- note: One builder per module. They must not touch each other's files.
- value: parser, planner, reporter

### behaviour_rule
- value: No behaviour changes. A test that had to be edited is a failed refactor.


## Prerequisites

### Ran the full test suite by hand and watched it pass
- evidence: paste the summary line
- done: false

### Refactored one of these modules by hand, start to finish
- done: false

### Confirmed `docker run hello-world` works, or accepted worktree isolation
- done: false


## Goals

### simplify
- description: Each named module is smaller and clearer, with its behaviour unchanged.

### still-green
- description: The whole test suite passes after every module has been changed.
- depends_on: ["simplify"]


## Success

### green-and-smaller
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
- container_image: rust:1.75

### survey
- isolation:
  - mode: none
- role: researcher
- instruction: List the three modules by name with their current line counts and the one thing in each that makes it hard to read. Change nothing.
- goals: ["simplify"]
- tier: cheap
- weight: 1.0

### refactor-parser
- isolation:
  - mode: container
  - network: true
- role: builder
- instruction: Simplify the parser module only. Do not edit tests. Do not touch any other module. Commit nothing.
- depends_on: ["survey"]
- goals: ["simplify"]
- tier: standard
- weight: 3.0

### refactor-planner
- isolation:
  - mode: container
  - network: true
- role: builder
- instruction: Simplify the planner module only. Do not edit tests. Do not touch any other module. Commit nothing.
- depends_on: ["survey"]
- goals: ["simplify"]
- tier: standard
- weight: 3.0

### refactor-reporter
- isolation:
  - mode: container
  - network: true
- role: builder
- instruction: Simplify the reporter module only. Do not edit tests. Do not touch any other module. Commit nothing.
- depends_on: ["survey"]
- goals: ["simplify"]
- tier: standard
- weight: 2.0

### review
- isolation:
  - mode: none
- role: judge
- instruction: Read the three diffs against the standard named in the check. Say which changes alter behaviour. Fix nothing.
- depends_on: ["refactor-parser","refactor-planner","refactor-reporter"]
- goals: ["still-green"]
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

- carry_summaries: 2
- max_summary_chars: 1200
- namespaces:
  - episodic:
    - enabled: true
    - promotion:
      - rule: never
    - min_confidence: 0.75
    - require_provenance: false
  - semantic:
    - enabled: true
    - promotion:
      - rule: repeated_validation
      - times: 3
    - min_confidence: 0.75
    - require_provenance: true
  - procedural:
    - enabled: true
    - promotion:
      - rule: repeated_validation
      - times: 3
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

- max_depth: 5
- dedup_window_seconds: 300

### manual
- enabled: true


## Checks

### modules-are-smaller
- target: simplify
- blocking: true
- mode: objective
- statement: Every named module has fewer lines than it started with.
- detector:
  - type: script
  - command: scripts/smaller.sh

### suite-passes
- target: still-green
- blocking: true
- mode: objective
- statement: The full test suite passes.
- detector:
  - type: script
  - command: scripts/test.sh

### tests-were-not-edited
- target: overall
- blocking: true
- mode: objective
- statement: No file under tests/ was modified.
- detector:
  - type: script
  - command: scripts/tests-untouched.sh


## Gates

- stop:
  - max_iterations: 6
  - max_revisions_per_node: 2
  - max_wall_clock_seconds: 5400
  - max_cost_usd: 4.0
  - no_progress_iterations: 3
  - no_progress_iterations_randomness: 2
  - stop_on_overall_success: true
- entry: [{"id":"clean-tree","statement":"The working tree has no uncommitted changes.","detector":{"type":"script","command":"scripts/clean-tree.sh","args":[],"expect_exit":null},"on_fail":"stop"}]
- rollback: [{"id":"suite-still-green","statement":"The test suite passes.","detector":{"type":"script","command":"scripts/test.sh","args":[],"expect_exit":null},"on_fail":"rollback"}]

## Limits

- global:
  - rules: ["Never git stash. Never git reset.","No git command except committing a specific file.","Do not edit any file under tests/."]
  - forbidden_paths: [".git/","state/","tests/"]
  - forbidden_commands: ["rm -rf","git push","git reset"]
  - max_seconds: 1200
  - human_checkpoint: ["pushing anything","deleting data"]

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

## Alerts

### slow
- message: An hour in and still refactoring. Check whether a container is stuck.
- metric: wall_clock_seconds
- above: 3600.0


## Evolution

- enabled: false
- max_regression: 0.02
- allowed_kinds: ["new_skill","skill_update","prompt_change","validation_change"]
- require_sandbox: true
- require_approval: true
- keep_rollback: true
