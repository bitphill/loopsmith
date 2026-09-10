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
 * Anything not on this machine is still reachable through the expert editor;
 * this step deliberately offers what was found.
 */
import { Icon, Note, Select } from "../ui";
import GlideMenu from "../glide-menu";
import type { Agent, Detection, LoopConfig, ProviderSpec, Tier } from "../types";

/** A detected CLI, as the config would record it. */
function toSpec(a: Agent): ProviderSpec {
  return {
    id: a.id,
    kind: a.kind,
    tiers: a.tiers as Tier[],
    command: a.command,
    args: a.args,
    model: a.models[0] ?? null,
    requires_env: a.requires_env,
    timeout_seconds: null,
    prompt_on_stdin: a.prompt_on_stdin,
    usage_regex: null,
    cost_per_1k_tokens: a.cost_per_1k,
  };
}

export function ProvidersStepBody({
  cfg, patch, detection, scanning, onRescan, onTest, testing, results,
}: {
  cfg: LoopConfig;
  patch: (p: Partial<LoopConfig>) => void;
  detection: Detection | null;
  scanning: boolean;
  onRescan: (deep: boolean) => void;
  onTest: (p: ProviderSpec) => void;
  testing: string | null;
  results: Record<string, { ok: boolean; text: string }>;
}) {
  const chosen = cfg.providers?.providers ?? [];
  const agents = detection?.agents ?? [];

  const setProviders = (next: ProviderSpec[]) =>
    patch({ providers: { ...(cfg.providers ?? {}), providers: next } });

  const toggle = (a: Agent) => {
    const on = chosen.some((p) => p.id === a.id);
    setProviders(on ? chosen.filter((p) => p.id !== a.id) : [...chosen, toSpec(a)]);
  };

  const setModel = (id: string, model: string) =>
    setProviders(chosen.map((p) => (p.id === id ? { ...p, model: model || null } : p)));

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
          const on = chosen.some((p) => p.id === a.id);
          const spec = chosen.find((p) => p.id === a.id);
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

              {on && spec && (
                <div className="ml-7 mb-1.5 space-y-2 border-l pl-3">
                  {a.models.length > 0 && (
                    <label className="block">
                      <span className="hint">Model</span>
                      <div className="mt-1">
                        <Select
                          value={spec.model ?? ""}
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
                      onClick={() => onTest(spec)}
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
