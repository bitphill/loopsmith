/**
 * The action bar and the live run console.
 *
 * Every button here spawns the real `loopsmith` binary. The console follows a
 * job over a WebSocket that replays what has already been printed before
 * going live, so opening it halfway through a run still shows the whole run.
 */
import { useEffect, useRef, useState } from "react";
import { api, streamJob } from "./api";
import { Icon, Dialog, Note } from "./ui";
import type { JobLine, JobSummary, Review, PathFacts } from "./types";

export type ActionId =
  | "create" | "validate" | "plan" | "dry_run" | "run"
  | "watch" | "schedule_install" | "permissions_write" | "skills_install";

const LABELS: Record<ActionId, { label: string; note: string; spends: boolean }> = {
  validate: { label: "Check config", note: "Reads the config and reports every problem. Changes nothing, costs nothing.", spends: false },
  plan: { label: "Show the plan", note: "Waves, longest chain, and predicted speedup, without running anything.", spends: false },
  create: { label: "Create loop", note: "Writes the loop and its state directory. This is the one that makes it real.", spends: false },
  dry_run: { label: "Dry run", note: "Walks the whole loop without calling a single model. The safest way to see what would happen.", spends: false },
  run: { label: "Run once", note: "A real run. Calls models and spends money up to the ceilings you set.", spends: true },
  watch: { label: "Watch", note: "Stays resident and runs whenever a trigger fires. Ends when you stop it.", spends: true },
  schedule_install: { label: "Install schedule", note: "Hands the schedule to launchd or cron so it survives a reboot.", spends: false },
  permissions_write: { label: "Grant permissions", note: "Merges the derived permissions into .claude/settings.local.json in the loop folder.", spends: false },
  skills_install: { label: "Install sub-agents", note: "Installs everything section J declares. Idempotent.", spends: false },
};

export function ActionBar({
  review, facts, created, onRun, running,
}: {
  review: Review | null;
  facts: PathFacts | null;
  created: boolean;
  onRun: (a: ActionId, opts?: { force?: boolean }) => void;
  running: boolean;
}) {
  const [confirm, setConfirm] = useState<ActionId | null>(null);
  const blocked = !review?.parsed || review.error_count > 0;
  const cantWrite = !!facts && !facts.writable;

  const Btn = ({ id, primary }: { id: ActionId; primary?: boolean }) => {
    const m = LABELS[id];
    // Everything except the two read-only checks needs a config that parses.
    const needsConfig = id !== "validate" && id !== "plan";
    const needsLoop = id !== "create" && id !== "validate" && id !== "plan";
    const disabled =
      running ||
      (needsConfig && blocked) ||
      (id === "create" && cantWrite) ||
      (needsLoop && !created);
    return (
      <button
        type="button"
        className={`btn ${primary ? "btn-primary" : ""} ${m.spends ? "btn-quench" : ""}`}
        disabled={disabled}
        title={
          disabled
            ? needsLoop && !created
              ? "Create the loop first — these act on a loop that exists on disk."
              : blocked
                ? "Fix the errors in the right-hand panel first."
                : cantWrite
                  ? "That folder is not writable."
                  : "Busy"
            : m.note
        }
        onClick={() => (m.spends ? setConfirm(id) : onRun(id))}
      >
        {m.spends ? Icon.play({ size: 14 }) : primary ? Icon.bolt({ size: 14 }) : null}
        {m.label}
      </button>
    );
  };

  return (
    <>
      <div className="sticky bottom-0 z-20 border-t bg-surface/95 px-4 py-3 backdrop-blur">
        <div className="flex flex-wrap items-center gap-2">
          <Btn id="validate" />
          <Btn id="plan" />
          <span className="mx-1 hidden h-5 w-px bg-line md:block" />
          <Btn id="create" primary />
          <Btn id="permissions_write" />
          <Btn id="skills_install" />
          <span className="mx-1 hidden h-5 w-px bg-line md:block" />
          <Btn id="dry_run" />
          <Btn id="run" />
          <Btn id="watch" />
          <Btn id="schedule_install" />
          <span className="ml-auto hidden text-[11.5px] text-faint lg:block">
            {created
              ? "Loop exists on disk. Everything is available."
              : "Create the loop to unlock the run buttons."}
          </span>
        </div>
      </div>

      {confirm && (
        <Dialog
          title={`${LABELS[confirm].label} — this spends money`}
          onClose={() => setConfirm(null)}
          actions={
            <>
              <button className="btn" onClick={() => setConfirm(null)}>Cancel</button>
              <button className="btn btn-primary" onClick={() => { onRun(confirm); setConfirm(null); }}>
                {LABELS[confirm].label}
              </button>
            </>
          }
        >
          <p className="hint">{LABELS[confirm].note}</p>
          <div className="mt-3">
            {review?.cost.ceiling_usd != null ? (
              <Note tone="note">
                A cost ceiling of ${review.cost.ceiling_usd} is set. The run halts when the
                ledger crosses it.
              </Note>
            ) : (
              <Note tone="warning">
                No cost ceiling is set. The iteration limit
                {review?.plan ? ` and ${review.plan.concurrency} worker(s)` : ""} are the only
                bounds on this. Consider setting one in Stop gates first.
              </Note>
            )}
          </div>
          <p className="hint mt-3">
            You can stop a run at any time from the console, and Dry run walks the same path
            without calling a model.
          </p>
        </Dialog>
      )}
    </>
  );
}

