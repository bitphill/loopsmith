/**
 * The wizard as the server sends it.
 *
 * Every type here is the serde shape of something in
 * `loopsmith_wizard::spec`, and nothing here adds to it. That is the whole
 * point of 1.0's wizard: the questions, their order, their wording, and what
 * counts as an answer live in Rust once, and this file is the wire format they
 * arrive in — not a second copy with its own opinions.
 *
 * Two rules the browser deliberately does not implement:
 *
 * - **Which questions apply.** A detector's own fields, an unopened section,
 *   the judge-independence question that means nothing with one provider —
 *   `/api/wizard/answers` returns `visible`, and anything not in it is not
 *   asked.
 * - **What a dynamic select offers.** A check is aimed at a goal named three
 *   questions ago; the same response carries the resolved `options`.
 *
 * Both are conditions over answers, which is exactly the kind of rule that
 * rots when it exists in two languages. The round trip is in-process on
 * loopback, so asking costs less than a frame.
 */

/** Every answer, keyed by the config path it fills. Strings, all the way. */
export type Answers = Record<string, string>;

export type Choice = { value: string; label: string; note?: string | null };

export type Separator = "comma" | "semicolon" | "whitespace";

export type Options =
  | { source: "fixed"; choices: Choice[] }
  | { source: "goal_targets" }
  | { source: "provider_ids" }
  | { source: "phase_names" };

export type Input =
  | { kind: "text"; placeholder?: string | null; mono: boolean }
  | { kind: "area"; placeholder?: string | null; rows: number }
  | { kind: "number"; min?: number | null; step?: number | null; suffix?: string | null }
  | { kind: "bool"; true_label?: string | null; false_label?: string | null }
  | { kind: "select"; options: Options }
  | { kind: "items"; placeholder?: string | null; separator: Separator; mono: boolean };

export type Validator =
  | { rule: "anything" }
  | { rule: "non_empty" }
  | { rule: "uint"; min?: number | null; max?: number | null; required: boolean }
  | { rule: "float"; min?: number | null; max?: number | null; required: boolean }
  | { rule: "regex" }
  | { rule: "one_of"; values: string[] }
  | { rule: "semver" }
  | { rule: "path"; required: boolean };

export type When =
  | { when: "equals"; field: string; value: string }
  | { when: "not_equals"; field: string; value: string }
  | { when: "min_items"; field: string; count: number };

export interface Field {
  id: string;
  title: string;
  hint?: string | null;
  help: string[];
  input: Input;
  /** What an untouched field answers with. Pre-filled, not merely suggested. */
  default?: string | null;
  validator: Validator;
  when?: When | null;
}

export interface Summary {
  primary: string;
  secondary?: string | null;
}

export interface ListStep {
  kind: "list";
  id: string;
  title: string;
  hint?: string | null;
  help: string[];
  singular: string;
  min: number;
  summary: Summary;
  fields: Field[];
  when?: When | null;
}

export type Step =
  | ({ kind: "field" } & Field)
  | ListStep
  | { kind: "providers"; id: string; title: string; hint?: string | null };

export interface Gate {
  question: string;
  hint?: string | null;
}

export interface Section {
  id: string;
  title: string;
  gate?: Gate | null;
  steps: Step[];
}

export interface Spec {
  version: number;
  sections: Section[];
}

/** One answer the server will not take, named by the question it came from. */
export interface Issue {
  key: string;
  message: string;
}

/**
 * The server's reply to a draft: what it refuses, what it makes of it, and the
 * two facts about the form that only the spec can supply.
 */
export interface Assembled {
  issues: Issue[];
  /** Absent while an answer is still refused. */
  config: unknown | null;
  review: unknown | null;
  visible: string[];
  options: Record<string, Choice[]>;
}

/* --- reading the answer map ---------------------------------------------- */

/**
 * How many entries a list has, counted the way the server counts them: from
 * the highest index that appears in a key, so the two never disagree about
 * whether a half-typed goal exists.
 */
export function entryCount(path: string, answers: Answers): number {
  const prefix = `${path}[`;
  let highest = -1;
  for (const key of Object.keys(answers)) {
    if (!key.startsWith(prefix)) continue;
    const close = key.indexOf("]", prefix.length);
    if (close < 0) continue;
    const n = Number(key.slice(prefix.length, close));
    if (Number.isInteger(n) && n > highest) highest = n;
  }
  return highest + 1;
}

/** Every key belonging to one entry of a list. */
function entryKeys(path: string, index: number, answers: Answers): string[] {
  const prefix = `${path}[${index}].`;
  return Object.keys(answers).filter((k) => k.startsWith(prefix));
}

/** Drop one entry and close the gap, so indices stay contiguous. */
export function removeEntry(path: string, index: number, answers: Answers): Answers {
  const count = entryCount(path, answers);
  const out: Answers = { ...answers };
  for (const k of entryKeys(path, index, answers)) delete out[k];
  for (let i = index + 1; i < count; i += 1) {
    const from = `${path}[${i}].`;
    const to = `${path}[${i - 1}].`;
    for (const k of entryKeys(path, i, answers)) {
      delete out[k];
      out[to + k.slice(from.length)] = answers[k];
    }
  }
  return out;
}

/** Everything a half-typed entry left behind. */
export function clearEntry(path: string, index: number, answers: Answers): Answers {
  const out: Answers = { ...answers };
  for (const k of entryKeys(path, index, answers)) delete out[k];
  return out;
}

/** One entry, as a line in a list of them. */
export function describeEntry(step: ListStep, index: number, answers: Answers): string {
  const read = (field: string) => answers[`${step.id}[${index}].${field}`] ?? "";
  const primary = read(step.summary.primary) || `(unnamed ${step.singular})`;
  const secondary = step.summary.secondary ? read(step.summary.secondary) : "";
  return secondary ? `${primary} — ${truncate(secondary, 48)}` : primary;
}

export const truncate = (s: string, max: number): string =>
  [...s].length <= max ? s : `${[...s].slice(0, Math.max(0, max - 1)).join("")}…`;

/** The answer key a field fills, inside a list entry or at the top level. */
export const keyOf = (field: Field, prefix?: string): string =>
  prefix ? `${prefix}.${field.id}` : field.id;

/** What a bare field answers with before anyone touches it. */
export const valueOf = (answers: Answers, key: string, field: Field): string =>
  answers[key] ?? field.default ?? "";

/** The answer key holding whether an opt-in section was accepted. */
export const gateKey = (section: Section): string => `gate:${section.id}`;

/** The choices a select offers: resolved by the server, or fixed in the spec. */
export function choicesFor(
  input: Input,
  key: string,
  options: Record<string, Choice[]>,
): Choice[] {
  if (input.kind !== "select") return [];
  if (input.options.source === "fixed") return input.options.choices;
  return options[key] ?? [];
}
