# idea-radar-loop

- version: 0.1.0
- description: Mine recent public complaints across social media, forums, and RSS for recurring pain, cross-check against what is already selling, and report only ideas that trace to dated evidence.

## A. Information

### segment
- value: B2B SaaS for small agencies
- note: Replace. "Any good idea" is not a segment and returns nothing usable.

### pain_threshold
- value: A pain qualifies when at least eight distinct people complained about it in the last 90 days, across at least two venues.

### market_check
- value: https://x.com/acquiredotcom
- note: Public listings of what is being bought and sold. Used to check whether an idea already has a thriving competitor, and at what price.

### evidence_file
- value: out/complaints.json
- note: Dated complaints with venue, URL, and verbatim text.

### report_path
- value: out/ideas.md


## B. Pre-execution

### Read fifty complaints in the segment by hand and wrote down the top three pains
- done: false
- evidence: out/manual-scan.md

### Agreed the numeric pain threshold with whoever will act on the report
- done: false

### Confirmed the venues can be read without an account
- done: false


## C. Goals

### collected
- description: Recent public complaints in the segment are gathered verbatim, with venue, URL, and date.
- priority: 1

### clustered
- description: Complaints are grouped into pains, each meeting the numeric threshold.
- depends_on: ["collected"]
- priority: 2

### checked
- description: Each pain is cross-checked against what is already being sold, with the competitor and its price named where one exists.
- depends_on: ["clustered"]
- priority: 2

### reported
- description: A report exists where every idea cites its complaints and states its competitive position.
- depends_on: ["checked"]
- priority: 3


## D. Validations

### complaints-exist
- target: collected
- mode: objective
- statement: Dated complaints were collected.
- detector:
  - type: file_exists
  - path: out/complaints.json
  - non_empty: true
- blocking: true

### multi-venue
- target: collected
- mode: objective
- statement: Complaints come from more than one venue.
- detector:
  - type: script
  - command: scripts/check-venues.sh
  - expect_exit: 0
- blocking: true

### threshold-met
- target: clustered
- mode: objective
- statement: Every reported pain meets the complaint-count threshold.
- detector:
  - type: threshold
  - metric: min_complaints_per_pain
  - op: gte
  - value: 8.0
- blocking: true

### competitors-checked
- target: checked
- mode: objective
- statement: Every idea records whether a competitor exists and at what price.
- detector:
  - type: script
  - command: scripts/check-competitors.sh
  - expect_exit: 0
- blocking: true

### report-exists
- target: reported
- mode: objective
- statement: The report exists.
- detector:
  - type: file_exists
  - path: out/ideas.md
  - non_empty: true
- blocking: true

### every-idea-cited
- target: reported
- mode: objective
- statement: Every idea in the report cites complaint URLs.
- detector:
  - type: regex_match
  - artifact: ideas
  - pattern: https?://
- blocking: true

### no-invented-pain
- target: overall
- mode: subjective
- statement: Every pain in the report traces to verbatim complaints in out/complaints.json. A pain that is real but unevidenced does not belong here.
- detector:
  - type: judge
  - standard: each claim traces to a dated verbatim complaint in out/complaints.json
  - min_score: 8.0
- blocking: true


## E. Success

### evidenced-ideas
- target: overall
- mode: percentage
- statement: Every blocking check passes.
- threshold: 1.0


## F. Stop gates

- max_iterations: 8
- max_revisions_per_node: 3
- max_wall_clock_seconds: 7200
- max_tokens: 3000000
- max_cost_usd: 10.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 6 * * 1


## H. Constraints

- global:
  - rules: ["Quote complaints verbatim with their URL and date. Never paraphrase into evidence.","Read public pages only. Never log in, never scrape a logged-in view.","Never contact, reply to, or quote-post anyone whose complaint you collected.","Do not name private individuals in the report. The pain is the finding, not the person.","An idea below the complaint threshold does not go in the report, however good it sounds.","Report the competitor honestly, including when it means the idea is taken."]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["contacting anyone whose complaint was collected","publishing the report anywhere"]

## I. Execution guidelines

- dependency: ["collect -> cluster -> market -> write"]

### collect
- guideline: Gather verbatim complaints with venue, URL, and date. Interpret nothing here — the clustering must be reproducible from what you saved.

### cluster
- guideline: Group complaints into pains and count them. A group below the threshold is dropped, not argued up.

### market
- guideline: Check each surviving pain against what is already being sold. Name the competitor and its price where one exists.

### write
- guideline: Write the report. Every idea carries its complaint URLs, its count, and its competitive position.


## J. Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Finds where a segment complains publicly.


## Graph

- concurrency:
  - mode: auto
  - cap: 6
  - min_marginal_gain: 0.05

### collect-social
- role: researcher
- instruction: Collect verbatim public complaints in the segment from social media and forums over the last 90 days. Record venue, URL, date, and text into out/complaints.json.
- goals: ["collected"]
- tier: cheap
- skills: ["agent-reach"]
- stage: collect
- weight: 3.0
- isolated: false

### collect-feeds
- role: researcher
- instruction: Same, from blogs and RSS feeds in the segment. Append to out/complaints.json.
- goals: ["collected"]
- tier: cheap
- stage: collect
- weight: 2.0
- isolated: false

### cluster
- role: builder
- instruction: Group complaints into distinct pains, count distinct complainants per pain, and drop anything below the threshold. Write min_complaints_per_pain to metrics.json.
- depends_on: ["collect-social","collect-feeds"]
- goals: ["clustered"]
- tier: standard
- stage: cluster
- weight: 2.0
- isolated: false

### market-check
- role: researcher
- instruction: For each surviving pain, check what is already being sold against it, including the listings at the market_check source. Record the competitor, its price, and whether it appears to be thriving.
- depends_on: ["cluster"]
- goals: ["checked"]
- tier: standard
- stage: market
- weight: 2.0
- isolated: false

### write-report
- role: builder
- instruction: Write out/ideas.md. Each idea carries its complaint URLs, its count, and its competitive position, including when that position is "already taken".
- depends_on: ["market-check"]
- goals: ["reported"]
- tier: standard
- stage: write
- weight: 2.0
- isolated: true

### verify
- role: judge
- instruction: Check every claim in out/ideas.md against out/complaints.json. Report per-claim pass or fail, quoting the complaint or stating that none exists.
- depends_on: ["write-report"]
- goals: ["reported"]
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

