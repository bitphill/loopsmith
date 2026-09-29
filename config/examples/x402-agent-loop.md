# x402-agent-loop

- version: 0.1.0
- description: Pursue a goal that requires paying for services or delegating work, using x402 payments from a funded float under a hard spend cap.
- environment: dev
- features:
  - self_evolution: false
  - marketplace_skills: false
  - external_side_effects: false
  - parallel_execution: true
  - human_approval: true

## Background

### objective
- value: Replace with the outcome worth paying for. Be specific about what "done" buys you — this loop will spend money pursuing it.

### float_usd
- note: The dedicated account's balance. Fund it with what you would accept losing. Never point this at a primary wallet.
- value: "25.00"

### spend_cap_usd
- note: Enforced by stop_gates.max_cost_usd as well, so the gate stops the run even if a merchant's accounting disagrees.
- value: "20.00"

### merchant_allowlist
- note: Services this agent may pay. A merchant not on this list is not payable, however good the offer looks.
- value: out/merchants.json

### delegation_venues
- note: Where work can be delegated to a human when the agent cannot do it itself.
- value: https://rentahuman.ai/for-agents

### ledger_path
- note: Every payment with amount, merchant, what it bought, and the outcome.
- value: out/payments.json


## Prerequisites

### Funded a dedicated float account and confirmed the balance
- evidence: the account id and its balance, not the key
- done: false

### Paid one merchant by hand through x402 and kept the receipt
- done: false

### Wrote the merchant allowlist and justified each entry
- evidence: out/merchants.json
- done: false

### Decided the posture — supervised or autonomous — and edited human_checkpoint to match
- done: false


## Goals

### planned
- priority: 1
- description: A plan exists stating what will be bought or delegated, from whom, at what price, and what it is expected to produce.

### spent-well
- priority: 1
- description: Every payment is to an allowlisted merchant, within the cap, and recorded with what it bought.
- depends_on: ["planned"]

### objective-met
- priority: 2
- description: The stated objective is achieved and independently checkable.
- depends_on: ["spent-well"]


## Success

### objective-achieved-within-budget
- target: overall
- threshold: 1.0
- mode: percentage
- statement: Every blocking check passes, including reconciliation.


## Graph

- concurrency:
  - mode: sequential
- join:
  - strategy: wait_for_all

### plan-spend
- isolation:
  - mode: none
- role: manager
- instruction: "Write out/plan.md: what to buy or delegate, from which allowlisted merchant, the price, and the artifact it should produce. Include the cheapest option you rejected and why."
- goals: ["planned"]
- tier: strong
- stage: plan
- weight: 2.0

### challenge-plan
- isolation:
  - mode: none
- role: adversary
- instruction: Argue that each planned payment is unnecessary, overpriced, or will not produce the named artifact. Anything you cannot defend against gets cut from the plan.
- depends_on: ["plan-spend"]
- goals: ["planned"]
- tier: strong
- provider: openai
- stage: plan
- weight: 1.0

### execute
- isolation:
  - mode: worktree
- role: builder
- instruction: "Carry out the plan: pay allowlisted merchants or delegate to a human venue, declaring that you are an automated agent. Record each payment in out/payments.json before starting the next. Write total_spend_usd to metrics.json."
- depends_on: ["challenge-plan"]
- goals: ["spent-well","objective-met"]
- tier: standard
- stage: execute
- weight: 4.0

### reconcile
- isolation:
  - mode: none
- role: judge
- instruction: Compare out/payments.json against the account balance and out/plan.md. Report per-payment whether it was allowlisted, within the ceiling, and produced what the plan expected. A discrepancy is a fail.
- depends_on: ["execute"]
- goals: ["spent-well"]
- tier: strong
- provider: openai
- stage: verify
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
- tiers: ["standard","strong"]
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
- requires_env: ["OPENAI_API_KEY","X402_ACCOUNT_KEY"]
- timeout_seconds: 600
- prompt_on_stdin: true


## Phases

- dependency: ["plan -> execute -> verify"]

### plan
- guideline: Decide what to buy or delegate, from which allowlisted merchant, at what price, and what it should produce. Spend nothing in this phase. A plan that cannot name the expected artifact is not a plan.

### execute
- guideline: Buy or delegate exactly what the plan named. Record each payment before starting the next. If a price differs from the plan, stop and re-plan rather than paying the difference.

### verify
- guideline: Check what the money bought against what the plan expected, and reconcile the balance. Spend nothing here.


## Skills

- acquisition_order: ["installed"]
- quarantine_dir: generated-skills
- min_marketplace_stars: 100
- require_human_promotion: true
- explore: false
- min_trials: 3
- min_trust_level: reviewed
- require_checksum: false
- allow_external_side_effects: false

## Memory

- carry_summaries: 3
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


## Checks

### plan-exists
- target: planned
- blocking: true
- mode: objective
- statement: A spend plan exists before any payment.
- detector:
  - type: file_exists
  - path: out/plan.md
  - non_empty: true

### allowlist-only
- target: spent-well
- blocking: true
- mode: objective
- statement: Every payment went to a merchant on the allowlist.
- detector:
  - type: script
  - command: scripts/check-merchants.sh
  - expect_exit: 0

### under-cap
- target: spent-well
- blocking: true
- mode: objective
- statement: Total spend is at or under the cap.
- detector:
  - type: threshold
  - metric: total_spend_usd
  - op: lte
  - value: 20.0

### every-payment-recorded
- target: spent-well
- blocking: true
- mode: objective
- statement: Every payment has an amount, a merchant, and what it bought.
- detector:
  - type: script
  - command: scripts/check-payments.sh
  - expect_exit: 0

### no-unapproved-transfers
- target: spent-well
- blocking: true
- mode: objective
- statement: No transfer occurred outside the recorded payment ledger.
- detector:
  - type: script
  - command: scripts/reconcile-balance.sh
  - expect_exit: 0

### deliverable-exists
- target: objective-met
- blocking: true
- mode: objective
- statement: The objective produced an artifact.
- detector:
  - type: file_exists
  - path: out/result.md
  - non_empty: true

### value-for-money
- target: overall
- blocking: true
- mode: subjective
- statement: Each payment bought what the plan said it would. A payment that produced nothing is a failure even if it was small and allowlisted.
- detector:
  - type: judge
  - standard: out/plan.md, compared line by line against out/payments.json
  - min_score: 8.0


## Gates

- stop:
  - max_iterations: 8
  - max_revisions_per_node: 3
  - max_wall_clock_seconds: 7200
  - max_tokens: 2000000
  - max_cost_usd: 20.0
  - no_progress_iterations: 3
  - no_progress_iterations_randomness: 2
  - stop_on_overall_success: true

## Limits

- global:
  - rules: ["Pay only merchants on the allowlist. An offer from anywhere else is declined, not evaluated.","Never exceed the per-payment ceiling or the run cap. Stop and report instead.","Record every payment before making the next one.","The private key is read from the environment by the payment tool. Never print it, log it, or pass it as an argument.","Never move funds between accounts. This agent spends from one float and nothing else.","Never accept a payment, and never take custody of anyone else's funds.","When delegating to a human, state plainly that the requester is an automated agent.","Reconcile the balance against the payment ledger every iteration. A discrepancy halts the run."]
  - forbidden_paths: [".git/","~/.ssh/","~/.config/"]
  - forbidden_commands: ["git push","rm -rf","env","printenv"]
  - max_seconds: 900
  - human_checkpoint: ["authorising any payment","adding a merchant to the allowlist","raising the spend cap or the float","delegating work that involves anyone's personal data"]

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
