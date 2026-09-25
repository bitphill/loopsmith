/**
 * The parts of `safety` and `evolution` that decide what the loop may not do.
 *
 * These sections existed in the model from 1.0's first commit and had no
 * editor: the form parsed them, kept them through a round trip, and offered
 * no way to write one. That is a worse position than not having them at all,
 * because a field nobody can reach is a field nobody knows to use — and the
 * ones here are precisely the fields that matter when a run goes wrong at 4am.
 *
 * All five share a shape: they are policy, decided once, in advance, by
 * someone who is calm. The form's job is to make that decision visible rather
 * than to make it quick.
 */
import { Field, Num, Select, Text, Toggle, Repeater, Note, ListInput } from "./ui";
import { at, put } from "./types";
import { Section, DetectorEditor, type SectionProps } from "./sections-core";
import type {
  Alert, Baseline, Evolution, GateOutcome, GateRule, Metric, ProposalKind,
  Recovery, RecoveryAction,
} from "./types";

/* --- gate rules ---------------------------------------------------------- */

const OUTCOMES: readonly { value: GateOutcome; label: string }[] = [
  { value: "stop", label: "Stop — end the run" },
  { value: "escalate", label: "Escalate — stop and ask a person" },
  { value: "pause", label: "Pause — stop, resumable as it stands" },
  { value: "rollback", label: "Roll back — undo this iteration" },
  { value: "warn", label: "Warn — record it and carry on" },
];

/**
 * One of the three gate lists, which differ only in when they are checked.
 *
 * Kept as one component taking the path rather than three near-identical ones:
 * the rule is the same shape in all three, and the thing worth saying
 * differently is the section note, which comes from the server.
 */
function GateRules({
  cfg, patch, help, path, addLabel, empty,
}: SectionProps & { path: string; addLabel: string; empty: string }) {
  const rules = at<GateRule[]>(cfg, path) ?? [];
  return (
    <Section k={path} help={help} count={rules.length} defaultOpen={rules.length > 0}>
      <Repeater<GateRule>
        items={rules}
        onChange={(items) => patch(put(cfg, path, items))}
        blank={() => ({
          id: "",
          statement: "",
          on_fail: "stop",
          detector: { type: "script", command: "", args: [], expect_exit: 0 },
        })}
        addLabel={addLabel}
        empty={empty}
        render={(item, set) => (
          <>
            <Field label="Name" hint="A short handle, used in the ledger." required>
              {(id) => <Text id={id} mono value={item.id} onChange={(v) => set({ id: v })} placeholder="clean-tree" />}
            </Field>
            <Field label="If it fails" hint="What happens the moment this gate says no.">
              {(id) => (
                <Select id={id} value={item.on_fail ?? "stop"} onChange={(on_fail) => set({ on_fail })}
                  options={OUTCOMES} />
              )}
            </Field>
            <Field
              label="Statement"
              hint="Shown verbatim when this gate blocks — so write it for whoever is stopped, not for yourself."
              required
              wide
            >
              {(id) => (
                <Text id={id} value={item.statement} onChange={(statement) => set({ statement })}
                  placeholder="The working tree is clean before the loop touches anything." />
              )}
            </Field>
            <DetectorEditor value={item.detector} onChange={(detector) => set({ detector })} />
          </>
        )}
      />
    </Section>
  );
}

export const EntryGates = (p: SectionProps) => (
  <GateRules
    {...p}
    path="safety.gates.entry"
    addLabel="Add an entry gate"
    empty="Nothing yet. These are the conditions that make the whole run pointless if they are false, and they are checked before a single provider is called."
  />
);

export const ApprovalGates = (p: SectionProps) => (
  <GateRules
    {...p}
    path="safety.gates.approval"
    addLabel="Add an approval gate"
    empty="Nothing yet. Add one where the cost of being wrong is external — money moving, mail leaving, something published."
  />
);

export const RollbackGates = (p: SectionProps) => (
  <GateRules
    {...p}
    path="safety.gates.rollback"
    addLabel="Add a rollback gate"
    empty="Nothing yet. These run every iteration and, when one fails, the iteration that tripped it is discarded rather than built on."
  />
);

/* --- recovery ------------------------------------------------------------ */

/** The seven classes, in the order the model declares them. */
const CLASSES: readonly { key: keyof Recovery; label: string; hint: string }[] = [
  { key: "transient_error", label: "Transient error", hint: "A timeout, a rate limit, a provider briefly unreachable." },
  { key: "invalid_output", label: "Invalid output", hint: "The node answered, but not in a shape the run can use." },
  { key: "tool_unavailable", label: "Tool unavailable", hint: "A command or sub-agent the node needs is not there." },
  { key: "repeated_failure", label: "Repeated failure", hint: "The same node failing again after being given another go." },
  { key: "safety_violation", label: "Safety violation", hint: "A constraint was broken. This one is not a candidate for retrying." },
  { key: "resource_exhaustion", label: "Resource exhaustion", hint: "A budget, a token ceiling, or the disk ran out." },
  { key: "corrupted_state", label: "Corrupted state", hint: "The checkpoint or the store no longer reads." },
];

