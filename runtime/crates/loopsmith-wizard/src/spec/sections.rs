//! The questions, in the order they are asked.
//!
//! Wording is the terminal wizard's, carried over as-is where 1.0 did not
//! rename the thing being asked about: someone who has used one front end
//! should recognise the other sentence for sentence.
//!
//! Order mirrors the config's own: what the loop is for, how the work gets
//! done, what stops it, how it may change itself. The five sections every loop
//! needs come first and have no gate; everything after is opt-in behind one
//! yes/no, which is the shape the terminal already had and the browser already
//! drew.
//!
//! What is deliberately not here: `safety.recovery`, `safety.protected`, and
//! `execution.memory.namespaces`. Each is a policy table with sensible
//! defaults that a first-time author has no basis to change, and each is
//! better met in the file, with the comments around it, than as eight more
//! questions in a row.

use super::{
    Choice, Field, Gate, Input, List, Options, Providers, Section, Separator, Spec, Step, Summary,
    Validator, When, SPEC_VERSION,
};

/// The whole wizard, built fresh. Cheap enough to call per request.
pub fn spec() -> Spec {
    Spec {
        version: SPEC_VERSION,
        sections: vec![
            identity(),
            providers(),
            goals(),
            checks(),
            stop_gates(),
            graph(),
            background(),
            prerequisites(),
            success(),
            triggers(),
            limits(),
            phases(),
            default_skills(),
            memory(),
            alerts(),
            evolution(),
        ],
    }
}

// --- builders ----------------------------------------------------------------
//
// A field is ten fields of struct, most of them empty most of the time. These
// keep the question list readable as a question list.

fn field(id: &str, title: &str) -> Field {
    Field {
        id: id.into(),
        title: title.into(),
        hint: None,
        help: Vec::new(),
        input: Input::Text {
            placeholder: None,
            mono: false,
        },
        validator: Validator::Anything,
        when: None,
    }
}

impl Field {
    fn hint(mut self, h: &str) -> Self {
        self.hint = Some(h.into());
        self
    }

    fn help(mut self, lines: &[&str]) -> Self {
        self.help = lines.iter().map(|l| l.to_string()).collect();
        self
    }

    fn text(mut self, placeholder: &str) -> Self {
        self.input = Input::Text {
            placeholder: none_if_empty(placeholder),
            mono: false,
        };
        self
    }

    fn mono(mut self, placeholder: &str) -> Self {
        self.input = Input::Text {
            placeholder: none_if_empty(placeholder),
            mono: true,
        };
        self
    }

    fn area(mut self, rows: u8, placeholder: &str) -> Self {
        self.input = Input::Area {
            placeholder: none_if_empty(placeholder),
            rows,
        };
        self
    }

    fn number(mut self, min: Option<f64>, step: f64, suffix: Option<&str>) -> Self {
        self.input = Input::Number {
            min,
            step: Some(step),
            suffix: suffix.map(str::to_string),
        };
        self
    }

    fn yes_no(mut self) -> Self {
        self.input = Input::Bool {
            true_label: None,
            false_label: None,
        };
        self
    }

    fn labelled_bool(mut self, yes: &str, no: &str) -> Self {
        self.input = Input::Bool {
            true_label: Some(yes.into()),
            false_label: Some(no.into()),
        };
        self
    }

    fn select(mut self, choices: &[(&str, &str, &str)]) -> Self {
        let choices: Vec<Choice> = choices
            .iter()
            .map(|(value, label, note)| Choice {
                value: value.to_string(),
                label: label.to_string(),
                note: none_if_empty(note),
            })
            .collect();
        let values = choices.iter().map(|c| c.value.clone()).collect();
        self.input = Input::Select {
            options: Options::Fixed { choices },
        };
        self.validator = Validator::OneOf { values };
        self
    }

    fn select_from(mut self, options: Options) -> Self {
        self.input = Input::Select { options };
        self
    }

    fn items(mut self, separator: Separator, placeholder: &str) -> Self {
        self.input = Input::Items {
            placeholder: none_if_empty(placeholder),
            separator,
            mono: false,
        };
        self
    }

    fn mono_items(mut self, separator: Separator, placeholder: &str) -> Self {
        self.input = Input::Items {
            placeholder: none_if_empty(placeholder),
            separator,
            mono: true,
        };
        self
    }

    fn needed(mut self) -> Self {
        self.validator = Validator::NonEmpty;
        self
    }

