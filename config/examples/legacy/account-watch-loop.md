# account-watch-loop

- version: 0.1.0
- description: Follow a categorised set of public accounts, record what they are talking about before it is widely discussed, and score earlier predictions against what actually broke out.

## A. Information

### categories
- value: politicians, investors, entrepreneurs, celebrities, influencers, researchers
- note: Accounts are grouped by category because the categories move at different speeds. Researchers lead; celebrities lag and amplify.

### accounts_to_follow
- value: ""
- note: Comma-separated public handles. Leave empty on the first run — the loop proposes a starting set and a human adopts it.

### prevalence_rule
- value: A topic is pre-viral when at least three watched accounts across at least two categories mention it within 72 hours, while its overall platform volume is still below the 7-day median.

### predictions_file
- value: out/predictions.json
- note: Dated predictions. A later run scores them; nothing is scored in the run that made it.

### observations_file
- value: out/observations.json


## B. Pre-execution

### Followed twenty accounts by hand for a week and noted what they surfaced early
- done: false
- evidence: out/manual-watch.md

### Wrote down what "pre-viral" means as a number, not as a feeling
- done: false

### Confirmed platform API access and recorded the rate limits
- done: false


## C. Goals

### watching
- description: A categorised account list exists and each account's recent public posts are collected.
- priority: 1

### signals
- description: Topics meeting the pre-viral rule are identified and written down with a timestamp, before they are widely discussed.
- depends_on: ["watching"]
- priority: 2

### scored
- description: Predictions from earlier runs are scored against what actually broke out, and the hit rate is reported honestly.
- depends_on: ["signals"]
- priority: 3


## D. Validations

### accounts-listed
- target: watching
- mode: objective
- statement: A categorised account list exists.
- detector:
  - type: file_exists
  - path: out/accounts.json
  - non_empty: true
- blocking: true

### observations-collected
- target: watching
- mode: objective
- statement: Recent posts from the watched accounts were collected.
- detector:
  - type: file_exists
  - path: out/observations.json
  - non_empty: true
- blocking: true

### categories-covered
- target: watching
- mode: objective
- statement: Accounts span more than one category.
- detector:
  - type: script
  - command: scripts/check-categories.sh
  - expect_exit: 0
- blocking: true

### predictions-recorded
- target: signals
- mode: objective
- statement: Predictions were written with timestamps.
- detector:
  - type: file_exists
  - path: out/predictions.json
  - non_empty: true
- blocking: true

### prevalence-rule-applied
- target: signals
- mode: objective
- statement: Every prediction met the account-count threshold when it was made.
- detector:
  - type: script
  - command: scripts/check-prevalence.sh
  - expect_exit: 0
- blocking: true

### hit-rate-reported
- target: scored
- mode: objective
- statement: A hit rate over earlier predictions was computed.
- detector:
  - type: threshold
  - metric: predictions_scored
  - op: gte
  - value: 1.0
- blocking: true

### no-retroactive-predictions
- target: overall
- mode: objective
- statement: No prediction was added or edited with a timestamp earlier than the run that wrote it.
- detector:
  - type: script
  - command: scripts/check-timestamps.sh
  - expect_exit: 0
- blocking: true

### honest-scoring
- target: overall
- mode: subjective
- statement: The hit rate counts misses as misses. A prediction that was vague enough to be unfalsifiable is scored as a miss, not excluded.
- detector:
  - type: judge
  - standard: out/predictions.json compared against what actually broke out, with unfalsifiable predictions counted as misses
  - min_score: 8.0
- blocking: true


## E. Success

### signal-with-a-track-record
- target: overall
- mode: percentage
- statement: Every blocking check passes, including the anti-backdating check.
- threshold: 1.0


## F. Stop gates

- max_iterations: 6
- max_revisions_per_node: 3
- max_wall_clock_seconds: 5400
- max_tokens: 2500000
- max_cost_usd: 10.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 */6 * * *


## H. Constraints

- global:
  - rules: ["Read public posts through official APIs only. Never log in to view an account.","Never post, reply, like, follow, or DM. This loop watches.","Never edit a prediction after it is written. Add a new one instead.","Score misses as misses. A loop that grades itself generously is worse than no loop.","Do not report on private individuals' personal lives, only on topics discussed publicly.","Respect each platform's rate limit."]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["adopting a proposed account list into the config","anything that writes to a social platform"]

## I. Execution guidelines

- dependency: ["score-past -> watch -> detect"]

### score-past
- guideline: "Score the predictions earlier runs made, before making new ones. Doing this first is deliberate: it is much harder to grade yourself generously when you have not yet decided what to predict."

### watch
- guideline: Collect recent public posts from the watched accounts. Do not interpret yet.

### detect
- guideline: Apply the prevalence rule and write new predictions with timestamps. A topic that does not meet the rule is not a prediction, however obvious it seems.


## J. Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Finds and categorises the accounts worth watching in a domain.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05

### score-previous
- role: judge
- instruction: Read out/predictions.json and check each past prediction against what actually broke out. Count unfalsifiable predictions as misses. Write predictions_scored to metrics.json and report the hit rate.
- goals: ["scored"]
- tier: strong
- provider: openai
- stage: score-past
- weight: 1.0
- isolated: false

### curate-accounts
- role: researcher
- instruction: If accounts_to_follow is empty, propose a starting set grouped by category and write out/accounts.json, marking it as proposed. If it is set, write the configured accounts with their categories. Never adopt a proposal on your own — that is a config edit.
- goals: ["watching"]
- tier: cheap
- skills: ["agent-reach"]
- stage: watch
- weight: 2.0
- isolated: false

### collect
- role: researcher
- instruction: Collect recent public posts from every watched account into out/observations.json, with handle, category, timestamp, and text.
- depends_on: ["curate-accounts"]
- goals: ["watching"]
- tier: cheap
- stage: watch
- weight: 3.0
- isolated: false

### detect
- role: builder
- instruction: Apply the prevalence rule to the observations. Append new predictions to out/predictions.json with the current timestamp, the accounts that triggered them, and the platform volume at the time.
- depends_on: ["collect"]
- goals: ["signals"]
- tier: standard
- stage: detect
- weight: 2.0
- isolated: true

### challenge
- role: adversary
- instruction: For each new prediction, argue that it is either already widely discussed or too vague to be scored later. Anything that survives both arguments stays.
- depends_on: ["detect"]
- goals: ["signals"]
- tier: strong
- provider: openai
- stage: detect
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


## Skills

- acquisition_order: ["installed","marketplace","generate"]
- quarantine_dir: generated-skills
- min_marketplace_stars: 100
- require_human_promotion: true
- explore: false
- min_trials: 3

## Context

- carry_summaries: 3
- max_summary_chars: 1200

