/**
 * The providers step.
 *
 * The terminal has to scan `PATH` and print a numbered menu. The browser has
 * already scanned asynchronously before this step is reached, so the honest
 * shape here is a multi-select of what is actually installed rather than a
 * question about what might be: tick the CLIs this loop may spend, pick a model
 * where the CLI offers a list, and press Test to prove one answers before a
 * long run depends on it.
 *
 * Like every other step, what it writes is answers — `providers[i].id`,
 * `providers[i].command` and the rest — under the same keys the terminal uses.
 * The tier cascade is *not* written here: neither front end asks about it, so
 * the converter derives it from the ids that were picked.
 *
 * Anything not on this machine is still reachable through the expert editor;
 * this step deliberately offers what was found.
 */
import { Icon, Note, Select } from "../ui";
import GlideMenu from "../glide-menu";
import { entryCount, type Answers } from "./wire";
import type { Agent, Detection, ProviderSpec, Tier } from "../types";

/**
 * The answers one detected CLI becomes.
 *
 * Strings, joined the way `loopsmith_wizard::answers::provider_value` will
 * split them back — spaces for argv, commas for the name lists. That
 * agreement is a wire format, and `the_provider_wire_format_round_trips_the_
 * catalog` is what holds the two ends to it.
 */
function fields(a: Agent): Record<string, string> {
  return {
    id: a.id,
    kind: a.kind,
    command: a.command,
    args: a.args.join(" "),
    tiers: a.tiers.join(", "),
    requires_env: a.requires_env.join(", "),
    prompt_on_stdin: String(a.prompt_on_stdin),
    ...(a.models[0] ? { model: a.models[0] } : {}),
    ...(a.cost_per_1k != null ? { cost_per_1k_tokens: String(a.cost_per_1k) } : {}),
  };
}

/** What the Test button needs, rebuilt from the answers that were written. */
function toSpec(a: Agent, model: string): ProviderSpec {
  return {
    id: a.id,
    kind: a.kind,
    tiers: a.tiers as Tier[],
    command: a.command,
    args: a.args,
    model: model || null,
    requires_env: a.requires_env,
    timeout_seconds: null,
    prompt_on_stdin: a.prompt_on_stdin,
    usage_regex: null,
    cost_per_1k_tokens: a.cost_per_1k,
  };
}

