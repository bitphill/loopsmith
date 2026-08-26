/**
 * Motion primitives.
 *
 * The UI was a wall: sixteen cards in one scroll, three columns, and nine
 * buttons all live at once. These are the pieces that turn it into one thing at
 * a time — the patterns the current crop of component libraries have converged
 * on, implemented here rather than pulled in:
 *
 * - a step bar whose active tab expands to a labelled pill, with the panel
 *   below morphing height and sliding in the direction you moved
 * - a status pill that morphs between idle, running, and finished instead of a
 *   console column that is empty most of the time
 * - a ⌘K palette, which is what makes a deep form navigable without every
 *   section being on screen
 * - staggered reveals, a shimmer for indeterminate waits, rolling numbers, a
 *   skeleton that cross-fades to content, and a shake for a refused value
 *
 * Every one of them degrades to an instant, static equivalent under
 * `prefers-reduced-motion`. Motion here is for orientation — where did this
 * come from, where did it go — not decoration.
 */
import {
  AnimatePresence, motion, useReducedMotion, useMotionValue, useSpring, useTransform,
  type Transition,
} from "motion/react";
import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState,
  type ReactNode,
} from "react";

/** Weighted spring. Settles without the wobble that reads as a toy. */
export const SPRING: Transition = { type: "spring", stiffness: 420, damping: 38, mass: 0.9 };
/** Softer, for anything resizing — height changes are easy to overshoot. */
export const SPRING_SOFT: Transition = { type: "spring", stiffness: 260, damping: 32, mass: 1 };
/** The standard ease-out for opacity and small offsets. */
export const EASE: Transition = { duration: 0.34, ease: [0.16, 1, 0.3, 1] };

/* ------------------------------------------------------------------ reveal */

/**
 * Rise into place, offset per index.
 *
 * The stagger is what makes a step feel authored rather than dumped: the eye
 * gets a reading order for free. Capped, because past about a dozen items the
 * last one arrives late enough to feel broken rather than deliberate.
 */
export function Reveal({
  children, index = 0, className,
}: { children: ReactNode; index?: number; className?: string }) {
  const still = useReducedMotion();
  return (
    <motion.div
      className={className}
      initial={still ? false : { opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ ...EASE, delay: still ? 0 : Math.min(index, 12) * 0.035 }}
    >
      {children}
    </motion.div>
  );
}

/* ------------------------------------------------------- height-morph panel */

/**
 * One panel that morphs its height between views, sliding the outgoing view
 * out the way you came and the incoming view in from where you are going.
 *
 * Direction matters more than it sounds: without it, moving back through a
 * stepper feels like a fresh page rather than a return, and people lose track
 * of where they are.
 */
export function MorphPanel({
  view, direction, children, className,
}: {
  /** Changing this swaps the content. */
  view: string;
  /** 1 = moving forward, -1 = back. */
  direction: number;
  children: ReactNode;
  className?: string;
}) {
  const still = useReducedMotion();
  const [height, setHeight] = useState<number | "auto">("auto");
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!box.current || still) return;
    // Observed rather than measured once: sections expand and collapse inside
    // the panel, and a height frozen at mount clips them.
    const ro = new ResizeObserver(([entry]) => setHeight(entry.contentRect.height));
    ro.observe(box.current);
    return () => ro.disconnect();
  }, [view, still]);

  if (still) return <div className={className}>{children}</div>;

  return (
    <motion.div
      className={className}
      animate={{ height }}
      transition={SPRING_SOFT}
      style={{ overflow: "hidden" }}
    >
      <AnimatePresence initial={false} mode="popLayout" custom={direction}>
        <motion.div
          key={view}
          ref={box}
          custom={direction}
          initial={{ opacity: 0, x: direction * 26, filter: "blur(4px)" }}
          animate={{ opacity: 1, x: 0, filter: "blur(0px)" }}
          exit={{ opacity: 0, x: direction * -26, filter: "blur(4px)", position: "absolute" }}
          transition={EASE}
          style={{ width: "100%" }}
        >
          {children}
        </motion.div>
      </AnimatePresence>
    </motion.div>
  );
}

