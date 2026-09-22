/**
 * Sections A–F: what the loop knows, what it is for, and how it ends.
 *
 * Each maps one-to-one onto a key of `LoopConfig`. The section order here is
 * the model's own order, so the form, the YAML, the schema, and HOW-TO-USE.md
 * all describe the same thing in the same sequence.
 */
import { Card, Field, Text, Area, Num, Select, Toggle, Repeater, Note, ListInput } from "./ui";
import type {
  LoopConfig, SectionHelp, Goal, Validation, Detector, Mode, CompareOp,
  InfoItem, WorkItem, SuccessScenario, StopGates,
} from "./types";

export type Patch = (patch: Partial<LoopConfig>) => void;
export type SectionProps = { cfg: LoopConfig; patch: Patch; help: Map<string, SectionHelp> };

const MODES: readonly { value: Mode; label: string }[] = [
  { value: "objective", label: "Objective — a machine decides" },
  { value: "subjective", label: "Subjective — a judgement is recorded" },
  { value: "percentage", label: "Percentage — a proportion must pass" },
];

const OPS: readonly { value: CompareOp; label: string }[] = [
  { value: "gte", label: "at least (≥)" },
  { value: "gt", label: "more than (>)" },
  { value: "lte", label: "at most (≤)" },
  { value: "lt", label: "less than (<)" },
  { value: "eq", label: "exactly (=)" },
];

/** Section shell that pulls its own copy out of the server's help catalogue. */
export function Section({
  k, help, children, actions, count, defaultOpen,
}: {
  k: string;
  help: Map<string, SectionHelp>;
  children: React.ReactNode;
  actions?: React.ReactNode;
  count?: number;
  defaultOpen?: boolean;
}) {
  const h = help.get(k);
  return (
    <Card
      title={h?.title ?? k}
      letter={h?.letter}
      summary={h?.summary}
      detail={h?.detail}
      failure={h?.failure}
      required={h?.required}
      actions={actions}
      count={count}
      defaultOpen={defaultOpen}
    >
      {children}
    </Card>
  );
}

/* --- A ------------------------------------------------------------------- */

export function Information({ cfg, patch, help }: SectionProps) {
  return (
    <Section k="information" help={help} count={cfg.information?.length}>
      <Repeater<InfoItem>
        items={cfg.information}
        onChange={(information) => patch({ information })}
        blank={() => ({ key: "", value: "" })}
        addLabel="Add a fact"
        empty="Nothing yet. Add the facts that stay true for the whole loop: the repository, the audience, the brand voice, the API being called."
        render={(item, set) => (
          <>
            <Field label="Key" hint="A short handle for this fact.">
              {(id) => <Text id={id} value={item.key} onChange={(key) => set({ key })} placeholder="repository" />}
            </Field>
            <Field label="Value" hint="The fact itself, as you would tell a colleague.">
              {(id) => <Text id={id} value={item.value} onChange={(value) => set({ value })} placeholder="github.com/acme/site, main branch" />}
            </Field>
            <Field label="Note" hint="Optional. Why this matters, if it is not obvious." wide>
              {(id) => <Text id={id} value={item.note ?? ""} onChange={(note) => set({ note: note || null })} />}
            </Field>
          </>
        )}
      />
    </Section>
  );
}

/* --- B ------------------------------------------------------------------- */

export function PreExecution({ cfg, patch, help }: SectionProps) {
  const pending = (cfg.pre_execution ?? []).filter((w) => !w.done).length;
  return (
    <Section
      k="pre_execution"
      help={help}
      count={cfg.pre_execution?.length}
      actions={pending > 0 ? <span className="chip chip-warn">{pending} not done</span> : undefined}
    >
      <Repeater<WorkItem>
        items={cfg.pre_execution}
        onChange={(pre_execution) => patch({ pre_execution })}
        blank={() => ({ step: "", done: false })}
        addLabel="Add a step"
        empty="Nothing yet. List what you must do by hand before automating it — publish one post yourself, send one email, run the change on one file."
        render={(item, set) => (
          <>
            <Field label="Step" hint="One thing to do by hand, once." wide>
              {(id) => <Text id={id} value={item.step} onChange={(step) => set({ step })} placeholder="Write and publish one post manually, start to finish" />}
            </Field>
            <div className="self-end pb-1">
              <Toggle
                checked={item.done ?? false}
                onChange={(done) => set({ done })}
                label="I have actually done this"
                hint="Tick it only when you can point at the result."
              />
            </div>
            <Field label="Evidence" hint="Where the proof is. A URL, a file, a commit.">
              {(id) => <Text id={id} value={item.evidence ?? ""} onChange={(evidence) => set({ evidence: evidence || null })} placeholder="posts/2026-03-first.md" />}
            </Field>
          </>
        )}
      />
    </Section>
  );
}

