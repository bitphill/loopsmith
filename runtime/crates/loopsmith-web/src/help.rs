//! What every field means, written for someone who has never built a loop.
//!
//! This is the part of the web UI that does the actual work. A form with
//! thirty inputs and no explanation is not easier than a YAML file — it is the
//! same difficulty with worse ergonomics. So every field carries two things:
//! a one-line hint that sits under it permanently, and a longer "why this
//! exists" note behind an info control.
//!
//! The long notes say *why*, not *what*. `max_iterations` is obviously a
//! maximum number of iterations; what a newcomer needs to know is that it is
//! the last thing standing between them and a loop that runs all night, and
//! that ten is a starting point rather than a recommendation.
//!
//! Ordering matters here: [`SECTIONS`] is the order the UI lays the form out
//! in, and it is the A–J order of the model itself, so the browser, the YAML,
//! the schema, and `HOW-TO-USE.md` all describe the same thing in the same
//! sequence.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SectionHelp {
    /// `A`–`J`, or a bare id for the sections outside the lettered model.
    pub letter: &'static str,
    /// Config key this section edits.
    pub key: &'static str,
    pub title: &'static str,
    /// One line, shown under the section heading.
    pub summary: &'static str,
    /// The paragraph behind the info control.
    pub detail: &'static str,
    /// What goes wrong when this section is skipped or filled in badly.
    pub failure: &'static str,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldHelp {
    /// Dotted path, matching the config and the validator's `field` strings.
    pub path: &'static str,
    pub label: &'static str,
    /// Permanent hint under the input.
    pub hint: &'static str,
    /// The longer note behind the info control.
    pub detail: &'static str,
    /// A concrete value worth copying.
    pub example: &'static str,
}