    fn valid(mut self, v: Validator) -> Self {
        self.validator = v;
        self
    }

    fn when(mut self, w: When) -> Self {
        self.when = Some(w);
        self
    }
}

fn uint(required: bool) -> Validator {
    Validator::Uint {
        min: None,
        max: None,
        required,
    }
}

fn uint_from(min: u64) -> Validator {
    Validator::Uint {
        min: Some(min),
        max: None,
        required: true,
    }
}

fn float(required: bool) -> Validator {
    Validator::Float {
        min: None,
        max: None,
        required,
    }
}

fn fraction(required: bool) -> Validator {
    Validator::Float {
        min: Some(0.0),
        max: Some(1.0),
        required,
    }
}

fn none_if_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn section(id: &str, title: &str, steps: Vec<Step>) -> Section {
    Section {
        id: id.into(),
        title: title.into(),
        gate: None,
        steps,
    }
}

fn gated(id: &str, title: &str, question: &str, hint: &str, steps: Vec<Step>) -> Section {
    Section {
        id: id.into(),
        title: title.into(),
        gate: Some(Gate {
            question: question.into(),
            hint: none_if_empty(hint),
        }),
        steps,
    }
}

fn list(id: &str, title: &str, singular: &str, min: usize, primary: &str, fields: Vec<Field>) -> Step {
    Step::List(List {
        id: id.into(),
        title: title.into(),
        hint: None,
        help: Vec::new(),
        singular: singular.into(),
        min,
        summary: Summary {
            primary: primary.into(),
            secondary: None,
        },
        fields,
        when: None,
    })
}

impl Step {
    fn hint(mut self, h: &str) -> Self {
        if let Step::List(l) = &mut self {
            l.hint = Some(h.into());
        }
        self
    }

    fn help(mut self, lines: &[&str]) -> Self {
        if let Step::List(l) = &mut self {
            l.help = lines.iter().map(|x| x.to_string()).collect();
        }
        self
    }

    fn also(mut self, secondary: &str) -> Self {
        if let Step::List(l) = &mut self {
            l.summary.secondary = Some(secondary.into());
        }
        self
    }
}

fn one(f: Field) -> Step {
    Step::Field(f)
}

// --- the sections ------------------------------------------------------------

fn identity() -> Section {
    section(
        "identity",
        "Loop identity",
        vec![
            one(field("name", "What is this loop called?")
                .hint("Becomes the generated skill name, so lower-case and hyphens travel best.")
                .help(&["For example: weekly-competitor-brief"])
                .text("weekly-competitor-brief")
                .needed()),
            one(field("description", "What is it for, in a sentence?")
                .hint("For the humans reading the config later. Never sent to a node.")
                .area(2, "Track what competitors shipped this week and brief the team.")),
            one(field("version", "Version")
                .hint("Semantic version for your own tracking. 0.1.0 is a fine start.")
                .mono("0.1.0")
                .valid(Validator::Semver)),
            one(field("environment", "Which environment does this loop run in?")
                .hint("`prod` is stricter: it refuses unchecked approvals and unreviewed self-evolution.")
                .select(&[
                    ("dev", "dev", "your machine, where mistakes are cheap"),
                    ("staging", "staging", "rehearsal against real systems"),
                    ("prod", "prod", "the strict one"),
                ])),
        ],
    )
}

fn providers() -> Section {
    section(
        "providers",
        "Providers",
        vec![
            Step::Providers(Providers {
                id: "execution.providers.providers".into(),
                title: "Which agent CLIs may this loop call?".into(),
                hint: Some(
                    "Everything found on this machine is listed. Pick the ones this loop is \
                     allowed to spend."
                        .into(),
                ),
            }),
            one(field(
                "execution.providers.enforce_judge_independence",
                "Refuse a judge that runs on the same provider as the work it grades?",
            )
            .hint("A model grading its own family's output is not an independent check. On is the safe default.")
            .labelled_bool("Yes — require an independent judge", "No — allow same-provider judging")
            .when(When::MinItems {
                field: "execution.providers.providers".into(),
                count: 2,
            })),
        ],
    )
}

