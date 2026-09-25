/**
 * A repeating section, asked as one card.
 *
 * The terminal shows an Add / Edit / Remove / Done menu and loops until you
 * pick Done. The browser can show the list and the entry being edited at the
 * same time, so this is that menu flattened: filled entries collapse to a
 * summary row, one entry is open for editing, `+` opens another, and the step's
 * primary button is "This part is done".
 *
 * Entries live in the answer map under `path[i].field`, which is the same key
 * the terminal writes, so a draft is portable between the two front ends and a
 * removal has to close the index gap behind it — the converter counts entries
 * by the highest index it sees.
 */
import { useEffect, useState } from "react";
import { Field as FieldRow, Icon, Note } from "../ui";
import { FieldInput } from "./inputs";
import {
  choicesFor, describeEntry, entryCount, removeEntry, valueOf,
  type Answers, type Choice, type ListStep,
} from "./wire";

export function ListStepBody({
  step, answers, setAnswers, visible, options, issues,
}: {
  step: ListStep;
  answers: Answers;
  setAnswers: (next: Answers) => void;
  /** The answer keys the server says are being asked right now. */
  visible: Set<string>;
  options: Record<string, Choice[]>;
  issues: Map<string, string>;
}) {
  const count = entryCount(step.id, answers);
  // Nothing yet means the first entry is already open: an empty card with a
  // lone `+` makes you click once before you can start typing.
  const [open, setOpen] = useState<number>(Math.max(0, count - 1));

  // An entry exists when it has answers, so a brand-new one has nothing to
  // count. `open` past the end is what holds its row on screen until the
  // first field is filled in.
  const shown = Math.max(count, open + 1, 1);

  const set = (key: string, value: string) => {
    const next = { ...answers };
    if (value.trim() === "") delete next[key];
    else next[key] = value;
    setAnswers(next);
  };

  const add = () => setOpen(count);

  /**
   * Give a brand-new entry its defaults.
   *
   * An entry is whatever keys exist under its index, so an untouched one does
   * not exist at all — and a server that cannot see it cannot say which of its
   * fields apply. Writing the defaults is what brings it into being, and it is
   * the same thing the terminal does by offering them at the prompt.
   */
  useEffect(() => {
    if (open < 0 || open < count) return;
    const seeded: Answers = {};
    for (const f of step.fields) {
      if (f.when || !f.default) continue;
      seeded[`${step.id}[${open}].${f.id}`] = f.default;
    }
    if (Object.keys(seeded).length > 0) setAnswers({ ...answers, ...seeded });
    // `answers` is deliberately absent: this runs when a new index is opened,
    // not on every keystroke inside it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, count, step]);

  const remove = (i: number) => {
    setAnswers(removeEntry(step.id, i, answers));
    setOpen((o) => Math.max(0, Math.min(o, count - 2)));
  };

  return (
    <div className="space-y-2">
      {count < step.min && (
        <Note tone="warning">
          Add at least {step.min} {step.singular}{step.min === 1 ? "" : "s"} to continue.
        </Note>
      )}

      {Array.from({ length: shown }, (_, i) => {
        const isOpen = i === open;
        const prefix = `${step.id}[${i}]`;
        // A brand-new entry has no keys yet, so nothing is visible for it. Its
        // unconditional fields are the way in.
        const fields = step.fields.filter(
          (f) => !f.when || visible.has(`${prefix}.${f.id}`),
        );
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
                {describeEntry(step, i, answers)}
              </button>
              <span
                className="text-faint transition-transform duration-150"
                style={{ transform: isOpen ? "none" : "rotate(-90deg)" }}
                aria-hidden="true"
              >
                {Icon.chevron({ size: 14 })}
              </span>
              {count > 1 && (
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
                {fields.map((f) => {
                  const key = `${prefix}.${f.id}`;
                  const problem = issues.get(key);
                  return (
                    <FieldRow
                      key={f.id}
                      label={f.title}
                      hint={f.hint ?? undefined}
                      error={problem}
                    >
                      {(id) => (
                        <FieldInput
                          id={id}
                          input={f.input}
                          choices={choicesFor(f.input, key, options)}
                          value={valueOf(answers, key, f)}
                          invalid={!!problem}
                          onChange={(v) => set(key, v)}
                        />
                      )}
                    </FieldRow>
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
