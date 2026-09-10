/**
 * The first thing `loopsmith --web` shows.
 *
 * Two doors, and a plain sentence about what is behind them. The expert editor
 * is a six-step form with every section reachable at once, which is the right
 * tool once you know what a loop is and the wrong one on the first day. Asking
 * outright is cheaper than guessing from behaviour, and the answer is
 * remembered so it is asked once.
 */
import type { CSSProperties } from "react";
import { Icon } from "./ui";

export type Smith = "experienced" | "new";

export function SmithGate({ onPick }: { onPick: (s: Smith) => void }) {
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
      </div>
    </div>
  );
}
