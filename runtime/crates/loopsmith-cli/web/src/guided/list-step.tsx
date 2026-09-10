/**
 * A repeating section, asked as one card.
 *
 * The terminal shows an Add / Edit / Remove / Done menu and loops until you
 * pick Done. The browser can show the list and the entry being edited at the
 * same time, so this is that menu flattened: filled entries collapse to a
 * summary row, one entry is open for editing, `+` opens another, and the step's
 * primary button is "This part is done".
 *
 * Entries are held in the real config as you type — there is no separate draft
 * to lose, and the review rail reacts to a half-typed goal the same way it
 * reacts to a finished one.
 */
import { useState } from "react";
import { Field, Icon, Note } from "../ui";
import { FieldInput } from "./inputs";
import { resolveInput, type AnyListStep, type EntryField } from "./spec-types";
import type { LoopConfig } from "../types";

export function ListStepBody({
  step, cfg, items, onChange,
}: {
  step: AnyListStep;
  cfg: LoopConfig;
  items: unknown[];
  onChange: (items: unknown[]) => void;
}) {
  // Nothing yet means the first entry is already open: an empty card with a
  // lone `+` makes you click once before you can start typing.
  const [open, setOpen] = useState<number>(items.length === 0 ? 0 : items.length - 1);

  const list = items.length === 0 ? [step.blank()] : items;
  const commit = (next: unknown[]) => onChange(next);

  const setEntry = (i: number, e: unknown) => commit(list.map((x, j) => (i === j ? e : x)));

  const add = () => {
    const next = [...list, step.blank()];
    commit(next);
    setOpen(next.length - 1);
  };

  const remove = (i: number) => {
    const next = list.filter((_, j) => j !== i);
    commit(next);
    setOpen((o) => Math.max(0, Math.min(o, next.length - 1)));
  };

  const visibleFields = (entry: unknown): EntryField<unknown>[] =>
    step.fields.filter((f: EntryField<unknown>) => !f.when || f.when(entry));

  return (
    <div className="space-y-2">
      {list.length < step.min && (
        <Note tone="warning">
          Add at least {step.min} {step.singular}{step.min === 1 ? "" : "s"} to continue.
        </Note>
      )}

      {list.map((entry, i) => {
        const isOpen = i === open;
        return (
          <div key={i} className={`rounded-[10px] border ${isOpen ? "bg-surface" : "bg-raised"}`}>
            <div className="flex items-center gap-2 px-2.5 py-2">
              <span className="font-mono text-[11px] text-faint">{i + 1}</span>
              <button
                type="button"
                className="min-w-0 flex-1 text-left text-[12.5px] text-text"
                aria-expanded={isOpen}
                onClick={() => setOpen(isOpen ? -1 : i)}
              >
                {step.describe(entry)}
              </button>
              <span
                className="text-faint transition-transform duration-150"
                style={{ transform: isOpen ? "none" : "rotate(-90deg)" }}
                aria-hidden="true"
              >
                {Icon.chevron({ size: 14 })}
              </span>
              {list.length > 1 && (
                <button
                  type="button"
                  className="btn btn-ghost btn-sm btn-danger btn-icon"
                  aria-label={`Remove ${step.singular} ${i + 1}`}
                  onClick={() => remove(i)}
                >
                  {Icon.trash({ size: 13 })}
                </button>
              )}
            </div>

            {isOpen && (
              <div className="space-y-3 border-t px-2.5 py-3">
                {visibleFields(entry).map((f) => {
                  const value = f.get(entry);
                  const problem = f.validate ? f.validate(value) : null;
                  return (
                    <Field
                      key={f.id}
                      label={f.label}
                      hint={f.hint}
                      required={f.required}
                      error={problem ?? undefined}
                    >
                      {(id) => (
                        <FieldInput
                          id={id}
                          input={resolveInput(f.input, cfg)}
                          value={value}
                          invalid={!!problem}
                          onChange={(v) => setEntry(i, f.set(entry, v))}
                        />
                      )}
                    </Field>
                  );
                })}
              </div>
            )}
          </div>
        );
      })}

      <button type="button" className="btn btn-sm w-full" onClick={add}>
        {Icon.plus({ size: 14 })} Add another {step.singular}
      </button>
    </div>
  );
}
