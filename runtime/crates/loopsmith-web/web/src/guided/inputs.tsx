/**
 * The controls a guided step can ask with.
 *
 * The terminal offers a numbered list and reads a line. The browser can do
 * better without doing something else: a short enum is a row of circles you can
 * see all of at once, a long one collapses to a select, and a multi-answer
 * question uses squares so the shape of the control says how many answers it
 * takes. The hover layer is the ported GlideMenu, so a row of options here
 * behaves like every other option list in the app.
 */
import GlideMenu from "../glide-menu";
import { Area, Num, Select, Text } from "../ui";
import type { Choice, Input, Value } from "./spec-types";

/** Above this many options a list of circles stops being scannable. */
const CIRCLE_LIMIT = 6;

function Marker({ on, shape }: { on: boolean; shape: "circle" | "square" }) {
  return (
    <span
      className={`flex size-4 shrink-0 items-center justify-center transition-colors duration-200
        ${shape === "circle" ? "rounded-full" : "rounded-[5px]"}
        ${on ? "bg-ember text-on-ember" : "shadow-[inset_0_0_0_1.5px_var(--line-strong)] text-transparent"}`}
    >
      {shape === "circle" ? (
        <span
          className="size-1.5 rounded-full bg-on-ember transition-transform duration-200"
          style={{ transform: on ? "scale(1)" : "scale(0)" }}
        />
      ) : (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor"
          strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M20 6L9 17l-5-5" />
        </svg>
      )}
    </span>
  );
}

/**
 * A list of selectable rows. `multi` swaps the circles for squares and lets
 * more than one be on at a time.
 */
export function OptionRows({
  options, value, onChange, multi = false, name,
}: {
  options: Choice[];
  /** A single value, or the list of chosen values when `multi`. */
  value: string | string[];
  onChange: (v: string | string[]) => void;
  multi?: boolean;
  name: string;
}) {
  const chosen = Array.isArray(value) ? value : [value];
  const shape = multi ? "square" : "circle";

  const toggle = (v: string) => {
    if (!multi) return onChange(v);
    onChange(chosen.includes(v) ? chosen.filter((x) => x !== v) : [...chosen, v]);
  };

  return (
    <GlideMenu className="flex flex-col gap-0.5" highlightClassName="inset-x-0 rounded-[10px] bg-raised">
      {options.map((o) => {
        const on = chosen.includes(o.value);
        return (
          <button
            key={o.value}
            type="button"
            data-menu-row
            role={multi ? "checkbox" : "radio"}
            aria-checked={on}
            aria-label={o.label}
            name={name}
            onClick={() => toggle(o.value)}
            className="relative z-10 flex items-start gap-2 rounded-[10px] px-1.5 py-1.5 text-left transition-colors duration-100"
          >
            <span className="mt-0.5"><Marker on={on} shape={shape} /></span>
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

/** Render whichever control this field's `Input` calls for. */
export function FieldInput({
  input, value, onChange, id, invalid,
}: {
  input: Input;
  value: Value;
  onChange: (v: Value) => void;
  id: string;
  invalid?: boolean;
}) {
  switch (input.kind) {
    case "text":
      return (
        <Text
          id={id}
          value={String(value ?? "")}
          onChange={onChange}
          placeholder={input.placeholder}
          mono={input.mono}
          invalid={invalid}
        />
      );

    case "area":
      return (
        <Area
          id={id}
          value={String(value ?? "")}
          onChange={onChange}
          placeholder={input.placeholder}
          rows={input.rows ?? 3}
        />
      );

    case "number":
      return (
        <Num
          id={id}
          value={value === "" || value == null ? null : Number(value)}
          onChange={(n) => onChange(n === null ? "" : String(n))}
          min={input.min}
          step={input.step}
          suffix={input.suffix}
        />
      );

    case "bool":
      return (
        <OptionRows
          name={id}
          value={value ? "yes" : "no"}
          onChange={(v) => onChange(v === "yes")}
          options={[
            { value: "yes", label: input.trueLabel ?? "Yes" },
            { value: "no", label: input.falseLabel ?? "No" },
          ]}
        />
      );

    case "select":
      // A long list of options is a dropdown; a short one is worth seeing whole.
      return input.options.length > CIRCLE_LIMIT ? (
        <Select
          id={id}
          value={String(value ?? "")}
          onChange={onChange}
          options={input.options.map((o) => ({ value: o.value, label: o.label }))}
        />
      ) : (
        <OptionRows
          name={id}
          value={String(value ?? "")}
          onChange={onChange}
          options={input.options}
        />
      );

    case "multi":
      return (
        <OptionRows
          name={id}
          multi
          value={Array.isArray(value) ? value : []}
          onChange={onChange}
          options={input.options}
        />
      );
  }
}
