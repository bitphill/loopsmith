/**
 * How the work actually gets done: `execution`, and the `safety` limits that
 * bound it.
 *
 * These are the sections that decide what starts the loop, what it may not do,
 * who does the work, and how much of the past each prompt drags along. Like
 * the intent sections, each owns one dotted path and reaches it through `at`
 * and `put`.
 */
import { Section, type SectionProps } from "./sections-core";
import { Field, Text, Area, Num, Select, Toggle, Repeater, Note, ListInput, Icon } from "./ui";
import { at, put } from "./types";
import type {
  Fires, Goal, GraphSpec, Guideline, DefaultSkill, Memory, NodeSpec, Phases,
  ProviderRouting, ProviderSpec, Role, SkillPolicy, Tier, Trigger,
  Concurrency, ConstraintSet, Constraints, Detection,
} from "./types";

/** Every goal named so far, for the selects that point at one. */
const goalNames = (cfg: import("./types").LoopConfig): string[] =>
  (at<Goal[]>(cfg, "intent.goals") ?? []).map((g) => g.name).filter(Boolean);

/** Every provider id declared so far, for the selects that pin one. */
const providerNames = (cfg: import("./types").LoopConfig): string[] =>
  (at<ProviderRouting>(cfg, "execution.providers")?.providers ?? []).map((p) => p.id).filter(Boolean);

const TIERS: readonly { value: Tier; label: string }[] = [
  { value: "cheap", label: "cheap — routine work" },
  { value: "standard", label: "standard — most things" },
  { value: "strong", label: "strong — the hard parts" },
];

const ROLES: readonly { value: Role; label: string }[] = [
  { value: "builder", label: "Builder — does the work" },
  { value: "judge", label: "Judge — grades the work" },
  { value: "manager", label: "Manager — plans and splits it up" },
  { value: "adversary", label: "Adversary — tries to break it" },
  { value: "researcher", label: "Researcher — gathers what is needed" },
];

/* --- G ------------------------------------------------------------------- */

export function Schedules({ cfg, patch, help }: SectionProps) {
  const triggers = at<Trigger[]>(cfg, "execution.triggers.triggers") ?? [];
  const setAll = (items: Trigger[]) => patch(put(cfg, "execution.triggers.triggers", items));

  const change = (type: Fires["type"]): Fires => {
    const blanks: Record<Fires["type"], Fires> = {
      manual: { type: "manual" },
      cron: { type: "cron", expr: "0 9 * * 1" },
      interval: { type: "interval", seconds: 3600 },
      file_change: { type: "file_change", path: "" },
      goal_satisfied: { type: "goal_satisfied", goal: goalNames(cfg)[0] ?? "" },
    };
    return blanks[type];
  };

  return (
    <Section k="execution.triggers" help={help} count={triggers.length}>
      <Repeater<Trigger>
        items={triggers}
        onChange={setAll}
        blank={() => ({ on: { type: "manual" } })}
        addLabel="Add a trigger"
        empty="No triggers means this loop only runs when you press a button, which is the right place to start."
        render={(item, set, i) => {
          // Bound once so TypeScript can narrow it: `item.on` re-reads the
          // union on every access and narrows nothing.
          const fires = item.on;
          return (
          <>
            <Field label="Trigger type">
              {(id) => (
                <Select
                  id={id}
                  value={fires.type}
                  // Replacing rather than merging: each trigger shape has its
                  // own required field and the model denies unknown ones.
                  onChange={(t) => setAll(triggers.map((s, j) => (j === i ? { ...s, on: change(t) } : s)))}
                  options={[
                    { value: "manual", label: "Manual — only when you press run" },
                    { value: "interval", label: "Interval — every N seconds" },
                    { value: "cron", label: "Cron — a schedule, read in UTC" },
                    { value: "file_change", label: "File change — when a path changes" },
                    { value: "goal_satisfied", label: "Goal met — when another goal closes" },
                  ]}
                />
              )}
            </Field>
            {fires.type === "cron" && (
              <Field label="Cron expression" helpPath="execution.triggers.triggers[].on.expr">
                {(id) => <Text id={id} mono value={fires.expr}
                  onChange={(expr) => set({ on: { type: "cron", expr } })} placeholder="0 9 * * 1" />}
              </Field>
            )}
            {fires.type === "interval" && (
              <Field label="Every" hint="Seconds. Confirm a run finishes faster than this, or runs pile up.">
                {(id) => <Num id={id} min={1} suffix="sec" value={fires.seconds}
                  onChange={(v) => set({ on: { type: "interval", seconds: v ?? 3600 } })} />}
              </Field>
            )}
            {fires.type === "file_change" && (
              <Field label="Path to watch">
                {(id) => <Text id={id} mono value={fires.path}
                  onChange={(path) => set({ on: { type: "file_change", path } })} placeholder="src/" />}
              </Field>
            )}
            {fires.type === "goal_satisfied" && (
              <Field label="Which goal">
                {(id) => (
                  <Select id={id} value={fires.goal}
                    onChange={(goal) => set({ on: { type: "goal_satisfied", goal } })}
                    options={goalNames(cfg).map((n) => ({ value: n, label: n }))} />
                )}
              </Field>
            )}
          </>
          );
        }}
      />
      {triggers.some((t) => t.on.type === "cron") && (
        <div className="mt-3">
          <Note>
            Cron expressions are read in UTC. That is the usual explanation for a job that fires
            an hour off. An interval has no timezone to get wrong.
          </Note>
        </div>
      )}
    </Section>
  );
}

