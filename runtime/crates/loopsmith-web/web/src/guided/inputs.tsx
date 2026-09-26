/**
 * The controls a question can be asked with.
 *
 * Every answer is a string, because that is what the wizard's answer map holds
 * and what the server types on the way back — a number field that handed React
 * a `number` would have to hand the server a string anyway, and the conversion
 * is one more place to disagree about what an empty answer means.
 *
 * The terminal offers a numbered list and reads a line. The browser can do
 * better without doing something else: a short enum is a row of circles you can
 * see all of at once, and a long one collapses to a select. The hover layer is
 * the ported GlideMenu, so a row of options here behaves like every other
 * option list in the app.
 */
import GlideMenu from "../glide-menu";
import { Area, Num, Select, Text } from "../ui";
import type { Choice, Input } from "./wire";

/** Above this many options a list of circles stops being scannable. */
const CIRCLE_LIMIT = 6;

function Marker({ on }: { on: boolean }) {
  return (
    <span
      className={`flex size-4 shrink-0 items-center justify-center rounded-full transition-colors duration-200
        ${on ? "bg-ember text-on-ember" : "shadow-[inset_0_0_0_1.5px_var(--line-strong)] text-transparent"}`}
    >
      <span
        className="size-1.5 rounded-full bg-on-ember transition-transform duration-200"
        style={{ transform: on ? "scale(1)" : "scale(0)" }}
      />
    </span>
  );
}

/** A list of selectable rows, one answer at a time. */
export function OptionRows({
  options, value, onChange, name,
}: {
  options: Choice[];
  value: string;
  onChange: (v: string) => void;
  name: string;
}) {
  return (
    <GlideMenu className="flex flex-col gap-0.5" highlightClassName="inset-x-0 rounded-[10px] bg-raised">
      {options.map((o) => {
        const on = o.value === value;
        return (
          <button
            key={o.value || "(blank)"}
            type="button"
            data-menu-row
            role="radio"
            aria-checked={on}
            aria-label={o.label}
            name={name}
            onClick={() => onChange(o.value)}
            className="relative z-10 flex items-start gap-2 rounded-[10px] px-1.5 py-1.5 text-left transition-colors duration-100"
          >
            <span className="mt-0.5"><Marker on={on} /></span>
            <span className="min-w-0 flex-1">
              <span className={`block text-[13px] leading-snug transition-colors duration-200 ${on ? "text-text" : "text-dim"}`}>
                {o.label}
              </span>
              {o.note && <span className="mt-0.5 block text-[11.5px] text-faint">{o.note}</span>}
            </span>
          </button>
        );
      })}
    </GlideMenu>
  );
}

/**
 * Render whichever control this field's `Input` calls for.
 *
 * `choices` is passed in rather than read off the input, because a select
 * whose options come from earlier answers is resolved by the server and the
 * two kinds must render identically once they arrive.
 */
export function FieldInput({
  input, value, onChange, id, invalid, choices,
}: {
  input: Input;
  value: string;
  onChange: (v: string) => void;
  id: string;
  invalid?: boolean;
  choices: Choice[];
}) {
  switch (input.kind) {
    case "text":
      return (
        <Text
          id={id}
          value={value}
          onChange={onChange}
          placeholder={input.placeholder ?? undefined}
          mono={input.mono}
          invalid={invalid}
        />
      );

    case "area":
      return (
        <Area
          id={id}
          value={value}
          onChange={onChange}
          placeholder={input.placeholder ?? undefined}
          rows={input.rows}
        />
      );

    case "items":
      // Several values in one answer. The separator is the spec's, so what is
      // typed here splits the same way the server will split it.
      return (
        <Text
          id={id}
          value={value}
          onChange={onChange}
          placeholder={input.placeholder ?? undefined}
          mono={input.mono}
          invalid={invalid}
        />
      );

    case "number":
      return (
        <Num
          id={id}
          value={value.trim() === "" ? null : Number(value)}
          onChange={(n) => onChange(n === null ? "" : String(n))}
          min={input.min ?? undefined}
          step={input.step ?? undefined}
          suffix={input.suffix ?? undefined}
        />
      );

    case "bool":
      return (
        <OptionRows
          name={id}
          value={value === "false" ? "false" : "true"}
          onChange={onChange}
          options={[
            { value: "true", label: input.true_label ?? "Yes" },
            { value: "false", label: input.false_label ?? "No" },
          ]}
        />
      );

    case "select":
      // A long list of options is a dropdown; a short one is worth seeing whole.
      return choices.length > CIRCLE_LIMIT ? (
        <Select
          id={id}
          value={value}
          onChange={onChange}
          options={choices.map((o) => ({ value: o.value, label: o.label }))}
        />
      ) : (
        <OptionRows name={id} value={value} onChange={onChange} options={choices} />
      );
  }
}