fn goals() -> Section {
    section(
        "goals",
        "Goals",
        vec![list(
            "intent.goals",
            "What is this loop trying to achieve?",
            "goal",
            1,
            "name",
            vec![
                field("name", "Goal name")
                    .hint("A short handle, referenced by checks and nodes.")
                    .text("brief-published")
                    .needed(),
                field("description", "Description")
                    .hint("Subjective phrasing is fine here. The check is what must be decidable.")
                    .area(2, "")
                    .needed(),
                field("depends_on", "Depends on")
                    .hint("Other goal names that must be satisfied first. Blank for none.")
                    .items(Separator::Comma, "research-done, draft-written"),
                field("priority", "Priority")
                    .hint("Lower runs first when it matters. Blank leaves it unordered.")
                    .number(Some(0.0), 1.0, None)
                    .valid(uint(false)),
            ],
        )
        .hint("Name each outcome in plain language. Add at least one.")
        .also("description")],
    )
}

fn checks() -> Section {
    let detector = |id: &str, title: &str, kind: &str| {
        field(id, title).when(When::Equals {
            field: "detector.type".into(),
            value: kind.into(),
        })
    };
    section(
        "checks",
        "Checks",
        vec![list(
            "safety.checks",
            "How is each goal checked?",
            "check",
            1,
            "name",
            vec![
                field("target", "Which goal does this check?")
                    .hint("The goal this check gates, or `overall` for the whole loop.")
                    .select_from(Options::GoalTargets)
                    .needed(),
                field("name", "Check name")
                    .hint("A short handle for this check.")
                    .text("")
                    .needed(),
                field("mode", "Mode")
                    .hint("How the result is read. Objective is the usual choice.")
                    .select(&[
                        ("objective", "objective", "pass/fail on evidence"),
                        ("subjective", "subjective", "a judged opinion"),
                        ("percentage", "percentage", "a fraction of checks"),
                    ]),
                field("statement", "Statement")
                    .hint("Natural-language description of the condition being checked.")
                    .area(2, "")
                    .needed(),
                field("detector.type", "How is it decided?")
                    .hint("The mechanism. Deterministic detectors are stronger than a judge.")
                    .select(&[
                        ("script", "script", "run a command, exit 0 passes (strongest)"),
                        ("file_exists", "file_exists", "a path must exist"),
                        ("regex_match", "regex_match", "a pattern must match an artifact"),
                        ("threshold", "threshold", "a number against a limit"),
                        ("judge", "judge", "a model verdict against a named standard"),
                    ]),
                detector("detector.command", "Command to run", "script")
                    .mono("npm")
                    .needed(),
                detector("detector.args", "Arguments", "script")
                    .hint("Argv tokens, split on spaces. Not a shell line.")
                    .mono_items(Separator::Whitespace, "test --silent"),
                detector("detector.path", "Path that must exist", "file_exists")
                    .mono("out/report.md")
                    .valid(Validator::Path { required: true }),
                detector("detector.non_empty", "Must it also be non-empty?", "file_exists").yes_no(),
                detector("detector.artifact", "Artifact to search", "regex_match")
                    .hint("A file named by a `file_exists` check, by path or by stem.")
                    .mono("")
                    .needed(),
                detector("detector.pattern", "Regular expression", "regex_match")
                    .mono("")
                    .valid(Validator::Regex),
                detector("detector.metric", "Metric name", "threshold")
                    .hint("Read from `metrics.json` in the loop root.")
                    .text("")
                    .needed(),
                detector("detector.op", "Comparison", "threshold").select(&[
                    ("gte", "≥ at least", ""),
                    ("gt", "> greater than", ""),
                    ("lte", "≤ at most", ""),
                    ("lt", "< less than", ""),
                    ("eq", "= equal to", ""),
                ]),
                detector("detector.value", "Threshold value", "threshold")
                    .number(None, 0.1, None)
                    .valid(float(true)),
                detector("detector.standard", "The standard the judge checks against", "judge")
                    .hint("Naming a standard is what turns an opinion into a check.")
                    .area(2, "")
                    .needed(),
                detector("detector.min_score", "Minimum score to pass", "judge")
                    .hint("Between 0 and 1. Blank for none.")
                    .number(Some(0.0), 0.05, None)
                    .valid(fraction(false)),
                field("blocking", "Must this pass for the goal to count as satisfied?")
                    .hint("A blocking check holds the gate shut; a non-blocking one is only recorded.")
                    .yes_no(),
            ],
        )
        .hint("A check is what actually decides 'done'. Prefer a deterministic check over a model judge.")
        .help(&["A goal with no blocking check can never be satisfied. This is the section that makes the rest safe."])
        .also("target")],
    )
}

