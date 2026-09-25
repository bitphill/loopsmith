/**
 * The wire types, mirroring the Rust side field for field.
 *
 * `LoopConfig` here is the same model `loopsmith-core` deserializes, and it is
 * posted back verbatim. There is deliberately no second schema and no
 * transformation layer: a field the browser renames is a field the config
 * model rejects, loudly, the moment it is typed.
 *
 * 1.0 groups the config by what a key is *for* — what the loop is for
 * (`intent`), how the work gets done (`execution`), what must not happen and
 * when to stop (`safety`), and how it may change itself (`evolution`). The
 * form reaches into those bundles through [`at`] and [`put`] rather than
 * spreading four levels of object literal at every call site.
 */

export type Tier = "cheap" | "standard" | "strong";
export type Role = "builder" | "judge" | "manager" | "adversary" | "researcher";
export type Mode = "subjective" | "objective" | "percentage";
export type CompareOp = "gt" | "gte" | "lt" | "lte" | "eq";
export type Format = "yaml" | "markdown";
export type SecretStore = "profile" | "keychain";

export type Detector =
  | { type: "script"; command: string; args?: string[]; expect_exit?: number | null }
  | { type: "file_exists"; path: string; non_empty?: boolean }
  | { type: "regex_match"; artifact: string; pattern: string }
  | { type: "threshold"; metric: string; op: CompareOp; value: number }
  | { type: "judge"; standard: string; min_score?: number | null };

export type Fires =
  | { type: "cron"; expr: string }
  | { type: "interval"; seconds: number }
  | { type: "file_change"; path: string }
  | { type: "goal_satisfied"; goal: string }
  | { type: "manual" };

/**
 * One trigger. What fires it is nested under `on`, because 1.0 gave a trigger
 * things of its own to say — whether it is enabled, and the key that makes two
 * firings of the same event count once.
 */
export interface Trigger {
  on: Fires;
  enabled?: boolean;
  idempotency_key?: string | null;
}

export type Concurrency =
  | { mode: "sequential" }
  | { mode: "fixed"; max_parallel: number }
  | { mode: "auto"; cap?: number; min_marginal_gain?: number };

export interface InfoItem { key: string; value: string; note?: string | null }
export interface WorkItem { step: string; done?: boolean; evidence?: string | null }
export interface Goal { name: string; description: string; depends_on?: string[]; priority?: number | null }

export interface Validation {
  target: string;
  name: string;
  mode: Mode;
  statement: string;
  detector: Detector;
  blocking?: boolean;
}

export interface SuccessScenario {
  target: string;
  name: string;
  mode: Mode;
  statement: string;
  threshold?: number | null;
}

export interface StopGates {
  max_iterations?: number;
  max_revisions_per_node?: number;
  max_wall_clock_seconds?: number | null;
  max_tokens?: number | null;
  max_cost_usd?: number | null;
  no_progress_iterations?: number;
  no_progress_iterations_randomness?: number | null;
  stop_on_overall_success?: boolean;
}

export interface ConstraintSet {
  rules?: string[];
  forbidden_paths?: string[];
  forbidden_commands?: string[];
  max_tokens?: number | null;
  max_seconds?: number | null;
  human_checkpoint?: string[];
}

export interface Constraints {
  global?: ConstraintSet;
  per_node?: Record<string, ConstraintSet>;
}

export interface Guideline { name: string; guideline: string; note?: string | null }
export interface Phases { items?: Guideline[]; dependency?: string[] }

export interface DefaultSkill {
  name: string;
  source?: "marketplace" | "github" | "local";
  url?: string | null;
  init_command?: string | null;
  note?: string | null;
}

/** Where a node runs. A container degrades to a worktree when Docker is absent. */
export interface Isolation {
  mode: "none" | "worktree" | "container";
  image?: string | null;
  network?: boolean;
}

export interface NodeSpec {
  id: string;
  role: Role;
  instruction: string;
  depends_on?: string[];
  goals?: string[];
  tier?: Tier;
  provider?: string | null;
  skills?: string[];
  stage?: string | null;
  weight?: number;
  isolation?: Isolation;
}

