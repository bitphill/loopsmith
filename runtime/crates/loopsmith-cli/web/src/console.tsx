/**
 * The action bar and the live run console.
 *
 * Every button here spawns the real `loopsmith` binary. The console follows a
 * job over a WebSocket that replays what has already been printed before
 * going live, so opening it halfway through a run still shows the whole run.
 */
import { useEffect, useRef, useState } from "react";
import { api, streamJob } from "./api";
import { Icon } from "./ui";
import type { JobLine, JobSummary } from "./types";

export type ActionId =
  | "create" | "validate" | "plan" | "dry_run" | "run"
  | "watch" | "schedule_install" | "permissions_write" | "skills_install";

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

      {state === "running" && (
        <p className="hint border-b px-2.5 py-1.5">
          This is running inside the loopsmith server, not this page. Close the tab or the whole
          browser and it keeps going — reopen and the log picks up from the start. It stops if you
          stop the server in the terminal; for something that must outlive that, use{" "}
          <span className="font-semibold">Install schedule</span>.
        </p>
      )}

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
