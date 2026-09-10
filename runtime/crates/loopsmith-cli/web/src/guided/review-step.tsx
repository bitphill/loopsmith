/**
 * The closing step.
 *
 * The terminal prints `loopsmith validate`'s findings and asks whether to
 * write. This is the same verdict from the same code — the review rail has been
 * running it on every keystroke — collected into one card so the decision to
 * create the loop is made while looking at what is wrong with it.
 *
 * Errors block Create. Warnings do not: they are shown, and creating anyway is
 * a choice rather than an accident.
 */
import { CountUp } from "../motion";
import { Icon, Note } from "../ui";
import type { Review } from "../types";

export function ReviewStepBody({
  review, onJump,
}: {
  review: Review | null;
  /** Send a problem to the field that owns it, in the expert editor. */
  onJump: (field: string) => void;
}) {
  if (!review) return <p className="hint">Checking…</p>;

  if (!review.parsed) {
    return <Note tone="error">{review.parse_error ?? "The config could not be read."}</Note>;
  }

  const errors = review.issues.filter((i) => i.severity === "error");
  const warnings = review.issues.filter((i) => i.severity === "warning");

  return (
    <div className="space-y-3">
      {review.issues.length === 0 ? (
        <p className="flex items-center gap-2 text-[13px] text-good">
          {Icon.check({ size: 15 })} Valid, with no warnings.
        </p>
      ) : (
        <div className="space-y-1.5">
          {errors.map((i, n) => (
            <button
              key={`e${n}`}
              type="button"
              className="stripe stripe-error block w-full py-1 text-left"
              onClick={() => onJump(i.field)}
            >
              <span className="font-mono text-[11px] text-faint">{i.field}</span>
              <span className="hint block text-text">{i.message}</span>
            </button>
          ))}
          {warnings.map((i, n) => (
            <button
              key={`w${n}`}
              type="button"
              className="stripe stripe-warning block w-full py-1 text-left"
              onClick={() => onJump(i.field)}
            >
              <span className="font-mono text-[11px] text-faint">{i.field}</span>
              <span className="hint block text-text">{i.message}</span>
            </button>
          ))}
        </div>
      )}

      <div className="grid grid-cols-2 gap-2">
        <div className="card p-3">
          <p className="text-[11px] uppercase tracking-wide text-faint">Cost ceiling</p>
          <p className="mt-0.5 text-[22px] font-bold leading-none">
            {review.cost.ceiling_usd != null
              ? <CountUp value={review.cost.ceiling_usd} prefix="$" />
              : <span className="text-warn">unbounded</span>}
          </p>
          <p className="hint mt-1">{review.cost.basis}</p>
        </div>
        <div className="card p-3">
          <p className="text-[11px] uppercase tracking-wide text-faint">Runs at once</p>
          <p className="mt-0.5 text-[22px] font-bold leading-none tabular">
            {review.plan?.concurrency ?? "—"}
          </p>
          <p className="hint mt-1">
            {review.plan ? `${review.plan.waves.length} wave${review.plan.waves.length === 1 ? "" : "s"}` : "no graph yet"}
          </p>
        </div>
      </div>

      {errors.length > 0 && (
        <Note tone="error">
          {errors.length} error{errors.length === 1 ? "" : "s"} must be fixed before the loop can be created.
        </Note>
      )}
    </div>
  );
}