export interface Join {
  strategy: "wait_for_all" | "quorum" | "first_success";
  count?: number | null;
}

export interface GraphSpec {
  nodes?: NodeSpec[];
  concurrency?: Concurrency;
  join?: Join;
  container_image?: string | null;
}

export interface ProviderSpec {
  id: string;
  kind: string;
  tiers?: Tier[];
  command: string;
  args?: string[];
  model?: string | null;
  requires_env?: string[];
  timeout_seconds?: number | null;
  prompt_on_stdin?: boolean;
  usage_regex?: string | null;
  cost_per_1k_tokens?: number | null;
}

export interface ProviderRouting {
  providers?: ProviderSpec[];
  cascade?: Record<string, string[]>;
  enforce_judge_independence?: boolean;
}

export interface SkillPolicy {
  acquisition_order?: ("installed" | "marketplace" | "generate")[];
  quarantine_dir?: string;
  min_marketplace_stars?: number;
  require_human_promotion?: boolean;
  explore?: boolean;
  explore_candidates?: string[];
  min_trials?: number;
}

/** What it takes for a remembered record to become reusable. */
export type Promotion =
  | { rule: "never" }
  | { rule: "automatic" }
  | { rule: "repeated_validation"; times?: number }
  | { rule: "human_approval" };

export interface NamespacePolicy {
  enabled?: boolean;
  retention_days?: number | null;
  promotion?: Promotion;
  min_confidence?: number;
  require_provenance?: boolean;
}

/**
 * The four kinds of thing a loop remembers, each with its own rules.
 *
 * Fixed keys rather than a map: the engine knows these four and nothing else
 * reads a fifth, so a free-form table would let the form write a namespace
 * that is silently never consulted.
 */
export interface Namespaces {
  episodic?: NamespacePolicy;
  semantic?: NamespacePolicy;
  procedural?: NamespacePolicy;
  failure?: NamespacePolicy;
}

/** What each iteration remembers, and for how long. */
export interface Memory {
  carry_summaries?: number;
  summary_provider?: string | null;
  max_summary_chars?: number;
  max_retrieved?: number;
  namespaces?: Namespaces;
}

/** One of the run's own measurements, by name. */
export type Metric =
  | "iterations"
  | "tokens_used"
  | "cost_usd"
  | "wall_clock_seconds"
  | "failed_dispatches"
  | "retries"
  | "stale_iterations"
  | "validation_pass_rate";

/** One number of the run's own, watched. */
export interface Alert {
  id: string;
  metric: Metric;
  above?: number | null;
  below?: number | null;
  message?: string;
}

/** What a gate does when its detector says no. */
export type GateOutcome = "stop" | "escalate" | "pause" | "rollback" | "warn";

/**
 * One checkpoint, decided by a detector.
 *
 * The same shape serves all three kinds of gate; what differs is *when* it is
 * checked, which is the list it is in rather than anything in the rule.
 */
export interface GateRule {
  id: string;
  statement: string;
  detector: Detector;
  on_fail?: GateOutcome;
}

export interface Gates {
  stop?: StopGates;
  entry?: GateRule[];
  approval?: GateRule[];
  rollback?: GateRule[];
}

/** The numbers a self-evolution proposal has to beat. */
export interface Baseline {
  completion_rate?: number | null;
  validation_pass_rate?: number | null;
  cost_usd?: number | null;
  latency_seconds?: number | null;
  iterations_to_success?: number | null;
  measured_at?: string | null;
}

export type ProposalKind =
  | "new_skill"
  | "skill_update"
  | "prompt_change"
  | "graph_change"
  | "provider_routing"
  | "validation_change"
  | "success_criteria";

export interface Evolution {
  enabled?: boolean;
  baseline?: Baseline | null;
  allowed_kinds?: ProposalKind[];
  max_regression?: number;
  require_sandbox?: boolean;
  require_approval?: boolean;
  keep_rollback?: boolean;
}

export interface Features {
  self_evolution?: boolean;
  marketplace_skills?: boolean;
  external_side_effects?: boolean;
  parallel_execution?: boolean;
  human_approval?: boolean;
}

/* --- the four bundles ---------------------------------------------------- */

