# traffic-loop

- version: 0.1.0
- description: Find the places a defined audience already gathers, post there within each venue's own rules, and measure referred sessions rather than posts made.
- environment: dev
- features:
  - self_evolution: false
  - marketplace_skills: false
  - external_side_effects: false
  - parallel_execution: true
  - human_approval: true

## Background

### site_url
- note: Replace. Every claim a node makes about the product must be checkable here.
- value: https://example.com

### target_audience
- note: The narrower this is, the fewer venues qualify and the better they convert.
- value: Solo founders shipping their first paid product, technical, price-sensitive, active on Hacker News, Indie Hackers, and two or three niche subreddits.

### value_proposition
- value: One sentence on what the site does that the audience cannot already do.

### analytics_export
- note: Referral sessions by source, exported by your analytics tool. The loop reads this; it never estimates traffic from the number of posts it made.
- value: out/sessions.json

### venue_rules
- note: "Per-venue: whether promotion is allowed, the rate limit, and the link policy. A venue with no entry here is not eligible."
- value: out/venues.json


## Prerequisites

### Posted by hand in three venues and recorded what happened
- evidence: link the three posts and their outcomes
- done: false

### Read and wrote down each venue's self-promotion rules
- evidence: out/venues.json
- done: false

### Confirmed analytics attributes referral sources correctly
- evidence: a session in out/sessions.json traceable to a known post
- done: false


## Goals

### find-venues
- priority: 1
- description: Identify venues where the target audience is active AND where promotion is permitted by the venue's own written rules.

### earn-attention
- priority: 2
- description: Publish contributions that stand on their own merit, so the link is the least interesting part of the post.
- depends_on: ["find-venues"]

### measured-traffic
- priority: 3
- description: Referred sessions arrive from those venues, attributable in analytics.
- depends_on: ["earn-attention"]


## Success

### traffic-earned
- target: overall
- threshold: 1.0
- mode: percentage
- statement: Every blocking check passes, including the venue-rules check.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05
- join:
  - strategy: wait_for_all

### find-venues
- isolation:
  - mode: none
- role: researcher
- instruction: Identify venues where the target audience is active. For each, read the venue's own posting rules and record whether promotion is permitted, the rate limit, and the link policy. Write out/venues.json. A venue whose rules you cannot find is recorded as promotion_allowed = false.
- goals: ["find-venues"]
- tier: cheap
- skills: ["agent-reach"]
- stage: research
- weight: 2.0

### write-posts
- isolation:
  - mode: worktree
- role: builder
- instruction: Write one contribution per eligible venue, in that venue's register and length. Each must be useful with the link removed. Include the affiliation disclosure. Write them to out/posts/.
- depends_on: ["find-venues"]
- goals: ["earn-attention"]
- tier: standard
- stage: draft
- weight: 3.0

### review-posts
- isolation:
  - mode: none
- role: judge
- instruction: Check each drafted post against the venue's own guidelines as recorded in out/venues.json. Report per-post pass or fail with the guideline quoted.
- depends_on: ["write-posts"]
- goals: ["earn-attention"]
- tier: strong
- provider: openai
- stage: draft
- weight: 1.0

### publish
- isolation:
  - mode: none
- role: builder
- instruction: Publish the approved posts, one per venue, and record each URL in out/posts/published.json. Stop at each venue's rate limit.
- depends_on: ["review-posts"]
- goals: ["measured-traffic"]
- tier: standard
- stage: publish
- weight: 1.0

### measure
- isolation:
  - mode: none
- role: researcher
- instruction: Export referral sessions by source into out/sessions.json and write the referred_sessions metric to metrics.json. Report the number, whatever it is.
- depends_on: ["publish"]
- goals: ["measured-traffic"]
- tier: cheap
- stage: measure
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

- dependency: ["research -> draft -> publish -> measure"]

### research
- guideline: Find venues and read their rules. Write nothing promotional yet. A venue whose rules you have not read is not a venue you have found.

### draft
- guideline: Write one contribution per venue, in that venue's register. The link is a footnote to something worth reading.

### publish
- guideline: Post what was drafted, one venue at a time, and record the URL. Stop at the venue's rate limit even if the loop has budget left.

### measure
- guideline: Read analytics only. Do not post in this phase, and do not explain away a low number — a low number is the finding.


## Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Finds where an audience actually gathers. Read its SKILL.md before promoting it out of quarantine.
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

### cron
- on:
  - expr: 0 9 * * 1
- enabled: true


## Checks

### venues-recorded
- target: find-venues
- blocking: true
- mode: objective
- statement: A venue file exists listing each venue's promotion policy.
- detector:
  - type: file_exists
  - path: out/venues.json
  - non_empty: true

### promotion-permitted
- target: find-venues
- blocking: true
- mode: objective
- statement: Every listed venue is marked as permitting promotion.
- detector:
  - type: script
  - command: scripts/check-venues.sh
  - expect_exit: 0

### post-quality
- target: earn-attention
- blocking: true
- mode: subjective
- statement: Each post is useful to the venue's readers even if the link is removed, and discloses the author's interest.
- detector:
  - type: judge
  - standard: the venue's own posting guidelines, quoted in out/venues.json
  - min_score: 7.0

### sessions-arrived
- target: measured-traffic
- blocking: true
- mode: objective
- statement: Referred sessions from the posted venues exceed the floor.
- detector:
  - type: threshold
  - metric: referred_sessions
  - op: gte
  - value: 50.0

### no-banned-venues
- target: overall
- blocking: true
- mode: objective
- statement: No post was made to a venue that forbids promotion.
- detector:
  - type: script
  - command: scripts/check-venues.sh
  - args: ["--posted-only"]
  - expect_exit: 0


## Gates

- stop:
  - max_iterations: 10
  - max_revisions_per_node: 3
  - max_wall_clock_seconds: 10800
  - max_tokens: 3000000
  - max_cost_usd: 12.0
  - no_progress_iterations: 3
  - no_progress_iterations_randomness: 2
  - stop_on_overall_success: true

## Limits

- global:
  - rules: ["Post only to venues listed in out/venues.json with promotion_allowed = true.","Respect each venue's stated rate limit. One post per venue per run, never more.","Disclose that you are affiliated with the site, in the post itself.","Never create an account, and never solve a CAPTCHA.","Never post the same text to two venues. Rewrite for each audience.","A post that would be useless with the link removed does not get published."]
  - forbidden_paths: [".git/","node_modules/"]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["publishing any post","creating or logging into any account","anything that would be a first contact with a named individual"]

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