/* --------------------------------------------------------------- step bar */

export type Step = {
  id: string;
  label: string;
  icon: ReactNode;
  /** Shown as a dot on the tab when this step still has errors. */
  problems?: number;
  done?: boolean;
};

/**
 * Icon tabs where only the active one carries its label.
 *
 * Six labelled tabs is a menu bar; six icons with one label is a position
 * indicator. The pill slides between tabs as a shared element, so the active
 * state moves rather than blinking from one place to another.
 */
export function StepBar({
  steps, active, onPick,
}: { steps: Step[]; active: string; onPick: (id: string) => void }) {
  const still = useReducedMotion();
  return (
    <div className="flex flex-wrap items-center gap-1" role="tablist" aria-label="Build steps">
      {steps.map((s, i) => {
        const on = s.id === active;
        return (
          <button
            key={s.id}
            role="tab"
            aria-selected={on}
            aria-label={s.label}
            onClick={() => onPick(s.id)}
            className={`relative flex items-center gap-2 rounded-full px-3 py-1.5 text-[12.5px] font-semibold transition-colors ${
              on ? "text-on-ember" : "text-dim hover:text-text"
            }`}
          >
            {on && !still && (
              <motion.span
                layoutId="step-pill"
                className="absolute inset-0 rounded-full bg-ember"
                transition={SPRING}
              />
            )}
            {on && still && <span className="absolute inset-0 rounded-full bg-ember" />}
            <span className="relative z-10 flex items-center gap-1.5">
              <span className="grid h-4 w-4 place-items-center opacity-90">{s.icon}</span>
              <AnimatePresence initial={false}>
                {on && (
                  <motion.span
                    initial={still ? false : { width: 0, opacity: 0 }}
                    animate={{ width: "auto", opacity: 1 }}
                    exit={{ width: 0, opacity: 0 }}
                    transition={EASE}
                    className="overflow-hidden whitespace-nowrap"
                  >
                    {s.label}
                  </motion.span>
                )}
              </AnimatePresence>
              {!on && s.problems ? (
                <span className="h-1.5 w-1.5 rounded-full bg-bad" aria-label={`${s.problems} problems`} />
              ) : null}
              {!on && !s.problems && s.done ? (
                <span className="h-1.5 w-1.5 rounded-full bg-good" aria-hidden="true" />
              ) : null}
            </span>
            <span className="sr-only">{`step ${i + 1} of ${steps.length}`}</span>
          </button>
        );
      })}
    </div>
  );
}

/* ---------------------------------------------------------- status island */

export type IslandState = {
  tone: "idle" | "busy" | "good" | "bad";
  label: string;
  detail?: string;
  onClick?: () => void;
};

/**
 * A pill in the header that morphs between states rather than a panel that is
 * empty until something happens.
 *
 * A run is the only genuinely live thing in this app, so it gets the only
 * looping animation — and it stops the moment the run does.
 */
export function StatusIsland({ state, scanning = false }: { state: IslandState; scanning?: boolean }) {
  const still = useReducedMotion();
  const tone =
    state.tone === "busy" ? "chip-ember" :
    state.tone === "good" ? "chip-good" :
    state.tone === "bad" ? "chip-bad" : "";

  return (
    <motion.button
      layout={!still}
      transition={SPRING}
      onClick={state.onClick}
      disabled={!state.onClick}
      className={`chip ${tone} h-7 max-w-[22rem] overflow-hidden ${state.onClick ? "cursor-pointer hover:border-ember" : "cursor-default"}`}
    >
      {state.tone === "busy" && (
        <motion.span
          className="h-1.5 w-1.5 shrink-0 rounded-full bg-current"
          animate={still ? {} : { opacity: [1, 0.25, 1] }}
          transition={{ duration: 1.4, repeat: Infinity, ease: "easeInOut" }}
        />
      )}
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span
          key={state.label}
          initial={still ? false : { opacity: 0, y: 6, filter: "blur(3px)" }}
          animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
          exit={{ opacity: 0, y: -6, filter: "blur(3px)" }}
          transition={EASE}
          className="truncate"
        >
          {scanning ? <Shimmer>{state.label}</Shimmer> : state.label}
        </motion.span>
      </AnimatePresence>
      {state.detail && <span className="shrink-0 opacity-70">{state.detail}</span>}
    </motion.button>
  );
}