/* --- H ------------------------------------------------------------------- */

export function ConstraintsSection({ cfg, patch, help }: SectionProps) {
  const c = at<Constraints>(cfg, "safety.limits") ?? {};
  const g: ConstraintSet = c.global ?? {};
  const set = (p: Partial<ConstraintSet>) =>
    patch(put(cfg, "safety.limits", { ...c, global: { ...g, ...p } }));

  return (
    <Section
      k="safety.limits"
      help={help}
      actions={
        <span className={`chip ${(g.human_checkpoint ?? []).length > 0 ? "chip-good" : "chip-warn"}`}>
          {(g.human_checkpoint ?? []).length} checkpoint(s)
        </span>
      }
    >
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Field label="Human checkpoints" helpPath="safety.limits.global.human_checkpoint" wide>
          {(id) => <ListInput id={id} value={g.human_checkpoint} onChange={(human_checkpoint) => set({ human_checkpoint })} placeholder="send email, publish post, spend money, delete files" />}
        </Field>
        <Field label="Rules" hint="Plain-language limits, comma separated. Handed to every node." wide>
          {(id) => <ListInput id={id} value={g.rules} onChange={(rules) => set({ rules })} placeholder="Never force-push, Never edit files outside this directory" />}
        </Field>
        <Field label="Forbidden paths" hint="Paths nothing in this loop may touch.">
          {(id) => <ListInput id={id} mono value={g.forbidden_paths} onChange={(forbidden_paths) => set({ forbidden_paths })} placeholder=".env, .git/config" />}
        </Field>
        <Field label="Forbidden commands" hint="Commands nothing in this loop may run.">
          {(id) => <ListInput id={id} mono value={g.forbidden_commands} onChange={(forbidden_commands) => set({ forbidden_commands })} placeholder="rm, git push --force" />}
        </Field>
        <Field label="Token ceiling per node" hint="Optional. Applies to each node individually.">
          {(id) => <Num id={id} min={1} value={g.max_tokens ?? null} onChange={(max_tokens) => set({ max_tokens })} />}
        </Field>
        <Field label="Time ceiling per node" hint="Optional. Seconds, per node.">
          {(id) => <Num id={id} min={1} suffix="sec" value={g.max_seconds ?? null} onChange={(max_seconds) => set({ max_seconds })} />}
        </Field>
      </div>
      {(g.human_checkpoint ?? []).length === 0 && (
        <div className="mt-3">
          <Note tone="warning">
            No human checkpoints. Anything irreversible — sending, publishing, spending, deleting
            — will happen without asking. A hands-off loop with no checkpoints is not hands-off,
            it is unsupervised.
          </Note>
        </div>
      )}
    </Section>
  );
}

/* --- I ------------------------------------------------------------------- */

