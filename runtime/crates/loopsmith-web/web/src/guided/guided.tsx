/**
 * The guided wizard.
 *
 * One question at a time, in the order Rust asks them. The list is fetched
 * from `/api/wizard/spec` and rendered generically — nothing in this file
 * knows what a goal or a detector is, which is exactly why a new question is
 * an entry in `loopsmith_wizard::spec::sections` and not a component here.
 *
 * The draft is the answer map: strings keyed by the config path each fills,
 * the same keys the terminal writes. It is posted back to
 * `/api/wizard/answers` as it changes, and the server replies with the typed
 * config, the rail's review, which questions apply right now, and the choices
 * for the selects that depend on earlier answers. Typing stays instant because
 * the inputs read the local map; only the parts that need a rule the server
 * owns wait for the reply, and that reply is in-process on loopback.
 *
 * Validation is deliberately soft. The terminal lets you answer a field badly
 * and reviews the whole config at the end, because stopping someone mid-thought
 * to fix a field they are still typing is worse than showing them the problem
 * and letting them come back. Only Create is gated, and it is gated on the real
 * validator.
 */
import { useMemo, useState } from "react";
import { MorphPanel } from "../motion";
import { GuidedCard } from "./guided-card";
import { FieldInput } from "./inputs";
import { ListStepBody } from "./list-step";
import { PlacementStepBody } from "./placement-step";
import { ProvidersStepBody } from "./providers-step";
import { ReviewStepBody } from "./review-step";
import {
  choicesFor, gateKey, valueOf,
  type Answers, type Assembled, type Field, type ListStep, type Section, type Spec,
} from "./wire";
import type { Detection, Format, PathFacts, ProviderSpec, Review } from "../types";

/** One card in the walk: a gate, a question, a list, or one of the two ends. */
type Card =
  | { kind: "gate"; id: string; section: Section }
  | { kind: "field"; id: string; section: Section; field: Field }
  | { kind: "list"; id: string; section: Section; step: ListStep }
  | { kind: "providers"; id: string; section: Section; path: string; title: string; hint?: string | null }
  | { kind: "placement"; id: string; section: Section }
  | { kind: "review"; id: string; section: Section };

/**
 * The two closing steps, which are about the file rather than the config.
 *
 * They are not in the spec on purpose: where a loop is created, in which
 * grammar, and whether to `git init` are facts about a directory on disk, and
 * the terminal asks them after the interview rather than inside it.
 */
const PLACEMENT: Section = { id: "placement", title: "Where it lives", steps: [] };
const REVIEW: Section = { id: "review", title: "Review", steps: [] };

/**
 * Flatten the spec into the cards that apply, in order.
 *
 * `visible` is the server's answer to "which questions are being asked", so
 * this walk only has to lay them out. A gate is always offered; everything
 * behind it appears once it has been answered yes.
 */
function cards(spec: Spec | null, visible: Set<string>): Card[] {
  const out: Card[] = [];
  for (const section of spec?.sections ?? []) {
    if (section.gate) {
      out.push({ kind: "gate", id: gateKey(section), section });
    }
    if (!visible.has(section.id)) continue;
    for (const step of section.steps) {
      if (!visible.has(step.id)) continue;
      if (step.kind === "field") {
        out.push({ kind: "field", id: step.id, section, field: step });
      } else if (step.kind === "list") {
        out.push({ kind: "list", id: step.id, section, step });
      } else {
        out.push({
          kind: "providers",
          id: step.id,
          section,
          path: step.id,
          title: step.title,
          hint: step.hint,
        });
      }
    }
  }
  out.push({ kind: "placement", id: "place", section: PLACEMENT });
  out.push({ kind: "review", id: "review", section: REVIEW });
  return out;
}