const ACTIONS: readonly { value: RecoveryAction["action"]; label: string }[] = [
  { value: "retry", label: "Retry — same work, again" },
  { value: "revise", label: "Revise — again, told what was wrong" },
  { value: "fallback", label: "Fall back — the next provider in the cascade" },
  { value: "escalate", label: "Escalate — hold the node and ask a person" },
  { value: "pause", label: "Pause — stop the run, resumable" },
  { value: "restore_checkpoint", label: "Restore — roll back to the last good checkpoint" },
  { value: "stop", label: "Stop — end the run now" },
];

export function RecoverySection({ cfg, patch, help, defaults }: SectionProps) {
  const recovery = at<Recovery>(cfg, "safety.recovery") ?? {};
  // What the engine does with a class nobody set. Read from the server's own
  // model rather than written down here: the defaults are deliberately
  // unequal — a transient error is retried, a safety violation never is — and
  // a second copy of that table would be wrong the first time one changed.
  const unset = (defaults && at<Recovery>(defaults, "safety.recovery")) ?? {};
  const set = (key: keyof Recovery, action: RecoveryAction) =>
    patch(put(cfg, "safety.recovery", { ...recovery, [key]: action }));

  // Switching action replaces the object rather than merging it: the model
  // denies unknown fields, so a `max_attempts` left on a `stop` is a parse
  // error rather than a harmless extra.
  const change = (key: keyof Recovery, action: RecoveryAction["action"]) => {
    const blanks: Record<RecoveryAction["action"], RecoveryAction> = {
      retry: { action: "retry", max_attempts: 3, base_delay_seconds: 2, backoff: "exponential" },
      revise: { action: "revise", max_attempts: 3 },
      fallback: { action: "fallback" },
      escalate: { action: "escalate" },
      pause: { action: "pause" },
      restore_checkpoint: { action: "restore_checkpoint" },
      stop: { action: "stop" },
    };
    set(key, blanks[action]);
  };

  return (
    <Section k="safety.recovery" help={help} defaultOpen={false}>
      <div className="space-y-3">
        <Note tone="note">
          Every class already has an answer — the ones shown are what the engine does when you say
          nothing. Change one where your loop's failures are not the usual ones.
        </Note>
        {CLASSES.map(({ key, label, hint }) => {
          const current = recovery[key] ?? unset[key];
          if (!current) return null;
          return (
            <div key={key} className="rounded-[10px] border bg-raised p-3">
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <Field label={label} hint={hint}>
                  {(id) => (
                    <Select id={id} value={current.action} onChange={(a) => change(key, a)} options={ACTIONS} />
                  )}
                </Field>
                {current.action === "retry" && (
                  <>
                    <Field label="Attempts" hint="In total, counting the one that failed.">
                      {(id) => (
                        <Num id={id} min={1} value={current.max_attempts ?? 3}
                          onChange={(v) => set(key, { ...current, max_attempts: v ?? 3 })} />
                      )}
                    </Field>
                    <Field label="First delay" hint="Before the first retry. The backoff decides the rest.">
                      {(id) => (
                        <Num id={id} min={0} suffix="seconds" value={current.base_delay_seconds ?? 2}
                          onChange={(v) => set(key, { ...current, base_delay_seconds: v ?? 0 })} />
                      )}
                    </Field>
                    <Field label="Backoff" hint="How the delay grows between attempts.">
                      {(id) => (
                        <Select id={id} value={current.backoff ?? "exponential"}
                          onChange={(backoff) => set(key, { ...current, backoff })}
                          options={[
                            { value: "fixed" as const, label: "Fixed — the same wait each time" },
                            { value: "linear" as const, label: "Linear — one delay longer each time" },
                            { value: "exponential" as const, label: "Exponential — doubling" },
                          ]} />
                      )}
                    </Field>
                  </>
                )}
                {current.action === "revise" && (
                  <Field label="Attempts" hint="In total, counting the one that was refused.">
                    {(id) => (
                      <Num id={id} min={1} value={current.max_attempts ?? 3}
                        onChange={(v) => set(key, { action: "revise", max_attempts: v ?? 3 })} />
                    )}
                  </Field>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </Section>
  );
}

/* --- alerts -------------------------------------------------------------- */

const METRICS: readonly { value: Metric; label: string }[] = [
  { value: "cost_usd", label: "Cost so far (USD)" },
  { value: "tokens_used", label: "Tokens used" },
  { value: "iterations", label: "Iterations" },
  { value: "wall_clock_seconds", label: "Wall clock (seconds)" },
  { value: "failed_dispatches", label: "Failed dispatches" },
  { value: "retries", label: "Retries" },
  { value: "stale_iterations", label: "Iterations with no progress" },
  { value: "validation_pass_rate", label: "Proportion of checks passing (0–1)" },
];

export function Alerts({ cfg, patch, help }: SectionProps) {
  const alerts = at<Alert[]>(cfg, "safety.alerts") ?? [];
  return (
    <Section k="safety.alerts" help={help} count={alerts.length} defaultOpen={alerts.length > 0}>
      <Repeater<Alert>
        items={alerts}
        onChange={(items) => patch(put(cfg, "safety.alerts", items))}
        blank={() => ({ id: "", metric: "cost_usd", above: null, below: null })}
        addLabel="Add an alert"
        empty="Nothing yet. An alert is how a run going sideways tells you an hour before the ceiling does."
        render={(item, set) => (
          <>
            <Field label="Name" hint="A short handle, used in the ledger." required>
              {(id) => <Text id={id} mono value={item.id} onChange={(v) => set({ id: v })} placeholder="spend-halfway" />}
            </Field>
            <Field label="Watching" hint="Which of the run's own numbers.">
              {(id) => <Select id={id} value={item.metric} onChange={(metric) => set({ metric })} options={METRICS} />}
            </Field>
            <Field label="Fires above" hint="Leave empty if only a floor matters.">
              {(id) => <Num id={id} step={0.1} value={item.above ?? null} onChange={(above) => set({ above })} />}
            </Field>
            <Field label="Fires below" hint="For the numbers where falling is the bad direction.">
              {(id) => <Num id={id} step={0.1} value={item.below ?? null} onChange={(below) => set({ below })} />}
            </Field>
            <Field label="Message" hint="Optional. What to say instead of the bare number." wide>
              {(id) => (
                <Text id={id} value={item.message ?? ""} onChange={(v) => set({ message: v || undefined })}
                  placeholder="Half the budget is gone and nothing has passed yet." />
              )}
            </Field>
          </>
        )}
      />
    </Section>
  );
}

/* --- protected ----------------------------------------------------------- */

/**
 * The ten components the engine knows how to protect.
 *
 * The names are `ProtectedComponent`'s own, and every one of them is on by
 * default in the model — untick one and you are widening what the loop may
 * rewrite about itself, which is why the labels say what is at stake rather
 * than restating the name.
 */
const COMPONENTS: readonly { id: string; label: string; hint: string }[] = [
  { id: "gates", label: "Gates", hint: "Every way the run is allowed to end, and every checkpoint on the way." },
  { id: "limits", label: "Limits", hint: "Forbidden paths and commands, and the budget ceilings." },
  { id: "recovery", label: "Recovery", hint: "How each class of failure is answered." },
  { id: "protected", label: "This list", hint: "A loop that can shorten this list has no protection at all." },
  { id: "approvals", label: "Approvals", hint: "Human checkpoints, and what needs signing off." },
  { id: "credentials", label: "Credentials", hint: "Which secrets the loop may reach for." },
  { id: "audit", label: "Audit", hint: "The ledger, and the alerts watching it." },
  { id: "baselines", label: "Baselines", hint: "A loop that can move its own baseline can call any change an improvement." },
  { id: "retention", label: "Retention", hint: "How long each kind of memory is kept." },
  { id: "environment", label: "Environment", hint: "Which environment this is, and the feature switches." },
];

export function Protected({ cfg, patch, help }: SectionProps) {
  const chosen = at<string[]>(cfg, "safety.protected.components") ?? [];
  const toggle = (id: string, on: boolean) =>
    patch(put(cfg, "safety.protected.components",
      on ? [...chosen, id] : chosen.filter((c) => c !== id)));

  return (
    <Section k="safety.protected" help={help} count={chosen.length} defaultOpen={false}>
      <div className="space-y-2.5">
        {COMPONENTS.map((c) => (
          <Toggle
            key={c.id}
            checked={chosen.includes(c.id)}
            onChange={(on) => toggle(c.id, on)}
            label={c.label}
            hint={c.hint}
          />
        ))}
        <Field
          label="And these paths"
          hint="Anything else a proposal may not rewrite. Comma-separated."
          wide
        >
          {(id) => (
            <ListInput
              id={id}
              mono
              value={at<string[]>(cfg, "safety.protected.extra_paths")}
              onChange={(v) => patch(put(cfg, "safety.protected.extra_paths", v))}
              placeholder="scripts/check.sh, .github/workflows"
            />
          )}
        </Field>
      </div>
    </Section>
  );
}

/* --- evolution ----------------------------------------------------------- */

const KINDS: readonly { value: ProposalKind; label: string }[] = [
  { value: "new_skill", label: "Acquire a new sub-agent" },
  { value: "skill_update", label: "Update a sub-agent it already has" },
  { value: "prompt_change", label: "Reword a node's instruction" },
  { value: "graph_change", label: "Change the graph" },
  { value: "provider_routing", label: "Change which model serves a tier" },
  { value: "validation_change", label: "Change a check" },
  { value: "success_criteria", label: "Change what counts as success" },
];

/** The baseline's own fields, with the direction that counts as better. */
const BASELINE_FIELDS: readonly {
  key: keyof Baseline; label: string; hint: string; step: number;
}[] = [
  { key: "completion_rate", label: "Completion rate", hint: "Proportion of runs that reached success. 0–1.", step: 0.01 },
  { key: "validation_pass_rate", label: "Check pass rate", hint: "Proportion of checks passing. 0–1.", step: 0.01 },
  { key: "cost_usd", label: "Cost per run", hint: "In USD. Lower is better.", step: 0.01 },
  { key: "latency_seconds", label: "Time per run", hint: "In seconds. Lower is better.", step: 1 },
  { key: "iterations_to_success", label: "Iterations to success", hint: "Lower is better.", step: 0.1 },
];

export function EvolutionSection({ cfg, patch, help }: SectionProps) {
  const ev = at<Evolution>(cfg, "evolution") ?? {};
  const on = ev.enabled ?? false;
  const baseline = ev.baseline ?? {};
  const kinds = ev.allowed_kinds ?? [];

  const setEv = (p: Partial<Evolution>) => patch(put(cfg, "evolution", { ...ev, ...p }));
  const setBaseline = (p: Partial<Baseline>) => setEv({ baseline: { ...baseline, ...p } });

  return (
    <Section k="evolution" help={help} defaultOpen={on}>
      <div className="space-y-3">
        <Toggle
          checked={on}
          onChange={(enabled) => setEv({ enabled })}
          label="Let this loop propose changes to itself"
          hint="Proposals only. loopsmith will not apply one — `run proposals` is how you read them."
        />

        {on && (
          <>
            <Note tone="warning">
              A proposal is measured against the baseline below. With no baseline recorded there is
              nothing to be better than, so every proposal looks like an improvement.
            </Note>

            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <Field label="Allowed regression" hint="How much worse a metric may get and still pass. 0.05 is five per cent.">
                {(id) => (
                  <Num id={id} min={0} step={0.01} value={ev.max_regression ?? 0.05}
                    onChange={(v) => setEv({ max_regression: v ?? 0 })} />
                )}
              </Field>
              <Field label="Measured at" hint="Optional. When these numbers were taken.">
                {(id) => (
                  <Text id={id} mono value={baseline.measured_at ?? ""}
                    onChange={(v) => setBaseline({ measured_at: v || null })}
                    placeholder="2026-09-01" />
                )}
              </Field>
              {BASELINE_FIELDS.map((f) => (
                <Field key={f.key} label={f.label} hint={f.hint}>
                  {(id) => (
                    <Num id={id} min={0} step={f.step} value={(baseline[f.key] as number | null) ?? null}
                      onChange={(v) => setBaseline({ [f.key]: v } as Partial<Baseline>)} />
                  )}
                </Field>
              ))}
            </div>

            <div className="space-y-2">
              <p className="label">What it may propose</p>
              <p className="hint">Anything not ticked is refused before it is evaluated.</p>
              {KINDS.map((k) => (
                <Toggle
                  key={k.value}
                  checked={kinds.includes(k.value)}
                  onChange={(tick) =>
                    setEv({ allowed_kinds: tick ? [...kinds, k.value] : kinds.filter((x) => x !== k.value) })
                  }
                  label={k.label}
                />
              ))}
            </div>

            <div className="space-y-2.5">
              <Toggle checked={ev.require_sandbox ?? true} onChange={(v) => setEv({ require_sandbox: v })}
                label="Trial a proposal in a sandbox first"
                hint="A proposal that has never run is a guess." />
              <Toggle checked={ev.require_approval ?? true} onChange={(v) => setEv({ require_approval: v })}
                label="A person signs off before anything changes" />
              <Toggle checked={ev.keep_rollback ?? true} onChange={(v) => setEv({ keep_rollback: v })}
                label="Keep a way back from every accepted change" />
            </div>
          </>
        )}
      </div>
    </Section>
  );
}
