# sales-leads-loop

- version: 0.1.0
- description: Find business contacts matching an ICP from public, permitted sources, with a recorded source and lawful basis for every record.

## A. Information

### ideal_customer
- value: "Replace. Be specific: industry, company size, role, and the problem they already know they have. A vague ICP produces a large, worthless list."

### my_site
- value: https://example.com
- note: Optional. Used to infer who already buys, not to make claims.

### competitor_sites
- value: ""
- note: Optional, comma-separated. Public pages only.

### permitted_sources
- value: Company websites, public business directories, official APIs, and Google Maps' Places API. Business contact details only.

### forbidden_sources
- value: Anything behind a login, LinkedIn scraping (it breaches their terms), personal social accounts, and any purchased list of unknown provenance.

### leads_file
- value: out/leads.json
- note: Each record carries source_url, collected_at, and lawful_basis.


## B. Pre-execution

### Found ten leads by hand and recorded where each came from
- done: false
- evidence: link the ten records

### Wrote down the lawful basis for processing, per jurisdiction you target
- done: false
- evidence: out/lawful-basis.md

### Confirmed a working suppression list and an opt-out route exist
- done: false
- evidence: out/suppression.json


## C. Goals

### sourced
- description: Leads are collected only from permitted sources, each with a recorded source URL and collection timestamp.
- priority: 1

### lawful
- description: Every record carries a lawful basis, and anyone on the suppression list is absent.
- depends_on: ["sourced"]
- priority: 1

### qualified
- description: Records match the ICP on evidence from the source, not on inference about a named individual.
- depends_on: ["sourced"]
- priority: 2


## D. Validations

### leads-exist
- target: sourced
- mode: objective
- statement: A leads file was produced.
- detector:
  - type: file_exists
  - path: out/leads.json
  - non_empty: true
- blocking: true

### every-lead-has-a-source
- target: sourced
- mode: objective
- statement: Every record carries source_url and collected_at.
- detector:
  - type: script
  - command: scripts/check-provenance.sh
  - expect_exit: 0
- blocking: true

### permitted-sources-only
- target: sourced
- mode: objective
- statement: No record came from a forbidden source.
- detector:
  - type: script
  - command: scripts/check-sources.sh
  - expect_exit: 0
- blocking: true

### lawful-basis-recorded
- target: lawful
- mode: objective
- statement: Every record states a lawful basis for processing.
- detector:
  - type: script
  - command: scripts/check-basis.sh
  - expect_exit: 0
- blocking: true

### suppression-honoured
- target: lawful
- mode: objective
- statement: No record appears on the suppression list.
- detector:
  - type: script
  - command: scripts/check-suppression.sh
  - expect_exit: 0
- blocking: true

### icp-match-rate
- target: qualified
- mode: objective
- statement: The share of records matching the ICP is above the floor.
- detector:
  - type: threshold
  - metric: icp_match_rate
  - op: gte
  - value: 0.8
- blocking: true

### business-contacts-only
- target: overall
- mode: subjective
- statement: Records are business contact details for a role at a company, not personal data about a private individual.
- detector:
  - type: judge
  - standard: GDPR Article 6(1)(f) legitimate interest as applied to B2B contact data, and the recorded lawful basis in out/lawful-basis.md
  - min_score: 8.0
- blocking: true


## E. Success

### usable-and-defensible
- target: overall
- mode: percentage
- statement: Every blocking check passes, including provenance and lawful basis.
- threshold: 1.0


## F. Stop gates

- max_iterations: 8
- max_revisions_per_node: 3
- max_wall_clock_seconds: 7200
- max_tokens: 2500000
- max_cost_usd: 10.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 6 * * 1


## H. Constraints

- global:
  - rules: ["Collect business contact details only. Never personal addresses or personal phone numbers.","Use official APIs where one exists. Honour robots.txt where one does not.","Never access anything behind a login, and never create an account.","Never scrape LinkedIn. It breaches their terms of service regardless of what the data is.","Never solve a CAPTCHA or otherwise defeat bot detection.","Rate-limit every source to at most one request per second.","Record source_url, collected_at, and lawful_basis for every record, at collection time.","Drop any record matching the suppression list, immediately and permanently.","This loop collects. It does not contact anyone."]
  - forbidden_paths: [".git/"]
  - forbidden_commands: ["git push","rm -rf","curl -X POST"]
  - max_seconds: 900
  - human_checkpoint: ["contacting anyone on this list","exporting the list anywhere outside this directory","adding a source not listed under permitted_sources"]

## I. Execution guidelines

- dependency: ["define -> collect -> qualify"]

### define
- guideline: Turn the ICP into checkable criteria and confirm which sources are permitted for it. Collect nothing yet.

### collect
- guideline: Gather records from permitted sources only, recording provenance at the moment of collection. A record whose source you cannot name is discarded, not backfilled.

### qualify
- guideline: Score records against the ICP using evidence already collected. Do not fetch more data about a named person to improve a score.


## J. Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Finds where a given audience is publicly visible.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05

### define-icp
- role: researcher
- instruction: Turn the ICP into checkable criteria and write out/icp.md. List which of the permitted sources can actually be used for it, and why.
- goals: ["sourced"]
- tier: cheap
- stage: define
- weight: 1.0
- isolated: false

### collect-directories
- role: researcher
- instruction: Collect matching business records from public directories and company websites. Record source_url, collected_at, and lawful_basis per record.
- depends_on: ["define-icp"]
- goals: ["sourced","lawful"]
- tier: standard
- skills: ["agent-reach"]
- stage: collect
- weight: 3.0
- isolated: false

### collect-maps
- role: researcher
- instruction: Collect matching business records from the Google Maps Places API. Same provenance fields. Stop at the API's documented rate limit.
- depends_on: ["define-icp"]
- goals: ["sourced","lawful"]
- tier: standard
- stage: collect
- weight: 2.0
- isolated: false

### qualify
- role: builder
- instruction: Merge, de-duplicate, drop suppressed records, and score each against the ICP criteria. Write out/leads.json and icp_match_rate to metrics.json.
- depends_on: ["collect-directories","collect-maps"]
- goals: ["qualified"]
- tier: standard
- stage: qualify
- weight: 2.0
- isolated: true

### compliance-review
- role: judge
- instruction: Check a sample of records against the recorded lawful basis and the permitted-source list. Report per-record pass or fail with the source URL quoted. Treat a missing basis as a fail.
- depends_on: ["qualify"]
- goals: ["lawful"]
- tier: strong
- provider: openai
- stage: qualify
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

