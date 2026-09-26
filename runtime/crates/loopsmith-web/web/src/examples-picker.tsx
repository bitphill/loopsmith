/**
 * Start from a working loop, or start empty.
 *
 * Shown once, after the explanation, to someone who has said they are new. The
 * fastest way to understand the config is to read one that already works, so
 * the examples come first and "I will make my own" is the deliberate opt-out
 * rather than the default path.
 *
 * Whichever is chosen, the guided wizard walks every step afterwards: an
 * example pre-fills the answers, it does not skip the questions.
 */
import { useState, type CSSProperties } from "react";
import GlideMenu from "./glide-menu";
import { Icon } from "./ui";
import type { ExampleCard } from "./types";

/** The sentinel for "start from nothing". */
export const OWN = "__own__";

export function ExamplesPicker({
  examples, onGo, loading,
}: {
  examples: ExampleCard[];
  /** `null` when the user chose to start empty. */
  onGo: (exampleId: string | null) => void;
  loading: boolean;
}) {
  const [picked, setPicked] = useState<string>(OWN);

  const rows: { value: string; label: string; note: string }[] = [
    { value: OWN, label: "I will make my own", note: "Start from an empty config and answer every question." },
    ...examples.map((e) => ({ value: e.id, label: e.name, note: e.blurb })),
  ];

  return (
    <div className="dialog-backdrop grid place-items-center p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Load an existing loop"
        className="card card-raised rise flex max-h-[85vh] w-full max-w-[36rem] flex-col overflow-hidden"
        style={{ "--card-spacing": "18px" } as CSSProperties}
      >
        <div className="card-header">
          <h2 className="card-title text-[17px]">Load an existing loop and modify as you need</h2>
          <p className="card-description">
            Every example is a loop that runs. Pick one to fill in the answers, or start from nothing.
          </p>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-2.5">
          <GlideMenu className="flex flex-col gap-0.5" highlightClassName="inset-x-0 rounded-[10px] bg-raised">
            {rows.map((r) => {
              const on = picked === r.value;
              return (
                <button
                  key={r.value}
                  type="button"
                  data-menu-row
                  role="radio"
                  aria-checked={on}
                  aria-label={r.label}
                  onClick={() => setPicked(r.value)}
                  className="relative z-10 flex items-start gap-2.5 rounded-[10px] px-2 py-2 text-left transition-colors duration-100"
                >
                  <span
                    className={`mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full transition-colors duration-200
                      ${on ? "bg-ember" : "shadow-[inset_0_0_0_1.5px_var(--line-strong)]"}`}
                  >
                    <span
                      className="size-1.5 rounded-full bg-on-ember transition-transform duration-200"
                      style={{ transform: on ? "scale(1)" : "scale(0)" }}
                    />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className={`block text-[13px] font-medium leading-snug ${on ? "text-text" : "text-dim"}`}>
                      {r.label}
                    </span>
                    <span className="mt-0.5 block text-[11.5px] leading-relaxed text-faint">{r.note}</span>
                  </span>
                </button>
              );
            })}
          </GlideMenu>
        </div>

        <div className="card-footer justify-end">
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={loading}
            onClick={() => onGo(picked === OWN ? null : picked)}
          >
            {Icon.bolt({ size: 13 })} {loading ? "Loading…" : "Let's hammer"}
          </button>
        </div>
      </div>
    </div>
  );
}
