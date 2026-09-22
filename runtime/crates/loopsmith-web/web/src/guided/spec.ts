/**
 * The guided step list — the browser's copy of `src/guided/sections.rs`.
 *
 * Order mirrors `guided/mod.rs::stages()`: identity, providers, goals,
 * validations, stop gates, then the opt-in advanced sections behind their
 * gates. Field labels, help text, defaults and validators are lifted from the
 * terminal wizard so the two front ends ask the same questions in the same
 * words — a person who has used one should recognise the other.
 */
import type {
  DefaultSkill, Detector, Goal, Guideline, InfoItem, LoopConfig, NodeSpec,
  SuccessScenario, Trigger, Validation, WorkItem,
} from "../types";
import {
  optFloat, optUint, reqFloat, reqUint, required, splitCommas, splitSemis, truncate,
  type Choice, type Step, type Value,
} from "./spec-types";

/** Goal names plus `overall`, the two things a check can be aimed at. */
function goalTargets(c: LoopConfig): Choice[] {
  return [
    ...(c.goals ?? []).map((g) => ({ value: g.name, label: g.name })),
    { value: "overall", label: "overall — the loop as a whole" },
  ];
}

/** A fresh detector of the chosen kind, so switching kind never leaves stale fields. */
function blankDetector(type: string): Detector {
  switch (type) {
    case "file_exists": return { type: "file_exists", path: "", non_empty: false };
    case "regex_match": return { type: "regex_match", artifact: "", pattern: "" };
    case "threshold": return { type: "threshold", metric: "", op: "gte", value: 0 };
    case "judge": return { type: "judge", standard: "", min_score: null };
    default: return { type: "script", command: "", args: [], expect_exit: null };
  }
}

/** A fresh trigger of the chosen kind, for the same reason as `blankDetector`. */
function blankTrigger(type: string): Trigger {
  switch (type) {
    case "cron": return { type: "cron", expr: "" };
    case "file_change": return { type: "file_change", path: "" };
    case "goal_satisfied": return { type: "goal_satisfied", goal: "" };
    case "manual": return { type: "manual" };
    default: return { type: "interval", seconds: 3600 };
  }
}

const str = (v: Value) => String(v ?? "");
const numOrNull = (v: Value) => {
  const s = str(v).trim();
  return s === "" ? null : Number(s);
};

