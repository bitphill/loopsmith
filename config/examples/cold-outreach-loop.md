# cold-outreach-loop

- version: 0.1.0
- description: Turn a qualified lead list into personalised first-contact messages, queued for human release, with suppression and opt-out enforced by the gate.
- environment: dev
- features:
  - self_evolution: false
  - marketplace_skills: false
  - external_side_effects: false
  - parallel_execution: true
  - human_approval: true

## Background

### agenda
- value: Replace with the one thing this campaign is for. A campaign with two purposes gets neither.

### leads_file
- note: Produced by sales-leads-loop. Each record carries source and lawful basis.
- value: out/leads.json

### suppression_file
- note: Anyone who has opted out, bounced, or complained. Checked before drafting and again before sending.
- value: out/suppression.json

### channels
- note: Add "phone" only where you have confirmed the number is not on a do-not-call register for its jurisdiction.
- value: email

### daily_cap
- note: Per sending identity, per day. Higher volumes are how a domain gets burned.
- value: "40"

### queue_file
- value: out/outreach-queue.json


## Prerequisites

### Sent twenty of these by hand and kept every reply, including the angry ones
- evidence: link the thread
- done: false

### Confirmed the sending domain has SPF, DKIM, and DMARC configured
- done: false

### Confirmed the opt-out link works end to end and writes to the suppression list
- done: false

### Checked the do-not-call and marketing-consent rules for every jurisdiction targeted
- evidence: out/jurisdictions.md
- done: false


## Goals

### targeted
- priority: 1
- description: The send list is drawn from qualified leads with none of the suppressed contacts on it.

### personalised
- priority: 2
- description: Each message references something specific and verifiable about that recipient's company, not a merge field.
- depends_on: ["targeted"]

### compliant
- priority: 1
- description: Every message identifies the sender, states why they are being contacted, and carries a working opt-out.
- depends_on: ["personalised"]


## Success

### ready-to-release
- target: overall
- threshold: 1.0
- mode: percentage
- statement: Every blocking check passes, including suppression and opt-out.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05
- join:
  - strategy: wait_for_all

### select
- isolation:
  - mode: none
- role: researcher
- instruction: Build the send list from out/leads.json, removing everyone on the suppression list and anyone already contacted. Respect the daily cap. Write out/send-list.json.
- goals: ["targeted"]
- tier: cheap
- stage: select
- weight: 1.0

### personalise
- isolation:
  - mode: worktree
- role: builder
- instruction: Write one message per recipient into out/outreach-queue.json. Each references something specific and checkable from that company's public site, cites it, and carries the identification block and opt-out link. Drop any recipient you cannot personalise honestly.
- depends_on: ["select"]
- goals: ["personalised","compliant"]
- tier: standard
- stage: personalise
- weight: 3.0

### compliance-check
- isolation:
  - mode: none
- role: judge
- instruction: Check every queued message against the jurisdiction notes and the identification, subject-line, and opt-out requirements. Report per-message pass or fail with the offending text quoted. A missing opt-out is a fail.
- depends_on: ["personalise"]
- goals: ["compliant"]
- tier: strong
- provider: openai
- stage: review
- weight: 1.0

### specificity-check
- isolation:
  - mode: none
- role: adversary
- instruction: For each queued message, argue that it could have been sent to any other company. Where that argument succeeds, the message is not personalised.
- depends_on: ["personalise"]
- goals: ["personalised"]
- tier: strong
- provider: openai
- stage: review
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

- dependency: ["select -> personalise -> review"]

### select
- guideline: Build the send list from qualified leads minus the suppression list. Draft nothing yet. A recipient you cannot justify contacting is removed here, not argued for later.

### personalise
- guideline: Write one message per recipient, each referencing something specific and checkable about their company. If you cannot find anything specific, drop the recipient rather than writing a generic message.

### review
- guideline: Check every message for the compliance requirements before anything is queued for release. This phase removes messages; it does not improve them.


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
  - expr: 0 7 * * 2-4
- enabled: true


## Checks

### queue-exists
- target: targeted
- blocking: true
- mode: objective
- statement: An outreach queue was produced.
- detector:
  - type: file_exists
  - path: out/outreach-queue.json
  - non_empty: true

### suppression-honoured
- target: targeted
- blocking: true
- mode: objective
- statement: No queued contact appears on the suppression list.
- detector:
  - type: script
  - command: scripts/check-suppression.sh
  - args: ["out/outreach-queue.json"]
  - expect_exit: 0

### daily-cap
- target: targeted
- blocking: true
- mode: objective
- statement: The queue is within the daily cap per sending identity.
- detector:
  - type: script
  - command: scripts/check-cap.sh
  - expect_exit: 0

### opt-out-present
- target: compliant
- blocking: true
- mode: objective
- statement: Every queued message contains a working opt-out link.
- detector:
  - type: script
  - command: scripts/check-optout.sh
  - expect_exit: 0

### sender-identified
- target: compliant
- blocking: true
- mode: objective
- statement: Every message names the sender and a physical postal address.
- detector:
  - type: script
  - command: scripts/check-identification.sh
  - expect_exit: 0

### lawful-review
- target: compliant
- blocking: true
- mode: subjective
- statement: Each message meets the identification, subject-line, and opt-out requirements for its recipient's jurisdiction.
- detector:
  - type: judge
  - standard: CAN-SPAM 15 U.S.C. 7704 and the jurisdiction notes in out/jurisdictions.md
  - min_score: 9.0

### specificity
- target: personalised
- blocking: true
- mode: subjective
- statement: Each message references something specific and checkable about that recipient's company, and would not make sense sent to a different one.
- detector:
  - type: judge
  - standard: the recipient's own public site, cited in the draft
  - min_score: 7.0

### nothing-sent-unreviewed
- target: overall
- blocking: true
- mode: objective
- statement: No message left the queue without a recorded human approval.
- detector:
  - type: script
  - command: scripts/check-approvals.sh
  - expect_exit: 0


## Gates

- stop:
  - max_iterations: 6
  - max_revisions_per_node: 3
  - max_wall_clock_seconds: 3600
  - max_tokens: 2000000
  - max_cost_usd: 8.0
  - no_progress_iterations: 3
  - no_progress_iterations_randomness: 2
  - stop_on_overall_success: true

## Limits

- global:
  - rules: ["This loop drafts and queues. A human releases. Never send without a recorded approval.","Check the suppression list before drafting and again before sending.","Every message identifies the sender, gives a physical postal address, and carries a one-click opt-out.","Every message states plainly why this recipient is being contacted.","Stop at the daily cap per sending identity even if the loop has budget left.","Never contact anyone who has replied asking not to be contacted, on any channel, ever.","Never contact a number on a do-not-call register for its jurisdiction.","Never use a misleading subject line, a fake reply-to, or a spoofed sender.","Never claim a prior relationship, a referral, or a meeting that did not happen.","One follow-up maximum. Silence is an answer."]
  - forbidden_paths: [".git/"]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["sending any message to anyone","placing any call","adding a recipient not present in the qualified lead list","raising the daily cap"]

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