export function Guided({
  spec, answers, setAnswers, assembled, review, detection, scanning, onRescan,
  onTest, testing, testResults,
  parent, setParent, loopPath, initGit, setInitGit, format, setFormat, facts,
  onExit, onCreate, createDisabled, onJump,
}: {
  spec: Spec | null;
  answers: Answers;
  setAnswers: (next: Answers) => void;
  /** The server's last reply about this draft. */
  assembled: Assembled | null;
  review: Review | null;
  detection: Detection | null;
  scanning: boolean;
  onRescan: (deep: boolean) => void;
  onTest: (p: ProviderSpec) => void;
  testing: string | null;
  testResults: Record<string, { ok: boolean; text: string }>;
  parent: string;
  setParent: (v: string) => void;
  loopPath: string;
  initGit: boolean;
  setInitGit: (v: boolean) => void;
  format: Format;
  setFormat: (f: Format) => void;
  facts: PathFacts | null;
  /** Leave the wizard for the expert editor, keeping the draft. */
  onExit: () => void;
  onCreate: () => void;
  createDisabled: boolean;
  onJump: (field: string) => void;
}) {
  const [at, setAt] = useState(0);
  const [direction, setDirection] = useState(1);

  const visible = useMemo(
    () => new Set(assembled?.visible ?? []),
    [assembled],
  );
  const options = assembled?.options ?? {};
  const issues = useMemo(
    () => new Map((assembled?.issues ?? []).map((i) => [i.key, i.message])),
    [assembled],
  );

  const steps = useMemo(() => cards(spec, visible), [spec, visible]);

  const index = Math.min(at, steps.length - 1);
  const step = steps[index];
  if (!step) {
    return (
      <div className="min-h-0 overflow-y-auto bg-ground p-4">
        <p className="hint">Loading the questions…</p>
      </div>
    );
  }

  const go = (delta: number) => {
    setDirection(delta >= 0 ? 1 : -1);
    setAt((n) => Math.max(0, Math.min(steps.length - 1, n + delta)));
    document.getElementById("guided-panel")?.scrollTo({ top: 0 });
  };

  const set = (key: string, value: string) => {
    const next = { ...answers };
    if (value.trim() === "") delete next[key];
    else next[key] = value;
    setAnswers(next);
  };

  const answerGate = (yes: boolean) => {
    set(step.id, String(yes));
    go(1);
  };

  const shell = (
    body: React.ReactNode,
    extra: Partial<React.ComponentProps<typeof GuidedCard>> = {},
    over: { title?: string; hint?: string | null; help?: string[] } = {},
  ) => (
    <GuidedCard
      section={step.section.title}
      title={over.title ?? step.section.title}
      hint={over.hint ?? undefined}
      help={over.help}
      index={index + 1}
      total={steps.length}
      canBack={index > 0}
      onBack={() => go(-1)}
      onNext={() => go(1)}
      onExit={onExit}
      {...extra}
    >
      {body}
    </GuidedCard>
  );

  let card: React.ReactNode;

  switch (step.kind) {
    case "gate": {
      const gate = step.section.gate!;
      const on = answers[step.id] === "true";
      card = shell(
        <p className="hint">This section is optional. Most loops do not need it.</p>,
        {
          nextLabel: on ? "Keep it" : "Add it",
          onNext: () => answerGate(true),
          onSkip: () => answerGate(false),
          skipLabel: "Skip",
        },
        { title: gate.question, hint: gate.hint },
      );
      break;
    }

    case "field": {
      const f = step.field;
      const problem = issues.get(f.id);
      card = shell(
        <FieldInput
          id={`guided-${f.id}`}
          input={f.input}
          choices={choicesFor(f.input, f.id, options)}
          value={valueOf(answers, f.id, f)}
          invalid={!!problem}
          onChange={(v) => set(f.id, v)}
        />,
        { problem },
        { title: f.title, hint: f.hint, help: f.help },
      );
      break;
    }

    case "list":
      card = shell(
        <ListStepBody
          step={step.step}
          answers={answers}
          setAnswers={setAnswers}
          visible={visible}
          options={options}
          issues={issues}
        />,
        { nextLabel: "This part is done" },
        { title: step.step.title, hint: step.step.hint, help: step.step.help },
      );
      break;

    case "providers":
      card = shell(
        <ProvidersStepBody
          path={step.path}
          answers={answers}
          setAnswers={setAnswers}
          detection={detection}
          scanning={scanning}
          onRescan={onRescan}
          onTest={onTest}
          testing={testing}
          results={testResults}
        />,
        {},
        { title: step.title, hint: step.hint },
      );
      break;

    case "placement":
      card = shell(
        <PlacementStepBody
          parent={parent}
          setParent={setParent}
          loopPath={loopPath}
          initGit={initGit}
          setInitGit={setInitGit}
          format={format}
          setFormat={setFormat}
          facts={facts}
        />,
        {},
        {
          title: "Where should this loop be created?",
          hint: "The loop gets its own directory, named after it, inside the folder you pick.",
        },
      );
      break;

    case "review":
      card = shell(
        <ReviewStepBody review={review} onJump={onJump} />,
        {
          nextLabel: "Create loop",
          nextDisabled: createDisabled,
          onNext: onCreate,
        },
        { title: "Everything checked" },
      );
      break;
  }

  return (
    <div id="guided-panel" className="min-h-0 overflow-y-auto bg-ground p-4">
      <MorphPanel view={step.id} direction={direction}>
        {card}
      </MorphPanel>
    </div>
  );
}
