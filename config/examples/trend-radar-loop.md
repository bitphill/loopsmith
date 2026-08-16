# trend-radar-loop

- version: 0.1.0
- description: Track a category across X, Instagram, and TikTok, and report only trends that are evidenced by dated posts and a measurable rise.

## A. Information

### category
- value: AI developer tooling
- note: Replace. One category per loop — a radar pointed everywhere sees nothing.

### platforms
- value: x, instagram, tiktok
- note: Drop any platform you do not have API access for. A missing key is a missing platform, not a guess.

### rise_definition
- value: A trend is rising if its mention volume in the last 48 hours is at least 2x its trailing 7-day daily average.

### evidence_file
- value: out/observations.json
- note: Raw dated posts. The report is derived from this file and nothing else.

### report_path
- value: out/trends.md


## B. Pre-execution

### Pulled one day of data by hand and looked at it
- done: false
- evidence: link the raw pull

### Wrote down what separates a trend from noise, as a number
- done: false
- evidence: the rise_definition above, agreed with whoever reads the report

### Confirmed each platform's API credentials work and recorded the rate limits
- done: false


## C. Goals

### collect
- description: Gather dated posts for the category from each configured platform.
- priority: 1

### detect
- description: Identify which topics are rising by the stated numeric definition, not by how interesting they sound.
- depends_on: ["collect"]
- priority: 2

### report
- description: Produce a report where every trend cites the post IDs and dates it came from, and states its rise ratio.
- depends_on: ["detect"]
- priority: 3


## D. Validations

### observations-exist
- target: collect
- mode: objective
- statement: Raw dated observations were written.
- detector:
  - type: file_exists
  - path: out/observations.json
  - non_empty: true
- blocking: true

### multi-platform
- target: collect
- mode: objective
- statement: Observations cover more than one platform.
- detector:
  - type: script
  - command: scripts/check-platforms.sh
  - expect_exit: 0
- blocking: true

### rise-computed
- target: detect
- mode: objective
- statement: Every reported trend has a computed rise ratio at or above the threshold.
- detector:
  - type: threshold
  - metric: min_rise_ratio
  - op: gte
  - value: 2.0
- blocking: true

### report-exists
- target: report
- mode: objective
- statement: The report file exists.
- detector:
  - type: file_exists
  - path: out/trends.md
  - non_empty: true
- blocking: true

### every-trend-cited
- target: report
- mode: objective
- statement: Every trend in the report cites at least one post ID.
- detector:
  - type: regex_match
  - artifact: report
  - pattern: "post_id:"
- blocking: true

### no-invented-trends
- target: overall
- mode: subjective
- statement: Every trend in the report traces to observations in out/observations.json. A trend that is true but unevidenced is still a failure here.
- detector:
  - type: judge
  - standard: each claim traces to a dated observation in out/observations.json
  - min_score: 8.0
- blocking: true


## E. Success

### evidenced-report
- target: overall
- mode: percentage
- statement: Every blocking check passes.
- threshold: 1.0


## F. Stop gates

- max_iterations: 8
- max_revisions_per_node: 3
- max_wall_clock_seconds: 5400
- max_tokens: 2500000
- max_cost_usd: 8.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 7 * * *


## H. Constraints

- global:
  - rules: ["Use the platforms' official APIs. Never scrape a logged-in view.","Respect the recorded rate limit for each platform.","Never post, like, follow, or reply. This loop reads.","A trend without post IDs does not go in the report, however plausible.","Report the number you measured, not the number that would be a better story."]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["anything that writes to a social platform"]

## I. Execution guidelines

- dependency: ["collect -> analyse -> write"]

### collect
- guideline: Pull raw dated posts and write them down verbatim. Do not summarise, rank, or interpret in this phase — the interpretation must be reproducible from what you saved.

### analyse
- guideline: Compute rise ratios from the saved observations. Read no new data here; if something is missing, the collect phase was incomplete and should say so.

### write
- guideline: Write the report. Every claim carries its post IDs and its rise ratio.


## J. Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Locates where a category is being discussed across platforms.


## Graph

- concurrency:
  - mode: auto
  - cap: 8
  - min_marginal_gain: 0.05

### pull-x
- role: researcher
- instruction: Pull posts in the category from X over the trailing 7 days using the official API. Write each with its post id, author, timestamp, and text into out/observations.json. Save nothing you did not fetch.
- goals: ["collect"]
- tier: cheap
- stage: collect
- weight: 2.0
- isolated: false

### pull-instagram
- role: researcher
- instruction: Same as the X pull, for Instagram. If credentials are missing, record that the platform was unavailable rather than filling the gap.
- goals: ["collect"]
- tier: cheap
- stage: collect
- weight: 2.0
- isolated: false

### pull-tiktok
- role: researcher
- instruction: Same as the X pull, for TikTok. If credentials are missing, record that the platform was unavailable rather than filling the gap.
- goals: ["collect"]
- tier: cheap
- stage: collect
- weight: 2.0
- isolated: false

### detect
- role: builder
- instruction: Group observations into topics and compute each topic's rise ratio against its trailing 7-day average. Write min_rise_ratio to metrics.json. Discard topics below the threshold.
- depends_on: ["pull-x","pull-instagram","pull-tiktok"]
- goals: ["detect"]
- tier: standard
- stage: analyse
- weight: 3.0
- isolated: false

### write-report
- role: builder
- instruction: "Write out/trends.md. Each trend gets its rise ratio and the post IDs it was derived from, formatted as `post_id: <id>`."
- depends_on: ["detect"]
- goals: ["report"]
- tier: standard
- stage: write
- weight: 1.0
- isolated: true

### verify
- role: judge
- instruction: Check every claim in out/trends.md against out/observations.json. Report per-claim pass or fail, quoting the observation or its absence.
- depends_on: ["write-report"]
- goals: ["report"]
- tier: strong
- provider: openai
- stage: write
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

- carry_summaries: 2
- max_summary_chars: 1200

