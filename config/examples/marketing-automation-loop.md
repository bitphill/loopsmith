# marketing-automation-loop

- version: 0.1.0
- description: Turn a product's own documentation into platform-appropriate posts on a schedule, with optional generated imagery, and publish them behind a human checkpoint.

## A. Information

### business_url
- value: https://example.com
- note: The single source of truth. Every claim in every post must be supported here.

### target_audience
- value: Replace with who these posts are for and what they already believe.

### platforms
- value: x, pinterest, instagram, facebook, tiktok, rss
- note: Remove any platform you have not configured credentials for. A missing key means the platform is skipped, never faked.

### cadence
- value: At most one post per platform per day. Never the same text twice.

### media_generation
- value: off
- note: Set to "image", "video", or "both" to generate media. Requires an image or video model to be configured as a provider. Off by default — generated media is the most expensive part of this loop.

### queue_file
- value: out/queue.json
- note: Drafted posts awaiting the human checkpoint.


## B. Pre-execution

### Posted by hand on each platform for a week and kept the engagement numbers
- done: false
- evidence: link the posts

### Wrote down what a good post looks like per platform, in checkable terms
- done: false
- evidence: out/platform-standards.md

### Confirmed each platform's API credentials and rate limits
- done: false

### Confirmed the account is allowed to post automated content under each platform's terms
- done: false


## C. Goals

### drafted
- description: A queue of posts exists, one per configured platform, each in that platform's format and length.
- priority: 1

### accurate
- description: Every claim in every post is supported by the business site. No invented statistic, testimonial, or feature.
- depends_on: ["drafted"]
- priority: 1

### published
- description: Approved posts reach their platforms within the stated cadence.
- depends_on: ["accurate"]
- priority: 2


## D. Validations

### queue-exists
- target: drafted
- mode: objective
- statement: A post queue was produced.
- detector:
  - type: file_exists
  - path: out/queue.json
  - non_empty: true
- blocking: true

### per-platform-format
- target: drafted
- mode: objective
- statement: Every queued post fits its platform's length and media rules.
- detector:
  - type: script
  - command: scripts/check-formats.sh
  - expect_exit: 0
- blocking: true

### no-duplicates
- target: drafted
- mode: objective
- statement: No queued post repeats text already published.
- detector:
  - type: script
  - command: scripts/check-duplicates.sh
  - expect_exit: 0
- blocking: true

### claims-supported
- target: accurate
- mode: subjective
- statement: Every factual claim in every post is supported by a page on the business site, quoted in the judgment.
- detector:
  - type: judge
  - standard: the business site's own documentation, plus the FTC endorsement guides on disclosure
  - min_score: 8.0
- blocking: true

### cadence-respected
- target: published
- mode: objective
- statement: No platform received more than one post in the last 24 hours.
- detector:
  - type: script
  - command: scripts/check-cadence.sh
  - expect_exit: 0
- blocking: true

### disclosure-present
- target: overall
- mode: objective
- statement: Every post that promotes the product says so.
- detector:
  - type: script
  - command: scripts/check-disclosure.sh
  - expect_exit: 0
- blocking: true


## E. Success

### queue-shipped
- target: overall
- mode: percentage
- statement: Every blocking check passes.
- threshold: 1.0


## F. Stop gates

- max_iterations: 6
- max_revisions_per_node: 3
- max_wall_clock_seconds: 5400
- max_tokens: 3000000
- max_cost_usd: 15.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 8 * * 1-5


## H. Constraints

- global:
  - rules: ["Every claim must be supported by a page on the business site. Quote it in the draft.","Never invent a statistic, a testimonial, a customer name, or a rating.","Disclose the commercial relationship in any post that promotes the product.","One post per platform per day. Never exceed a platform's own rate limit.","Never reply to, like, or follow anyone. This loop publishes; it does not engage.","Never post the same text to two platforms. Rewrite for each.","Generated media must be labelled as generated wherever the platform requires it.","If a platform's credentials are missing, skip that platform and say so."]
  - forbidden_paths: [".git/"]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["publishing any post to any platform","generating paid media","changing the cadence or adding a platform"]

## I. Execution guidelines

- dependency: ["source -> draft","draft -> media -> publish"]

### source
- guideline: Read the business site and collect the supported claims you may draw on. Write nothing promotional yet.

### draft
- guideline: Write one post per platform, in that platform's register and length, each citing the page that supports its claim.

### media
- guideline: Generate imagery only if media_generation is on and the draft calls for it. Skip this phase entirely otherwise — it is the expensive one.

### publish
- guideline: Publish approved posts within the cadence, one platform at a time, and record each post URL.


## J. Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Finds the venues and formats an audience actually responds to.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05

### gather-claims
- role: researcher
- instruction: "Read the business site and write out/claims.md: each supportable claim with the URL that supports it. A claim you cannot source does not go on the list."
- goals: ["accurate"]
- tier: cheap
- stage: source
- weight: 2.0
- isolated: false

### draft-posts
- role: builder
- instruction: Write one post per configured platform into out/queue.json, in that platform's format and length, each citing the claim it rests on and carrying the disclosure.
- depends_on: ["gather-claims"]
- goals: ["drafted"]
- tier: standard
- skills: ["agent-reach"]
- stage: draft
- weight: 3.0
- isolated: true

### fact-check
- role: judge
- instruction: Check every claim in every queued post against out/claims.md and the business site. Report per-post pass or fail, quoting the supporting page or stating that none exists.
- depends_on: ["draft-posts"]
- goals: ["accurate"]
- tier: strong
- provider: openai
- stage: draft
- weight: 1.0
- isolated: false

### make-media
- role: builder
- instruction: If media_generation is on, produce the imagery the drafts call for into out/media/ and label it as generated. If it is off, do nothing and say so.
- depends_on: ["fact-check"]
- goals: ["drafted"]
- tier: standard
- stage: media
- weight: 2.0
- isolated: false

### publish
- role: builder
- instruction: Publish the approved posts within the cadence and record each post URL in out/published.json. Skip any platform without credentials and report it.
- depends_on: ["make-media"]
- goals: ["published"]
- tier: standard
- stage: publish
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