fn stop_gates() -> Section {
    section(
        "stop_gates",
        "Stop gates",
        vec![
            one(field("safety.gates.stop.max_iterations", "How many whole-loop iterations at most?")
                .hint("Hard ceiling on passes over the graph.")
                .number(Some(1.0), 1.0, None)
                .valid(uint_from(1))),
            one(field(
                "safety.gates.stop.max_revisions_per_node",
                "How many revisions may a single node take?",
            )
            .hint("One stuck node cannot burn the whole budget past this.")
            .number(Some(1.0), 1.0, None)
            .valid(uint_from(1))),
            one(field(
                "safety.gates.stop.max_wall_clock_seconds",
                "Wall-clock budget for the whole run?",
            )
            .hint("In seconds. Blank for no time limit.")
            .number(Some(0.0), 60.0, Some("s"))
            .valid(uint(false))),
            one(field("safety.gates.stop.max_tokens", "Token budget for the whole run?")
                .hint("Blank for no token ceiling.")
                .number(Some(0.0), 1000.0, None)
                .valid(uint(false))),
            one(field("safety.gates.stop.max_cost_usd", "Cost ceiling in USD?")
                .hint("The clearest safety limit for an overnight run. Strongly recommended.")
                .help(&["Leave this blank and the review panel will call the run unbounded, because it is."])
                .number(Some(0.0), 1.0, Some("USD"))
                .valid(float(false))),
            one(field(
                "safety.gates.stop.no_progress_iterations",
                "Halt after how many iterations with no change?",
            )
            .hint("Stop the line rather than spin when nothing is improving.")
            .number(Some(1.0), 1.0, None)
            .valid(uint_from(1))),
            one(field(
                "safety.gates.stop.stop_on_overall_success",
                "Stop as soon as every overall success is met?",
            )
            .yes_no()),
        ],
    )
}

fn graph() -> Section {
    gated(
        "graph",
        "Execution graph",
        "Define an execution graph of work nodes now?",
        "Nodes are the units of work. Without one, the loop runs a single implicit builder.",
        vec![
            list(
                "execution.graph.nodes",
                "What are the units of work?",
                "node",
                0,
                "id",
                vec![
                    field("id", "Node id").text("draft").needed(),
                    field("role", "Role").select(&[
                        ("builder", "builder", "produces the work"),
                        ("judge", "judge", "grades it against a standard"),
                        ("manager", "manager", "routes on the verdict"),
                        ("adversary", "adversary", "argues the other side"),
                        ("researcher", "researcher", "gathers material"),
                    ]),
                    field("instruction", "Instruction")
                        .hint("What this node actually does.")
                        .area(2, "")
                        .needed(),
                    field("depends_on", "Depends on")
                        .hint("Node ids. Only list an edge if this node truly reads that node's output.")
                        .items(Separator::Comma, "research"),
                    field("tier", "Tier").select(&[
                        ("cheap", "cheap", "high volume, low judgment"),
                        ("standard", "standard", ""),
                        ("strong", "strong", "low volume, high judgment"),
                    ]),
                    field("goals", "Goals this node advances")
                        .hint("Goal names. Blank for none.")
                        .items(Separator::Comma, ""),
                    field("provider", "Pin a provider?")
                        .hint("Leave unpinned to route by tier.")
                        .select_from(Options::ProviderIds),
                    field("isolation.mode", "Where does it run?")
                        .hint("Parallel writers need a worktree each; anything less and two nodes fight over one directory.")
                        .select(&[
                            ("none", "shared directory", "fine when nothing else writes"),
                            ("worktree", "its own git worktree", "required for parallel writers"),
                            ("container", "a container", "its own worktree, and its own process"),
                        ]),
                    field("isolation.image", "Container image")
                        .hint("The provider's CLI must exist inside it. Blank uses the graph's default image.")
                        .mono("ghcr.io/example/agent:1")
                        .when(When::Equals {
                            field: "isolation.mode".into(),
                            value: "container".into(),
                        }),
                    field("isolation.network", "Give the container a network?")
                        .hint("Off by default. A hosted model cannot be reached without one.")
                        .yes_no()
                        .when(When::Equals {
                            field: "isolation.mode".into(),
                            value: "container".into(),
                        }),
                ],
            )
            .hint("An edge means 'this node reads that node's output'. Only claim one where that is true.")
            .also("role"),
            one(field("execution.graph.concurrency.mode", "How much should run at once?")
                .hint("Auto derives the parallelism from the graph, which is usually right.")
                .select(&[
                    ("auto", "auto", "derive parallelism from the graph"),
                    ("sequential", "sequential", "one node at a time"),
                    ("fixed", "fixed", "a set width"),
                ])
                .when(When::MinItems {
                    field: "execution.graph.nodes".into(),
                    count: 1,
                })),
            one(field("execution.graph.concurrency.max_parallel", "How many nodes in parallel?")
                .number(Some(1.0), 1.0, None)
                .valid(uint_from(1))
                .when(When::Equals {
                    field: "execution.graph.concurrency.mode".into(),
                    value: "fixed".into(),
                })),
            one(field("execution.graph.join.strategy", "When may the next wave start?")
                .hint("Waiting for all of them is right when the next wave reads every output.")
                .select(&[
                    ("wait_for_all", "wait for all", "every node in the wave finishes"),
                    ("quorum", "quorum", "enough of them succeed"),
                    ("first_success", "first success", "one of them succeeds"),
                ])
                .when(When::MinItems {
                    field: "execution.graph.nodes".into(),
                    count: 2,
                })),
            one(field("execution.graph.join.count", "How many successes release a wave?")
                .number(Some(1.0), 1.0, None)
                .valid(uint_from(1))
                .when(When::Equals {
                    field: "execution.graph.join.strategy".into(),
                    value: "quorum".into(),
                })),
            one(field("execution.graph.container_image", "Default container image")
                .hint("Used by any node whose isolation is `container` and names no image of its own.")
                .mono("ghcr.io/example/agent:1")),
        ],
    )
}

