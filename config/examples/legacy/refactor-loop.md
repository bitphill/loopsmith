# refactor-loop

- version: 0.1.0
- description: Refactor a module without changing behaviour. The test suite is the gate, and the loop is forbidden from editing tests to make them pass.

## A. Information

### target_module
- value: src/
- note: Scope. Widening this without widening the tests is how a refactor loop goes wrong.

### test_command
- value: cargo test --workspace

### behaviour_rule
- value: Public API and observable behaviour do not change. If they must, that is a different task.


## B. Pre-execution

### Ran the refactor by hand on one file and kept the diff
- done: false
- evidence: link the diff here

### Confirmed the suite is green before the loop starts
- done: false

### Recorded the baseline: test count, coverage, clippy warning count
- done: false


## C. Goals

### simplify
- description: Reduce duplication and nesting in the target module without changing behaviour.

### stay-green
- description: Keep the entire test suite passing at every step, not just at the end.

### no-test-edits
- description: Achieve the above without weakening, deleting, or rewriting any test.


## D. Validations

### suite-passes
- target: stay-green
- mode: objective
- statement: The full test suite exits clean.
- detector:
  - type: script
  - command: cargo
  - args: ["test","--workspace"]
- blocking: true

### tests-unchanged
- target: no-test-edits
- mode: objective
- statement: "No file under tests/ or any #[cfg(test)] block was modified."
- detector:
  - type: script
  - command: scripts/assert-tests-untouched.sh
- blocking: true

### clippy-clean
- target: simplify
- mode: objective
- statement: Clippy reports no warnings at the configured level.
- detector:
  - type: script
  - command: cargo
  - args: ["clippy","--workspace","--","-D","warnings"]
- blocking: true

### complexity-down
- target: simplify
- mode: percentage
- statement: Measured complexity is at or below the baseline.
- detector:
  - type: threshold
  - metric: complexity_ratio
  - op: lte
  - value: 1.0
- blocking: true

### readable
- target: simplify
- mode: subjective
- statement: The refactor reads as clearer to a reviewer who did not write it.
- detector:
  - type: judge
  - standard: the repository's own CONTRIBUTING guidance on naming and function length
  - min_score: 7.0
- blocking: false

### builds-and-passes
- target: overall
- mode: objective
- statement: The workspace builds and the suite is green.
- detector:
  - type: script
  - command: cargo
  - args: ["test","--workspace"]
- blocking: true


## E. Success

### green-and-simpler
- target: overall
- mode: percentage
- statement: Every blocking validation passes.
- threshold: 1.0


## F. Stop gates

- max_iterations: 12
- max_revisions_per_node: 3
- max_wall_clock_seconds: 7200
- max_tokens: 4000000
- max_cost_usd: 10.0
- no_progress_iterations: 3
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 2 * * *

### file_change
- path: src/


## H. Constraints

- global:
  - rules: ["Never git stash. Never git reset.","No git command except committing a specific file.","No slow commands before the test phase.","Never edit a test to make it pass. Fix the code or report that you cannot.","One behaviour-preserving change per commit."]
  - forbidden_paths: [".git/","target/","state/"]
  - forbidden_commands: ["rm -rf","git push","git reset","git stash"]
  - max_seconds: 1200
  - human_checkpoint: ["pushing to a remote","changing a public API signature","deleting a file"]

## Graph

- concurrency:
  - mode: auto
  - cap: 16
  - min_marginal_gain: 0.05

### survey
- role: researcher
- instruction: Map the target module. Report duplication, deep nesting, and long functions with file and line references. Do not change anything.
- goals: ["simplify"]
- tier: cheap
- weight: 1.0
- isolated: false

### refactor-a
- role: builder
- instruction: Apply behaviour-preserving simplifications to the first half of the surveyed findings. Run the suite before reporting.
- depends_on: ["survey"]
- goals: ["simplify","stay-green"]
- tier: standard
- weight: 3.0
- isolated: true

### refactor-b
- role: builder
- instruction: Apply behaviour-preserving simplifications to the second half of the surveyed findings. Run the suite before reporting.
- depends_on: ["survey"]
- goals: ["simplify","stay-green"]
- tier: standard
- weight: 3.0
- isolated: true

### review
- role: judge
- instruction: Check each change against the behaviour-preservation rule and the test-edit ban. Report per-change PASS or FAIL with the diff hunk as evidence.
- depends_on: ["refactor-a","refactor-b"]
- goals: ["no-test-edits","simplify"]
- tier: strong
- provider: openai
- weight: 1.0
- isolated: false


## Providers

- cascade:
  - cheap: ["ollama","claude"]
  - standard: ["claude"]
  - strong: ["openai","claude"]
- enforce_judge_independence: true

### ollama
- kind: ollama
- tiers: ["cheap"]
- command: ollama
- args: ["run","{model}"]
- model: qwen2.5-coder
- timeout_seconds: 600
- prompt_on_stdin: true

### claude
- kind: claude_code
- tiers: ["standard","strong"]
- command: claude
- args: ["-p","{prompt}"]
- timeout_seconds: 1800
- prompt_on_stdin: false

### openai
- kind: openai
- tiers: ["strong"]
- command: curl
- args: ["-sS","https://api.openai.com/v1/chat/completions","-H","Content-Type: application/json","-H","Authorization: Bearer $OPENAI_API_KEY","-d","@-"]
- model: gpt-4o
- requires_env: ["OPENAI_API_KEY"]
- timeout_seconds: 600
- prompt_on_stdin: true


## Skills

- acquisition_order: ["installed","generate"]
- quarantine_dir: generated-skills
- min_marketplace_stars: 100
- require_human_promotion: true
- explore: false
- min_trials: 3

## Context

- carry_summaries: 2
- max_summary_chars: 1200

