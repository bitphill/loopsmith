# viral-game-loop

- version: 0.1.0
- description: Build, test, and package a small Godot game around one mechanic, gated on build health, time-to-first-play, and a cold playtest.

## A. Information

### idea
- value: "Replace with one sentence: the single mechanic, and the twist on it. If it takes two sentences, it is two games."

### engine
- value: Godot 4.x — https://github.com/godotengine/godot, installed locally
- note: The engine runs on this machine. Nothing here uploads a build anywhere.

### shareability_rule
- value: A player reaches something worth screenshotting within 30 seconds of first input, and the whole game is finishable in under 5 minutes.

### export_dir
- value: build/

### publish_notes
- value: docs/publishing.md
- note: Where and how to publish, written as a checklist for a human. The loop writes the checklist; it does not publish.


## B. Pre-execution

### Built and exported a Godot hello-world on this machine
- done: false
- evidence: the export path

### Played three games in the target genre and wrote down what made each shareable
- done: false
- evidence: docs/references.md

### Prototyped the core mechanic by hand and confirmed it is fun for 60 seconds
- done: false


## C. Goals

### builds
- description: The project builds and exports headlessly with no errors.
- priority: 1

### playable
- description: The game runs, the core mechanic works, and the first interesting moment arrives inside the stated window.
- depends_on: ["builds"]
- priority: 2

### shareable
- description: A cold playtester finishes it and can say what the game is in one sentence.
- depends_on: ["playable"]
- priority: 3

### publishable
- description: A publishing checklist exists that a human can follow.
- depends_on: ["playable"]
- priority: 3


## D. Validations

### exports-clean
- target: builds
- mode: objective
- statement: A headless export exits zero.
- detector:
  - type: script
  - command: scripts/godot-export.sh
  - expect_exit: 0
- blocking: true

### no-script-errors
- target: builds
- mode: objective
- statement: The headless run logs no script errors.
- detector:
  - type: script
  - command: scripts/godot-check.sh
  - expect_exit: 0
- blocking: true

### time-to-first-play
- target: playable
- mode: objective
- statement: Seconds from first input to the first scoring moment is under the window.
- detector:
  - type: threshold
  - metric: seconds_to_first_play
  - op: lt
  - value: 30.0
- blocking: true

### completable
- target: playable
- mode: objective
- statement: An automated playthrough reaches the end state.
- detector:
  - type: script
  - command: scripts/godot-playthrough.sh
  - expect_exit: 0
- blocking: true

### cold-read
- target: shareable
- mode: subjective
- statement: Someone who has never seen this game can say what it is in one sentence after watching thirty seconds of it.
- detector:
  - type: judge
  - standard: the shareability_rule stated in this config, and docs/references.md
  - min_score: 7.0
- blocking: true

### checklist-exists
- target: publishable
- mode: objective
- statement: A publishing checklist exists.
- detector:
  - type: file_exists
  - path: docs/publishing.md
  - non_empty: true
- blocking: true

### assets-licensed
- target: overall
- mode: objective
- statement: Every asset has a recorded licence permitting distribution.
- detector:
  - type: script
  - command: scripts/check-licences.sh
  - expect_exit: 0
- blocking: true


## E. Success

### ready-to-publish
- target: overall
- mode: percentage
- statement: Every blocking check passes, including asset licensing.
- threshold: 1.0


## F. Stop gates

- max_iterations: 15
- max_revisions_per_node: 4
- max_wall_clock_seconds: 14400
- max_tokens: 4000000
- max_cost_usd: 15.0
- no_progress_iterations: 4
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### manual

### file_change
- path: scenes/


## H. Constraints

- global:
  - rules: ["One mechanic. A second mechanic is a second game; write it down and do not build it.","Every asset must be original or carry a licence permitting distribution. Record the licence next to the asset.","Never commit a binary export to git. Exports go to build/, which is ignored.","Never publish, upload, or post anything. The loop writes the checklist; a human follows it.","If the game is not fun in the first thirty seconds, cut content rather than adding it.","Never git stash. Never git reset."]
  - forbidden_paths: [".git/","build/"]
  - forbidden_commands: ["git push","rm -rf","butler push"]
  - max_seconds: 1800
  - human_checkpoint: ["publishing or uploading a build anywhere","adding an asset whose licence you cannot name"]

## I. Execution guidelines

- dependency: ["prototype -> build-out -> polish -> package"]

### prototype
- guideline: Get the core mechanic working and nothing else. No menus, no art, no sound. If the mechanic is not fun as a grey box, art will not save it.

### build-out
- guideline: "Add exactly what the mechanic needs to be finishable: the loop, the end state, and the feedback that makes progress legible."

### polish
- guideline: Cut and tighten. In this phase the only additions allowed are ones that reduce time-to-first-play.

### package
- guideline: Export, check licences, and write the publishing checklist for a human. Publish nothing.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05

### prototype
- role: builder
- instruction: Implement the core mechanic as a grey-box scene. No art, no menus. It must run headlessly and be playable with one input.
- goals: ["builds","playable"]
- tier: standard
- stage: prototype
- weight: 4.0
- isolated: true

### build-out
- role: builder
- instruction: Add the game loop, the end state, and the feedback that makes progress visible. Keep it finishable in five minutes.
- depends_on: ["prototype"]
- goals: ["playable"]
- tier: standard
- stage: build-out
- weight: 4.0
- isolated: true

### measure
- role: researcher
- instruction: Run the headless playthrough, measure seconds_to_first_play, and write it to metrics.json along with the playthrough result.
- depends_on: ["build-out"]
- goals: ["playable"]
- tier: cheap
- stage: polish
- weight: 1.0
- isolated: false

### polish
- role: builder
- instruction: Cut whatever delays the first interesting moment. Add nothing that does not reduce it.
- depends_on: ["measure"]
- goals: ["shareable"]
- tier: standard
- stage: polish
- weight: 2.0
- isolated: true

### playtest
- role: judge
- instruction: Watch the recorded playthrough cold. Say in one sentence what the game is, then report whether that sentence is interesting. Quote the moment the game became legible, or say it never did.
- depends_on: ["polish"]
- goals: ["shareable"]
- tier: strong
- provider: openai
- stage: polish
- weight: 1.0
- isolated: false

### package
- role: builder
- instruction: Export to build/, verify every asset's licence, and write docs/publishing.md as a checklist a human can follow. Publish nothing.
- depends_on: ["playtest"]
- goals: ["publishable"]
- tier: standard
- stage: package
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
- explore: true
- explore_candidates: ["frontend-design","prototype"]
- min_trials: 3

## Context

- carry_summaries: 2
- max_summary_chars: 1200

