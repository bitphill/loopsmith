/**
 * The frame every guided step is asked inside.
 *
 * One card, one question. The section is a chip rather than a heading so the
 * question itself can be the title — the same decision the approval card makes,
 * and the reason a guided step reads as a question rather than as a form with a
 * label above it.
 *
 * The footer carries the whole navigation contract: where you are, how to go
 * back, how to leave without losing the draft, and the one primary action.
 */
import type { ReactNode } from "react";
import { Icon, Note } from "../ui";
import { RollingDigits } from "../rolling-digits";

export function GuidedCard({
  section, title, hint, help, children,
  index, total,
  onBack, canBack = true,
  onNext, nextLabel = "Continue", nextDisabled = false,
  onSkip, skipLabel = "Skip",
  onExit,
  problem,
}: {
  section: string;
  title: string;
  hint?: string;
  help?: string[];
  children: ReactNode;
  /** 1-based, for the odometer. */
  index: number;
  total: number;
  onBack: () => void;
  canBack?: boolean;
  onNext: () => void;
  nextLabel?: string;
  nextDisabled?: boolean;
  onSkip?: () => void;
  skipLabel?: string;
  /** Leave the wizard, keeping everything filled in so far. */
  onExit: () => void;
  /** Soft validation: shown, never blocking. */
  problem?: string | null;
}) {
  return (
    <div className="mx-auto w-full max-w-[34rem]">
      <div className="card rise overflow-hidden">
        <div className="card-header">
          <div className="mb-0.5 flex items-center gap-2">
            <span className="chip chip-ember">{section}</span>
          </div>
          <h2 className="card-title pr-6 text-[15.5px]">{title}</h2>
          {hint && <p className="card-description">{hint}</p>}
        </div>

        <div className="card-content">
          {children}

          {help && help.length > 0 && (
            <div className="mt-3 space-y-1">
              {help.map((h, i) => (
                <p key={i} className="text-[11.5px] leading-relaxed text-faint">{h}</p>
              ))}
            </div>
          )}

          {problem && (
            <div className="mt-3">
              <Note tone="warning">{problem}</Note>
            </div>
          )}
        </div>

        <div className="card-footer justify-between">
          <div className="flex items-center gap-1.5 text-faint">
            <button
              type="button"
              aria-label="Previous step"
              disabled={!canBack}
              onClick={onBack}
              className="flex size-[20px] items-center justify-center rounded-[5px] transition-colors duration-100 enabled:hover:text-text disabled:opacity-30"
            >
              {Icon.chevron({ size: 14, className: "rotate-90" })}
            </button>
            <span
              className="inline-flex items-center text-[12px] font-medium tabular-nums text-faint"
              style={{ letterSpacing: "-0.1px", lineHeight: 1 }}
              aria-label={`Step ${index} of ${total}`}
            >
              <RollingDigits value={`${index} / ${total}`} />
            </span>
          </div>

          <div className="flex items-center gap-1.5">
            <button type="button" className="btn btn-ghost btn-sm" onClick={onExit}
              title="Leave the wizard and keep everything filled in so far">
              Expert editor
            </button>
            {onSkip && (
              <button type="button" className="btn btn-ghost btn-sm" onClick={onSkip}>
                {skipLabel}
              </button>
            )}
            <button type="button" className="btn btn-primary btn-sm" disabled={nextDisabled} onClick={onNext}>
              {nextLabel}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
