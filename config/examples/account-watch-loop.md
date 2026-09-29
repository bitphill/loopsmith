# account-watch-loop

- version: 0.1.0
- description: Follow a categorised set of public accounts, record what they are talking about before it is widely discussed, and score earlier predictions against what actually broke out.
- environment: dev
- features:
  - self_evolution: false
  - marketplace_skills: false
  - external_side_effects: false
  - parallel_execution: true
  - human_approval: true

## Background

### categories
- note: Accounts are grouped by category because the categories move at different speeds. Researchers lead; celebrities lag and amplify.
- value: politicians, investors, entrepreneurs, celebrities, influencers, researchers

### accounts_to_follow
- note: Comma-separated public handles. Leave empty on the first run — the loop proposes a starting set and a human adopts it.
- value: ""

### prevalence_rule
- value: A topic is pre-viral when at least three watched accounts across at least two categories mention it within 72 hours, while its overall platform volume is still below the 7-day median.

### predictions_file
- note: Dated predictions. A later run scores them; nothing is scored in the run that made it.
- value: out/predictions.json

### observations_file
- value: out/observations.json


## Prerequisites

### Followed twenty accounts by hand for a week and noted what they surfaced early
- evidence: out/manual-watch.md
- done: false

### Wrote down what "pre-viral" means as a number, not as a feeling
- done: false

### Confirmed platform API access and recorded the rate limits
- done: false


## Goals

### watching
- priority: 1
- description: A categorised account list exists and each account's recent public posts are collected.

### signals
- priority: 2
- description: Topics meeting the pre-viral rule are identified and written down with a timestamp, before they are widely discussed.
- depends_on: ["watching"]

### scored
- priority: 3
- description: Predictions from earlier runs are scored against what actually broke out, and the hit rate is reported honestly.
- depends_on: ["signals"]


## Success

### signal-with-a-track-record
- target: overall
- threshold: 1.0
- mode: percentage
- statement: Every blocking check passes, including the anti-backdating check.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05
- join:
  - strategy: wait_for_all

### score-previous
- isolation:
  - mode: none
- role: judge
- instruction: Read out/predictions.json and check each past prediction against what actually broke out. Count unfalsifiable predictions as misses. Write predictions_scored to metrics.json and report the hit rate.
- goals: ["scored"]
- tier: strong
- provider: openai
- stage: score-past
- weight: 1.0

### curate-accounts
- isolation:
  - mode: none
- role: researcher
- instruction: If accounts_to_follow is empty, propose a starting set grouped by category and write out/accounts.json, marking it as proposed. If it is set, write the configured accounts with their categories. Never adopt a proposal on your own — that is a config edit.
- goals: ["watching"]
- tier: cheap
- skills: ["agent-reach"]
- stage: watch
- weight: 2.0

### collect
- isolation:
  - mode: none
- role: researcher
- instruction: Collect recent public posts from every watched account into out/observations.json, with handle, category, timestamp, and text.
- depends_on: ["curate-accounts"]
- goals: ["watching"]
- tier: cheap
- stage: watch
- weight: 3.0

### detect
- isolation:
  - mode: worktree
- role: builder
- instruction: Apply the prevalence rule to the observations. Append new predictions to out/predictions.json with the current timestamp, the accounts that triggered them, and the platform volume at the time.
- depends_on: ["collect"]
- goals: ["signals"]
- tier: standard
- stage: detect
- weight: 2.0

### challenge
- isolation:
  - mode: none
- role: adversary
- instruction: For each new prediction, argue that it is either already widely discussed or too vague to be scored later. Anything that survives both arguments stays.
- depends_on: ["detect"]
- goals: ["signals"]
- tier: strong
- provider: openai
- stage: detect
- weight: 1.0


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
- tiers: ["standard"]
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


## Phases

- dependency: ["score-past -> watch -> detect"]

### score-past
- guideline: "Score the predictions earlier runs made, before making new ones. Doing this first is deliberate: it is much harder to grade yourself generously when you have not yet decided what to predict."

### watch
- guideline: Collect recent public posts from the watched accounts. Do not interpret yet.

### detect
- guideline: Apply the prevalence rule and write new predictions with timestamps. A topic that does not meet the rule is not a prediction, however obvious it seems.


## Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Finds and categorises the accounts worth watching in a domain.
- trust_level: untrusted


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

### cron
- on:
  - expr: 0 */6 * * *
- enabled: true


## Checks

### accounts-listed
- target: watching
- blocking: true
- mode: objective
- statement: A categorised account list exists.
- detector:
  - type: file_exists
  - path: out/accounts.json
  - non_empty: true

### observations-collected
- target: watching
- blocking: true
- mode: objective
- statement: Recent posts from the watched accounts were collected.
- detector:
  - type: file_exists
  - path: out/observations.json
  - non_empty: true

### categories-covered
- target: watching
- blocking: true
- mode: objective
- statement: Accounts span more than one category.
- detector:
  - type: script
  - command: scripts/check-categories.sh
  - expect_exit: 0

### predictions-recorded
- target: signals
- blocking: true
- mode: objective
- statement: Predictions were written with timestamps.
- detector:
  - type: file_exists
  - path: out/predictions.json
  - non_empty: true

### prevalence-rule-applied
- target: signals
- blocking: true
- mode: objective
- statement: Every prediction met the account-count threshold when it was made.
- detector:
  - type: script
  - command: scripts/check-prevalence.sh
  - expect_exit: 0

### hit-rate-reported
- target: scored
- blocking: true
- mode: objective
- statement: A hit rate over earlier predictions was computed.
- detector:
  - type: threshold
  - metric: predictions_scored
  - op: gte
  - value: 1.0

### no-retroactive-predictions
- target: overall
- blocking: true
- mode: objective
- statement: No prediction was added or edited with a timestamp earlier than the run that wrote it.
- detector:
  - type: script
  - command: scripts/check-timestamps.sh
  - expect_exit: 0

### honest-scoring
- target: overall
- blocking: true
- mode: subjective
- statement: The hit rate counts misses as misses. A prediction that was vague enough to be unfalsifiable is scored as a miss, not excluded.
- detector:
  - type: judge
  - standard: out/predictions.json compared against what actually broke out, with unfalsifiable predictions counted as misses
  - min_score: 8.0


## Gates

- stop:
  - max_iterations: 6
  - max_revisions_per_node: 3
  - max_wall_clock_seconds: 5400
  - max_tokens: 2500000
  - max_cost_usd: 10.0
  - no_progress_iterations: 3
  - no_progress_iterations_randomness: 2
  - stop_on_overall_success: true

## Limits

- global:
  - rules: ["Read public posts through official APIs only. Never log in to view an account.","Never post, reply, like, follow, or DM. This loop watches.","Never edit a prediction after it is written. Add a new one instead.","Score misses as misses. A loop that grades itself generously is worse than no loop.","Do not report on private individuals' personal lives, only on topics discussed publicly.","Respect each platform's rate limit."]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["adopting a proposed account list into the config","anything that writes to a social platform"]

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

## Evolution

- enabled: false
- max_regression: 0.02
- allowed_kinds: ["new_skill","skill_update","prompt_change","validation_change"]
- require_sandbox: true
- require_approval: true
- keep_rollback: true
