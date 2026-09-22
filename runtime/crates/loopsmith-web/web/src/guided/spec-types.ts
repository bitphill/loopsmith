/**
 * The shape of a guided step.
 *
 * The terminal wizard's field list lives in `src/guided/sections.rs`; this is
 * the same list expressed as data so the browser can walk it one field at a
 * time. Keeping it declarative is the point: a new field is an entry in
 * `spec.ts`, not a new component, and the ordering here can be read against
 * `guided/mod.rs::stages()` without holding two implementations in your head.
 *
 * Nothing here transforms the config. Every `set` returns a `Partial<LoopConfig>`
 * that is merged into the same `cfg` the expert editor edits, so the two modes
 * are two views of one draft rather than two drafts.
 */
import type { LoopConfig } from "../types";

/** Every answer a step can hold. */
export type Value = string | number | boolean | string[] | null;

export type Choice = { value: string; label: string; note?: string };

/**
 * How a field is asked. `select` renders as circles when the list is short
 * enough to read at a glance and falls back to a dropdown when it is not;
 * `multi` is always squares, because a checkbox that looks like a radio is a
 * lie about how many answers are allowed.
 */
export type Input =
  | { kind: "text"; placeholder?: string; mono?: boolean }
  | { kind: "area"; placeholder?: string; rows?: number }
  | { kind: "number"; min?: number; step?: number; suffix?: string }
  | { kind: "bool"; trueLabel?: string; falseLabel?: string }
  | { kind: "select"; options: Choice[] }
  | { kind: "multi"; options: Choice[] };

/** Options that depend on what has been filled in already (goal names, provider ids). */
export type InputOf = Input | ((c: LoopConfig) => Input);

export function resolveInput(i: InputOf, c: LoopConfig): Input {
  return typeof i === "function" ? i(c) : i;
}

/** One field inside a list entry (a goal, a validation, a node). */
export interface EntryField<T> {
  id: string;
  label: string;
  /** One line under the label. The "why", not a restatement of the name. */
  hint?: string;
  input: InputOf;
  required?: boolean;
  get: (e: T) => Value;
  set: (e: T, v: Value) => T;
  /** Detector-specific fields appear only once their detector is chosen. */
  when?: (e: T) => boolean;
  validate?: (v: Value) => string | null;
}

interface Base {
  id: string;
  /** Section heading, shown above the question so the step has a place. */
  section: string;
  /** Only shown when this gate has been answered yes. */
  gate?: string;
  /**
   * Steps that only make sense once something else is filled in — the judge
   * independence question needs two providers before it means anything.
   */
  available?: (c: LoopConfig) => boolean;
}

/** A single scalar field: one card, one question. */
export interface FieldStep extends Base {
  kind: "field";
  title: string;
  hint?: string;
  help?: string[];
  input: InputOf;
  required?: boolean;
  get: (c: LoopConfig) => Value;
  set: (c: LoopConfig, v: Value) => Partial<LoopConfig>;
  validate?: (v: Value) => string | null;
}

/**
 * A repeating section: one card that accumulates entries. `+` opens another
 * blank entry and the step is not left until "This part is done".
 */
export interface ListStep<T = unknown> extends Base {
  kind: "list";
  title: string;
  hint?: string;
  help?: string[];
  singular: string;
  /** Smallest count that lets the step be finished cleanly. */
  min: number;
  get: (c: LoopConfig) => T[];
  set: (c: LoopConfig, items: T[]) => Partial<LoopConfig>;
  blank: () => T;
  describe: (e: T) => string;
  fields: EntryField<T>[];
}

/** The yes/no in front of an opt-in section, mirroring the terminal's gate. */
export interface GateStep extends Base {
  kind: "gate";
  title: string;
  hint?: string;
}

/** Providers get their own step: the machine has already been scanned. */
export interface ProvidersStep extends Base {
  kind: "providers";
  title: string;
  hint?: string;
}

/**
 * Where the loop is created. This is the one step that edits app state rather
 * than the config — the directory, the grammar and whether to `git init` are
 * facts about the file on disk, not fields in the A-J model.
 */
export interface PlacementStep extends Base {
  kind: "placement";
  title: string;
  hint?: string;
}

/** The closing step: the real validator's verdict, then Create. */
export interface ReviewStep extends Base {
  kind: "review";
  title: string;
}

/**
 * The heterogeneous step list. `ListStep` is erased to `any` here on purpose:
 * a list of goals and a list of validations are the same *kind* of step over
 * different entry types, and TypeScript's contravariant parameters mean
 * `EntryField<Goal>` is not assignable to `EntryField<unknown>`. The entry type
 * is recovered at the one place it matters, where the list step is rendered.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyListStep = ListStep<any>;

export type Step =
  | FieldStep
  | AnyListStep
  | GateStep
  | ProvidersStep
  | PlacementStep
  | ReviewStep;

/* --- shared validators, mirroring guided/sections.rs ---------------------- */

export const required = (v: Value): string | null =>
  String(v ?? "").trim() ? null : "this field is required";

export const reqUint = (v: Value): string | null => {
  const s = String(v ?? "").trim();
  if (!s) return "enter a whole number";
  return /^\d+$/.test(s) ? null : "enter a whole number";
};

export const optUint = (v: Value): string | null => {
  const s = String(v ?? "").trim();
  return s ? reqUint(v) : null;
};

export const reqFloat = (v: Value): string | null => {
  const s = String(v ?? "").trim();
  if (!s) return "enter a number";
  return Number.isFinite(Number(s)) ? null : "enter a number";
};

export const optFloat = (v: Value): string | null => {
  const s = String(v ?? "").trim();
  return s ? reqFloat(v) : null;
};

/* --- list/text helpers, mirroring the Rust split_* helpers ---------------- */

export const splitCommas = (s: string): string[] =>
  s.split(",").map((x) => x.trim()).filter(Boolean);

export const splitSemis = (s: string): string[] =>
  s.split(";").map((x) => x.trim()).filter(Boolean);

export const splitWs = (s: string): string[] => s.split(/\s+/).filter(Boolean);

export const truncate = (s: string, max: number): string =>
  [...s].length <= max ? s : `${[...s].slice(0, Math.max(0, max - 1)).join("")}…`;