/* --- C ------------------------------------------------------------------- */

export function Goals({ cfg, patch, help }: SectionProps) {
  return (
    <Section k="goals" help={help} count={cfg.goals?.length}>
      <Repeater<Goal>
        items={cfg.goals}
        onChange={(goals) => patch({ goals })}
        blank={() => ({ name: "", description: "" })}
        addLabel="Add a goal"
        empty="A loop needs at least one goal. Give it a short name and say what done looks like."
        render={(item, set) => (
          <>
            <Field label="Name" helpPath="goals[].name" required>
              {(id) => <Text id={id} mono value={item.name} onChange={(name) => set({ name })} placeholder="draft-quality" />}
            </Field>
            <Field label="Depends on" hint="Other goal names that must be met first. Usually none.">
              {(id) => <ListInput id={id} mono value={item.depends_on} onChange={(depends_on) => set({ depends_on })} placeholder="research" />}
            </Field>
            <Field label="Description" hint="What done looks like, in ordinary words." required wide>
              {(id) => (
                <Area id={id} value={item.description} onChange={(description) => set({ description })}
                  placeholder="Every published post reads in the house voice and every factual claim in it has a source." />
              )}
            </Field>
          </>
        )}
      />
    </Section>
  );
}

/* --- D ------------------------------------------------------------------- */

function DetectorEditor({ value, onChange }: { value: Detector; onChange: (d: Detector) => void }) {
  // Switching kind replaces the object wholesale rather than merging: the
  // config model denies unknown fields, so a leftover `path` on a script
  // detector is a parse error rather than a harmless extra.
  const change = (type: Detector["type"]) => {
    const blanks: Record<Detector["type"], Detector> = {
      script: { type: "script", command: "", args: [], expect_exit: 0 },
      file_exists: { type: "file_exists", path: "", non_empty: false },
      regex_match: { type: "regex_match", artifact: "", pattern: "" },
      threshold: { type: "threshold", metric: "", op: "gte", value: 0 },
      judge: { type: "judge", standard: "", min_score: null },
    };
    onChange(blanks[type]);
  };

  return (
    <>
      <Field label="How it is checked" helpPath="validations[].detector" required>
        {(id) => (
          <Select
            id={id}
            value={value.type}
            onChange={change}
            options={[
              { value: "script", label: "Script — run a command, read its exit code" },
              { value: "file_exists", label: "File exists — a file was written" },
              { value: "regex_match", label: "Pattern — an artifact matches a regex" },
              { value: "threshold", label: "Threshold — a number crosses a line" },
              { value: "judge", label: "Judge — a model's verdict (not deterministic)" },
            ]}
          />
        )}
      </Field>

      {value.type === "script" && (
        <>
          <Field label="Command" hint="The executable to run. Its exit code is the answer.">
            {(id) => <Text id={id} mono value={value.command} onChange={(command) => onChange({ ...value, command })} placeholder="npm" />}
          </Field>
          <Field label="Arguments" hint="Comma separated. Passed literally, never through a shell.">
            {(id) => <ListInput id={id} mono value={value.args} onChange={(args) => onChange({ ...value, args })} placeholder="test, --silent" />}
          </Field>
          <Field label="Expected exit code" hint="0 for almost everything.">
            {(id) => <Num id={id} value={value.expect_exit ?? 0} onChange={(expect_exit) => onChange({ ...value, expect_exit })} />}
          </Field>
        </>
      )}

      {value.type === "file_exists" && (
        <>
          <Field label="Path" hint="Relative to the loop directory.">
            {(id) => <Text id={id} mono value={value.path} onChange={(path) => onChange({ ...value, path })} placeholder="dist/index.html" />}
          </Field>
          <div className="self-end pb-1">
            <Toggle
              checked={value.non_empty ?? false}
              onChange={(non_empty) => onChange({ ...value, non_empty })}
              label="Must not be empty"
              hint="A zero-byte file counts as written unless this is on."
            />
          </div>
        </>
      )}

      {value.type === "regex_match" && (
        <>
          <Field label="Artifact" hint="The file to search.">
            {(id) => <Text id={id} mono value={value.artifact} onChange={(artifact) => onChange({ ...value, artifact })} placeholder="report.md" />}
          </Field>
          <Field label="Pattern" hint="A regular expression. It only has to match somewhere.">
            {(id) => <Text id={id} mono value={value.pattern} onChange={(pattern) => onChange({ ...value, pattern })} placeholder="^## Sources" />}
          </Field>
        </>
      )}

      {value.type === "threshold" && (
        <>
          <Field label="Metric" hint="The name the run records this number under.">
            {(id) => <Text id={id} mono value={value.metric} onChange={(metric) => onChange({ ...value, metric })} placeholder="coverage" />}
          </Field>
          <Field label="Comparison">
            {(id) => <Select id={id} value={value.op} onChange={(op) => onChange({ ...value, op })} options={OPS} />}
          </Field>
          <Field label="Value">
            {(id) => <Num id={id} step={0.01} value={value.value} onChange={(v) => onChange({ ...value, value: v ?? 0 })} />}
          </Field>
        </>
      )}

      {value.type === "judge" && (
        <>
          <Field label="Standard" hint="Name the external standard being checked against.">
            {(id) => <Text id={id} value={value.standard} onChange={(standard) => onChange({ ...value, standard })} placeholder="the house style guide, section 3" />}
          </Field>
          <Field label="Minimum score" hint="Optional. Leave blank for a plain pass or fail.">
            {(id) => <Num id={id} step={0.1} value={value.min_score ?? null} onChange={(min_score) => onChange({ ...value, min_score })} />}
          </Field>
          <div className="col-span-full">
            <Note tone="warning">
              A judge is a model's opinion, not a measurement. It must name a standard, it must
              run on a different provider from the work it grades, and it cannot open the gate
              by itself — loopsmith decides that, reading this among other things.
            </Note>
          </div>
        </>
      )}
    </>
  );
}