/// The lettered model, in order.
pub const SECTIONS: &[SectionHelp] = &[
    SectionHelp {
        letter: "A",
        key: "information",
        title: "Information",
        summary: "Facts every step of the loop should know, written once.",
        detail: "Anything durable and specific: the repository this works on, the brand voice, \
                 the API you are calling, the house style. It is handed to every node on every \
                 iteration, so a fact recorded here does not have to be repeated in each \
                 instruction. Keep it to things that stay true — anything that changes during \
                 a run belongs in the run's own memory, not here.",
        failure: "Left empty, every instruction has to restate the same context, and the \
                  restatements drift apart until two nodes are working from different facts.",
        required: false,
    },
    SectionHelp {
        letter: "B",
        key: "pre_execution",
        title: "Pre-execution work",
        summary: "What you must do by hand, once, before letting this run unattended.",
        detail: "The list of things that have to be proven manually first: publish one post \
                 yourself, send one outreach email, run the refactor on one file. This is the \
                 single most skipped section and the most expensive one to skip. Automating a \
                 process you have never performed does not save you the work — it produces the \
                 wrong result faster, and at a scale that is harder to undo.",
        failure: "A loop built on an unproven manual process automates a mistake. Tick a step \
                  only when you have actually done it and can point at the evidence.",
        required: false,
    },
    SectionHelp {
        letter: "C",
        key: "goals",
        title: "Goals",
        summary: "What you want, in plain language, one named goal at a time.",
        detail: "Each goal gets a short name the rest of the config refers to, and a \
                 description in ordinary words. Write the description as if explaining to a \
                 colleague what 'done' looks like. The reserved name `overall` means the loop \
                 as a whole rather than one goal, and it is what the stop gates read.",
        failure: "A goal too vague to check is a goal that never closes. If you cannot say how \
                  you would know it was met, it is not a goal yet.",
        required: true,
    },
    SectionHelp {
        letter: "D",
        key: "validations",
        title: "Validations",
        summary: "How each goal is checked. This is the part that makes a loop trustworthy.",
        detail: "A validation names a goal and says how a machine decides whether it is met. \
                 Four of the five detectors are deterministic — a script's exit code, a file \
                 existing, a pattern matching, a number crossing a threshold — and one, the \
                 judge, is a model's verdict. loopsmith is built on the rule that a model must \
                 not certify its own completion, so `goal_satisfied` is written by this gate \
                 and by nothing else. A judge can inform the gate; it cannot open it.",
        failure: "Goals with no validation cannot ever be marked satisfied, and the config is \
                  refused rather than run. That refusal is the feature.",
        required: true,
    },
    SectionHelp {
        letter: "E",
        key: "success",
        title: "Success scenarios",
        summary: "What counts as good enough — all of it, or a proportion.",
        detail: "Validations say what is checked; success says how much of it has to pass. \
                 `objective` means every blocking validation passes. `percentage` means a \
                 fraction of them does, and needs a threshold. `subjective` records a \
                 judgement without making it the bar. Use percentage when partial progress is \
                 genuinely useful and objective when it is not.",
        failure: "Without a success scenario the loop runs to its iteration limit even after \
                  it has already done the job.",
        required: false,
    },
    SectionHelp {
        letter: "F",
        key: "stop_gates",
        title: "Stop gates",
        summary: "Every way this loop is allowed to end. Set these before the first run.",
        detail: "Layered exits, all checked every iteration, any one of which halts the run. \
                 The iteration cap is the blunt one. The cost and wall-clock caps are the ones \
                 that matter overnight. The no-progress cap is the subtle one: it halts a loop \
                 that is still busy but no longer changing anything, which is the failure mode \
                 that quietly burns a budget while looking like work.",
        failure: "A loop with only an iteration cap and an expensive provider is an unbounded \
                  bill waiting for a slow night.",
        required: false,
    },
    SectionHelp {
        letter: "G",
        key: "schedules",
        title: "Schedules",
        summary: "What makes this loop start. Leave it empty to run it only by hand.",
        detail: "A cron expression, a plain interval, a file changing, another goal being met, \
                 or nothing at all. `Watch` keeps loopsmith resident and fires these while it \
                 runs; `Install schedule` hands the job to launchd or cron so it survives a \
                 reboot. Cron expressions are read in UTC, which is the usual reason a job \
                 fires at what looks like the wrong hour.",
        failure: "An interval shorter than a run takes stacks runs on top of each other until \
                  something gives.",
        required: false,
    },
    SectionHelp {
        letter: "H",
        key: "constraints",
        title: "Constraints",
        summary: "What the loop may not do, and what it must stop and ask about.",
        detail: "Rules in plain language, paths and commands that are off limits, and per-node \
                 token and time ceilings. `human_checkpoint` is the important one: actions \
                 listed there stop and wait for a person no matter what permissions have been \
                 granted. Anything irreversible — sending mail, spending money, publishing, \
                 deleting — belongs there.",
        failure: "A hands-off loop with no checkpoints is not hands-off, it is unsupervised.",
        required: false,
    },
    SectionHelp {
        letter: "I",
        key: "execution_guidelines",
        title: "Execution guidelines",
        summary: "Named phases, each with a standing instruction and a place in the order.",
        detail: "Optional. Use it when the work has real stages — gather, then draft, then \
                 review — and a node should not start before its stage is active. Order is \
                 written as a chain: `gather -> draft -> review`. A node with no stage is \
                 always eligible, which is the right default for most loops.",
        failure: "Phases invented for tidiness rather than for real ordering just delay work \
                  that could have run.",
        required: false,
    },
    SectionHelp {
        letter: "J",
        key: "default_skills",
        title: "Sub-agents",
        summary: "Specialist agents installed before the loop starts.",
        detail: "Skills the loop should have available from the first iteration. Installation \
                 is idempotent, so listing one that is already present costs nothing. Anything \
                 acquired at run time lands in a quarantine directory and stays there until a \
                 person promotes it — the loop is not allowed to grant itself new abilities \
                 unsupervised.",
        failure: "Nothing breaks without this; nodes simply work without the specialist.",
        required: false,
    },
    SectionHelp {
        letter: "graph",
        key: "graph",
        title: "Nodes and dependencies",
        summary: "The units of work, who they are, and what genuinely feeds what.",
        detail: "Each node has a role, an instruction, and optionally the nodes whose output \
                 it actually reads. Only list a dependency the node truly consumes: an 'and \
                 then' that is not read is not an edge, and every false edge makes the loop \
                 slower for no reason. loopsmith derives the parallel schedule from these \
                 edges, so the graph is what decides whether four nodes run at once or one at \
                 a time.",
        failure: "Two builders writing the same files in the same wave, neither marked \
                  isolated, will overwrite each other. The plan panel flags exactly this.",
        required: false,
    },
    SectionHelp {
        letter: "providers",
        key: "providers",
        title: "Providers",
        summary: "Which models do the work, and which fallback runs when one is unreachable.",
        detail: "Every provider is a command template, which is why any CLI on this machine \
                 can serve a loop without a code change. The cascade orders them per tier — \
                 cheap, standard, strong — and the first reachable one serves the call. Judge \
                 independence is on by default: a judge node is refused if it would run on the \
                 same provider as the work it is grading, because a model marking its own \
                 homework is not a check.",
        failure: "One provider plus judge independence means judges cannot run at all.",
        required: false,
    },
    SectionHelp {
        letter: "context",
        key: "context",
        title: "Carried context",
        summary: "How much of the previous iterations each prompt drags along.",
        detail: "Every iteration produces a summary; this decides how many of them the next \
                 prompt carries and how long each may be. Two is usually right. More gives the \
                 model a longer memory and a bigger bill, and past a point it starts \
                 rehearsing old attempts instead of trying new ones.",
        failure: "A summary with no ceiling grows until it crowds out the actual instruction.",
        required: false,
    },
];

