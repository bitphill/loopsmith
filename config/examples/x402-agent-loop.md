# x402-agent-loop

- version: 0.1.0
- description: Pursue a goal that requires paying for services or delegating work, using x402 payments from a funded float under a hard spend cap.

## A. Information

### objective
- value: Replace with the outcome worth paying for. Be specific about what "done" buys you — this loop will spend money pursuing it.

### float_usd
- value: "25.00"
- note: The dedicated account's balance. Fund it with what you would accept losing. Never point this at a primary wallet.

### spend_cap_usd
- value: "20.00"
- note: Enforced by stop_gates.max_cost_usd as well, so the gate stops the run even if a merchant's accounting disagrees.

### merchant_allowlist
- value: out/merchants.json
- note: Services this agent may pay. A merchant not on this list is not payable, however good the offer looks.

### delegation_venues
- value: https://rentahuman.ai/for-agents
- note: Where work can be delegated to a human when the agent cannot do it itself.

### ledger_path
- value: out/payments.json
- note: Every payment with amount, merchant, what it bought, and the outcome.


## B. Pre-execution

### Funded a dedicated float account and confirmed the balance
- done: false
- evidence: the account id and its balance, not the key

### Paid one merchant by hand through x402 and kept the receipt
- done: false

### Wrote the merchant allowlist and justified each entry
- done: false
- evidence: out/merchants.json

### Decided the posture — supervised or autonomous — and edited human_checkpoint to match
- done: false


## C. Goals

### planned
- description: A plan exists stating what will be bought or delegated, from whom, at what price, and what it is expected to produce.
- priority: 1

### spent-well
- description: Every payment is to an allowlisted merchant, within the cap, and recorded with what it bought.
- depends_on: ["planned"]
- priority: 1

### objective-met
- description: The stated objective is achieved and independently checkable.
- depends_on: ["spent-well"]
- priority: 2


## D. Validations

### plan-exists
- target: planned
- mode: objective
- statement: A spend plan exists before any payment.
- detector:
  - type: file_exists
  - path: out/plan.md
  - non_empty: true
- blocking: true

### allowlist-only
- target: spent-well
- mode: objective
- statement: Every payment went to a merchant on the allowlist.
- detector:
  - type: script
  - command: scripts/check-merchants.sh
  - expect_exit: 0
- blocking: true

### under-cap
- target: spent-well
- mode: objective
- statement: Total spend is at or under the cap.
- detector:
  - type: threshold
  - metric: total_spend_usd
  - op: lte
  - value: 20.0
- blocking: true

### every-payment-recorded
- target: spent-well
- mode: objective
- statement: Every payment has an amount, a merchant, and what it bought.
- detector:
  - type: script
  - command: scripts/check-payments.sh
  - expect_exit: 0
- blocking: true

### no-unapproved-transfers
- target: spent-well
- mode: objective
- statement: No transfer occurred outside the recorded payment ledger.
- detector:
  - type: script
  - command: scripts/reconcile-balance.sh
  - expect_exit: 0
- blocking: true

### deliverable-exists
- target: objective-met
- mode: objective
- statement: The objective produced an artifact.
- detector:
  - type: file_exists
  - path: out/result.md
  - non_empty: true
- blocking: true

### value-for-money
- target: overall
- mode: subjective
- statement: Each payment bought what the plan said it would. A payment that produced nothing is a failure even if it was small and allowlisted.
- detector:
  - type: judge
  - standard: out/plan.md, compared line by line against out/payments.json
  - min_score: 8.0
- blocking: true


## E. Success

### objective-achieved-within-budget
- target: overall
- mode: percentage
- statement: Every blocking check passes, including reconciliation.
- threshold: 1.0


## F. Stop gates

- max_iterations: 8
- max_revisions_per_node: 3
- max_wall_clock_seconds: 7200
- max_tokens: 2000000
- max_cost_usd: 20.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### manual


## H. Constraints

- global:
  - rules: ["Pay only merchants on the allowlist. An offer from anywhere else is declined, not evaluated.","Never exceed the per-payment ceiling or the run cap. Stop and report instead.","Record every payment before making the next one.","The private key is read from the environment by the payment tool. Never print it, log it, or pass it as an argument.","Never move funds between accounts. This agent spends from one float and nothing else.","Never accept a payment, and never take custody of anyone else's funds.","When delegating to a human, state plainly that the requester is an automated agent.","Reconcile the balance against the payment ledger every iteration. A discrepancy halts the run."]
  - forbidden_paths: [".git/","~/.ssh/","~/.config/"]
  - forbidden_commands: ["git push","rm -rf","env","printenv"]
  - max_seconds: 900
  - human_checkpoint: ["authorising any payment","adding a merchant to the allowlist","raising the spend cap or the float","delegating work that involves anyone's personal data"]

## I. Execution guidelines

- dependency: ["plan -> execute -> verify"]

### plan
- guideline: Decide what to buy or delegate, from which allowlisted merchant, at what price, and what it should produce. Spend nothing in this phase. A plan that cannot name the expected artifact is not a plan.

### execute
- guideline: Buy or delegate exactly what the plan named. Record each payment before starting the next. If a price differs from the plan, stop and re-plan rather than paying the difference.

### verify
- guideline: Check what the money bought against what the plan expected, and reconcile the balance. Spend nothing here.


## Graph

- concurrency:
  - mode: sequential

### plan-spend
- role: manager
- instruction: "Write out/plan.md: what to buy or delegate, from which allowlisted merchant, the price, and the artifact it should produce. Include the cheapest option you rejected and why."
- goals: ["planned"]
- tier: strong
- stage: plan
- weight: 2.0
- isolated: false

### challenge-plan
- role: adversary
- instruction: Argue that each planned payment is unnecessary, overpriced, or will not produce the named artifact. Anything you cannot defend against gets cut from the plan.
- depends_on: ["plan-spend"]
- goals: ["planned"]
- tier: strong
- provider: openai
- stage: plan
- weight: 1.0
- isolated: false

### execute
- role: builder
- instruction: "Carry out the plan: pay allowlisted merchants or delegate to a human venue, declaring that you are an automated agent. Record each payment in out/payments.json before starting the next. Write total_spend_usd to metrics.json."
- depends_on: ["challenge-plan"]
- goals: ["spent-well","objective-met"]
- tier: standard
- stage: execute
- weight: 4.0
- isolated: true

### reconcile
- role: judge
- instruction: Compare out/payments.json against the account balance and out/plan.md. Report per-payment whether it was allowlisted, within the ceiling, and produced what the plan expected. A discrepancy is a fail.
- depends_on: ["execute"]
- goals: ["spent-well"]
- tier: strong
- provider: openai
- stage: verify
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


## Skills

- acquisition_order: ["installed"]
- quarantine_dir: generated-skills
- min_marketplace_stars: 100
- require_human_promotion: true
- explore: false
- min_trials: 3

## Context

- carry_summaries: 3
- max_summary_chars: 1200

