/**
 * The first thing `loopsmith --web` shows.
 *
 * Three doors, and a plain sentence about what is behind them. The expert
 * editor is a six-step form with every section reachable at once, which is the
 * right tool once you know what a loop is and the wrong one on the first day.
 * Asking outright is cheaper than guessing from behaviour, and the answer is
 * remembered so it is asked once.
 *
 * The third door exists because neither of the other two answers the question
 * most people actually arrive with, which is not "how do I write one" but
 * "what does one *do*". It builds a working loop in a throwaway directory and
 * dry-runs it: real scheduling, real gate, real ledger, and no provider
 * called, so it costs nothing and cannot touch anything of yours.
 */
import type { CSSProperties } from "react";
import { Icon } from "./ui";

export type Smith = "experienced" | "new";

export function SmithGate({
  onPick, onDemo,
}: {
  onPick: (s: Smith) => void;
  /** Take the third door. Not remembered: it is a thing to watch once. */
  onDemo: () => void;
}) {
  return (
    <div className="dialog-backdrop grid place-items-center p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Choose how to start"
        className="card card-raised rise w-full max-w-[32rem] overflow-hidden"
        style={{ "--card-spacing": "22px" } as CSSProperties}
      >
        <div className="card-header">
          <div className="flex items-center gap-3">
            <span className="grid h-[38px] w-[38px] shrink-0 place-items-center rounded-[8px] bg-white">
              <img src="/logo.png" alt="" width={32} height={32} className="select-none"
                aria-hidden="true" draggable={false} />
            </span>
            <h1 className="forge-mark text-[24px] leading-none">loopsmith</h1>
          </div>
        </div>

        <div className="card-content space-y-2">
          <p className="text-[14px] leading-relaxed text-text">
            loopsmith runs a job you describe once — over and over, on its own — and stops when a
            check you wrote says it is done, or a limit you set says enough.
          </p>
          <p className="hint">
            Nothing runs until you press a button. This next part is only a description.
          </p>
        </div>

        <div className="card-footer flex-col gap-2 sm:flex-row">
          <button
            type="button"
            className="btn flex-1 justify-start gap-2.5 py-3"
            onClick={() => onPick("experienced")}
          >
            <span className="text-ember">{Icon.bolt({ size: 16 })}</span>
            <span className="text-left">
              <span className="block text-[13px] font-semibold">I am an experienced smith</span>
              <span className="block text-[11.5px] font-normal text-dim">Straight to the full editor</span>
            </span>
          </button>

          <button
            type="button"
            className="btn btn-primary flex-1 justify-start gap-2.5 py-3"
            onClick={() => onPick("new")}
          >
            <span>{Icon.target({ size: 16 })}</span>
            <span className="text-left">
              <span className="block text-[13px] font-semibold">I am a new smith</span>
              <span className="block text-[11.5px] font-normal opacity-80">Walk me through it</span>
            </span>
          </button>
        </div>

        <div className="card-footer border-t-0 pt-0">
          <button
            type="button"
            className="btn btn-ghost w-full justify-start gap-2.5 py-2.5"
            onClick={onDemo}
          >
            <span className="text-ember">{Icon.play({ size: 15 })}</span>
            <span className="text-left">
              <span className="block text-[13px] font-semibold">Show me one running</span>
              <span className="block text-[11.5px] font-normal text-dim">
                Builds a working loop somewhere disposable and walks it through without calling a
                model. Nothing of yours is touched and nothing is spent.
              </span>
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}