export function ProvidersStepBody({
  path, answers, setAnswers, detection, scanning, onRescan, onTest, testing, results,
}: {
  /** The list's answer path, from the spec. */
  path: string;
  answers: Answers;
  setAnswers: (next: Answers) => void;
  detection: Detection | null;
  scanning: boolean;
  onRescan: (deep: boolean) => void;
  onTest: (p: ProviderSpec) => void;
  testing: string | null;
  results: Record<string, { ok: boolean; text: string }>;
}) {
  const agents = detection?.agents ?? [];
  const count = entryCount(path, answers);

  /** The ids currently in the answer map, in order. */
  const chosen: string[] = Array.from({ length: count }, (_, i) => answers[`${path}[${i}].id`] ?? "");

  const write = (ids: string[], extra?: (id: string) => Record<string, string>) => {
    // Rewritten whole, so indices stay contiguous — the converter counts
    // entries by the highest index it sees, and a hole would resurrect a
    // provider that was unticked as an empty one.
    const next: Answers = {};
    for (const [k, v] of Object.entries(answers)) {
      if (!k.startsWith(`${path}[`)) next[k] = v;
    }
    ids.forEach((id, i) => {
      const agent = agents.find((a) => a.id === id);
      const values = extra?.(id) ?? (agent ? fields(agent) : { id });
      for (const [leaf, v] of Object.entries(values)) {
        if (v !== "") next[`${path}[${i}].${leaf}`] = v;
      }
    });
    setAnswers(next);
  };

  const toggle = (a: Agent) => {
    const on = chosen.includes(a.id);
    const ids = on ? chosen.filter((x) => x !== a.id) : [...chosen, a.id];
    // Keep what is already answered for the ones that stay, so a model picked
    // by hand survives someone ticking a second CLI.
    const existing = new Map(
      chosen.map((id, i) => {
        const prefix = `${path}[${i}].`;
        const held: Record<string, string> = {};
        for (const [k, v] of Object.entries(answers)) {
          if (k.startsWith(prefix)) held[k.slice(prefix.length)] = v;
        }
        return [id, held];
      }),
    );
    write(ids, (id) => existing.get(id) ?? fields(agents.find((x) => x.id === id) ?? a));
  };

  const modelOf = (id: string) => {
    const i = chosen.indexOf(id);
    return i < 0 ? "" : answers[`${path}[${i}].model`] ?? "";
  };

  const setModel = (id: string, model: string) => {
    const i = chosen.indexOf(id);
    if (i < 0) return;
    const next = { ...answers };
    const key = `${path}[${i}].model`;
    if (model) next[key] = model;
    else delete next[key];
    setAnswers(next);
  };

  if (scanning) {
    return <p className="hint">Reading this machine…</p>;
  }

  if (agents.length === 0) {
    return (
      <div className="space-y-3">
        <Note tone="warning">
          None of the known agent CLIs were found on PATH. You can still add one by hand in the
          expert editor, or install one and rescan.
        </Note>
        <button type="button" className="btn btn-sm" onClick={() => onRescan(true)}>
          {Icon.refresh({ size: 13 })} Scan again
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <GlideMenu className="flex flex-col gap-0.5" highlightClassName="inset-x-0 rounded-[10px] bg-raised">
        {agents.map((a) => {
          const on = chosen.includes(a.id);
          const result = results[a.id];
          return (
            <div key={a.id} className="relative z-10">
              <button
                type="button"
                data-menu-row
                role="checkbox"
                aria-checked={on}
                aria-label={a.label}
                onClick={() => toggle(a)}
                className="flex w-full items-start gap-2 rounded-[10px] px-1.5 py-1.5 text-left transition-colors duration-100"
              >
                <span
                  className={`mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-[5px] transition-colors duration-200
                    ${on ? "bg-ember text-on-ember" : "shadow-[inset_0_0_0_1.5px_var(--line-strong)] text-transparent"}`}
                >
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                    strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                    <path d="M20 6L9 17l-5-5" />
                  </svg>
                </span>
                <span className="min-w-0 flex-1">
                  <span className={`block text-[13px] leading-snug ${on ? "text-text" : "text-dim"}`}>
                    {a.label}
                    {a.version && <span className="ml-1.5 font-mono text-[11px] text-faint">{a.version}</span>}
                  </span>
                  <span className="mt-0.5 block truncate font-mono text-[11px] text-faint">{a.path}</span>
                  {!a.env_ready && a.missing_env.length > 0 && (
                    <span className="mt-0.5 block text-[11.5px] text-warn">
                      needs {a.missing_env.join(", ")}
                    </span>
                  )}
                </span>
                {a.confidence === "template" && <span className="chip">template</span>}
              </button>

              {on && (
                <div className="ml-7 mb-1.5 space-y-2 border-l pl-3">
                  {a.models.length > 0 && (
                    <label className="block">
                      <span className="hint">Model</span>
                      <div className="mt-1">
                        <Select
                          value={modelOf(a.id)}
                          onChange={(v) => setModel(a.id, v)}
                          options={[
                            { value: "", label: "(the CLI's own default)" },
                            ...a.models.map((m) => ({ value: m, label: m })),
                          ]}
                        />
                      </div>
                    </label>
                  )}
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      className="btn btn-sm"
                      disabled={testing === a.id}
                      onClick={() => onTest(toSpec(a, modelOf(a.id)))}
                    >
                      {testing === a.id ? "Testing…" : "Test"}
                    </button>
                    {result && (
                      <span className={`text-[11.5px] ${result.ok ? "text-good" : "text-bad"}`}>
                        {result.text}
                      </span>
                    )}
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </GlideMenu>

      <div className="flex items-center gap-2 pt-1">
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => onRescan(true)}>
          {Icon.refresh({ size: 13 })} Scan again
        </button>
        <span className="hint">{chosen.length} selected</span>
      </div>
    </div>
  );
}
