# landing-page-loop

- version: 0.1.0
- description: Produce a static, accessible, fast landing page with working calls to action, buildable and deployable to GitHub Pages.

## A. Information

### site_goal
- value: Get a visitor to start a free trial. One primary action; everything on the page either supports it or is cut.

### audience
- value: Replace with who this page is for and what they already believe.

### cta_links
- value: primary = https://example.com/signup, secondary = https://example.com/docs. Every CTA on the page must be one of these.

### output_dir
- value: site/
- note: Static output. GitHub Pages serves this directory.

### budget
- value: Lighthouse performance >= 90, accessibility >= 95, total page weight < 500KB


## B. Pre-execution

### Wrote the one sentence a visitor should be able to repeat after leaving
- done: false

### Built one page section by hand and ran Lighthouse on it
- done: false
- evidence: link the report

### Confirmed the GitHub Pages branch and build command work end to end
- done: false


## C. Goals

### build
- description: A static site builds from source into site/ with no errors.
- priority: 1

### fast-and-accessible
- description: The built page meets the stated Lighthouse and page-weight budget. These are numbers, not opinions.
- depends_on: ["build"]
- priority: 2

### ctas-work
- description: Every call to action resolves to a live URL from the approved list.
- depends_on: ["build"]
- priority: 2

### reads-well
- description: The page states the value proposition above the fold and does not overclaim.
- depends_on: ["build"]
- priority: 3


## D. Validations

### builds-clean
- target: build
- mode: objective
- statement: The static build exits zero.
- detector:
  - type: script
  - command: npm
  - args: ["run","build"]
  - expect_exit: 0
- blocking: true

### index-exists
- target: build
- mode: objective
- statement: The build produced an index page.
- detector:
  - type: file_exists
  - path: site/index.html
  - non_empty: true
- blocking: true

### performance
- target: fast-and-accessible
- mode: objective
- statement: Lighthouse performance is at or above the budget.
- detector:
  - type: threshold
  - metric: lighthouse_performance
  - op: gte
  - value: 90.0
- blocking: true

### accessibility
- target: fast-and-accessible
- mode: objective
- statement: Lighthouse accessibility is at or above the budget.
- detector:
  - type: threshold
  - metric: lighthouse_accessibility
  - op: gte
  - value: 95.0
- blocking: true

### page-weight
- target: fast-and-accessible
- mode: objective
- statement: Total transferred bytes are under the budget.
- detector:
  - type: threshold
  - metric: page_weight_kb
  - op: lt
  - value: 500.0
- blocking: true

### links-resolve
- target: ctas-work
- mode: objective
- statement: Every link in the built page returns a success status.
- detector:
  - type: script
  - command: scripts/check-links.sh
  - args: ["site/"]
  - expect_exit: 0
- blocking: true

### copy-honest
- target: reads-well
- mode: subjective
- statement: Every claim on the page is one the product can support. No superlative that is not measured, no testimonial that was not given.
- detector:
  - type: judge
  - standard: the FTC endorsement guides and the product's own documentation
  - min_score: 8.0
- blocking: true

### deployable
- target: overall
- mode: objective
- statement: The site directory is self-contained and needs no server.
- detector:
  - type: script
  - command: scripts/check-static.sh
  - args: ["site/"]
  - expect_exit: 0
- blocking: true


## E. Success

### shippable
- target: overall
- mode: percentage
- statement: Every blocking check passes.
- threshold: 1.0


## F. Stop gates

- max_iterations: 12
- max_revisions_per_node: 4
- max_wall_clock_seconds: 7200
- max_tokens: 3000000
- max_cost_usd: 10.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### manual

### file_change
- path: src/


## H. Constraints

- global:
  - rules: ["Static output only. No server-side rendering, no runtime API calls on load.","Every CTA href must be one of the approved cta_links. No new destinations.","No third-party script that is not already in package.json.","No claim on the page that is not supported by the product documentation.","Do not invent testimonials, logos, user counts, or ratings.","Never git stash. Never git reset."]
  - forbidden_paths: [".github/workflows/",".git/"]
  - forbidden_commands: ["git push","npm publish","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["deploying to a live domain","changing DNS or GitHub Pages settings"]

## I. Execution guidelines

- dependency: ["structure -> implement -> harden"]

### structure
- guideline: Decide the page's sections and the single action they lead to. No styling yet — a page that is beautiful and says nothing fails here.

### implement
- guideline: Build the sections as static HTML and CSS. Every asset you add counts against the weight budget, so add it deliberately.

### harden
- guideline: Measure. Fix what the numbers say is wrong, in the order of how much it costs the visitor. Do not add features in this phase.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05

### outline
- role: researcher
- instruction: "Write out/outline.md: the page's sections, the one action they lead to, and the claim each section makes. Cite where each claim is supported."
- goals: ["reads-well"]
- tier: cheap
- stage: structure
- weight: 1.0
- isolated: false

### implement
- role: builder
- instruction: Build the static site from the outline into site/. Semantic HTML, no third-party scripts, every CTA pointing at an approved link.
- depends_on: ["outline"]
- goals: ["build","ctas-work"]
- tier: standard
- stage: implement
- weight: 4.0
- isolated: true

### measure
- role: researcher
- instruction: Run Lighthouse and a byte count against the built site. Write lighthouse_performance, lighthouse_accessibility, and page_weight_kb to metrics.json. Report the numbers you got.
- depends_on: ["implement"]
- goals: ["fast-and-accessible"]
- tier: cheap
- stage: harden
- weight: 1.0
- isolated: false

### optimise
- role: builder
- instruction: Fix whatever the measurements say is failing, cheapest fix first. Do not add content in this phase.
- depends_on: ["measure"]
- goals: ["fast-and-accessible"]
- tier: standard
- stage: harden
- weight: 2.0
- isolated: true

### review-copy
- role: judge
- instruction: Check every claim on the built page against the product documentation. Report per-claim pass or fail, quoting the supporting text or its absence.
- depends_on: ["implement"]
- goals: ["reads-well"]
- tier: strong
- provider: openai
- stage: harden
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