export function Guidelines({ cfg, patch, help }: SectionProps) {
  const eg = at<Phases>(cfg, "execution.phases") ?? {};
  return (
    <Section k="execution.phases" help={help} count={eg.items?.length} defaultOpen={false}>
      <Repeater<Guideline>
        items={eg.items}
        onChange={(items) => patch(put(cfg, "execution.phases", { ...eg, items }))}
        blank={() => ({ name: "", guideline: "" })}
        addLabel="Add a phase"
        empty="Optional. Only worth using when the work has genuinely ordered stages."
        render={(item, set) => (
          <>
            <Field label="Phase name" required>
              {(id) => <Text id={id} mono value={item.name} onChange={(name) => set({ name })} placeholder="gather" />}
            </Field>
            <Field label="Note" hint="Optional.">
              {(id) => <Text id={id} value={item.note ?? ""} onChange={(note) => set({ note: note || null })} />}
            </Field>
            <Field label="Standing instruction" hint="What every node in this phase is told." required wide>
              {(id) => <Area id={id} value={item.guideline} onChange={(guideline) => set({ guideline })} placeholder="Collect sources only. Do not draft anything yet." />}
            </Field>
          </>
        )}
      />
      <div className="mt-4">
        <Field label="Ordering" hint="One chain per line, written as `earlier -> later`.">
          {(id) => (
            <Area id={id} mono rows={2}
              value={(eg.dependency ?? []).join("\n")}
              onChange={(v) => patch(put(cfg, "execution.phases", { ...eg, dependency: v.split("\n").map((s) => s.trim()).filter(Boolean) }))}
              placeholder="gather -> draft -> review" />
          )}
        </Field>
      </div>
    </Section>
  );
}

/* --- J ------------------------------------------------------------------- */

export function Skills({ cfg, patch, help, detection }: SectionProps & { detection: Detection | null }) {
  const installed = detection?.skills ?? [];
  return (
    <Section
      k="execution.default_skills"
      help={help}
      count={at<DefaultSkill[]>(cfg, "execution.default_skills")?.length}
      defaultOpen={false}
      actions={installed.length > 0 ? <span className="chip">{installed.length} on this machine</span> : undefined}
    >
      <Repeater<DefaultSkill>
        items={at<DefaultSkill[]>(cfg, "execution.default_skills")}
        onChange={(items) => patch(put(cfg, "execution.default_skills", items))}
        blank={() => ({ name: "", source: "marketplace" })}
        addLabel="Add a sub-agent"
        empty="Optional. Nothing breaks without one; nodes simply work without the specialist."
        render={(item, set) => (
          <>
            <Field label="Name" required>
              {(id) => (
                <>
                  <Text id={id} mono value={item.name} onChange={(name) => set({ name })} placeholder="web-research" />
                  {installed.length > 0 && (
                    <div className="mt-1.5 flex flex-wrap gap-1">
                      {installed.slice(0, 12).map((s) => (
                        <button key={s.name} type="button" className="chip hover:border-ember"
                          title={s.description || s.origin} onClick={() => set({ name: s.name, source: "local" })}>
                          {s.name}
                        </button>
                      ))}
                    </div>
                  )}
                </>
              )}
            </Field>
            <Field label="Where it comes from">
              {(id) => (
                <Select id={id} value={item.source ?? "marketplace"} onChange={(source) => set({ source })}
                  options={[
                    { value: "marketplace", label: "Marketplace" },
                    { value: "github", label: "GitHub repository" },
                    { value: "local", label: "Already on this machine" },
                  ]} />
              )}
            </Field>
            {item.source === "github" && (
              <Field label="Repository URL" hint="https only.">
                {(id) => <Text id={id} mono value={item.url ?? ""} onChange={(url) => set({ url: url || null })} placeholder="https://github.com/owner/repo" />}
              </Field>
            )}
            <Field label="Setup command" hint="Optional. Run once after install. Argv, never a shell line.">
              {(id) => <Text id={id} mono value={item.init_command ?? ""} onChange={(init_command) => set({ init_command: init_command || null })} placeholder="npm install --production" />}
            </Field>
          </>
        )}
      />
      <hr className="rule my-4" />
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <div className="self-start">
          <Toggle
            checked={at<SkillPolicy>(cfg, "execution.skills")?.explore ?? false}
            onChange={(explore) =>
              patch(put(cfg, "execution.skills", { ...(at<SkillPolicy>(cfg, "execution.skills") ?? {}), explore }))}
            label="Let the loop trial new sub-agents"
            hint="Off until a loop is working. Candidates are scored against gate outcomes and written up as proposals the loop cannot apply itself."
          />
        </div>
        {at<SkillPolicy>(cfg, "execution.skills")?.explore && (
          <Field label="Candidates to trial" hint="Comma separated.">
            {(id) => (
              <ListInput id={id} mono value={at<SkillPolicy>(cfg, "execution.skills")?.explore_candidates}
                onChange={(explore_candidates) =>
                  patch(put(cfg, "execution.skills", { ...(at<SkillPolicy>(cfg, "execution.skills") ?? {}), explore_candidates }))} />
            )}
          </Field>
        )}
      </div>
    </Section>
  );
}