/* --------------------------------------------------------- command palette */

export type Command = {
  id: string;
  label: string;
  hint?: string;
  group: string;
  disabled?: boolean;
  run: () => void;
};

const PaletteContext = createContext<{ open: () => void }>({ open: () => {} });
export const usePalette = () => useContext(PaletteContext);

/**
 * ⌘K over everything: steps, sections, actions, examples.
 *
 * This is what pays for hiding most of the form. A deep interface is only
 * hostile when there is no way to jump — with one, depth becomes tidiness.
 */
export function PaletteProvider({
  commands, children,
}: { commands: Command[]; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const value = useMemo(() => ({ open: () => setOpen(true) }), []);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      }
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, []);

  return (
    <PaletteContext.Provider value={value}>
      {children}
      <AnimatePresence>
        {open && <Palette commands={commands} onClose={() => setOpen(false)} />}
      </AnimatePresence>
    </PaletteContext.Provider>
  );
}

/** Subsequence match, so "gols" still finds "Goals". */
function fuzzy(needle: string, hay: string): boolean {
  if (!needle) return true;
  const h = hay.toLowerCase();
  let i = 0;
  for (const c of needle.toLowerCase()) {
    i = h.indexOf(c, i);
    if (i === -1) return false;
    i += 1;
  }
  return true;
}

