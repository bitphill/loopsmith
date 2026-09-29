/**
 * The first-run explanation.
 *
 * Five panels, shown once and reachable afterwards from the header. It exists
 * because the hardest part of loopsmith is not any single field — it is the
 * shape of the idea: that you describe what you want, describe how a machine
 * would check it, and the checking is what makes the rest safe to leave alone.
 *
 * Someone who reads this should be able to load an example and understand what
 * they are looking at.
 */
import { useEffect, useState } from "react";
import { Icon } from "./ui";

type Panel = { title: string; body: string[]; aside?: string };

const PANELS: Panel[] = [
  {
    title: "What a loop is",
    body: [
      "A loop is a job you describe once and then let run without watching it. You say what you want, how a machine can check whether you got it, and when it should give up. loopsmith handles the rest: what order things run in, what it remembers between attempts, and whether the job is actually done.",
      "It keeps going until it succeeds, or until one of the limits you set stops it. That second half is the important half.",
    ],
    aside: "You are filling in a description. Nothing runs until you press a button.",
  },
  {
    title: "Why the checks matter more than the goals",
    body: [
      "Anyone can write down what they want. The part that makes an unattended loop trustworthy is the part that decides whether it happened — and that decision is never left to a model.",
      "loopsmith is built on one rule: a model must not certify its own completion. A script's exit code, a file existing, a number crossing a line — those can be checked. A model can offer an opinion alongside them, but it cannot mark its own homework, and there is deliberately no way to tell loopsmith a goal is done.",
    ],
    aside: "This is why a goal with no check is refused rather than run.",
  },
  {
    title: "The order of the form",
    body: [
      "Top to bottom is the order you should fill it in. Where the loop lives, then what does the work, then what you want, then how it is checked, then how it ends.",
      "Every field has a line under it saying what it is for, and an ⓘ next to the ones where the honest advice is not obvious from the name. Nothing is hidden behind jargon on purpose.",
    ],
    aside: "Stuck on a field? Press the ⓘ. It says why the field exists, not just what it holds.",
  },
  {
    title: "The panel on the right is watching",
    body: [
      "Every edit is re-checked as you type. It shows what is wrong, what a run could cost at the ceilings you have set, how much will run at once, and exactly which permissions the loop will need.",
      "If something there says a run is unbounded, believe it. That panel is the same code that decides whether a real run is allowed to start.",
    ],
    aside: "Click a problem in that panel to jump to the field it is about.",
  },
  {
    title: "The fastest way to learn this",
    body: [
      "Load an example from the left. Thirteen working loops ship with this, covering research, refactoring, outreach, publishing and more. Loading one fills in every section so you can read a real config with all the explanations attached.",
      "Then change one thing, watch the right-hand panel react, and press Dry run — it walks the entire loop without calling a single model or spending anything.",
    ],
    aside: "Dry run costs nothing. Use it as often as you like.",
  },
];

export function Tour({ onClose }: { onClose: () => void }) {
  const [i, setI] = useState(0);
  const last = i === PANELS.length - 1;
  const p = PANELS[i];

  useEffect(() => {
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "ArrowRight" && !last) setI((n) => n + 1);
      if (e.key === "ArrowLeft" && i > 0) setI((n) => n - 1);
    };
    document.addEventListener("keydown", keys);
    return () => document.removeEventListener("keydown", keys);
  }, [i, last, onClose]);

  return (
    <div className="dialog-backdrop grid place-items-center p-4">
      <div role="dialog" aria-modal="true" aria-label="How loopsmith works"
        className="card rise w-full max-w-[38rem] p-6">
        <div className="mb-4 flex items-center gap-3">
          <img src="/logo.png" alt="" width={40} height={40} className="shrink-0" aria-hidden="true" draggable={false} />
          <h2 className="text-[19px] font-bold tracking-tight">{p.title}</h2>
          <button type="button" className="btn btn-ghost btn-sm btn-icon ml-auto"
            aria-label="Skip" onClick={onClose}>
            {Icon.x({ size: 15 })}
          </button>
        </div>

        <div className="space-y-3">
          {p.body.map((t, n) => (
            <p key={n} className="text-[13.5px] leading-relaxed text-dim">{t}</p>
          ))}
          {p.aside && (
            <p className="stripe stripe-ember py-1 text-[12.5px] font-medium">{p.aside}</p>
          )}
        </div>

        <div className="mt-6 flex items-center gap-3">
          <div className="flex gap-1.5" role="tablist" aria-label="Tour progress">
            {PANELS.map((panel, n) => (
              <button key={n} type="button" role="tab" aria-selected={n === i}
                aria-label={panel.title} onClick={() => setI(n)}
                className="h-1.5 rounded-full transition-all"
                style={{
                  width: n === i ? 22 : 8,
                  background: n === i ? "var(--ember)" : "var(--line-strong)",
                }} />
            ))}
          </div>
          <div className="ml-auto flex gap-2">
            {i > 0 && <button className="btn btn-sm" onClick={() => setI((n) => n - 1)}>Back</button>}
            <button className="btn btn-sm btn-primary"
              onClick={() => (last ? onClose() : setI((n) => n + 1))}>
              {last ? "Start building" : "Next"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