/* --- graph --------------------------------------------------------------- */

export function Graph({ cfg, patch, help }: SectionProps) {
  const graph = at<GraphSpec>(cfg, "execution.graph") ?? {};
  const nodes = graph.nodes ?? [];
  const setGraph = (p: Partial<GraphSpec>) => patch(put(cfg, "execution.graph", { ...graph, ...p }));
  const conc: Concurrency = graph.concurrency ?? { mode: "auto", cap: 16, min_marginal_gain: 0.05 };
  const providerIds = providerNames(cfg);
  const stages = (at<Phases>(cfg, "execution.phases")?.items ?? []).map((g) => g.name).filter(Boolean);

  return (
    <Section k="execution.graph" help={help} count={nodes.length}>
      <Repeater<NodeSpec>
        items={nodes}
        onChange={(n) => setGraph({ nodes: n })}
        blank={() => ({ id: "", role: "builder", instruction: "", tier: "standard", weight: 1, isolated: false })}
        addLabel="Add a node"
        empty="No nodes yet. A node is one unit of work: who does it, what they are told, and what they read first."
        render={(item, set) => (
          <>
            <Field label="Node id" required>
              {(id) => <Text id={id} mono value={item.id} onChange={(v) => set({ id: v })} placeholder="draft" />}
            </Field>
            <Field label="Role" helpPath="execution.graph.nodes[].role" required>
              {(id) => <Select id={id} value={item.role} onChange={(role) => set({ role })} options={ROLES} />}
            </Field>
            <Field label="Instruction" hint="Tight instructions produce tight output. At least 16 characters." required wide>
              {(id) => (
                <Area id={id} value={item.instruction} onChange={(instruction) => set({ instruction })}
                  placeholder="Draft the post from the gathered sources. Cite every factual claim inline." />
              )}
            </Field>
            <Field label="Depends on" helpPath="execution.graph.nodes[].depends_on">
              {(id) => <ListInput id={id} mono value={item.depends_on} onChange={(depends_on) => set({ depends_on })} placeholder="research" />}
            </Field>
            <Field label="Goals it serves" hint="Which goals this node's work counts toward.">
              {(id) => <ListInput id={id} mono value={item.goals} onChange={(goals) => set({ goals })} />}
            </Field>
            <Field label="Tier" helpPath="execution.graph.nodes[].tier">
              {(id) => <Select id={id} value={item.tier ?? "standard"} onChange={(tier) => set({ tier })} options={TIERS} />}
            </Field>
            <Field label="Pin to a provider" hint="Optional. Judges should be pinned to a different family from the builder.">
              {(id) => (
                <Select id={id} value={item.provider ?? ""} onChange={(v) => set({ provider: v || null })}
                  options={[{ value: "", label: "whichever the tier picks" }, ...providerIds.map((p) => ({ value: p, label: p }))]} />
              )}
            </Field>
            {stages.length > 0 && (
              <Field label="Phase" hint="Optional. The node waits until this phase is active.">
                {(id) => (
                  <Select id={id} value={item.stage ?? ""} onChange={(v) => set({ stage: v || null })}
                    options={[{ value: "", label: "always eligible" }, ...stages.map((s) => ({ value: s, label: s }))]} />
                )}
              </Field>
            )}
            <Field label="Weight" hint="Roughly how much work this is, relative to the others. Used to size the schedule.">
              {(id) => <Num id={id} step={0.5} min={0.1} value={item.weight ?? 1} onChange={(v) => set({ weight: v ?? 1 })} />}
            </Field>
            <div className="self-end pb-1">
              <Toggle
                checked={(item.isolation?.mode ?? "none") !== "none"}
                onChange={(on) => set({ isolation: { mode: on ? "worktree" : "none" } })}
                label="Isolated" hint="Its own git worktree. Required for builders that may run in parallel." />
            </div>
          </>
        )}
      />

      <hr className="rule my-4" />
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Field label="How much runs at once" hint="Auto sizes the worker count from the graph rather than from a guess.">
          {(id) => (
            <Select
              id={id}
              value={conc.mode}
              onChange={(mode) => {
                const blanks: Record<Concurrency["mode"], Concurrency> = {
                  sequential: { mode: "sequential" },
                  fixed: { mode: "fixed", max_parallel: 2 },
                  auto: { mode: "auto", cap: 16, min_marginal_gain: 0.05 },
                };
                setGraph({ concurrency: blanks[mode] });
              }}
              options={[
                { value: "auto", label: "Auto — sized from the graph" },
                { value: "fixed", label: "Fixed — exactly this many" },
                { value: "sequential", label: "Sequential — one at a time" },
              ]}
            />
          )}
        </Field>
        {conc.mode === "fixed" && (
          <Field label="Workers" hint="How many nodes may run at the same time.">
            {(id) => (
              <Num id={id} min={1} value={conc.max_parallel}
                onChange={(v) => setGraph({ concurrency: { mode: "fixed", max_parallel: v ?? 1 } })} />
            )}
          </Field>
        )}
        {conc.mode === "auto" && (
          <Field label="Worker cap" hint="Auto never goes above this, however parallel the graph looks.">
            {(id) => (
              <Num id={id} min={1} value={conc.cap ?? 16}
                onChange={(v) => setGraph({ concurrency: { ...conc, cap: v ?? 16 } })} />
            )}
          </Field>
        )}
      </div>
    </Section>
  );
}

