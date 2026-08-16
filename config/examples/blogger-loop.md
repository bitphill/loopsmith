# blogger-loop

- version: 0.1.0
- description: Research a trending topic in a chosen category and publish a post with a point of view, gated on mechanical style measurements and an independent read.

## A. Information

### categories
- value: science and technology
- note: One or more, comma-separated. Each run picks one.

### stance_rule
- value: Every post takes a position and says who would disagree. A post that only surveys is not finished.

### style_budget
- value: Sentence-length variance above 20, at most one stock transition per 500 words, hedging density below 2%.
- note: These are the mechanical tells. They are not sufficient for good writing; they are necessary, and unlike "sounds human" they are measurable.

### post_path
- value: out/post.md

### research_path
- value: out/research.md
- note: Sources with URLs. Every factual claim in the post traces here.


## B. Pre-execution

### Wrote one post by hand in the target voice and kept it as the reference
- done: false
- evidence: out/reference-post.md

### Ran the style measurement over the reference post and recorded the numbers
- done: false

### Decided what the blog will never do, and wrote it into the constraints
- done: false


## C. Goals

### researched
- description: A trending topic in the category is chosen and its sources gathered with URLs.
- priority: 1

### written
- description: A post exists that takes a position and names the counter-argument.
- depends_on: ["researched"]
- priority: 2

### sounds-human
- description: The post clears the mechanical style budget and reads to an independent judge as written rather than generated.
- depends_on: ["written"]
- priority: 3


## D. Validations

### research-exists
- target: researched
- mode: objective
- statement: Research notes with sources exist.
- detector:
  - type: file_exists
  - path: out/research.md
  - non_empty: true
- blocking: true

### sources-cited
- target: researched
- mode: objective
- statement: Research notes contain source URLs.
- detector:
  - type: regex_match
  - artifact: research
  - pattern: https?://
- blocking: true

### post-exists
- target: written
- mode: objective
- statement: A post was written.
- detector:
  - type: file_exists
  - path: out/post.md
  - non_empty: true
- blocking: true

### has-a-stance
- target: written
- mode: subjective
- statement: The post states a position and names who would disagree with it, rather than surveying every side equally.
- detector:
  - type: judge
  - standard: the stance_rule stated in this config
  - min_score: 7.0
- blocking: true

### sentence-variance
- target: sounds-human
- mode: objective
- statement: Sentence-length variance is above the floor.
- detector:
  - type: threshold
  - metric: sentence_length_variance
  - op: gt
  - value: 20.0
- blocking: true

### stock-transitions
- target: sounds-human
- mode: objective
- statement: Stock transitions per 500 words are at or below the ceiling.
- detector:
  - type: threshold
  - metric: stock_transitions_per_500w
  - op: lte
  - value: 1.0
- blocking: true

### hedging-density
- target: sounds-human
- mode: objective
- statement: Hedging density is below the ceiling.
- detector:
  - type: threshold
  - metric: hedging_density
  - op: lt
  - value: 0.02
- blocking: true

### independent-read
- target: sounds-human
- mode: subjective
- statement: "Read cold, the post has a voice: specific detail, an argument that costs the author something, and no sentence that could have been written about any other topic."
- detector:
  - type: judge
  - standard: out/reference-post.md as the voice to match
  - min_score: 7.0
- blocking: true

### claims-sourced
- target: overall
- mode: objective
- statement: The post cites sources.
- detector:
  - type: regex_match
  - artifact: post
  - pattern: https?://
- blocking: true


## E. Success

### publishable
- target: overall
- mode: percentage
- statement: Every blocking check passes.
- threshold: 1.0


## F. Stop gates

- max_iterations: 10
- max_revisions_per_node: 4
- max_wall_clock_seconds: 5400
- max_tokens: 3000000
- max_cost_usd: 8.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 5 * * 2,5


## H. Constraints

- global:
  - rules: ["Every factual claim carries a source URL. Fiction is allowed; fiction presented as fact is not.","Label opinion as opinion and speculation as speculation.","Never invent a quote, a study, a statistic, or a person.","Do not write about a named living person's private life.","Do not use a stock opening. No \"In today's fast-paced world\", no \"Let's dive in\".","If the post says nothing the reader could disagree with, it is not finished."]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["publishing the post anywhere","writing about a named living individual"]

## I. Execution guidelines

- dependency: ["research -> draft -> revise"]

### research
- guideline: Find what is being argued about in the category right now and collect the sources. Do not draft — the position should come from the reading, not from what you assumed before it.

### draft
- guideline: Write the post. Take a position, name the counter-argument, and use the specific details from the research rather than general ones.

### revise
- guideline: Fix what the style measurements flag and what the reader flagged. Do not add new claims here; a revision that needs new research means the research phase was short.


## J. Default skills

### agent-reach
- source: github
- url: https://github.com/Panniantong/agent-reach
- note: Finds what is being argued about in a category, and where.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05

### find-topic
- role: researcher
- instruction: Pick one trending topic in the category that people are actually disagreeing about, and collect sources into out/research.md with URLs. Note both sides of the disagreement.
- goals: ["researched"]
- tier: cheap
- skills: ["agent-reach"]
- stage: research
- weight: 2.0
- isolated: false

### write
- role: builder
- instruction: Write out/post.md from the research. Take a position, name who would disagree and why they are not stupid, and use the specific details you found. Match the voice in out/reference-post.md.
- depends_on: ["find-topic"]
- goals: ["written"]
- tier: standard
- stage: draft
- weight: 3.0
- isolated: true

### measure-style
- role: researcher
- instruction: Compute sentence_length_variance, stock_transitions_per_500w, and hedging_density over out/post.md and write them to metrics.json.
- depends_on: ["write"]
- goals: ["sounds-human"]
- tier: cheap
- stage: revise
- weight: 1.0
- isolated: false

### cold-read
- role: judge
- instruction: Read out/post.md cold against out/reference-post.md. Report whether it has a voice, quoting the sentences that carry it or the ones that read as generated.
- depends_on: ["write"]
- goals: ["sounds-human","written"]
- tier: strong
- provider: openai
- stage: revise
- weight: 1.0
- isolated: false

### revise
- role: builder
- instruction: Fix what the measurements and the cold read flagged. Do not introduce new claims; if the post needs them, say so instead.
- depends_on: ["measure-style","cold-read"]
- goals: ["sounds-human"]
- tier: standard
- stage: revise
- weight: 2.0
- isolated: true


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
- explore: true
- explore_candidates: ["writing-for-agents","design-taste-frontend"]
- min_trials: 3

## Context

- carry_summaries: 2
- max_summary_chars: 1200

