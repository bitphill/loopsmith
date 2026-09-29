# traffic-loop

- version: 0.1.0
- description: Find the places a defined audience already gathers, post there within each venue's own rules, and measure referred sessions rather than posts made.

## A. Information

### site_url
- value: https://example.com
- note: Replace. Every claim a node makes about the product must be checkable here.

### target_audience
- value: Solo founders shipping their first paid product, technical, price-sensitive, active on Hacker News, Indie Hackers, and two or three niche subreddits.
- note: The narrower this is, the fewer venues qualify and the better they convert.

### value_proposition
- value: One sentence on what the site does that the audience cannot already do.

### analytics_export
- value: out/sessions.json
- note: Referral sessions by source, exported by your analytics tool. The loop reads this; it never estimates traffic from the number of posts it made.

### venue_rules
- value: out/venues.json
- note: "Per-venue: whether promotion is allowed, the rate limit, and the link policy. A venue with no entry here is not eligible."


## B. Pre-execution

### Posted by hand in three venues and recorded what happened
- done: false
- evidence: link the three posts and their outcomes

### Read and wrote down each venue's self-promotion rules
- done: false
- evidence: out/venues.json

### Confirmed analytics attributes referral sources correctly
- done: false
- evidence: a session in out/sessions.json traceable to a known post


## C. Goals

### find-venues
- description: Identify venues where the target audience is active AND where promotion is permitted by the venue's own written rules.
- priority: 1

### earn-attention
- description: Publish contributions that stand on their own merit, so the link is the least interesting part of the post.
- depends_on: ["find-venues"]
- priority: 2

### measured-traffic
- description: Referred sessions arrive from those venues, attributable in analytics.
- depends_on: ["earn-attention"]
- priority: 3


## D. Validations

### venues-recorded
- target: find-venues
- mode: objective
- statement: A venue file exists listing each venue's promotion policy.
- detector:
  - type: file_exists
  - path: out/venues.json
  - non_empty: true
- blocking: true

### promotion-permitted
- target: find-venues
- mode: objective
- statement: Every listed venue is marked as permitting promotion.
- detector:
  - type: script
  - command: scripts/check-venues.sh
  - expect_exit: 0
- blocking: true

### post-quality
- target: earn-attention
- mode: subjective
- statement: Each post is useful to the venue's readers even if the link is removed, and discloses the author's interest.
- detector:
  - type: judge
  - standard: the venue's own posting guidelines, quoted in out/venues.json
  - min_score: 7.0
- blocking: true

### sessions-arrived
- target: measured-traffic
- mode: objective
- statement: Referred sessions from the posted venues exceed the floor.
- detector:
  - type: threshold
  - metric: referred_sessions
  - op: gte
  - value: 50.0
- blocking: true

### no-banned-venues
- target: overall
- mode: objective
- statement: No post was made to a venue that forbids promotion.
- detector:
  - type: script
  - command: scripts/check-venues.sh
  - args: ["--posted-only"]
  - expect_exit: 0
- blocking: true


## E. Success

### traffic-earned
- target: overall
- mode: percentage
- statement: Every blocking check passes, including the venue-rules check.
- threshold: 1.0


## F. Stop gates

- max_iterations: 10
- max_revisions_per_node: 3
- max_wall_clock_seconds: 10800
- max_tokens: 3000000
- max_cost_usd: 12.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 9 * * 1


## H. Constraints

- global:
  - rules: ["Post only to venues listed in out/venues.json with promotion_allowed = true.","Respect each venue's stated rate limit. One post per venue per run, never more.","Disclose that you are affiliated with the site, in the post itself.","Never create an account, and never solve a CAPTCHA.","Never post the same text to two venues. Rewrite for each audience.","A post that would be useless with the link removed does not get published."]
  - forbidden_paths: [".git/","node_modules/"]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["publishing any post","creating or logging into any account","anything that would be a first contact with a named individual"]

## I. Execution guidelines

- dependency: ["research -> draft -> publish -> measure"]

### research
- guideline: Find venues and read their rules. Write nothing promotional yet. A venue whose rules you have not read is not a venue you have found.

### draft
- guideline: Write one contribution per venue, in that venue's register. The link is a footnote to something worth reading.

### publish
- guideline: Post what was drafted, one venue at a time, and record the URL. Stop at the venue's rate limit even if the loop has budget left.

### measure
- guideline: Read analytics only. Do not post in this phase, and do not explain away a low number — a low number is the finding.


## J. Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Finds where an audience actually gathers. Read its SKILL.md before promoting it out of quarantine.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05

### find-venues
- role: researcher
- instruction: Identify venues where the target audience is active. For each, read the venue's own posting rules and record whether promotion is permitted, the rate limit, and the link policy. Write out/venues.json. A venue whose rules you cannot find is recorded as promotion_allowed = false.
- goals: ["find-venues"]
- tier: cheap
- skills: ["agent-reach"]
- stage: research
- weight: 2.0
- isolated: false

### write-posts
- role: builder
- instruction: Write one contribution per eligible venue, in that venue's register and length. Each must be useful with the link removed. Include the affiliation disclosure. Write them to out/posts/.
- depends_on: ["find-venues"]
- goals: ["earn-attention"]
- tier: standard
- stage: draft
- weight: 3.0
- isolated: true

### review-posts
- role: judge
- instruction: Check each drafted post against the venue's own guidelines as recorded in out/venues.json. Report per-post pass or fail with the guideline quoted.
- depends_on: ["write-posts"]
- goals: ["earn-attention"]
- tier: strong
- provider: openai
- stage: draft
- weight: 1.0
- isolated: false

### publish
- role: builder
- instruction: Publish the approved posts, one per venue, and record each URL in out/posts/published.json. Stop at each venue's rate limit.
- depends_on: ["review-posts"]
- goals: ["measured-traffic"]
- tier: standard
- stage: publish
- weight: 1.0
- isolated: false

### measure
- role: researcher
- instruction: Export referral sessions by source into out/sessions.json and write the referred_sessions metric to metrics.json. Report the number, whatever it is.
- depends_on: ["publish"]
- goals: ["measured-traffic"]
- tier: cheap
- stage: measure
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