fn background() -> Section {
    gated(
        "background",
        "Background",
        "Add static information every node receives?",
        "Facts that never change between iterations, handed to every node.",
        vec![list(
            "intent.background",
            "What should every node know?",
            "fact",
            0,
            "key",
            vec![
                field("key", "Key").text("house-style").needed(),
                field("value", "Value").area(2, "").needed(),
                field("note", "Note").text(""),
            ],
        )
        .also("value")],
    )
}

fn prerequisites() -> Section {
    gated(
        "prerequisites",
        "Prerequisites",
        "Record the manual steps you have already done by hand?",
        "You cannot automate a process you cannot yet describe by hand.",
        vec![list(
            "intent.prerequisites",
            "What has to be proved by hand first?",
            "step",
            0,
            "step",
            vec![
                field("step", "Manual step").area(2, "").needed(),
                field("done", "Have you actually done this by hand yet?").yes_no(),
                field("evidence", "Evidence")
                    .hint("Where the result of doing it by hand can be seen.")
                    .text(""),
            ],
        )],
    )
}

fn success() -> Section {
    gated(
        "success",
        "Success",
        "Define explicit success scenarios?",
        "What counts as done for the loop as a whole, beyond the per-goal checks.",
        vec![list(
            "intent.success",
            "What counts as done?",
            "scenario",
            0,
            "name",
            vec![
                field("target", "Target goal")
                    .select_from(Options::GoalTargets)
                    .needed(),
                field("name", "Scenario name").text("").needed(),
                field("mode", "Mode").select(&[
                    ("objective", "objective", ""),
                    ("subjective", "subjective", ""),
                    ("percentage", "percentage", "needs a threshold"),
                ]),
                field("statement", "Statement").area(2, "").needed(),
                field("threshold", "Threshold")
                    .hint("Between 0 and 1. Only used by percentage mode.")
                    .number(Some(0.0), 0.05, None)
                    .valid(fraction(false))
                    .when(When::Equals {
                        field: "mode".into(),
                        value: "percentage".into(),
                    }),
            ],
        )
        .also("target")],
    )
}