export function Validations({ cfg, patch, help }: SectionProps) {
  const targets = ["overall", ...(cfg.goals ?? []).map((g) => g.name).filter(Boolean)];
  const deterministic = (cfg.validations ?? []).filter((v) => v.detector.type !== "judge").length;

  return (
    <Section
      k="validations"
      help={help}
      count={cfg.validations?.length}
      actions={
        <span className={`chip ${deterministic > 0 ? "chip-quench" : "chip-warn"}`}>
          {deterministic} deterministic
        </span>
      }
    >
      <Repeater<Validation>
        items={cfg.validations}
        onChange={(validations) => patch({ validations })}
        blank={() => ({
          target: targets[1] ?? "overall",
          name: "",
          mode: "objective",
          statement: "",
          blocking: true,
          detector: { type: "file_exists", path: "", non_empty: false },
        })}
        addLabel="Add a validation"
        empty="Every goal needs at least one check, or the config is refused rather than run. That refusal is the feature."
        render={(item, set) => (
          <>
            <Field label="Checks which goal" required>
              {(id) => (
                <Select id={id} value={item.target} onChange={(target) => set({ target })}
                  options={targets.map((t) => ({ value: t, label: t === "overall" ? "overall — the loop as a whole" : t }))} />
              )}
            </Field>
            <Field label="Name" hint="A short handle for this check." required>
              {(id) => <Text id={id} mono value={item.name} onChange={(name) => set({ name })} placeholder="tests-pass" />}
            </Field>
            <Field label="Statement" hint="What this check asserts, in words." required wide>
              {(id) => <Text id={id} value={item.statement} onChange={(statement) => set({ statement })} placeholder="The test suite passes with no failures." />}
            </Field>
            <Field label="Mode" hint="How the check is expressed. The detector below decides the rigour.">
              {(id) => <Select id={id} value={item.mode} onChange={(mode) => set({ mode })} options={MODES} />}
            </Field>
            <div className="self-end pb-1">
              <Toggle checked={item.blocking ?? true} onChange={(blocking) => set({ blocking })}
                label="Blocking" hint="Blocking checks hold the gate shut. Others are recorded only." />
            </div>
            <DetectorEditor value={item.detector} onChange={(detector) => set({ detector })} />
          </>
        )}
      />
    </Section>
  );
}

/* --- E ------------------------------------------------------------------- */

