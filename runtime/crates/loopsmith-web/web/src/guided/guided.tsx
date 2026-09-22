/**
 * The guided wizard.
 *
 * One question at a time, in the order `guided/mod.rs::stages()` asks them, over
 * the same `cfg` the expert editor edits. Walking away is never destructive:
 * "Expert editor" drops into the six-step form with everything filled in so far,
 * and the review rail has been watching the whole time.
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
import { STEPS } from "./spec";
import { resolveInput, type AnyListStep, type FieldStep, type Step } from "./spec-types";
import type { Detection, Format, LoopConfig, PathFacts, ProviderSpec, Review } from "../types";

export function Guided({
  cfg, patch, review, detection, scanning, onRescan,
  onTest, testing, testResults,
  parent, setParent, loopPath, initGit, setInitGit, format, setFormat, facts,
  onExit, onCreate, createDisabled, onJump,
}: {
  cfg: LoopConfig;
  patch: (p: Partial<LoopConfig>) => void;
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
  /** Which opt-in sections were accepted. Absent means "not asked yet". */
  const [gates, setGates] = useState<Record<string, boolean>>({});
  const [at, setAt] = useState(0);
  const [direction, setDirection] = useState(1);

  const steps = useMemo(
    () =>
      STEPS.filter((s: Step) => {
        if (s.gate && gates[s.gate] !== true) return false;
        if (s.available && !s.available(cfg)) return false;
        return true;
      }),
    [gates, cfg],
  );

  const index = Math.min(at, steps.length - 1);
  const step = steps[index];
  if (!step) return null;

  const go = (delta: number) => {
    setDirection(delta >= 0 ? 1 : -1);
    setAt((n) => Math.max(0, Math.min(steps.length - 1, n + delta)));
    document.getElementById("guided-panel")?.scrollTo({ top: 0 });
  };

  const answerGate = (yes: boolean) => {
    setGates((g) => ({ ...g, [step.id]: yes }));
    go(1);
  };

  const shell = (body: React.ReactNode, extra: Partial<React.ComponentProps<typeof GuidedCard>> = {}) => (
    <GuidedCard
      section={step.section}
      title={step.title}
      hint={"hint" in step ? step.hint : undefined}
      help={"help" in step ? step.help : undefined}
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
    case "field": {
      const f = step as FieldStep;
      const value = f.get(cfg);
      const problem = f.validate ? f.validate(value) : null;
      card = shell(
        <FieldInput
          id={`guided-${f.id}`}
          input={resolveInput(f.input, cfg)}
          value={value}
          invalid={!!problem}
          onChange={(v) => patch(f.set(cfg, v))}
        />,
        { problem },
      );
      break;
    }

    case "list": {
      const l = step as AnyListStep;
      const items = l.get(cfg);
      card = shell(
        <ListStepBody
          step={l}
          cfg={cfg}
          items={items}
          onChange={(next) => patch(l.set(cfg, next))}
        />,
        { nextLabel: "This part is done" },
      );
      break;
    }

    case "providers":
      card = shell(
        <ProvidersStepBody
          cfg={cfg}
          patch={patch}
          detection={detection}
          scanning={scanning}
          onRescan={onRescan}
          onTest={onTest}
          testing={testing}
          results={testResults}
        />,
      );
      break;

    case "gate":
      card = shell(
        <p className="hint">This section is optional. Most loops do not need it.</p>,
        {
          nextLabel: "Add it",
          onNext: () => answerGate(true),
          onSkip: () => answerGate(false),
          skipLabel: "Skip",
        },
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