fn triggers() -> Section {
    let of_type = |id: &str, title: &str, kind: &str| {
        field(id, title).when(When::Equals {
            field: "on.type".into(),
            value: kind.into(),
        })
    };
    gated(
        "triggers",
        "Triggers",
        "Add schedules or triggers?",
        "What makes the loop fire on its own. Without one it only runs when you ask.",
        vec![
            list(
                "execution.triggers.triggers",
                "When should the loop fire?",
                "trigger",
                0,
                "on.type",
                vec![
                    field("on.type", "Trigger type").select(&[
                        ("interval", "interval", "every N seconds"),
                        ("cron", "cron", "a five-field schedule"),
                        ("file_change", "file_change", "when a path changes"),
                        ("goal_satisfied", "goal_satisfied", "when a goal becomes true"),
                        ("manual", "manual", "only on demand"),
                    ]),
                    of_type("on.seconds", "Interval in seconds", "interval")
                        .hint("Runs must finish faster than this, or they pile up.")
                        .number(Some(1.0), 60.0, Some("s"))
                        .valid(uint_from(1)),
                    of_type("on.expr", "Cron expression", "cron")
                        .hint("Five fields, evaluated in UTC: `0 9 * * 1` is 09:00 every Monday.")
                        .mono("0 9 * * 1")
                        .needed(),
                    of_type("on.path", "Path to watch", "file_change")
                        .mono("inbox")
                        .valid(Validator::Path { required: true }),
                    of_type("on.goal", "Upstream goal name", "goal_satisfied")
                        .text("")
                        .needed(),
                    field("idempotency_key", "Idempotency key")
                        .hint("Firings sharing a key inside the dedup window count as one. Blank derives it from the event.")
                        .text(""),
                ],
            )
            .help(&["A `file_change` on a directory this loop writes to, or a `goal_satisfied` on a goal it satisfies, can fire itself. The depth cap below is what bounds that."]),
            one(field("execution.triggers.max_depth", "How long may a self-started chain get?")
                .hint("A run started by the last run's own output is one link. Depth 0 is a run a human started.")
                .number(Some(0.0), 1.0, None)
                .valid(uint(true))),
            one(field(
                "execution.triggers.dedup_window_seconds",
                "How far apart must two identical firings be to count twice?",
            )
            .hint("In seconds. Six files landing together are one event, not six.")
            .number(Some(0.0), 60.0, Some("s"))
            .valid(uint(true))),
        ],
    )
}

fn limits() -> Section {
    gated(
        "limits",
        "Limits",
        "Set global constraints?",
        "Rules applied to every node. Per-node overrides stay a file and expert-editor job.",
        vec![
            one(field("safety.limits.global.rules", "Standing rules for every node")
                .hint("Literal instructions injected into every node's prompt. Separate them with a semicolon.")
                .items(Separator::Semicolon, "Never force-push; never edit CI config")),
            one(field("safety.limits.global.forbidden_paths", "Paths nothing may touch")
                .hint("An isolated node that changes one of these publishes nothing and stops the run.")
                .mono_items(Separator::Comma, ".env, secrets/")),
            one(field("safety.limits.global.forbidden_commands", "Commands nothing may run")
                .mono_items(Separator::Comma, "rm, curl")),
            one(field("safety.limits.global.max_tokens", "Per-node token cap")
                .number(Some(0.0), 1000.0, None)
                .valid(uint(false))),
            one(field("safety.limits.global.max_seconds", "Per-node time cap")
                .number(Some(0.0), 30.0, Some("s"))
                .valid(uint(false))),
            one(field("safety.limits.global.human_checkpoint", "Actions that need a human")
                .hint("Irreversible decisions do not get made at machine speed.")
                .items(Separator::Comma, "deploy, send email")),
        ],
    )
}

fn phases() -> Section {
    gated(
        "phases",
        "Phases",
        "Define execution-guideline phases?",
        "A phase is a stretch of the run with a standing instruction.",
        vec![
            list(
                "execution.phases.items",
                "What are the phases?",
                "phase",
                0,
                "name",
                vec![
                    field("name", "Phase name").text("gather").needed(),
                    field("guideline", "Standing instruction")
                        .hint("Usually about what not to do yet.")
                        .area(2, "")
                        .needed(),
                    field("note", "Note").text(""),
                ],
            ),
            one(field("execution.phases.dependency", "In what order do the phases run?")
                .hint("For example `gather -> draft -> review`. A semicolon separates independent chains; blank leaves them parallel.")
                .mono_items(Separator::Semicolon, "gather -> draft -> review")
                .when(When::MinItems {
                    field: "execution.phases.items".into(),
                    count: 2,
                })),
        ],
    )
}