/// Per-field notes. Only fields where the name genuinely does not explain
/// itself, or where the honest advice is non-obvious.
pub const FIELDS: &[FieldHelp] = &[
    FieldHelp {
        path: "name",
        label: "Loop name",
        hint: "Short, lowercase, no spaces. Becomes the loop's identity on disk.",
        detail: "Used as the storage tree name and the generated skill name, so it should be \
                 stable. Renaming later means the new name has no history.",
        example: "blog-pipeline",
    },
    FieldHelp {
        path: "description",
        label: "Purpose",
        hint: "One sentence on what this loop is for.",
        detail: "Shown on the loop's card and handed to every node as context. Write it for a \
                 colleague, not for a search engine.",
        example: "Draft, fact-check, and publish one post a week in the house voice.",
    },
    FieldHelp {
        path: "goals[].name",
        label: "Goal name",
        hint: "A short handle. Validations and nodes refer to the goal by this.",
        detail: "`overall` is reserved for the loop as a whole. Everything else is yours.",
        example: "draft-quality",
    },
    FieldHelp {
        path: "validations[].detector",
        label: "How it is checked",
        hint: "Script, file, pattern, and threshold are decided by machine. Judge is a model.",
        detail: "Prefer a deterministic detector wherever one exists. A script's exit code \
                 cannot be talked into a different answer; a judge can. Judges earn their \
                 place on things genuinely not machine-checkable — tone, clarity, whether an \
                 argument holds — and even then they must name the external standard they are \
                 checking against, because an unnamed standard is just an opinion.",
        example: "script: `npm test`, expecting exit 0",
    },
    FieldHelp {
        path: "validations[].blocking",
        label: "Blocking",
        hint: "Blocking checks hold the gate shut. Non-blocking ones are recorded only.",
        detail: "Make a check non-blocking when you want to watch a number without letting it \
                 stop the run. Everything that actually defines done should be blocking.",
        example: "on",
    },
    FieldHelp {
        path: "stop_gates.max_iterations",
        label: "Maximum iterations",
        hint: "Hard ceiling on how many times round the loop goes.",
        detail: "Ten is the default, not a recommendation. Work out roughly what one iteration \
                 costs, multiply, and decide whether you would pay that to find out it did not \
                 converge.",
        example: "10",
    },
    FieldHelp {
        path: "stop_gates.no_progress_iterations",
        label: "Stop after no progress",
        hint: "Halt after this many iterations that change nothing measurable.",
        detail: "The quiet money-saver. A loop can look busy for hours while making no \
                 difference at all; this catches that without waiting for the iteration cap. \
                 Zero disables it, which is rarely what you want.",
        example: "3",
    },
    FieldHelp {
        path: "stop_gates.max_cost_usd",
        label: "Cost ceiling",
        hint: "Dollars. The run halts when the ledger crosses this.",
        detail: "The one ceiling to set before leaving a loop unattended. It needs each \
                 provider's `cost_per_1k_tokens` to be meaningful — without that, spend is \
                 estimated from character counts, which is enough to enforce a limit but not \
                 to bill against.",
        example: "5.00",
    },
    FieldHelp {
        path: "schedules[].expr",
        label: "Cron expression",
        hint: "Five fields, read in UTC. Minute, hour, day, month, weekday.",
        detail: "UTC is the usual explanation for a job that fires an hour off. If you want \
                 local time, work out the offset yourself, or use an interval instead — \
                 intervals have no timezone to get wrong.",
        example: "0 9 * * 1  (09:00 UTC every Monday)",
    },
    FieldHelp {
        path: "constraints.global.human_checkpoint",
        label: "Human checkpoints",
        hint: "Irreversible actions. These stop and wait however much permission was granted.",
        detail: "The backstop that makes an unattended loop safe to leave alone. Sending mail, \
                 spending money, publishing, deleting, pushing to a shared branch — anything \
                 you would want to be asked about at 3am belongs here.",
        example: "send email, publish post, delete files",
    },
    FieldHelp {
        path: "graph.nodes[].role",
        label: "Role",
        hint: "Builder does the work. Judge grades it. Others plan, attack, or research.",
        detail: "A judge must run on a different provider family from the builder whose work \
                 it reads, and loopsmith refuses the verdict if it does not. An adversary \
                 exists to try to break what the builder made, which is a different job from \
                 grading it.",
        example: "builder",
    },
    FieldHelp {
        path: "graph.nodes[].depends_on",
        label: "Depends on",
        hint: "Only nodes whose output this node actually reads.",
        detail: "The most common way to make a loop needlessly slow is to list an ordering \
                 preference as a dependency. If the node would still work with the other one's \
                 output missing, it is not an edge.",
        example: "research, outline",
    },
    FieldHelp {
        path: "graph.nodes[].isolated",
        label: "Isolated",
        hint: "Runs in its own git worktree. Required for builders that may run in parallel.",
        detail: "Two builders writing to one directory at the same time will corrupt each \
                 other's work. Isolation gives each its own checkout. It needs git, and it \
                 needs the loop path to be inside a repository.",
        example: "on, for any parallel builder",
    },
    FieldHelp {
        path: "graph.nodes[].tier",
        label: "Tier",
        hint: "Which rung of the cascade serves this node: cheap, standard, or strong.",
        detail: "Tiers are how one config runs on a fast cheap model for routine work and an \
                 expensive one where it matters, without naming a specific model anywhere in \
                 the graph.",
        example: "standard",
    },
    FieldHelp {
        path: "providers[].command",
        label: "Command",
        hint: "Any executable on this machine. Placeholders are substituted before it runs.",
        detail: "`{prompt}` `{system}` `{model}` `{tier}` `{node}` are replaced in the \
                 arguments. This is why loopsmith needs no plugin for a new model: if you can \
                 run it from a terminal, it can serve a loop.",
        example: "claude",
    },
    FieldHelp {
        path: "providers[].prompt_on_stdin",
        label: "Prompt on stdin",
        hint: "Send the prompt through stdin instead of as an argument.",
        detail: "Turn this on for anything that reads a document, and for any prompt long \
                 enough to hit the operating system's argument-length limit — which a carried \
                 context will reach sooner than you expect.",
        example: "on, for ollama",
    },
    FieldHelp {
        path: "providers[].requires_env",
        label: "Required keys",
        hint: "Variable names only. loopsmith never reads or logs the values.",
        detail: "Names here are checked for presence before a run, so a missing key fails \
                 immediately instead of three iterations in. Set the values in the Secrets \
                 panel, which writes them to your shell profile or your operating system's \
                 secret store.",
        example: "ANTHROPIC_API_KEY",
    },
    FieldHelp {
        path: "providers.cascade",
        label: "Cascade",
        hint: "Ordered fallback per tier. The first reachable provider serves the call.",
        detail: "This is what keeps a loop alive when a provider is rate-limited or down at \
                 two in the morning. Put the one you want first and something that will \
                 definitely answer last.",
        example: "standard: claude, gemini",
    },
    FieldHelp {
        path: "context.carry_summaries",
        label: "Carry summaries",
        hint: "How many previous iteration summaries each prompt includes. 0 disables it.",
        detail: "Two is a good default. A loop that carries too much starts rehearsing its \
                 previous attempts instead of trying something new, and pays for the privilege.",
        example: "2",
    },
    FieldHelp {
        path: "skills.explore",
        label: "Explore new sub-agents",
        hint: "Let the loop trial a specialist that is not in this config.",
        detail: "Off by default, and it should stay off until a loop is working. When on, \
                 candidates are trialled, scored against gate outcomes, and written up as \
                 proposals — which the loop cannot apply itself. A person promotes them or \
                 nothing happens.",
        example: "off",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_sections_are_the_lettered_model_in_order() {
        let letters: Vec<&str> = SECTIONS
            .iter()
            .map(|s| s.letter)
            .filter(|l| l.len() == 1)
            .collect();
        assert_eq!(
            letters,
            vec!["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"],
            "the browser must lay the form out in the model's own order"
        );
    }

    #[test]
    fn the_two_required_sections_are_the_two_the_model_requires() {
        let required: Vec<&str> = SECTIONS.iter().filter(|s| s.required).map(|s| s.key).collect();
        assert_eq!(required, vec!["goals", "validations"]);
    }

    #[test]
    fn every_section_says_what_goes_wrong_without_it() {
        // The failure line is the one that changes behaviour. A section that
        // only describes itself teaches nothing a field label did not.
        for s in SECTIONS {
            assert!(!s.summary.is_empty(), "{} has no summary", s.key);
            assert!(s.detail.len() > 80, "{} has a thin detail note", s.key);
            assert!(!s.failure.is_empty(), "{} does not say what breaks", s.key);
        }
    }

    #[test]
    fn every_field_note_carries_a_hint_a_detail_and_an_example() {
        for f in FIELDS {
            assert!(!f.hint.is_empty(), "{} has no hint", f.path);
            assert!(f.detail.len() > 40, "{} has a thin detail note", f.path);
            assert!(!f.example.is_empty(), "{} has no example", f.path);
        }
    }

    #[test]
    fn no_field_is_documented_twice() {
        let mut seen = BTreeSet::new();
        for f in FIELDS {
            assert!(seen.insert(f.path), "{} is documented twice", f.path);
        }
    }

    #[test]
    fn the_fields_most_likely_to_cost_money_are_all_explained() {
        for path in [
            "stop_gates.max_iterations",
            "stop_gates.max_cost_usd",
            "stop_gates.no_progress_iterations",
            "constraints.global.human_checkpoint",
        ] {
            assert!(
                FIELDS.iter().any(|f| f.path == path),
                "{path} must carry an explanation"
            );
        }
    }
}