export function Success({ cfg, patch, help }: SectionProps) {
  const targets = ["overall", ...(cfg.goals ?? []).map((g) => g.name).filter(Boolean)];
  return (
    <Section k="success" help={help} count={cfg.success?.length}>
      <Repeater<SuccessScenario>
        items={cfg.success}
        onChange={(success) => patch({ success })}
        blank={() => ({ target: "overall", name: "", mode: "objective", statement: "" })}
        addLabel="Add a success scenario"
        empty="Without one, the loop runs to its iteration limit even after it has already done the job."
        render={(item, set) => (
          <>
            <Field label="Applies to" required>
              {(id) => (
                <Select id={id} value={item.target} onChange={(target) => set({ target })}
                  options={targets.map((t) => ({ value: t, label: t === "overall" ? "overall — the loop as a whole" : t }))} />
              )}
            </Field>
            <Field label="Name" required>
              {(id) => <Text id={id} mono value={item.name} onChange={(name) => set({ name })} placeholder="all-checks-pass" />}
            </Field>
            <Field label="Statement" hint="What good enough means here." required wide>
              {(id) => <Text id={id} value={item.statement} onChange={(statement) => set({ statement })} placeholder="Every blocking validation passes." />}
            </Field>
            <Field label="Mode">
              {(id) => <Select id={id} value={item.mode} onChange={(mode) => set({ mode })} options={MODES} />}
            </Field>
            {item.mode === "percentage" && (
              <Field label="Threshold" hint="A fraction between 0 and 1. 0.8 means four in five." required>
                {(id) => <Num id={id} step={0.05} min={0} value={item.threshold ?? null} onChange={(threshold) => set({ threshold })} />}
              </Field>
            )}
          </>
        )}
      />
    </Section>
  );
}

/* --- F ------------------------------------------------------------------- */

export function StopGatesSection({ cfg, patch, help }: SectionProps) {
  const g = cfg.stop_gates ?? {};
  const set = (p: Partial<StopGates>) => patch({ stop_gates: { ...g, ...p } });
  const unbounded = g.max_cost_usd == null && g.max_wall_clock_seconds == null;

  return (
    <Section
      k="stop_gates"
      help={help}
      actions={
        <span className={`chip ${unbounded ? "chip-warn" : "chip-good"}`}>
          {unbounded ? "no spend ceiling" : "bounded"}
        </span>
      }
    >
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Field label="Maximum iterations" helpPath="stop_gates.max_iterations">
          {(id) => <Num id={id} min={1} value={g.max_iterations ?? 10} onChange={(v) => set({ max_iterations: v ?? 10 })} />}
        </Field>
        <Field label="Maximum revisions per node" hint="How many times one node may redo its own work before the loop moves on.">
          {(id) => <Num id={id} min={1} value={g.max_revisions_per_node ?? 3} onChange={(v) => set({ max_revisions_per_node: v ?? 3 })} />}
        </Field>
        <Field label="Cost ceiling" helpPath="stop_gates.max_cost_usd">
          {(id) => <Num id={id} step={0.5} min={0} suffix="USD" value={g.max_cost_usd ?? null} onChange={(max_cost_usd) => set({ max_cost_usd })} />}
        </Field>
        <Field label="Wall-clock ceiling" hint="Seconds. The run halts when it has been going this long.">
          {(id) => <Num id={id} min={1} suffix="sec" value={g.max_wall_clock_seconds ?? null} onChange={(max_wall_clock_seconds) => set({ max_wall_clock_seconds })} />}
        </Field>
        <Field label="Token ceiling" hint="Total tokens across the whole run.">
          {(id) => <Num id={id} min={1} value={g.max_tokens ?? null} onChange={(max_tokens) => set({ max_tokens })} />}
        </Field>
        <Field label="Stop after no progress" helpPath="stop_gates.no_progress_iterations">
          {(id) => <Num id={id} min={0} value={g.no_progress_iterations ?? 3} onChange={(v) => set({ no_progress_iterations: v ?? 3 })} />}
        </Field>
        <Field label="Perturb after stalling" hint="Optional. Shake the run up after this many stalled iterations instead of waiting for the halt.">
          {(id) => <Num id={id} min={1} value={g.no_progress_iterations_randomness ?? null} onChange={(no_progress_iterations_randomness) => set({ no_progress_iterations_randomness })} />}
        </Field>
        <div className="self-end pb-1">
          <Toggle checked={g.stop_on_overall_success ?? true} onChange={(stop_on_overall_success) => set({ stop_on_overall_success })}
            label="Stop as soon as the overall goal is met" hint="Off means the loop keeps going and keeps spending after it has already succeeded." />
        </div>
      </div>
      {unbounded && (
        <div className="mt-3">
          <Note tone="warning">
            Neither a cost nor a wall-clock ceiling is set. The iteration limit is the only thing
            standing between this loop and an unbounded bill. Set at least one before leaving it
            running overnight.
          </Note>
        </div>
      )}
    </Section>
  );
}