/* --- the console --------------------------------------------------------- */

export function RunConsole({
  jobId, onClose, onFinished,
}: {
  jobId: string;
  onClose: () => void;
  onFinished: (s: JobSummary) => void;
}) {
  const [lines, setLines] = useState<JobLine[]>([]);
  const [summary, setSummary] = useState<JobSummary | null>(null);
  const [follow, setFollow] = useState(true);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setLines([]);
    setSummary(null);
    const stop = streamJob(jobId, {
      // Keyed by seq so a reconnect cannot duplicate a line already shown.
      line: (l) => setLines((all) => (all.some((x) => x.seq === l.seq) ? all : [...all, l])),
      state: (s) => {
        setSummary(s);
        if (s.state !== "running") onFinished(s);
      },
      lagged: (_n, message) =>
        setLines((all) => [...all, { seq: -Date.now(), stream: "err", text: message }]),
    });
    return stop;
  }, [jobId, onFinished]);

  useEffect(() => {
    if (follow) box.current?.scrollTo({ top: box.current.scrollHeight });
  }, [lines, follow]);

  const state = summary?.state ?? "running";
  const tone = state === "succeeded" ? "chip-good" : state === "running" ? "chip-ember" : state === "cancelled" ? "chip-warn" : "chip-bad";

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 flex-wrap items-center gap-2 border-b p-2.5">
        <span className="text-ember">{Icon.terminal({ size: 15 })}</span>
        <span className="font-mono text-[12px] font-semibold">
          {summary?.argv.join(" ") ?? "starting…"}
        </span>
        <span className={`chip ${tone} ${state === "running" ? "running" : ""}`}>{state}</span>
        {summary?.exit_code != null && <span className="chip tabular">exit {summary.exit_code}</span>}
        <div className="ml-auto flex items-center gap-2">
          <label className="hint flex cursor-pointer items-center gap-1.5">
            <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} />
            Follow
          </label>
          {state === "running" && (
            <button type="button" className="btn btn-sm btn-danger" onClick={() => api.cancel(jobId).catch(() => {})}>
              {Icon.stop({ size: 12 })} Stop
            </button>
          )}
          <button type="button" className="btn btn-sm btn-ghost btn-icon" aria-label="Close console" onClick={onClose}>
            {Icon.x({ size: 14 })}
          </button>
        </div>
      </header>

      <div
        ref={box}
        className="console min-h-0 flex-1 overflow-y-auto p-3"
        // Scrolling up is an intent to read something; taking the view back to
        // the bottom on the next line is the single most irritating thing a
        // log viewer can do.
        onScroll={(e) => {
          const el = e.currentTarget;
          setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 40);
        }}
      >
        {lines.length === 0 && <p className="meta">waiting for output…</p>}
        {lines.map((l) => (
          <div key={l.seq} className={l.stream}>
            {l.text || " "}
          </div>
        ))}
      </div>
    </div>
  );
}