/* --- providers ----------------------------------------------------------- */

export function Providers({
  cfg, patch, help, detection, onTest, testing, results,
}: SectionProps & {
  detection: Detection | null;
  onTest: (p: ProviderSpec) => void;
  testing: string | null;
  results: Record<string, { ok: boolean; text: string }>;
}) {
  const routing = at<ProviderRouting>(cfg, "execution.providers") ?? {};
  const setRouting = (p: Partial<ProviderRouting>) =>
    patch(put(cfg, "execution.providers", { ...routing, ...p }));
  const providers = routing.providers ?? [];
  const cascade = routing.cascade ?? {};
  const agents = detection?.agents ?? [];
  const unused = agents.filter((a) => !providers.some((p) => p.command === a.command));

  const add = (agentId: string) => {
    const a = agents.find((x) => x.id === agentId);
    if (!a) return;
    const spec: ProviderSpec = {
      id: a.id,
      kind: a.kind,
      command: a.command,
      args: a.args,
      tiers: a.tiers as Tier[],
      model: a.models[0] ?? null,
      requires_env: a.requires_env,
      prompt_on_stdin: a.prompt_on_stdin,
      cost_per_1k_tokens: a.cost_per_1k,
    };
    // Joining the cascade is the point of adding a provider: one that is
    // configured but in no tier is never reached by anything.
    const next = { ...cascade };
    for (const t of a.tiers) next[t] = [...(next[t] ?? []), a.id];
    setRouting({ providers: [...providers, spec], cascade: next });
  };

  return (
    <Section
      k="execution.providers"
      help={help}
      count={providers.length}
      actions={<span className="chip">{agents.length} found on this machine</span>}
    >
      {unused.length > 0 && (
        <div className="mb-4">
          <p className="label mb-2">Found on this machine — click to add</p>
          <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
            {unused.map((a) => (
              <button key={a.id} type="button" onClick={() => add(a.id)}
                className="card flex items-start gap-3 p-3 text-left transition-colors hover:border-ember">
                <span className="mt-0.5 text-ember">{Icon.bolt({ size: 16 })}</span>
                <span className="min-w-0 flex-1">
                  <span className="flex flex-wrap items-center gap-1.5">
                    <span className="text-[13px] font-semibold">{a.label}</span>
                    {a.confidence === "template" && <span className="chip chip-warn">confirm argv</span>}
                    {!a.env_ready && <span className="chip chip-bad">needs {a.missing_env.join(", ")}</span>}
                  </span>
                  <span className="hint mt-0.5 block font-mono text-[11px]">{a.path}{a.version ? ` · ${a.version}` : ""}</span>
                  <span className="hint mt-1 block">{a.note}</span>
                </span>
              </button>
            ))}
          </div>
        </div>
      )}

      <Repeater<ProviderSpec>
        items={providers}
        onChange={(v) => setRouting({ providers: v })}
        blank={() => ({ id: "", kind: "byok", command: "", args: [], tiers: ["standard"] })}
        addLabel="Add a provider by hand"
        empty="No providers configured. A loop needs at least one thing to call."
        render={(item, set) => {
          const result = results[item.id];
          const models = agents.find((a) => a.command === item.command)?.models ?? [];
          const ollama = item.kind === "ollama" ? (detection?.ollama_models ?? []).map((m) => m.name) : [];
          const choices = ollama.length > 0 ? ollama : models;
          return (
            <>
              <Field label="Id" hint="How the cascade and the nodes refer to this provider." required>
                {(id) => <Text id={id} mono value={item.id} onChange={(v) => set({ id: v })} placeholder="claude" />}
              </Field>
              <Field label="Family" hint="Which provider family this is. Only affects judge independence.">
                {(id) => (
                  <Select id={id} value={item.kind} onChange={(kind) => set({ kind })}
                    options={[
                      { value: "claude_code", label: "Claude Code" }, { value: "ollama", label: "Ollama" },
                      { value: "gemini", label: "Gemini" }, { value: "openai", label: "OpenAI" },
                      { value: "grok_cli", label: "Grok CLI" }, { value: "grok_build", label: "Grok Build" },
                      { value: "hermes", label: "Hermes" }, { value: "mcp", label: "MCP server" },
                      { value: "byok", label: "Anything else (bring your own)" },
                    ]} />
                )}
              </Field>
              <Field label="Command" helpPath="execution.providers.providers[].command" required>
                {(id) => <Text id={id} mono value={item.command} onChange={(command) => set({ command })} placeholder="claude" />}
              </Field>
              <Field label="Arguments" hint="Comma separated. {prompt} {system} {model} {tier} {node} are substituted.">
                {(id) => <ListInput id={id} mono value={item.args} onChange={(args) => set({ args })} />}
              </Field>
              <Field label="Model" hint="Free text. The list is what this machine reports; anything else still works.">
                {(id) => (
                  <>
                    <Text id={id} mono value={item.model ?? ""} onChange={(model) => set({ model: model || null })} placeholder="sonnet" />
                    {choices.length > 0 && (
                      <div className="mt-1.5 flex flex-wrap gap-1">
                        {choices.slice(0, 8).map((m) => (
                          <button key={m} type="button" className="chip hover:border-ember" onClick={() => set({ model: m })}>{m}</button>
                        ))}
                      </div>
                    )}
                  </>
                )}
              </Field>
              <Field label="Serves which tiers" hint="Comma separated: cheap, standard, strong.">
                {(id) => <ListInput id={id} mono value={item.tiers} onChange={(t) => set({ tiers: t as Tier[] })} />}
              </Field>
              <Field label="Required keys" helpPath="execution.providers.providers[].requires_env">
                {(id) => <ListInput id={id} mono value={item.requires_env} onChange={(requires_env) => set({ requires_env })} />}
              </Field>
              <Field label="Cost per 1000 tokens" hint="Makes the cost ceiling real rather than estimated.">
                {(id) => <Num id={id} step={0.001} min={0} suffix="USD" value={item.cost_per_1k_tokens ?? null} onChange={(cost_per_1k_tokens) => set({ cost_per_1k_tokens })} />}
              </Field>
              <Field label="Timeout" hint="Seconds before a call is written off.">
                {(id) => <Num id={id} min={1} suffix="sec" value={item.timeout_seconds ?? null} onChange={(timeout_seconds) => set({ timeout_seconds })} />}
              </Field>
              <div className="self-end pb-1">
                <Toggle checked={item.prompt_on_stdin ?? false} onChange={(prompt_on_stdin) => set({ prompt_on_stdin })}
                  label="Prompt on stdin" hint="Turn on for anything that reads a document, and for long prompts." />
              </div>
              <div className="col-span-full flex flex-wrap items-center gap-2">
                <button type="button" className="btn btn-sm btn-quench" disabled={testing === item.id || !item.command}
                  onClick={() => onTest(item)}>
                  {Icon.play({ size: 13 })}
                  {testing === item.id ? "Testing…" : "Test this provider"}
                </button>
                <span className="hint">Sends one real prompt. This costs whatever this provider charges.</span>
                {result && (
                  <span className={`chip ${result.ok ? "chip-good" : "chip-bad"}`}>
                    {result.ok ? Icon.check({ size: 12 }) : Icon.x({ size: 12 })}
                    {result.text}
                  </span>
                )}
              </div>
            </>
          );
        }}
      />

      <hr className="rule my-4" />
      <p className="label mb-2">Cascade</p>
      <p className="hint mb-3">
        Ordered fallback per tier, comma separated. The first reachable provider serves the call.
        This is what keeps a loop alive when one provider is rate-limited at two in the morning.
      </p>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        {(["cheap", "standard", "strong"] as const).map((tier) => (
          <Field key={tier} label={tier} hint={`Tried in order for ${tier} nodes.`}>
            {(id) => (
              <ListInput id={id} mono value={cascade[tier]}
                onChange={(v) => setRouting({ cascade: { ...cascade, [tier]: v } })}
                placeholder="claude, gemini" />
            )}
          </Field>
        ))}
      </div>
      <div className="mt-4">
        <Toggle
          checked={routing.enforce_judge_independence ?? true}
          onChange={(enforce_judge_independence) => setRouting({ enforce_judge_independence })}
          label="Refuse a judge that grades its own family's work"
          hint="On by default, and it should stay on. A model marking its own homework is not a check."
        />
      </div>
      {providers.length < 2 && (routing.enforce_judge_independence ?? true) && (
        <div className="mt-3">
          <Note tone="warning">
            Only one provider is configured while judge independence is on, so any judge node
            would have to grade its own family's work and will be refused at run time. Add a
            second provider.
          </Note>
        </div>
      )}
    </Section>
  );
}