export interface Intent {
  background?: InfoItem[];
  prerequisites?: WorkItem[];
  goals?: Goal[];
  success?: SuccessScenario[];
}

export interface Execution {
  graph?: GraphSpec;
  providers?: ProviderRouting;
  phases?: Phases;
  skills?: SkillPolicy;
  default_skills?: DefaultSkill[];
  memory?: Memory;
  triggers?: { triggers?: Trigger[]; max_depth?: number; dedup_window_seconds?: number };
}

export type Backoff = "fixed" | "linear" | "exponential";

/**
 * What the run does about one class of failure.
 *
 * Tagged by `action`, matching the Rust enum: the parameters belong to the
 * action that has them, so a `stop` cannot carry a retry count that nothing
 * would ever read.
 */
export type RecoveryAction =
  | { action: "retry"; max_attempts?: number; base_delay_seconds?: number; backoff?: Backoff }
  | { action: "revise"; max_attempts?: number }
  | { action: "fallback" }
  | { action: "escalate" }
  | { action: "pause" }
  | { action: "restore_checkpoint" }
  | { action: "stop" };

/** The failure-class to action map. Seven named classes, no others. */
export interface Recovery {
  transient_error?: RecoveryAction;
  invalid_output?: RecoveryAction;
  tool_unavailable?: RecoveryAction;
  repeated_failure?: RecoveryAction;
  safety_violation?: RecoveryAction;
  resource_exhaustion?: RecoveryAction;
  corrupted_state?: RecoveryAction;
}

export interface Safety {
  checks?: Validation[];
  gates?: Gates;
  limits?: Constraints;
  alerts?: Alert[];
  recovery?: Recovery;
  protected?: { components?: string[]; extra_paths?: string[] };
}

export interface LoopConfig {
  name: string;
  version?: string;
  description?: string;
  environment?: "dev" | "staging" | "prod";
  intent?: Intent;
  execution?: Execution;
  safety?: Safety;
  evolution?: Evolution;
  features?: Features;
}

/* --- reaching into the bundles ------------------------------------------- */

/**
 * Read a dotted path out of the config.
 *
 * The form's sections each own one path — `intent.goals`, `safety.gates.stop`
 * — and reaching them through the path rather than through four chained
 * optional accesses is what keeps a section readable as a section. The path is
 * the same string the validator names in an issue and `help.rs` keys its notes
 * by, so all three agree about what a section *is*.
 */