function Palette({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const still = useReducedMotion();
  const [q, setQ] = useState("");
  const [cursor, setCursor] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  const hits = useMemo(
    () => commands.filter((c) => !c.disabled && fuzzy(q, `${c.group} ${c.label} ${c.hint ?? ""}`)).slice(0, 40),
    [commands, q],
  );

  useEffect(() => setCursor(0), [q]);
  useEffect(() => input.current?.focus(), []);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") return onClose();
      if (e.key === "ArrowDown") { e.preventDefault(); setCursor((c) => Math.min(c + 1, hits.length - 1)); }
      if (e.key === "ArrowUp") { e.preventDefault(); setCursor((c) => Math.max(c - 1, 0)); }
      if (e.key === "Enter") {
        e.preventDefault();
        const hit = hits[cursor];
        if (hit) { hit.run(); onClose(); }
      }
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [hits, cursor, onClose]);

  let lastGroup = "";

  return (
    <motion.div
      className="dialog-backdrop grid place-items-start justify-center pt-[12vh]"
      initial={still ? false : { opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onMouseDown={onClose}
    >
      <motion.div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        className="card w-[min(38rem,92vw)] overflow-hidden"
        initial={still ? false : { opacity: 0, y: -12, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: -8, scale: 0.98 }}
        transition={SPRING}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <input
          ref={input}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Jump to a section, run an action, load an example…"
          aria-label="Search commands"
          className="w-full border-0 bg-transparent px-4 py-3.5 text-[14px] outline-none placeholder:text-faint"
        />
        <hr className="rule" />
        <div className="max-h-[52vh] overflow-y-auto p-1.5">
          {hits.length === 0 && <p className="hint px-3 py-6 text-center">Nothing matches “{q}”.</p>}
          {hits.map((c, i) => {
            const header = c.group !== lastGroup ? ((lastGroup = c.group), c.group) : null;
            return (
              <div key={c.id}>
                {header && (
                  <p className="px-3 pb-1 pt-2.5 text-[10.5px] font-semibold uppercase tracking-wider text-faint">
                    {header}
                  </p>
                )}
                <button
                  onMouseEnter={() => setCursor(i)}
                  onClick={() => { c.run(); onClose(); }}
                  className="relative flex w-full items-center gap-2.5 rounded-[8px] px-3 py-2 text-left"
                >
                  {i === cursor && (
                    <motion.span
                      layoutId="palette-cursor"
                      className="absolute inset-0 rounded-[8px] bg-raised"
                      transition={still ? { duration: 0 } : SPRING}
                    />
                  )}
                  <span className="relative z-10 flex-1 text-[13px] font-medium">{c.label}</span>
                  {c.hint && <span className="relative z-10 text-[11.5px] text-faint">{c.hint}</span>}
                </button>
              </div>
            );
          })}
        </div>
        <hr className="rule" />
        <div className="flex items-center gap-3 px-4 py-2 text-[11px] text-faint">
          <span><span className="kbd">↑↓</span> move</span>
          <span><span className="kbd">↵</span> pick</span>
          <span><span className="kbd">esc</span> close</span>
        </div>
      </motion.div>
    </motion.div>
  );
}

/* --------------------------------------------------------------- shimmer */

/** A sweep across the text, for a wait with no known end. */
export function Shimmer({ children }: { children: ReactNode }) {
  const still = useReducedMotion();
  if (still) return <span className="text-dim">{children}</span>;
  return (
    <span
      className="bg-clip-text text-transparent"
      style={{
        backgroundImage:
          "linear-gradient(100deg, var(--text-faint) 30%, var(--ember) 50%, var(--text-faint) 70%)",
        backgroundSize: "200% 100%",
        animation: "shimmer-sweep 1.9s linear infinite",
      }}
    >
      {children}
    </span>
  );
}

/* -------------------------------------------------------------- count up */

/**
 * Roll to a new number instead of snapping.
 *
 * Used on the cost figure, which is the one number here worth a beat of
 * attention: a spend ceiling that silently changes is a spend ceiling nobody
 * notices changing.
 */
export function CountUp({ value, prefix = "", digits = 2 }: { value: number; prefix?: string; digits?: number }) {
  const still = useReducedMotion();
  const mv = useMotionValue(value);
  const spring = useSpring(mv, { stiffness: 140, damping: 26 });
  const text = useTransform(spring, (v) => `${prefix}${v.toFixed(digits)}`);
  useEffect(() => { mv.set(value); }, [value, mv]);
  if (still) return <span className="tabular">{`${prefix}${value.toFixed(digits)}`}</span>;
  return <motion.span className="tabular">{text}</motion.span>;
}

/* -------------------------------------------------------------- skeleton */

/** Pulse while waiting, then cross-fade to the real thing. */
export function Skeleton({ ready, lines = 3, children }: { ready: boolean; lines?: number; children: ReactNode }) {
  const still = useReducedMotion();
  return (
    <AnimatePresence mode="wait" initial={false}>
      {ready ? (
        <motion.div
          key="content"
          initial={still ? false : { opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={EASE}
        >
          {children}
        </motion.div>
      ) : (
        <motion.div key="skeleton" exit={{ opacity: 0 }} transition={{ duration: 0.16 }} className="space-y-2">
          {Array.from({ length: lines }).map((_, i) => (
            <motion.div
              key={i}
              className="h-3 rounded bg-raised"
              style={{ width: `${92 - i * 13}%` }}
              animate={still ? {} : { opacity: [0.45, 0.85, 0.45] }}
              transition={{ duration: 1.5, repeat: Infinity, delay: i * 0.12, ease: "easeInOut" }}
            />
          ))}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/* ----------------------------------------------------------------- shake */

/** Shake once. For a value the machine just refused. */
export function useShake() {
  const still = useReducedMotion();
  const [key, setKey] = useState(0);
  const shake = useCallback(() => setKey((k) => k + 1), []);
  const props = still || key === 0
    ? {}
    : { animate: { x: [0, -7, 6, -4, 2, 0] }, transition: { duration: 0.42, ease: [0.36, 0.07, 0.19, 0.97] as const } };
  return { shake, shakeKey: key, shakeProps: props };
}

export { motion, AnimatePresence, useReducedMotion };