/* --- context ------------------------------------------------------------- */

export function Context({ cfg, patch, help }: SectionProps) {
  const c = at<Memory>(cfg, "execution.memory") ?? {};
  const set = (p: Partial<Memory>) => patch(put(cfg, "execution.memory", { ...c, ...p }));
  const providerIds = providerNames(cfg);
  return (
    <Section k="execution.memory" help={help} defaultOpen={false}>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <Field label="Carry summaries" helpPath="execution.memory.carry_summaries">
          {(id) => <Num id={id} min={0} value={c.carry_summaries ?? 2} onChange={(v) => set({ carry_summaries: v ?? 0 })} />}
        </Field>
        <Field label="Summary length ceiling" hint="Characters. A summary with no ceiling eventually crowds out the instruction.">
          {(id) => <Num id={id} min={100} value={c.max_summary_chars ?? 1200} onChange={(v) => set({ max_summary_chars: v ?? 1200 })} />}
        </Field>
        <Field label="Who writes the summary" hint="Optional. A cheap provider is the right choice here.">
          {(id) => (
            <Select id={id} value={c.summary_provider ?? ""} onChange={(v) => set({ summary_provider: v || null })}
              options={[{ value: "", label: "no prose summary" }, ...providerIds.map((p) => ({ value: p, label: p }))]} />
          )}
        </Field>
      </div>
    </Section>
  );
}