fn default_skills() -> Section {
    gated(
        "default_skills",
        "Default skills",
        "Declare default skills to install?",
        "Sub-agents installed before the loop starts.",
        vec![list(
            "execution.default_skills",
            "Which skills should be installed first?",
            "skill",
            0,
            "name",
            vec![
                field("name", "Skill name")
                    .hint("Also the install directory.")
                    .text("")
                    .needed(),
                field("source", "Where does it come from?").select(&[
                    ("marketplace", "marketplace", "claudemarketplaces.com / skills CLI"),
                    ("github", "github", "an https git repo"),
                    ("local", "local", "already on disk"),
                ]),
                field("url", "URL or owner/repo@skill")
                    .hint("Required for github (https only); optional for marketplace.")
                    .mono("")
                    .when(When::NotEquals {
                        field: "source".into(),
                        value: "local".into(),
                    }),
                field("init_command", "Setup command")
                    .hint("Split on spaces and run directly — NOT a shell line, so &&, | and $() are literal.")
                    .mono_items(Separator::Whitespace, "npm install"),
                field("note", "Note").text(""),
            ],
        )],
    )
}

fn memory() -> Section {
    gated(
        "memory",
        "Memory",
        "Tune what each iteration remembers?",
        "The defaults carry two past summaries, which suits most loops.",
        vec![
            one(field(
                "execution.memory.carry_summaries",
                "How many past iteration summaries should a node see?",
            )
            .hint("0 disables carry-forward; 2 lets a node see its last two tries.")
            .number(Some(0.0), 1.0, None)
            .valid(uint(true))),
            one(field("execution.memory.summary_provider", "Which provider writes the summary prose?")
                .hint("Prose costs tokens every iteration. Blank keeps only the deterministic facts.")
                .select_from(Options::ProviderIds)),
            one(field("execution.memory.max_summary_chars", "Max characters per summary")
                .number(Some(0.0), 100.0, None)
                .valid(uint(true))),
        ],
    )
}

fn alerts() -> Section {
    gated(
        "alerts",
        "Alerts",
        "Watch any of the run's own numbers?",
        "An alert does not stop a run. It is for the numbers worth knowing about long before they are worth stopping for.",
        vec![list(
            "safety.alerts",
            "Which number, and where is the line?",
            "alert",
            0,
            "id",
            vec![
                field("id", "Alert name").text("spend-running-hot").needed(),
                field("metric", "Which number?").select(&[
                    ("cost_usd", "cost so far, in USD", ""),
                    ("tokens_used", "tokens charged so far", ""),
                    ("iterations", "iterations completed", ""),
                    ("wall_clock_seconds", "seconds since the run started", ""),
                    ("failed_dispatches", "dispatches that failed", ""),
                    ("retries", "dispatches sent round again", ""),
                    ("stale_iterations", "iterations with nothing moving", ""),
                    ("validation_pass_rate", "fraction of blocking checks passing", ""),
                ]),
                field("above", "Fire when it rises above")
                    .hint("Blank if only a floor matters.")
                    .number(None, 1.0, None)
                    .valid(float(false)),
                field("below", "Fire when it falls below")
                    .hint("Blank if only a ceiling matters.")
                    .number(None, 1.0, None)
                    .valid(float(false)),
                field("message", "What should it say?")
                    .hint("Blank builds a sentence from the metric and the threshold.")
                    .text(""),
            ],
        )
        .also("metric")],
    )
}

fn evolution() -> Section {
    gated(
        "evolution",
        "Evolution",
        "Let this loop propose changes to itself?",
        "Proposals are always written down. This decides whether the loop may treat them as its own work to adopt.",
        vec![
            one(field("features.self_evolution", "Turn the self-evolution feature on?")
                .hint("Both this and the section below must be on. Either one off means proposals are advice for a human and nothing more.")
                .yes_no()),
            one(field("evolution.enabled", "And switch it on for this loop?")
                .yes_no()
                .when(When::Equals {
                    field: "features.self_evolution".into(),
                    value: "true".into(),
                })),
            one(field("evolution.require_approval", "Require a human to approve an adoption?")
                .hint("Refused outright in `prod` if turned off.")
                .yes_no()
                .when(When::Equals {
                    field: "evolution.enabled".into(),
                    value: "true".into(),
                })),
            one(field("evolution.max_regression", "How much may a metric worsen and still count as an improvement?")
                .hint("A fraction: 0.02 allows a two-percent regression, which lets a change trade a hair of accuracy for half the cost.")
                .number(Some(0.0), 0.01, None)
                .valid(fraction(true))
                .when(When::Equals {
                    field: "evolution.enabled".into(),
                    value: "true".into(),
                })),
        ],
    )
}
