/**
 * The primitives every section is built from.
 *
 * The important one is `Field`. Every input in this app is wrapped in it, and
 * it is what carries the two-tier explanation: a hint that sits under the
 * control permanently, and a longer "why this exists" note behind an info
 * control. A form with thirty inputs and no explanation is not easier than a
 * YAML file — it is the same difficulty with worse ergonomics.
 */
import {
  createContext, useCallback, useContext, useEffect, useId, useRef, useState,
  type ReactNode,
} from "react";
import type { FieldHelp } from "./types";

/* --- help lookup --------------------------------------------------------- */

const HelpContext = createContext<Map<string, FieldHelp>>(new Map());

export function HelpProvider({ fields, children }: { fields: FieldHelp[]; children: ReactNode }) {
  const map = new Map(fields.map((f) => [f.path, f]));
  return <HelpContext.Provider value={map}>{children}</HelpContext.Provider>;
}

export const useFieldHelp = (path?: string) => {
  const map = useContext(HelpContext);
  return path ? map.get(path) : undefined;
};

/* --- icons ---------------------------------------------------------------
   Drawn rather than pulled from a library: this app needs nine glyphs, and a
   stroke-consistent set on a 24px grid is a smaller thing to own than a
   dependency. All share stroke-width 1.75 and inherit currentColor.
   ------------------------------------------------------------------------- */

type IconProps = { size?: number; className?: string };
const svg = (path: ReactNode, { size = 16, className = "" }: IconProps) => (
  <svg
    width={size} height={size} viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth={1.75} strokeLinecap="round"
    strokeLinejoin="round" className={className} aria-hidden="true"
  >
    {path}
  </svg>
);