export const STEPS: Step[] = [
  /* ---------------------------------------------------------- identity --- */
  {
    kind: "field",
    id: "name",
    section: "Loop identity",
    title: "What is this loop called?",
    hint: "Becomes the generated skill name, so lower-case and hyphens travel best.",
    help: ["For example: weekly-competitor-brief"],
    input: { kind: "text", placeholder: "weekly-competitor-brief" },
    required: true,
    validate: required,
    get: (c) => c.name ?? "",
    set: (_c, v) => ({ name: str(v) }),
  },
  {
    kind: "field",
    id: "description",
    section: "Loop identity",
    title: "What is it for, in a sentence?",
    hint: "For the humans reading the config later. Never sent to a node.",
    input: { kind: "area", rows: 2, placeholder: "Track what competitors shipped this week and brief the team." },
    get: (c) => c.description ?? "",
    set: (_c, v) => ({ description: str(v) }),
  },
  {
    kind: "field",
    id: "version",
    section: "Loop identity",
    title: "Version",
    hint: "Semantic version for your own tracking. 0.1.0 is a fine start.",
    input: { kind: "text", mono: true, placeholder: "0.1.0" },
    validate: required,
    get: (c) => c.version || "0.1.0",
    set: (_c, v) => ({ version: str(v) }),
  },

  /* --------------------------------------------------------- providers --- */
  {
    kind: "providers",
    id: "providers",
    section: "Providers",
    title: "Which agent CLIs may this loop call?",
    hint: "Everything found on this machine is listed. Pick the ones this loop is allowed to spend.",
  },
  {
    kind: "field",
    id: "judge_independence",
    section: "Providers",
    title: "Refuse a judge that runs on the same provider as the work it grades?",
    hint: "A model grading its own family's output is not an independent check. On is the safe default.",
    input: { kind: "bool", trueLabel: "Yes — require an independent judge", falseLabel: "No — allow same-provider judging" },
    available: (c) => (c.providers?.providers?.length ?? 0) >= 2,
    get: (c) => c.providers?.enforce_judge_independence ?? true,
    set: (c, v) => ({ providers: { ...(c.providers ?? {}), enforce_judge_independence: Boolean(v) } }),
  },

  /* ------------------------------------------------------------- goals --- */
  {
    kind: "list",
    id: "goals",
    section: "Goals",
    title: "What is this loop trying to achieve?",
    hint: "Name each outcome in plain language. Add at least one.",
    singular: "goal",
    min: 1,
    get: (c) => c.goals ?? [],
    set: (_c, items) => ({ goals: items as Goal[] }),
    blank: () => ({ name: "", description: "", depends_on: [], priority: null }) as Goal,
    describe: (g: Goal) => (g.name ? `${g.name} — ${truncate(g.description ?? "", 48)}` : "(unnamed goal)"),
    fields: [
      {
        id: "name",
        label: "Goal name",
        hint: "A short handle, referenced by validations and nodes.",
        input: { kind: "text", placeholder: "brief-published" },
        required: true,
        validate: required,
        get: (g: Goal) => g.name,
        set: (g: Goal, v) => ({ ...g, name: str(v) }),
      },
      {
        id: "description",
        label: "Description",
        hint: "Subjective phrasing is fine here. The validation is what must be checkable.",
        input: { kind: "area", rows: 2 },
        required: true,
        validate: required,
        get: (g: Goal) => g.description,
        set: (g: Goal, v) => ({ ...g, description: str(v) }),
      },
      {
        id: "depends_on",
        label: "Depends on",
        hint: "Other goal names that must be satisfied first, comma-separated. Blank for none.",
        input: { kind: "text", placeholder: "research-done, draft-written" },
        get: (g: Goal) => (g.depends_on ?? []).join(", "),
        set: (g: Goal, v) => ({ ...g, depends_on: splitCommas(str(v)) }),
      },
      {
        id: "priority",
        label: "Priority",
        hint: "Lower runs first when it matters. Blank leaves it unordered.",
        input: { kind: "number", min: 0, step: 1 },
        validate: optUint,
        get: (g: Goal) => (g.priority == null ? "" : String(g.priority)),
        set: (g: Goal, v) => ({ ...g, priority: numOrNull(v) }),
      },
    ],
  },

  /* ------------------------------------------------------- validations --- */
  {
    kind: "list",
    id: "validations",
    section: "Validations",
    title: "How is each goal checked?",
    hint: "A validation is what actually decides 'done'. Prefer a deterministic check over a model judge.",
    help: ["A goal with no check is refused rather than run. This is the section that makes the rest safe."],
    singular: "validation",
    min: 1,
    get: (c) => c.validations ?? [],
    set: (_c, items) => ({ validations: items as Validation[] }),
    blank: () => ({
      target: "overall", name: "", mode: "objective", statement: "",
      detector: blankDetector("script"), blocking: true,
    }) as Validation,
    describe: (v: Validation) =>
      v.name ? `${v.target} → ${v.name} [${v.detector?.type ?? "script"}]` : "(unnamed validation)",
    fields: [
      {
        id: "target",
        label: "Which goal does this check?",
        hint: "The goal this validation gates, or `overall` for the whole loop.",
        input: (c: LoopConfig) => ({ kind: "select", options: goalTargets(c) }),
        get: (v: Validation) => v.target,
        set: (v: Validation, x) => ({ ...v, target: str(x) }),
      },
      {
        id: "name",
        label: "Validation name",
        hint: "A short handle for this check.",
        input: { kind: "text" },
        required: true,
        validate: required,
        get: (v: Validation) => v.name,
        set: (v: Validation, x) => ({ ...v, name: str(x) }),
      },
      {
        id: "mode",
        label: "Mode",
        hint: "How the result is read. Objective is the usual choice.",
        input: {
          kind: "select",
          options: [
            { value: "objective", label: "objective", note: "pass/fail on evidence" },
            { value: "subjective", label: "subjective", note: "a judged opinion" },
            { value: "percentage", label: "percentage", note: "a fraction of checks" },
          ],
        },
        get: (v: Validation) => v.mode,
        set: (v: Validation, x) => ({ ...v, mode: str(x) as Validation["mode"] }),
      },
      {
        id: "statement",
        label: "Statement",
        hint: "Natural-language description of the condition being checked.",
        input: { kind: "area", rows: 2 },
        required: true,
        validate: required,
        get: (v: Validation) => v.statement,
        set: (v: Validation, x) => ({ ...v, statement: str(x) }),
      },
      {
        id: "detector",
        label: "How is it decided?",
        hint: "The mechanism. Deterministic detectors are stronger than a judge.",
        input: {
          kind: "select",
          options: [
            { value: "script", label: "script", note: "run a command, exit 0 passes (strongest)" },
            { value: "file_exists", label: "file_exists", note: "a path must exist" },
            { value: "regex_match", label: "regex_match", note: "a pattern must match an artifact" },
            { value: "threshold", label: "threshold", note: "a number vs a limit" },
            { value: "judge", label: "judge", note: "a model verdict against a named standard" },
          ],
        },
        get: (v: Validation) => v.detector?.type ?? "script",
        set: (v: Validation, x) => ({ ...v, detector: blankDetector(str(x)) }),
      },

      // script
      {
        id: "d_command",
        label: "Command to run",
        input: { kind: "text", mono: true, placeholder: "npm" },
        required: true,
        validate: required,
        when: (v: Validation) => v.detector?.type === "script",
        get: (v: Validation) => (v.detector.type === "script" ? v.detector.command : ""),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "script" ? { ...v.detector, command: str(x) } : v.detector,
        }),
      },
      {
        id: "d_args",
        label: "Arguments",
        hint: "Argv tokens, split on spaces.",
        input: { kind: "text", mono: true, placeholder: "test --silent" },
        when: (v: Validation) => v.detector?.type === "script",
        get: (v: Validation) => (v.detector.type === "script" ? (v.detector.args ?? []).join(" ") : ""),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "script"
            ? { ...v.detector, args: str(x).split(/\s+/).filter(Boolean) }
            : v.detector,
        }),
      },
      // file_exists
      {
        id: "d_path",
        label: "Path that must exist",
        input: { kind: "text", mono: true },
        required: true,
        validate: required,
        when: (v: Validation) => v.detector?.type === "file_exists",
        get: (v: Validation) => (v.detector.type === "file_exists" ? v.detector.path : ""),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "file_exists" ? { ...v.detector, path: str(x) } : v.detector,
        }),
      },
      {
        id: "d_non_empty",
        label: "Must it also be non-empty?",
        input: { kind: "bool" },
        when: (v: Validation) => v.detector?.type === "file_exists",
        get: (v: Validation) => (v.detector.type === "file_exists" ? Boolean(v.detector.non_empty) : false),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "file_exists" ? { ...v.detector, non_empty: Boolean(x) } : v.detector,
        }),
      },
      // regex_match
      {
        id: "d_artifact",
        label: "Artifact (file) to search",
        input: { kind: "text", mono: true },
        required: true,
        validate: required,
        when: (v: Validation) => v.detector?.type === "regex_match",
        get: (v: Validation) => (v.detector.type === "regex_match" ? v.detector.artifact : ""),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "regex_match" ? { ...v.detector, artifact: str(x) } : v.detector,
        }),
      },
      {
        id: "d_pattern",
        label: "Regular expression",
        input: { kind: "text", mono: true },
        required: true,
        validate: required,
        when: (v: Validation) => v.detector?.type === "regex_match",
        get: (v: Validation) => (v.detector.type === "regex_match" ? v.detector.pattern : ""),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "regex_match" ? { ...v.detector, pattern: str(x) } : v.detector,
        }),
      },
      // threshold
      {
        id: "d_metric",
        label: "Metric name",
        input: { kind: "text" },
        required: true,
        validate: required,
        when: (v: Validation) => v.detector?.type === "threshold",
        get: (v: Validation) => (v.detector.type === "threshold" ? v.detector.metric : ""),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "threshold" ? { ...v.detector, metric: str(x) } : v.detector,
        }),
      },
      {
        id: "d_op",
        label: "Comparison",
        input: {
          kind: "select",
          options: [
            { value: "gte", label: "≥ at least" },
            { value: "gt", label: "> greater than" },
            { value: "lte", label: "≤ at most" },
            { value: "lt", label: "< less than" },
            { value: "eq", label: "= equal to" },
          ],
        },
        when: (v: Validation) => v.detector?.type === "threshold",
        get: (v: Validation) => (v.detector.type === "threshold" ? v.detector.op : "gte"),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "threshold"
            ? { ...v.detector, op: str(x) as "gt" | "gte" | "lt" | "lte" | "eq" }
            : v.detector,
        }),
      },
      {
        id: "d_value",
        label: "Threshold value",
        input: { kind: "number", step: 0.1 },
        required: true,
        validate: reqFloat,
        when: (v: Validation) => v.detector?.type === "threshold",
        get: (v: Validation) => (v.detector.type === "threshold" ? String(v.detector.value) : ""),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "threshold"
            ? { ...v.detector, value: Number(str(x) || 0) }
            : v.detector,
        }),
      },
      // judge
      {
        id: "d_standard",
        label: "The standard the judge checks against",
        hint: "Naming a standard is what turns an opinion into a check.",
        input: { kind: "area", rows: 2 },
        required: true,
        validate: required,
        when: (v: Validation) => v.detector?.type === "judge",
        get: (v: Validation) => (v.detector.type === "judge" ? v.detector.standard : ""),
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "judge" ? { ...v.detector, standard: str(x) } : v.detector,
        }),
      },
      {
        id: "d_min_score",
        label: "Minimum score to pass",
        hint: "Between 0 and 1. Blank for none.",
        input: { kind: "number", min: 0, step: 0.05 },
        validate: optFloat,
        when: (v: Validation) => v.detector?.type === "judge",
        get: (v: Validation) =>
          v.detector.type === "judge" && v.detector.min_score != null ? String(v.detector.min_score) : "",
        set: (v: Validation, x) => ({
          ...v,
          detector: v.detector.type === "judge" ? { ...v.detector, min_score: numOrNull(x) } : v.detector,
        }),
      },

      {
        id: "blocking",
        label: "Must this pass for the goal to count as satisfied?",
        hint: "A blocking validation holds the gate shut; a non-blocking one is only recorded.",
        input: { kind: "bool" },
        get: (v: Validation) => v.blocking ?? true,
        set: (v: Validation, x) => ({ ...v, blocking: Boolean(x) }),
      },
    ],
  },

  /* -------------------------------------------------------- stop gates --- */
  {
    kind: "field",
    id: "max_iterations",
    section: "Stop gates",
    title: "How many whole-loop iterations at most?",
    hint: "Hard ceiling on passes over the graph.",
    input: { kind: "number", min: 1, step: 1 },
    validate: reqUint,
    get: (c) => String(c.stop_gates?.max_iterations ?? 10),
    set: (c, v) => ({ stop_gates: { ...(c.stop_gates ?? {}), max_iterations: Number(str(v) || 10) } }),
  },
  {
    kind: "field",
    id: "max_revisions_per_node",
    section: "Stop gates",
    title: "How many revisions may a single node take?",
    hint: "One stuck node cannot burn the whole budget past this.",
    input: { kind: "number", min: 1, step: 1 },
    validate: reqUint,
    get: (c) => String(c.stop_gates?.max_revisions_per_node ?? 3),
    set: (c, v) => ({ stop_gates: { ...(c.stop_gates ?? {}), max_revisions_per_node: Number(str(v) || 3) } }),
  },
  {
    kind: "field",
    id: "max_wall_clock_seconds",
    section: "Stop gates",
    title: "Wall-clock budget for the whole run?",
    hint: "In seconds. Blank for no time limit.",
    input: { kind: "number", min: 0, step: 60, suffix: "s" },
    validate: optUint,
    get: (c) => (c.stop_gates?.max_wall_clock_seconds == null ? "" : String(c.stop_gates.max_wall_clock_seconds)),
    set: (c, v) => ({ stop_gates: { ...(c.stop_gates ?? {}), max_wall_clock_seconds: numOrNull(v) } }),
  },
  {
    kind: "field",
    id: "max_tokens",
    section: "Stop gates",
    title: "Token budget for the whole run?",
    hint: "Blank for no token ceiling.",
    input: { kind: "number", min: 0, step: 1000 },
    validate: optUint,
    get: (c) => (c.stop_gates?.max_tokens == null ? "" : String(c.stop_gates.max_tokens)),
    set: (c, v) => ({ stop_gates: { ...(c.stop_gates ?? {}), max_tokens: numOrNull(v) } }),
  },
  {
    kind: "field",
    id: "max_cost_usd",
    section: "Stop gates",
    title: "Cost ceiling in USD?",
    hint: "The clearest safety limit for an overnight run. Strongly recommended.",
    help: ["Leave this blank and the review panel will call the run unbounded, because it is."],
    input: { kind: "number", min: 0, step: 1, suffix: "USD" },
    validate: optFloat,
    get: (c) => (c.stop_gates?.max_cost_usd == null ? "" : String(c.stop_gates.max_cost_usd)),
    set: (c, v) => ({ stop_gates: { ...(c.stop_gates ?? {}), max_cost_usd: numOrNull(v) } }),
  },
  {
    kind: "field",
    id: "no_progress_iterations",
    section: "Stop gates",
    title: "Halt after how many iterations with no change?",
    hint: "Stop the line rather than spin when nothing is improving.",
    input: { kind: "number", min: 1, step: 1 },
    validate: reqUint,
    get: (c) => String(c.stop_gates?.no_progress_iterations ?? 3),
    set: (c, v) => ({ stop_gates: { ...(c.stop_gates ?? {}), no_progress_iterations: Number(str(v) || 3) } }),
  },
  {
    kind: "field",
    id: "stop_on_overall_success",
    section: "Stop gates",
    title: "Stop as soon as every overall success is met?",
    input: { kind: "bool" },
    get: (c) => c.stop_gates?.stop_on_overall_success ?? true,
    set: (c, v) => ({ stop_gates: { ...(c.stop_gates ?? {}), stop_on_overall_success: Boolean(v) } }),
  },

  /* ------------------------------------------------------------- graph --- */
  {
    kind: "gate",
    id: "g_graph",
    section: "Execution graph",
    title: "Define an execution graph of work nodes now?",
    hint: "Nodes are the units of work. Without one, the loop runs a single implicit builder.",
  },
  {
    kind: "list",
    id: "graph_nodes",
    section: "Execution graph",
    gate: "g_graph",
    title: "What are the units of work?",
    hint: "An edge means 'this node reads that node's output'. Only claim one where that is true.",
    singular: "node",
    min: 0,
    get: (c) => c.graph?.nodes ?? [],
    set: (c, items) => ({ graph: { ...(c.graph ?? {}), nodes: items as NodeSpec[] } }),
    blank: () => ({
      id: "", role: "builder", instruction: "", depends_on: [], goals: [],
      tier: "standard", provider: null, isolated: false,
    }) as NodeSpec,
    describe: (n: NodeSpec) => (n.id ? `${n.id} (${n.role}, ${n.tier ?? "standard"})` : "(unnamed node)"),
    fields: [
      {
        id: "id", label: "Node id", input: { kind: "text" }, required: true, validate: required,
        get: (n: NodeSpec) => n.id,
        set: (n: NodeSpec, v) => ({ ...n, id: str(v) }),
      },
      {
        id: "role", label: "Role",
        input: {
          kind: "select",
          options: [
            { value: "builder", label: "builder", note: "produces the work" },
            { value: "judge", label: "judge", note: "grades it against a standard" },
            { value: "manager", label: "manager", note: "routes on the verdict" },
            { value: "adversary", label: "adversary", note: "argues the other side" },
            { value: "researcher", label: "researcher", note: "gathers material" },
          ],
        },
        get: (n: NodeSpec) => n.role,
        set: (n: NodeSpec, v) => ({ ...n, role: str(v) as NodeSpec["role"] }),
      },
      {
        id: "instruction", label: "Instruction", hint: "What this node actually does.",
        input: { kind: "area", rows: 2 }, required: true, validate: required,
        get: (n: NodeSpec) => n.instruction,
        set: (n: NodeSpec, v) => ({ ...n, instruction: str(v) }),
      },
      {
        id: "depends_on", label: "Depends on",
        hint: "Comma-separated node ids. Only list an edge if this node truly reads that node's output.",
        input: { kind: "text" },
        get: (n: NodeSpec) => (n.depends_on ?? []).join(", "),
        set: (n: NodeSpec, v) => ({ ...n, depends_on: splitCommas(str(v)) }),
      },
      {
        id: "tier", label: "Tier",
        input: {
          kind: "select",
          options: [
            { value: "cheap", label: "cheap", note: "high volume, low judgment" },
            { value: "standard", label: "standard" },
            { value: "strong", label: "strong", note: "low volume, high judgment" },
          ],
        },
        get: (n: NodeSpec) => n.tier ?? "standard",
        set: (n: NodeSpec, v) => ({ ...n, tier: str(v) as NodeSpec["tier"] }),
      },
      {
        id: "goals", label: "Goals this node advances",
        hint: "Comma-separated goal names. Blank for none.",
        input: { kind: "text" },
        get: (n: NodeSpec) => (n.goals ?? []).join(", "),
        set: (n: NodeSpec, v) => ({ ...n, goals: splitCommas(str(v)) }),
      },
      {
        id: "provider", label: "Pin a provider?",
        hint: "Leave unpinned to route by tier.",
        input: (c: LoopConfig) => ({
          kind: "select",
          options: [
            { value: "", label: "(none — route by tier)" },
            ...(c.providers?.providers ?? []).map((p) => ({ value: p.id, label: p.id })),
          ],
        }),
        get: (n: NodeSpec) => n.provider ?? "",
        set: (n: NodeSpec, v) => ({ ...n, provider: str(v) || null }),
      },
      {
        id: "isolated", label: "Run in its own git worktree?",
        hint: "Required for parallel writers. Anything less and two nodes fight over one directory.",
        input: { kind: "bool" },
        get: (n: NodeSpec) => Boolean(n.isolated),
        set: (n: NodeSpec, v) => ({ ...n, isolated: Boolean(v) }),
      },
    ],
  },
  {
    kind: "field",
    id: "concurrency",
    section: "Execution graph",
    gate: "g_graph",
    title: "How much should run at once?",
    hint: "Auto derives the parallelism from the graph, which is usually right.",
    available: (c) => (c.graph?.nodes?.length ?? 0) > 0,
    input: {
      kind: "select",
      options: [
        { value: "auto", label: "auto", note: "derive parallelism from the graph" },
        { value: "sequential", label: "sequential", note: "one node at a time" },
        { value: "fixed", label: "fixed", note: "a set width" },
      ],
    },
    get: (c) => c.graph?.concurrency?.mode ?? "auto",
    set: (c, v) => ({
      graph: {
        ...(c.graph ?? {}),
        concurrency:
          str(v) === "sequential"
            ? { mode: "sequential" }
            : str(v) === "fixed"
              ? { mode: "fixed", max_parallel: 4 }
              : { mode: "auto" },
      },
    }),
  },
  {
    kind: "field",
    id: "max_parallel",
    section: "Execution graph",
    gate: "g_graph",
    title: "How many nodes in parallel?",
    input: { kind: "number", min: 1, step: 1 },
    validate: reqUint,
    available: (c) => c.graph?.concurrency?.mode === "fixed",
    get: (c) =>
      String(c.graph?.concurrency?.mode === "fixed" ? c.graph.concurrency.max_parallel : 4),
    set: (c, v) => ({
      graph: { ...(c.graph ?? {}), concurrency: { mode: "fixed", max_parallel: Number(str(v) || 4) } },
    }),
  },

  /* ----------------------------------------------- A: information ------- */
  {
    kind: "gate",
    id: "g_information",
    section: "Information (A)",
    title: "Add static information every node receives?",
    hint: "Facts that never change between iterations, handed to every node.",
  },
  {
    kind: "list",
    id: "information",
    section: "Information (A)",
    gate: "g_information",
    title: "What should every node know?",
    singular: "info item",
    min: 0,
    get: (c) => c.information ?? [],
    set: (_c, items) => ({ information: items as InfoItem[] }),
    blank: () => ({ key: "", value: "", note: null }) as InfoItem,
    describe: (i: InfoItem) => (i.key ? `${i.key} = ${truncate(i.value ?? "", 40)}` : "(empty item)"),
    fields: [
      {
        id: "key", label: "Key", input: { kind: "text" }, required: true, validate: required,
        get: (i: InfoItem) => i.key, set: (i: InfoItem, v) => ({ ...i, key: str(v) }),
      },
      {
        id: "value", label: "Value", input: { kind: "area", rows: 2 }, required: true, validate: required,
        get: (i: InfoItem) => i.value, set: (i: InfoItem, v) => ({ ...i, value: str(v) }),
      },
      {
        id: "note", label: "Note", input: { kind: "text" },
        get: (i: InfoItem) => i.note ?? "", set: (i: InfoItem, v) => ({ ...i, note: str(v) || null }),
      },
    ],
  },

  /* --------------------------------------------- B: pre-execution ------- */
  {
    kind: "gate",
    id: "g_pre_execution",
    section: "Pre-execution (B)",
    title: "Record the manual steps you have already done by hand?",
    hint: "You cannot automate a process you cannot yet describe by hand.",
  },
  {
    kind: "list",
    id: "pre_execution",
    section: "Pre-execution (B)",
    gate: "g_pre_execution",
    title: "What has to be proved by hand first?",
    singular: "step",
    min: 0,
    get: (c) => c.pre_execution ?? [],
    set: (_c, items) => ({ pre_execution: items as WorkItem[] }),
    blank: () => ({ step: "", done: false, evidence: null }) as WorkItem,
    describe: (w: WorkItem) => (w.step ? `[${w.done ? "x" : " "}] ${truncate(w.step, 44)}` : "(empty step)"),
    fields: [
      {
        id: "step", label: "Manual step", input: { kind: "area", rows: 2 }, required: true, validate: required,
        get: (w: WorkItem) => w.step, set: (w: WorkItem, v) => ({ ...w, step: str(v) }),
      },
      {
        id: "done", label: "Have you actually done this by hand yet?", input: { kind: "bool" },
        get: (w: WorkItem) => Boolean(w.done), set: (w: WorkItem, v) => ({ ...w, done: Boolean(v) }),
      },
      {
        id: "evidence", label: "Evidence", input: { kind: "text" },
        get: (w: WorkItem) => w.evidence ?? "",
        set: (w: WorkItem, v) => ({ ...w, evidence: str(v) || null }),
      },
    ],
  },

  /* --------------------------------------------------- E: success ------- */
  {
    kind: "gate",
    id: "g_success",
    section: "Success (E)",
    title: "Define explicit success scenarios?",
    hint: "What counts as done for the loop as a whole, beyond the per-goal checks.",
  },
  {
    kind: "list",
    id: "success",
    section: "Success (E)",
    gate: "g_success",
    title: "What counts as done?",
    singular: "scenario",
    min: 0,
    get: (c) => c.success ?? [],
    set: (_c, items) => ({ success: items as SuccessScenario[] }),
    blank: () => ({ target: "overall", name: "", mode: "objective", statement: "", threshold: null }) as SuccessScenario,
    describe: (s: SuccessScenario) => (s.name ? `${s.target} → ${s.name}` : "(unnamed scenario)"),
    fields: [
      {
        id: "target", label: "Target goal",
        input: (c: LoopConfig) => ({ kind: "select", options: goalTargets(c) }),
        get: (s: SuccessScenario) => s.target,
        set: (s: SuccessScenario, v) => ({ ...s, target: str(v) }),
      },
      {
        id: "name", label: "Scenario name", input: { kind: "text" }, required: true, validate: required,
        get: (s: SuccessScenario) => s.name, set: (s: SuccessScenario, v) => ({ ...s, name: str(v) }),
      },
      {
        id: "mode", label: "Mode",
        input: {
          kind: "select",
          options: [
            { value: "objective", label: "objective" },
            { value: "subjective", label: "subjective" },
            { value: "percentage", label: "percentage", note: "needs a threshold" },
          ],
        },
        get: (s: SuccessScenario) => s.mode,
        set: (s: SuccessScenario, v) => ({ ...s, mode: str(v) as SuccessScenario["mode"] }),
      },
      {
        id: "statement", label: "Statement", input: { kind: "area", rows: 2 }, required: true, validate: required,
        get: (s: SuccessScenario) => s.statement,
        set: (s: SuccessScenario, v) => ({ ...s, statement: str(v) }),
      },
      {
        id: "threshold", label: "Threshold", hint: "Between 0 and 1. Only used by percentage mode.",
        input: { kind: "number", min: 0, step: 0.05 }, validate: optFloat,
        when: (s: SuccessScenario) => s.mode === "percentage",
        get: (s: SuccessScenario) => (s.threshold == null ? "" : String(s.threshold)),
        set: (s: SuccessScenario, v) => ({ ...s, threshold: numOrNull(v) }),
      },
    ],
  },

  /* ------------------------------------------------- G: schedules ------- */
  {
    kind: "gate",
    id: "g_schedules",
    section: "Schedules (G)",
    title: "Add schedules or triggers?",
    hint: "What makes the loop fire on its own. Without one it only runs when you ask.",
  },
  {
    kind: "list",
    id: "schedules",
    section: "Schedules (G)",
    gate: "g_schedules",
    title: "When should the loop fire?",
    singular: "trigger",
    min: 0,
    get: (c) => c.schedules ?? [],
    set: (_c, items) => ({ schedules: items as Trigger[] }),
    blank: () => blankTrigger("interval"),
    describe: (t: Trigger) => t.type,
    fields: [
      {
        id: "type", label: "Trigger type",
        input: {
          kind: "select",
          options: [
            { value: "interval", label: "interval", note: "every N seconds" },
            { value: "cron", label: "cron", note: "a five-field schedule" },
            { value: "file_change", label: "file_change", note: "when a path changes" },
            { value: "goal_satisfied", label: "goal_satisfied", note: "when a goal becomes true" },
            { value: "manual", label: "manual", note: "only on demand" },
          ],
        },
        get: (t: Trigger) => t.type,
        set: (_t: Trigger, v) => blankTrigger(str(v)),
      },
      {
        id: "seconds", label: "Interval in seconds",
        hint: "Runs must finish faster than this, or they pile up.",
        input: { kind: "number", min: 1, step: 60, suffix: "s" }, validate: reqUint,
        when: (t: Trigger) => t.type === "interval",
        get: (t: Trigger) => (t.type === "interval" ? String(t.seconds) : ""),
        set: (t: Trigger, v) => (t.type === "interval" ? { ...t, seconds: Number(str(v) || 3600) } : t),
      },
      {
        id: "expr", label: "Cron expression", hint: "Five fields, e.g. `0 9 * * 1` for 09:00 every Monday.",
        input: { kind: "text", mono: true }, required: true, validate: required,
        when: (t: Trigger) => t.type === "cron",
        get: (t: Trigger) => (t.type === "cron" ? t.expr : ""),
        set: (t: Trigger, v) => (t.type === "cron" ? { ...t, expr: str(v) } : t),
      },
      {
        id: "path", label: "Path to watch",
        input: { kind: "text", mono: true }, required: true, validate: required,
        when: (t: Trigger) => t.type === "file_change",
        get: (t: Trigger) => (t.type === "file_change" ? t.path : ""),
        set: (t: Trigger, v) => (t.type === "file_change" ? { ...t, path: str(v) } : t),
      },
      {
        id: "goal", label: "Upstream goal name",
        input: { kind: "text" }, required: true, validate: required,
        when: (t: Trigger) => t.type === "goal_satisfied",
        get: (t: Trigger) => (t.type === "goal_satisfied" ? t.goal : ""),
        set: (t: Trigger, v) => (t.type === "goal_satisfied" ? { ...t, goal: str(v) } : t),
      },
    ],
  },

  /* ----------------------------------------------- H: constraints ------- */
  {
    kind: "gate",
    id: "g_constraints",
    section: "Constraints (H)",
    title: "Set global constraints?",
    hint: "Rules applied to every node. Per-node overrides stay a file and expert-editor job.",
  },
  {
    kind: "field",
    id: "c_rules",
    section: "Constraints (H)",
    gate: "g_constraints",
    title: "Standing rules for every node",
    hint: "Literal instructions injected into every node's prompt. Separate them with a semicolon.",
    input: { kind: "area", rows: 3 },
    get: (c) => (c.constraints?.global?.rules ?? []).join("; "),
    set: (c, v) => ({
      constraints: { ...(c.constraints ?? {}), global: { ...(c.constraints?.global ?? {}), rules: splitSemis(str(v)) } },
    }),
  },
  {
    kind: "field",
    id: "c_forbidden_paths",
    section: "Constraints (H)",
    gate: "g_constraints",
    title: "Paths nothing may touch",
    hint: "Comma-separated.",
    input: { kind: "text", mono: true },
    get: (c) => (c.constraints?.global?.forbidden_paths ?? []).join(", "),
    set: (c, v) => ({
      constraints: { ...(c.constraints ?? {}), global: { ...(c.constraints?.global ?? {}), forbidden_paths: splitCommas(str(v)) } },
    }),
  },
  {
    kind: "field",
    id: "c_forbidden_commands",
    section: "Constraints (H)",
    gate: "g_constraints",
    title: "Commands nothing may run",
    hint: "Comma-separated.",
    input: { kind: "text", mono: true },
    get: (c) => (c.constraints?.global?.forbidden_commands ?? []).join(", "),
    set: (c, v) => ({
      constraints: { ...(c.constraints ?? {}), global: { ...(c.constraints?.global ?? {}), forbidden_commands: splitCommas(str(v)) } },
    }),
  },
  {
    kind: "field",
    id: "c_max_tokens",
    section: "Constraints (H)",
    gate: "g_constraints",
    title: "Per-node token cap",
    input: { kind: "number", min: 0, step: 1000 },
    validate: optUint,
    get: (c) => (c.constraints?.global?.max_tokens == null ? "" : String(c.constraints.global.max_tokens)),
    set: (c, v) => ({
      constraints: { ...(c.constraints ?? {}), global: { ...(c.constraints?.global ?? {}), max_tokens: numOrNull(v) } },
    }),
  },
  {
    kind: "field",
    id: "c_max_seconds",
    section: "Constraints (H)",
    gate: "g_constraints",
    title: "Per-node time cap",
    input: { kind: "number", min: 0, step: 30, suffix: "s" },
    validate: optUint,
    get: (c) => (c.constraints?.global?.max_seconds == null ? "" : String(c.constraints.global.max_seconds)),
    set: (c, v) => ({
      constraints: { ...(c.constraints ?? {}), global: { ...(c.constraints?.global ?? {}), max_seconds: numOrNull(v) } },
    }),
  },
  {
    kind: "field",
    id: "c_human_checkpoint",
    section: "Constraints (H)",
    gate: "g_constraints",
    title: "Actions that need a human",
    hint: "Comma-separated. Irreversible actions do not get made at machine speed.",
    input: { kind: "text" },
    get: (c) => (c.constraints?.global?.human_checkpoint ?? []).join(", "),
    set: (c, v) => ({
      constraints: { ...(c.constraints ?? {}), global: { ...(c.constraints?.global ?? {}), human_checkpoint: splitCommas(str(v)) } },
    }),
  },

  /* ------------------------------------- I: execution guidelines -------- */
  {
    kind: "gate",
    id: "g_guidelines",
    section: "Guidelines (I)",
    title: "Define execution-guideline phases?",
    hint: "A phase is a stretch of the run with a standing instruction.",
  },
  {
    kind: "list",
    id: "guidelines",
    section: "Guidelines (I)",
    gate: "g_guidelines",
    title: "What are the phases?",
    singular: "phase",
    min: 0,
    get: (c) => c.execution_guidelines?.items ?? [],
    set: (c, items) => ({
      execution_guidelines: { ...(c.execution_guidelines ?? {}), items: items as Guideline[] },
    }),
    blank: () => ({ name: "", guideline: "", note: null }) as Guideline,
    describe: (g: Guideline) => g.name || "(unnamed phase)",
    fields: [
      {
        id: "name", label: "Phase name", input: { kind: "text" }, required: true, validate: required,
        get: (g: Guideline) => g.name, set: (g: Guideline, v) => ({ ...g, name: str(v) }),
      },
      {
        id: "guideline", label: "Standing instruction", input: { kind: "area", rows: 2 },
        required: true, validate: required,
        get: (g: Guideline) => g.guideline, set: (g: Guideline, v) => ({ ...g, guideline: str(v) }),
      },
      {
        id: "note", label: "Note", input: { kind: "text" },
        get: (g: Guideline) => g.note ?? "", set: (g: Guideline, v) => ({ ...g, note: str(v) || null }),
      },
    ],
  },
  {
    kind: "field",
    id: "guideline_order",
    section: "Guidelines (I)",
    gate: "g_guidelines",
    title: "In what order do the phases run?",
    hint: "For example `gather -> draft -> review`. A semicolon separates independent chains; blank leaves them parallel.",
    input: { kind: "text", mono: true },
    available: (c) => (c.execution_guidelines?.items?.length ?? 0) >= 2,
    get: (c) => (c.execution_guidelines?.dependency ?? []).join("; "),
    set: (c, v) => ({
      execution_guidelines: { ...(c.execution_guidelines ?? {}), dependency: splitSemis(str(v)) },
    }),
  },

  /* ------------------------------------------- J: default skills -------- */
  {
    kind: "gate",
    id: "g_skills",
    section: "Default skills (J)",
    title: "Declare default skills to install?",
    hint: "Sub-agents installed before the loop starts.",
  },
  {
    kind: "list",
    id: "default_skills",
    section: "Default skills (J)",
    gate: "g_skills",
    title: "Which skills should be installed first?",
    singular: "skill",
    min: 0,
    get: (c) => c.default_skills ?? [],
    set: (_c, items) => ({ default_skills: items as DefaultSkill[] }),
    blank: () => ({ name: "", source: "marketplace", url: null, init_command: null, note: null }) as DefaultSkill,
    describe: (s: DefaultSkill) => s.name || "(unnamed skill)",
    fields: [
      {
        id: "name", label: "Skill name", hint: "Also the install directory.",
        input: { kind: "text" }, required: true, validate: required,
        get: (s: DefaultSkill) => s.name, set: (s: DefaultSkill, v) => ({ ...s, name: str(v) }),
      },
      {
        id: "source", label: "Where does it come from?",
        input: {
          kind: "select",
          options: [
            { value: "marketplace", label: "marketplace", note: "claudemarketplaces.com / skills CLI" },
            { value: "github", label: "github", note: "an https git repo" },
            { value: "local", label: "local", note: "already on disk" },
          ],
        },
        get: (s: DefaultSkill) => s.source ?? "marketplace",
        set: (s: DefaultSkill, v) => ({ ...s, source: str(v) as DefaultSkill["source"] }),
      },
      {
        id: "url", label: "URL or owner/repo@skill",
        hint: "Required for github (https only); optional for marketplace; ignored for local.",
        input: { kind: "text", mono: true },
        when: (s: DefaultSkill) => s.source !== "local",
        get: (s: DefaultSkill) => s.url ?? "",
        set: (s: DefaultSkill, v) => ({ ...s, url: str(v) || null }),
      },
      {
        id: "init_command", label: "Setup command",
        hint: "Split on spaces and run directly — NOT a shell line, so &&, | and $() are literal.",
        input: { kind: "text", mono: true },
        get: (s: DefaultSkill) => s.init_command ?? "",
        set: (s: DefaultSkill, v) => ({ ...s, init_command: str(v) || null }),
      },
      {
        id: "note", label: "Note", input: { kind: "text" },
        get: (s: DefaultSkill) => s.note ?? "", set: (s: DefaultSkill, v) => ({ ...s, note: str(v) || null }),
      },
    ],
  },

  /* ----------------------------------------------------- context -------- */
  {
    kind: "gate",
    id: "g_context",
    section: "Context",
    title: "Tune what each iteration remembers?",
    hint: "The defaults carry two past summaries, which suits most loops.",
  },
  {
    kind: "field",
    id: "carry_summaries",
    section: "Context",
    gate: "g_context",
    title: "How many past iteration summaries should a node see?",
    hint: "0 disables carry-forward; 2 lets a node see its last two tries.",
    input: { kind: "number", min: 0, step: 1 },
    validate: reqUint,
    get: (c) => String(c.context?.carry_summaries ?? 2),
    set: (c, v) => ({ context: { ...(c.context ?? {}), carry_summaries: Number(str(v) || 0) } }),
  },
  {
    kind: "field",
    id: "summary_provider",
    section: "Context",
    gate: "g_context",
    title: "Which provider writes the summary prose?",
    hint: "Prose costs tokens every iteration. Blank keeps only the deterministic facts.",
    input: (c: LoopConfig) => ({
      kind: "select",
      options: [
        { value: "", label: "(none — deterministic facts only)" },
        ...(c.providers?.providers ?? []).map((p) => ({ value: p.id, label: p.id })),
      ],
    }),
    get: (c) => c.context?.summary_provider ?? "",
    set: (c, v) => ({ context: { ...(c.context ?? {}), summary_provider: str(v) || null } }),
  },
  {
    kind: "field",
    id: "max_summary_chars",
    section: "Context",
    gate: "g_context",
    title: "Max characters per summary",
    input: { kind: "number", min: 0, step: 100 },
    validate: reqUint,
    get: (c) => String(c.context?.max_summary_chars ?? 1200),
    set: (c, v) => ({ context: { ...(c.context ?? {}), max_summary_chars: Number(str(v) || 1200) } }),
  },

  /* --------------------------------------------------------- placement --- */
  {
    kind: "placement",
    id: "placement",
    section: "Where it lives",
    title: "Where should this loop be created?",
    hint: "The loop gets its own directory, named after it, inside the folder you pick.",
  },

  /* ------------------------------------------------------------ review --- */
  {
    kind: "review",
    id: "review",
    section: "Review",
    title: "Everything checked",
  },
];
