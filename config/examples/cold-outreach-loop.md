# cold-outreach-loop

- version: 0.1.0
- description: Turn a qualified lead list into personalised first-contact messages, queued for human release, with suppression and opt-out enforced by the gate.

## A. Information

### agenda
- value: Replace with the one thing this campaign is for. A campaign with two purposes gets neither.

### leads_file
- value: out/leads.json
- note: Produced by sales-leads-loop. Each record carries source and lawful basis.

### suppression_file
- value: out/suppression.json
- note: Anyone who has opted out, bounced, or complained. Checked before drafting and again before sending.

### channels
- value: email
- note: Add "phone" only where you have confirmed the number is not on a do-not-call register for its jurisdiction.

### daily_cap
- value: "40"
- note: Per sending identity, per day. Higher volumes are how a domain gets burned.

### queue_file
- value: out/outreach-queue.json


## B. Pre-execution

### Sent twenty of these by hand and kept every reply, including the angry ones
- done: false
- evidence: link the thread

### Confirmed the sending domain has SPF, DKIM, and DMARC configured
- done: false

### Confirmed the opt-out link works end to end and writes to the suppression list
- done: false

### Checked the do-not-call and marketing-consent rules for every jurisdiction targeted
- done: false
- evidence: out/jurisdictions.md


## C. Goals

### targeted
- description: The send list is drawn from qualified leads with none of the suppressed contacts on it.
- priority: 1

### personalised
- description: Each message references something specific and verifiable about that recipient's company, not a merge field.
- depends_on: ["targeted"]
- priority: 2

### compliant
- description: Every message identifies the sender, states why they are being contacted, and carries a working opt-out.
- depends_on: ["personalised"]
- priority: 1


## D. Validations

### queue-exists
- target: targeted
- mode: objective
- statement: An outreach queue was produced.
- detector:
  - type: file_exists
  - path: out/outreach-queue.json
  - non_empty: true
- blocking: true

### suppression-honoured
- target: targeted
- mode: objective
- statement: No queued contact appears on the suppression list.
- detector:
  - type: script
  - command: scripts/check-suppression.sh
  - args: ["out/outreach-queue.json"]
  - expect_exit: 0
- blocking: true

### daily-cap
- target: targeted
- mode: objective
- statement: The queue is within the daily cap per sending identity.
- detector:
  - type: script
  - command: scripts/check-cap.sh
  - expect_exit: 0
- blocking: true

### opt-out-present
- target: compliant
- mode: objective
- statement: Every queued message contains a working opt-out link.
- detector:
  - type: script
  - command: scripts/check-optout.sh
  - expect_exit: 0
- blocking: true

### sender-identified
- target: compliant
- mode: objective
- statement: Every message names the sender and a physical postal address.
- detector:
  - type: script
  - command: scripts/check-identification.sh
  - expect_exit: 0
- blocking: true

### lawful-review
- target: compliant
- mode: subjective
- statement: Each message meets the identification, subject-line, and opt-out requirements for its recipient's jurisdiction.
- detector:
  - type: judge
  - standard: CAN-SPAM 15 U.S.C. 7704 and the jurisdiction notes in out/jurisdictions.md
  - min_score: 9.0
- blocking: true

### specificity
- target: personalised
- mode: subjective
- statement: Each message references something specific and checkable about that recipient's company, and would not make sense sent to a different one.
- detector:
  - type: judge
  - standard: the recipient's own public site, cited in the draft
  - min_score: 7.0
- blocking: true

### nothing-sent-unreviewed
- target: overall
- mode: objective
- statement: No message left the queue without a recorded human approval.
- detector:
  - type: script
  - command: scripts/check-approvals.sh
  - expect_exit: 0
- blocking: true


## E. Success

### ready-to-release
- target: overall
- mode: percentage
- statement: Every blocking check passes, including suppression and opt-out.
- threshold: 1.0


## F. Stop gates

- max_iterations: 6
- max_revisions_per_node: 3
- max_wall_clock_seconds: 3600
- max_tokens: 2000000
- max_cost_usd: 8.0
- no_progress_iterations: 3
- no_progress_iterations_randomness: 2
- stop_on_overall_success: true

## G. Schedules

### cron
- expr: 0 7 * * 2-4


## H. Constraints

- global:
  - rules: ["This loop drafts and queues. A human releases. Never send without a recorded approval.","Check the suppression list before drafting and again before sending.","Every message identifies the sender, gives a physical postal address, and carries a one-click opt-out.","Every message states plainly why this recipient is being contacted.","Stop at the daily cap per sending identity even if the loop has budget left.","Never contact anyone who has replied asking not to be contacted, on any channel, ever.","Never contact a number on a do-not-call register for its jurisdiction.","Never use a misleading subject line, a fake reply-to, or a spoofed sender.","Never claim a prior relationship, a referral, or a meeting that did not happen.","One follow-up maximum. Silence is an answer."]
  - forbidden_paths: [".git/"]
  - forbidden_commands: ["git push","rm -rf"]
  - max_seconds: 900
  - human_checkpoint: ["sending any message to anyone","placing any call","adding a recipient not present in the qualified lead list","raising the daily cap"]

## I. Execution guidelines

- dependency: ["select -> personalise -> review"]

### select
- guideline: Build the send list from qualified leads minus the suppression list. Draft nothing yet. A recipient you cannot justify contacting is removed here, not argued for later.

### personalise
- guideline: Write one message per recipient, each referencing something specific and checkable about their company. If you cannot find anything specific, drop the recipient rather than writing a generic message.

### review
- guideline: Check every message for the compliance requirements before anything is queued for release. This phase removes messages; it does not improve them.


## Graph

- concurrency:
  - mode: auto
  - cap: 4
  - min_marginal_gain: 0.05

### select
- role: researcher
- instruction: Build the send list from out/leads.json, removing everyone on the suppression list and anyone already contacted. Respect the daily cap. Write out/send-list.json.
- goals: ["targeted"]
- tier: cheap
- stage: select
- weight: 1.0
- isolated: false

### personalise
- role: builder
- instruction: Write one message per recipient into out/outreach-queue.json. Each references something specific and checkable from that company's public site, cites it, and carries the identification block and opt-out link. Drop any recipient you cannot personalise honestly.
- depends_on: ["select"]
- goals: ["personalised","compliant"]
- tier: standard
- stage: personalise
- weight: 3.0
- isolated: true

### compliance-check
- role: judge
- instruction: Check every queued message against the jurisdiction notes and the identification, subject-line, and opt-out requirements. Report per-message pass or fail with the offending text quoted. A missing opt-out is a fail.
- depends_on: ["personalise"]
- goals: ["compliant"]
- tier: strong
- provider: openai
- stage: review
- weight: 1.0
- isolated: false

### specificity-check
- role: adversary
- instruction: For each queued message, argue that it could have been sent to any other company. Where that argument succeeds, the message is not personalised.
- depends_on: ["personalise"]
- goals: ["personalised"]
- tier: strong
- provider: openai
- stage: review
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