export const Icon = {
  info: (p: IconProps = {}) => svg(<><circle cx="12" cy="12" r="9" /><path d="M12 16v-4M12 8h.01" /></>, p),
  plus: (p: IconProps = {}) => svg(<path d="M12 5v14M5 12h14" />, p),
  trash: (p: IconProps = {}) => svg(<><path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6" /></>, p),
  check: (p: IconProps = {}) => svg(<path d="M20 6 9 17l-5-5" />, p),
  x: (p: IconProps = {}) => svg(<path d="M18 6 6 18M6 6l12 12" />, p),
  chevron: (p: IconProps = {}) => svg(<path d="m6 9 6 6 6-6" />, p),
  play: (p: IconProps = {}) => svg(<path d="M6 4l14 8-14 8V4z" />, p),
  refresh: (p: IconProps = {}) => svg(<><path d="M21 12a9 9 0 1 1-3-6.7" /><path d="M21 4v5h-5" /></>, p),
  sun: (p: IconProps = {}) => svg(<><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></>, p),
  moon: (p: IconProps = {}) => svg(<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />, p),
  eye: (p: IconProps = {}) => svg(<><path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7z" /><circle cx="12" cy="12" r="3" /></>, p),
  eyeOff: (p: IconProps = {}) => svg(<><path d="M10.6 5.2A9.9 9.9 0 0 1 12 5c6.4 0 10 7 10 7a17.7 17.7 0 0 1-3.2 4.1M6.6 6.6A17.6 17.6 0 0 0 2 12s3.6 7 10 7a9.8 9.8 0 0 0 4-.8" /><path d="m2 2 20 20M9.9 9.9a3 3 0 0 0 4.2 4.2" /></>, p),
  folder: (p: IconProps = {}) => svg(<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />, p),
  bolt: (p: IconProps = {}) => svg(<path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z" />, p),
  shield: (p: IconProps = {}) => svg(<path d="M12 3l8 3v6c0 5-3.4 8.4-8 9-4.6-.6-8-4-8-9V6z" />, p),
  clock: (p: IconProps = {}) => svg(<><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></>, p),
  terminal: (p: IconProps = {}) => svg(<><path d="m4 17 6-5-6-5M12 19h8" /></>, p),
  download: (p: IconProps = {}) => svg(<><path d="M12 3v12M7 11l5 5 5-5M4 21h16" /></>, p),
  stop: (p: IconProps = {}) => svg(<rect x="6" y="6" width="12" height="12" rx="2" />, p),
  target: (p: IconProps = {}) => svg(<><circle cx="12" cy="12" r="8" /><circle cx="12" cy="12" r="3.5" /></>, p),
  graph: (p: IconProps = {}) => svg(<><circle cx="5" cy="6" r="2.2" /><circle cx="5" cy="18" r="2.2" /><circle cx="18" cy="12" r="2.2" /><path d="M7 7.2 15.9 11M7 16.8 15.9 13" /></>, p),
  command: (p: IconProps = {}) => svg(<path d="M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z" />, p),
  panel: (p: IconProps = {}) => svg(<><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M15 4v16" /></>, p),
};

/* --- info popover -------------------------------------------------------- */

export function Info({ title, children }: { title: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  return (
    <span ref={ref} className="relative inline-flex">
      <button
        type="button"
        className="btn-ghost inline-flex items-center justify-center rounded-full p-0.5 text-faint hover:text-ember"
        aria-label={`What is ${title}?`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {Icon.info({ size: 14 })}
      </button>
      {open && (
        <span
          role="tooltip"
          className="card rise absolute left-0 top-6 z-40 w-[22rem] max-w-[80vw] p-3 text-left"
        >
          <span className="mb-1 block text-[12.5px] font-semibold">{title}</span>
          <span className="hint block whitespace-pre-line">{children}</span>
        </span>
      )}
    </span>
  );
}

/* --- field wrapper ------------------------------------------------------- */

export function Field({
  label, hint, helpPath, required, error, children, wide,
}: {
  label: string;
  hint?: string;
  /** Dotted config path; pulls the long note out of the server's catalogue. */
  helpPath?: string;
  required?: boolean;
  error?: string;
  children: (id: string) => ReactNode;
  wide?: boolean;
}) {
  const id = useId();
  const h = useFieldHelp(helpPath);
  const shownHint = hint ?? h?.hint;

  return (
    <div className={wide ? "col-span-full" : ""}>
      <div className="mb-1 flex items-center gap-1.5">
        {/* The asterisk sits OUTSIDE the label element on purpose. Inside, it
            becomes part of the label's text content and therefore part of the
            field's accessible name — every required field would announce as
            "Loop name star", and no assistive tech or voice-control user would
            find it by the name printed next to it. */}
        <label htmlFor={id} className="label">{label}</label>
        {required && (
          <span className="text-ember" aria-hidden="true" title="required">*</span>
        )}
        {h && (
          <Info title={h.label}>
            {h.detail}
            {"\n\nFor example: "}
            {h.example}
          </Info>
        )}
      </div>
      {children(id)}
      {error ? (
        <p className="hint mt-1 text-bad">{error}</p>
      ) : shownHint ? (
        <p className="hint mt-1">{shownHint}</p>
      ) : null}
    </div>
  );
}

/* --- inputs -------------------------------------------------------------- */

type TextProps = {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  mono?: boolean;
  invalid?: boolean;
  id?: string;
};

export const Text = ({ value, onChange, placeholder, mono, invalid, id }: TextProps) => (
  <input
    id={id}
    className={`input ${mono ? "mono" : ""} ${invalid ? "invalid" : ""}`}
    value={value}
    placeholder={placeholder}
    onChange={(e) => onChange(e.target.value)}
  />
);

export const Area = ({
  value, onChange, placeholder, rows = 3, mono, id,
}: TextProps & { rows?: number }) => (
  <textarea
    id={id}
    className={`textarea ${mono ? "mono" : ""}`}
    rows={rows}
    value={value}
    placeholder={placeholder}
    onChange={(e) => onChange(e.target.value)}
  />
);

export function Num({
  value, onChange, placeholder, min, step = 1, id, suffix,
}: {
  value: number | null | undefined;
  onChange: (v: number | null) => void;
  placeholder?: string;
  min?: number;
  step?: number;
  id?: string;
  suffix?: string;
}) {
  return (
    <div className="relative">
      <input
        id={id}
        type="number"
        className="input tabular"
        value={value ?? ""}
        min={min}
        step={step}
        placeholder={placeholder}
        // Empty means "not set", which for an optional ceiling is a different
        // thing from zero. Coercing it to 0 would silently disable a gate.
        onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
      />
      {suffix && (
        <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[11px] text-faint">
          {suffix}
        </span>
      )}
    </div>
  );
}

export function Select<T extends string>({
  value, onChange, options, id,
}: {
  value: T;
  onChange: (v: T) => void;
  options: readonly { value: T; label: string }[];
  id?: string;
}) {
  return (
    <select id={id} className="select" value={value} onChange={(e) => onChange(e.target.value as T)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  );
}

export function Toggle({
  checked, onChange, label, hint,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  hint?: string;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2.5 select-none">
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className="relative mt-0.5 h-[20px] w-[34px] shrink-0 rounded-full border transition-colors"
        style={{
          background: checked ? "var(--ember)" : "var(--sunken)",
          borderColor: checked ? "var(--ember)" : "var(--line-strong)",
        }}
      >
        <span
          className="absolute top-[2px] h-[14px] w-[14px] rounded-full transition-[left] duration-150"
          style={{ left: checked ? 16 : 2, background: checked ? "var(--on-ember)" : "var(--text-faint)" }}
        />
      </button>
      <span className="min-w-0">
        <span className="label">{label}</span>
        {hint && <span className="hint mt-0.5 block">{hint}</span>}
      </span>
    </label>
  );
}

/** Comma-separated list editor. Right for short lists of names and paths. */
export function ListInput({
  value, onChange, placeholder, id, mono,
}: {
  value: string[] | undefined;
  onChange: (v: string[]) => void;
  placeholder?: string;
  id?: string;
  mono?: boolean;
}) {
  return (
    <Text
      id={id}
      mono={mono}
      value={(value ?? []).join(", ")}
      placeholder={placeholder}
      onChange={(v) =>
        onChange(v.split(",").map((s) => s.trim()).filter(Boolean))
      }
    />
  );
}

/* --- layout -------------------------------------------------------------- */

export function Card({
  title, letter, summary, detail, failure, required, children, actions, defaultOpen = true, count,
}: {
  title: string;
  letter?: string;
  summary?: string;
  detail?: string;
  failure?: string;
  required?: boolean;
  children: ReactNode;
  actions?: ReactNode;
  defaultOpen?: boolean;
  count?: number;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section className="card overflow-hidden" id={`section-${letter ?? title}`}>
      <header className="flex items-start gap-3 p-4">
        <button
          type="button"
          className="btn-ghost mt-0.5 shrink-0 rounded p-1"
          aria-expanded={open}
          aria-label={open ? `Collapse ${title}` : `Expand ${title}`}
          onClick={() => setOpen((v) => !v)}
        >
          <span className="block transition-transform duration-150" style={{ transform: open ? "none" : "rotate(-90deg)" }}>
            {Icon.chevron({ size: 16 })}
          </span>
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            {letter && letter.length === 1 && (
              <span className="chip chip-ember font-mono">{letter}</span>
            )}
            <h2 className="text-[15px] font-bold tracking-tight">{title}</h2>
            {required && <span className="chip chip-ember">required</span>}
            {typeof count === "number" && count > 0 && (
              <span className="chip tabular">{count}</span>
            )}
            {detail && (
              <Info title={title}>
                {detail}
                {failure ? `\n\nWhen this goes wrong: ${failure}` : ""}
              </Info>
            )}
          </div>
          {summary && <p className="hint mt-1">{summary}</p>}
        </div>
        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </header>
      {open && <div className="px-4 pb-4">{children}</div>}
    </section>
  );
}

/** A repeating list of sub-objects: goals, validations, nodes, and the rest. */
export function Repeater<T>({
  items, onChange, blank, render, addLabel, empty,
}: {
  items: T[] | undefined;
  onChange: (v: T[]) => void;
  blank: () => T;
  render: (item: T, set: (patch: Partial<T>) => void, index: number) => ReactNode;
  addLabel: string;
  empty?: string;
}) {
  const list = items ?? [];
  const set = useCallback(
    (i: number, patch: Partial<T>) =>
      onChange(list.map((it, j) => (i === j ? { ...it, ...patch } : it))),
    [list, onChange],
  );

  return (
    <div className="space-y-3">
      {list.length === 0 && empty && (
        <p className="hint stripe stripe-note py-1">{empty}</p>
      )}
      {list.map((item, i) => (
        <div key={i} className="rounded-[10px] border bg-raised p-3">
          <div className="mb-2.5 flex items-center justify-between">
            <span className="font-mono text-[11px] text-faint">{i + 1}</span>
            <button
              type="button"
              className="btn btn-ghost btn-sm btn-danger"
              onClick={() => onChange(list.filter((_, j) => j !== i))}
              aria-label={`Remove item ${i + 1}`}
            >
              {Icon.trash({ size: 13 })} Remove
            </button>
          </div>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            {render(item, (patch) => set(i, patch), i)}
          </div>
        </div>
      ))}
      <button type="button" className="btn btn-sm" onClick={() => onChange([...list, blank()])}>
        {Icon.plus({ size: 14 })} {addLabel}
      </button>
    </div>
  );
}

export function Dialog({
  title, children, onClose, actions, width = "32rem",
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  actions: ReactNode;
  width?: string;
}) {
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", esc);
    return () => document.removeEventListener("keydown", esc);
  }, [onClose]);

  return (
    <div className="dialog-backdrop grid place-items-center p-4" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="card rise w-full p-5"
        style={{ maxWidth: width }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <h3 className="text-[16px] font-bold tracking-tight">{title}</h3>
        <div className="mt-2.5">{children}</div>
        <div className="mt-5 flex flex-wrap justify-end gap-2">{actions}</div>
      </div>
    </div>
  );
}

export const Note = ({ tone = "note", children }: { tone?: "note" | "warning" | "error" | "ember"; children: ReactNode }) => (
  <p className={`hint stripe stripe-${tone} py-1`}>{children}</p>
);
