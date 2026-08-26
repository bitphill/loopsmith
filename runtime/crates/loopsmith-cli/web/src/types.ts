/**
 * The wire types, mirroring the Rust side field for field.
 *
 * `LoopConfig` here is the same A–J model `loopsmith-core` deserializes, and
 * it is posted back verbatim. There is deliberately no second schema and no
 * transformation layer: a field the browser renames is a field the config
 * model rejects, loudly, the moment it is typed.
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

export type Trigger =
  | { type: "cron"; expr: string }
  | { type: "interval"; seconds: number }
  | { type: "file_change"; path: string }
  | { type: "goal_satisfied"; goal: string }
  | { type: "manual" };

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
export interface ExecutionGuidelines { items?: Guideline[]; dependency?: string[] }

export interface DefaultSkill {
  name: string;
  source?: "marketplace" | "github" | "local";
  url?: string | null;
  init_command?: string | null;
  note?: string | null;
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
  isolated?: boolean;
}

export interface GraphSpec { nodes?: NodeSpec[]; concurrency?: Concurrency }

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

export interface ContextPolicy {
  carry_summaries?: number;
  summary_provider?: string | null;
  max_summary_chars?: number;
}

export interface LoopConfig {
  name: string;
  version?: string;
  description?: string;
  information?: InfoItem[];
  pre_execution?: WorkItem[];
  goals: Goal[];
  validations: Validation[];
  success?: SuccessScenario[];
  stop_gates?: StopGates;
  schedules?: Trigger[];
  constraints?: Constraints;
  execution_guidelines?: ExecutionGuidelines;
  default_skills?: DefaultSkill[];
  graph?: GraphSpec;
  providers?: ProviderRouting;
  skills?: SkillPolicy;
  context?: ContextPolicy;
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
  letter: string;
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
}