export function at<T>(cfg: LoopConfig, path: string): T | undefined {
  let cur: unknown = cfg;
  for (const part of path.split(".")) {
    if (cur == null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return cur as T | undefined;
}

/**
 * The patch that puts `value` at a dotted path.
 *
 * Every branch above it is copied rather than replaced, so writing
 * `execution.graph.nodes` leaves `execution.providers` — and anything else in
 * the file the form has no editor for — exactly where it was.
 */
export function put(cfg: LoopConfig, path: string, value: unknown): Partial<LoopConfig> {
  const [head, ...rest] = path.split(".");
  if (rest.length === 0) return { [head]: value } as Partial<LoopConfig>;
  const branch = (cfg as unknown as Record<string, unknown>)[head];
  const inner = (branch ?? {}) as LoopConfig;
  return {
    [head]: { ...(inner as object), ...put(inner, rest.join("."), value) },
  } as Partial<LoopConfig>;
}

/* --- detection ---------------------------------------------------------- */

export interface Agent {
  id: string;
  label: string;
  kind: string;
  path: string;
  version: string | null;
  command: string;
  args: string[];
  prompt_on_stdin: boolean;
  requires_env: string[];
  env_ready: boolean;
  missing_env: string[];
  tiers: string[];
  models: string[];
  cost_per_1k: number | null;
  note: string;
  confidence: "verified" | "template";
}

export interface OllamaModel { name: string; size: string }

export interface McpServer {
  name: string;
  origin: string;
  command: string;
  args: string[];
  env_keys: string[];
  url: string | null;
}

export interface EnvKey { name: string; purpose: string; present: boolean; fingerprint: string | null }
export interface SkillEntry { name: string; origin: string; description: string }
export interface GitFacts { installed: boolean; path: string | null; version: string | null }

export interface PlatformFacts {
  os: string;
  userland: string;
  bash: string | null;
  scheduler: string | null;
  home: string | null;
}

export interface Detection {
  agents: Agent[];
  ollama_models: OllamaModel[];
  mcp_servers: McpServer[];
  env_keys: EnvKey[];
  skills: SkillEntry[];
  git: GitFacts;
  platform: PlatformFacts;
  notes: string[];
  scanned_at_ms: number;
}

export interface HandshakeResult {
  ok: boolean;
  elapsed_ms: number;
  output: string;
  error: string | null;
}

export interface PathFacts {
  path: string;
  exists: boolean;
  is_dir: boolean;
  writable: boolean;
  empty: boolean;
  in_git_repo: boolean;
  git_root: string | null;
  has_claude_settings: boolean;
  existing_loop: string | null;
}

/* --- review ------------------------------------------------------------- */

export interface ReviewIssue { severity: "error" | "warning"; field: string; message: string }

export interface PlanView {
  waves: string[][];
  critical_path: string[];
  concurrency: number;
  predicted_speedup: number;
  speedup_ceiling: number;
  parallel_fraction: number;
  unisolated_parallel_writers: string[];
  error: string | null;
}

export interface CostView {
  ceiling_usd: number | null;
  worst_case_usd: number | null;
  basis: string;
  bounded: boolean;
}

export interface Review {
  parsed: boolean;
  parse_error: string | null;
  issues: ReviewIssue[];
  error_count: number;
  warning_count: number;
  plan: PlanView | null;
  permissions: string[];
  cost: CostView;
  notes: string[];
}

/* --- library and help --------------------------------------------------- */

export interface ExampleCard {
  id: string;
  name: string;
  blurb: string;
  origin: string;
  goals: number;
  validations: number;
  judge_validations: number;
  nodes: number;
  providers: number;
  trigger: string;
  max_iterations: number;
  max_cost_usd: number | null;
}

export interface LibraryEntry { path: string; name: string; config_file: string; created_ms: number }

export interface SectionHelp {
  /**
   * Which of the four bundles the section lives in, and `null` for the few
   * keys that sit above them. The form groups its cards by this.
   */
  bundle: string | null;
  /** The dotted path it edits, which the validator also names in an issue. */
  key: string;
  title: string;
  summary: string;
  detail: string;
  failure: string;
  required: boolean;
}

export interface FieldHelp { path: string; label: string; hint: string; detail: string; example: string }
export interface Help { sections: SectionHelp[]; fields: FieldHelp[] }

export interface SecretStatus {
  name: string;
  in_env: boolean;
  in_profile: boolean;
  in_keychain: boolean;
  keychain_kind: string | null;
}

/* --- jobs --------------------------------------------------------------- */

export type JobState = "running" | "succeeded" | "failed" | "cancelled";
export interface JobLine { seq: number; stream: "out" | "err" | "meta"; text: string }

/**
 * One thing that happened during a run, read off its own log by the server.
 *
 * `loopsmith_web::progress` does the reading, so the browser never parses the
 * log format — the format is Rust's and a regex here would be a copy of it
 * that nothing checks. The console still shows every line verbatim; this is
 * the same line, understood.
 */
export interface RunEvent {
  iteration: number;
  /** The ledger's own name for it: `NodeDispatched`, `GateEvaluated`, … */
  kind: string;
  node: string | null;
  detail: string;
  /** Where the run moved to, on a `StateChanged` and nowhere else. */
  state: string | null;
  /** Where it moved from. Only the first transition says anything new. */
  from: string | null;
  /** The reason, where the entry gives one. */
  why: string | null;
  /** The gate's running score: passed, then total. */
  satisfied: [number, number] | null;
}

export interface JobSummary {
  id: string;
  kind: string;
  argv: string[];
  cwd: string;
  state: JobState;
  exit_code: number | null;
  started_ms: number;
  finished_ms: number | null;
}

export interface Meta {
  version: string;
  exe: string;
  cwd: string;
  keychain: string | null;
  profile: string | null;
  /** Where the demonstration door builds its throwaway loop. */
  demo_dir: string;
}
