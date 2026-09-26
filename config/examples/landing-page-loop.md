# landing-page-loop

- version: 0.1.0
- description: Produce a static, accessible, fast landing page with working calls to action, buildable and deployable to GitHub Pages.
- environment: dev
- features:
  - self_evolution: false
  - marketplace_skills: false
  - external_side_effects: false
  - parallel_execution: true
  - human_approval: true

## Background

### site_goal
- value: Get a visitor to start a free trial. One primary action; everything on the page either supports it or is cut.

### audience
- value: Replace with who this page is for and what they already believe.

### cta_links
- value: primary = https://example.com/signup, secondary = https://example.com/docs. Every CTA on the page must be one of these.

### output_dir
- note: Static output. GitHub Pages serves this directory.
- value: site/

### budget
- value: Lighthouse performance >= 90, accessibility >= 95, total page weight < 500KB


## Prerequisites

### Wrote the one sentence a visitor should be able to repeat after leaving
- done: false

### Built one page section by hand and ran Lighthouse on it
- evidence: link the report
- done: false

### Confirmed the GitHub Pages branch and build command work end to end
- done: false


## Goals

### build
- priority: 1
- description: A static site builds from source into site/ with no errors.

### fast-and-accessible
- priority: 2
- description: The built page meets the stated Lighthouse and page-weight budget. These are numbers, not opinions.
- depends_on: ["build"]

### ctas-work
- priority: 2
- description: Every call to action resolves to a live URL from the approved list.
- depends_on: ["build"]

### reads-well
- priority: 3
- description: The page states the value proposition above the fold and does not overclaim.
- depends_on: ["build"]


## Success

### shippable
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

### outline
- isolation:
  - mode: none
- role: researcher
- instruction: "Write out/outline.md: the page's sections, the one action they lead to, and the claim each section makes. Cite where each claim is supported."
- goals: ["reads-well"]
- tier: cheap
- stage: structure
- weight: 1.0

### implement
- isolation:
  - mode: worktree
- role: builder
- instruction: Build the static site from the outline into site/. Semantic HTML, no third-party scripts, every CTA pointing at an approved link.
- depends_on: ["outline"]
- goals: ["build","ctas-work"]
- tier: standard
- stage: implement
- weight: 4.0

### measure
- isolation:
  - mode: none
- role: researcher
- instruction: Run Lighthouse and a byte count against the built site. Write lighthouse_performance, lighthouse_accessibility, and page_weight_kb to metrics.json. Report the numbers you got.
- depends_on: ["implement"]
- goals: ["fast-and-accessible"]
- tier: cheap
- stage: harden
- weight: 1.0

### optimise
- isolation:
  - mode: worktree
- role: builder
- instruction: Fix whatever the measurements say is failing, cheapest fix first. Do not add content in this phase.
- depends_on: ["measure"]
- goals: ["fast-and-accessible"]
- tier: standard
- stage: harden
- weight: 2.0

### review-copy
- isolation:
  - mode: none
- role: judge
- instruction: Check every claim on the built page against the product documentation. Report per-claim pass or fail, quoting the supporting text or its absence.
- depends_on: ["implement"]
- goals: ["reads-well"]
- tier: strong
- provider: openai
- stage: harden
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

- dependency: ["structure -> implement -> harden"]

### structure
- guideline: Decide the page's sections and the single action they lead to. No styling yet — a page that is beautiful and says nothing fails here.

### implement
- guideline: Build the sections as static HTML and CSS. Every asset you add counts against the weight budget, so add it deliberately.

### harden
- guideline: Measure. Fix what the numbers say is wrong, in the order of how much it costs the visitor. Do not add features in this phase.


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

### file_change
- on:
  - path: src/
- enabled: true


## Checks

### builds-clean
- target: build
- blocking: true
- mode: objective
- statement: The static build exits zero.
- detector:
  - type: script
  - command: npm
  - args: ["run","build"]
  - expect_exit: 0

### index-exists
- target: build
- blocking: true
- mode: objective
- statement: The build produced an index page.
- detector:
  - type: file_exists
  - path: site/index.html
  - non_empty: true

### performance
- target: fast-and-accessible
- blocking: true
- mode: objective
- statement: Lighthouse performance is at or above the budget.
- detector:
  - type: threshold
  - metric: lighthouse_performance
  - op: gte
  - value: 90.0

### accessibility
- target: fast-and-accessible
- blocking: true
- mode: objective
- statement: Lighthouse accessibility is at or above the budget.
- detector:
  - type: threshold
  - metric: lighthouse_accessibility
  - op: gte
  - value: 95.0

### page-weight
- target: fast-and-accessible
- blocking: true
- mode: objective
- statement: Total transferred bytes are under the budget.
- detector:
  - type: threshold
  - metric: page_weight_kb
  - op: lt
  - value: 500.0

### links-resolve
- target: ctas-work
- blocking: true
- mode: objective
- statement: Every link in the built page returns a success status.
- detector:
  - type: script
  - command: scripts/check-links.sh
  - args: ["site/"]
  - expect_exit: 0

### copy-honest
- target: reads-well
- blocking: true
- mode: subjective
- statement: Every claim on the page is one the product can support. No superlative that is not measured, no testimonial that was not given.
- detector:
  - type: judge
  - standard: the FTC endorsement guides and the product's own documentation
  - min_score: 8.0

### deployable
- target: overall
- blocking: true
- mode: objective
- statement: The site directory is self-contained and needs no server.
- detector:
  - type: script
  - command: scripts/check-static.sh
  - args: ["site/"]
  - expect_exit: 0


## Gates

- stop:
  - max_iterations: 12
  - max_revisions_per_node: 4
  - max_wall_clock_seconds: 7200
  - max_tokens: 3000000
  - max_cost_usd: 10.0
  - no_progress_iterations: 3
  - no_progress_iterations_randomness: 2
  - stop_on_overall_success: true

## Limits

- global:
  - rules: ["Static output only. No server-side rendering, no runtime API calls on load.","Every CTA href must be one of the approved cta_links. No new destinations.","No third-party script that is not already in package.json.","No claim on the page that is not supported by the product documentation.","Do not invent testimonials, logos, user counts, or ratings.","Never git stash. Never git reset."]
  - forbidden_paths: [".github/workflows/",".git/"]
  - forbidden_commands: ["git push","npm publish","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["deploying to a live domain","changing DNS or GitHub Pages settings"]

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
