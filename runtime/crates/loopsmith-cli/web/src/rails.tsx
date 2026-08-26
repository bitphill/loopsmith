/**
 * The two side rails: examples and saved loops on the left, live review on
 * the right.
 *
 * The right rail is the reason the form is usable. Every edit is re-validated,
 * re-planned, re-priced, and re-permissioned in the same process, so the
 * consequences of a change are visible next to the change rather than after
 * pressing run.
 */
import { useState } from "react";
import { Icon, Note } from "./ui";
import { PickFolder } from "./setup";
import type { ExampleCard, LibraryEntry, Review } from "./types";

/* --- left: examples and saved loops -------------------------------------- */

export function LeftRail({
  examples, library, onLoad, onOpen, onForget, loadingId,
}: {
  examples: ExampleCard[];
  library: LibraryEntry[];
  onLoad: (id: string) => void;
  onOpen: (path: string) => void;
  onForget: (path: string) => void;
  loadingId: string | null;
}) {
  const [tab, setTab] = useState<"examples" | "library">("examples");
  const [q, setQ] = useState("");

  const filtered = examples.filter((e) =>
    !q || `${e.name} ${e.blurb}`.toLowerCase().includes(q.toLowerCase()));

  return (
    <aside className="flex h-full min-h-0 flex-col border-r bg-surface">
      <div className="flex shrink-0 gap-1 border-b p-2">
        {(["examples", "library"] as const).map((t) => (
          <button key={t} type="button" onClick={() => setTab(t)}
            className={`btn btn-sm flex-1 ${tab === t ? "btn-primary" : "btn-ghost"}`}>
            {t === "examples" ? "Examples" : `Your loops${library.length ? ` (${library.length})` : ""}`}
          </button>
        ))}
      </div>

      {tab === "examples" && (
        <>
          <div className="shrink-0 p-2.5">
            <input className="input" placeholder="Filter examples…" value={q}
              onChange={(e) => setQ(e.target.value)} aria-label="Filter examples" />
            <p className="hint mt-2">
              Thirteen working loops. Loading one fills in every section, which is the quickest
              way to see what a validation or a stop gate looks like in practice.
            </p>
          </div>
          <div className="min-h-0 flex-1 space-y-2 overflow-y-auto px-2.5 pb-3">
            {filtered.map((e) => (
              <article key={e.id} className="card p-3">
                <div className="flex items-start justify-between gap-2">
                  <h3 className="text-[13px] font-bold tracking-tight">{e.name}</h3>
                  <button type="button" className="btn btn-sm shrink-0" disabled={loadingId === e.id}
                    onClick={() => onLoad(e.id)}>
                    {loadingId === e.id ? "Loading…" : "Load"}
                  </button>
                </div>
                <p className="hint mt-1">{e.blurb}</p>
                <div className="mt-2 flex flex-wrap gap-1">
                  <span className="chip tabular">{e.goals} goal{e.goals === 1 ? "" : "s"}</span>
                  <span className="chip tabular">{e.validations} check{e.validations === 1 ? "" : "s"}</span>
                  {e.nodes > 0 && <span className="chip tabular">{e.nodes} node{e.nodes === 1 ? "" : "s"}</span>}
                  <span className="chip">{e.trigger}</span>
                  {e.judge_validations > 0 && (
                    <span className="chip chip-warn" title="Checks decided by a model rather than by a script">
                      {e.judge_validations} judged
                    </span>
                  )}
                  {e.max_cost_usd != null && <span className="chip chip-good tabular">${e.max_cost_usd} cap</span>}
                  {e.origin !== "embedded" && <span className="chip chip-quench">{e.origin}</span>}
                </div>
              </article>
            ))}
            {filtered.length === 0 && <p className="hint px-1">Nothing matches “{q}”.</p>}
          </div>
        </>
      )}

      {tab === "library" && (
        <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
          <OpenByPath onOpen={onOpen} />
          <div className="mt-3 space-y-2">
            {library.length === 0 && (
              <p className="hint">
                No loops yet. Create one and it appears here, so you never have to remember the
                path again.
              </p>
            )}
            {library.map((l) => (
              <article key={l.path} className="card p-3">
                <div className="flex items-start justify-between gap-2">
                  <h3 className="min-w-0 text-[13px] font-bold tracking-tight">{l.name}</h3>
                  <div className="flex shrink-0 gap-1">
                    <button type="button" className="btn btn-sm" onClick={() => onOpen(l.path)}>Open</button>
                    <button type="button" className="btn btn-sm btn-ghost btn-icon" aria-label={`Forget ${l.name}`}
                      onClick={() => onForget(l.path)}>{Icon.x({ size: 13 })}</button>
                  </div>
                </div>
                <p className="hint mt-1 break-all font-mono text-[11px]">{l.path}</p>
              </article>
            ))}
          </div>
        </div>
      )}
    </aside>
  );
}

function OpenByPath({ onOpen }: { onOpen: (path: string) => void }) {
  const [path, setPath] = useState("");
  return (
    <form
      onSubmit={(e) => { e.preventDefault(); if (path.trim()) onOpen(path.trim()); }}
      className="card p-3"
    >
      <label className="label mb-1.5 block" htmlFor="open-path">Open a loop by path</label>
      <div className="flex items-start gap-2">
        <input id="open-path" className="input mono min-w-0 flex-1" placeholder="~/loops/blog-pipeline"
          value={path} onChange={(e) => setPath(e.target.value)} />
        <PickFolder
          startIn={path || undefined}
          label="Browse for a loop folder"
          // Opening is the whole point of this control, so a pick goes straight
          // through rather than filling the box and waiting for a second click.
          onPick={(p) => { setPath(p); onOpen(p); }}
        />
        <button type="submit" className="btn btn-sm shrink-0" disabled={!path.trim()}>Open</button>
      </div>
      <p className="hint mt-1.5">Browse to a folder, or type the path to one.</p>
    </form>
  );
}

/* --- right: live review -------------------------------------------------- */

export function ReviewRail({ review, onJump }: { review: Review | null; onJump: (field: string) => void }) {
  if (!review) {
    return <aside className="border-l bg-surface p-3"><p className="hint">Checking…</p></aside>;
  }

  const money = (n: number) =>
    n >= 100 ? `$${Math.round(n)}` : `$${n.toFixed(2)}`;

  return (
    <aside className="flex h-full min-h-0 flex-col overflow-y-auto border-l bg-surface">
      <div className="sticky top-0 z-10 border-b bg-surface p-3">
        <div className="flex flex-wrap items-center gap-1.5">
          {review.parsed && review.error_count === 0 ? (
            <span className="chip chip-good">{Icon.check({ size: 12 })} ready to create</span>
          ) : (
            <span className="chip chip-bad">{review.error_count || 1} error{review.error_count === 1 ? "" : "s"}</span>
          )}
          {review.warning_count > 0 && <span className="chip chip-warn">{review.warning_count} warning{review.warning_count === 1 ? "" : "s"}</span>}
        </div>
      </div>

      <div className="space-y-4 p-3">
        {review.parse_error && (
          <Note tone="error">
            <span className="font-semibold">This config cannot be read.</span>
            <br />
            <span className="font-mono text-[11px]">{review.parse_error}</span>
          </Note>
        )}

        {review.issues.length > 0 && (
          <section>
            <h3 className="label mb-2">Problems</h3>
            <ul className="space-y-2">
              {review.issues.map((i, n) => (
                <li key={n}>
                  <button type="button" onClick={() => onJump(i.field)}
                    className={`stripe stripe-${i.severity} block w-full py-0.5 text-left`}>
                    <span className="block font-mono text-[11px] text-faint">{i.field}</span>
                    <span className="hint block">{i.message}</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}

        <section>
          <h3 className="label mb-2">What a run could cost</h3>
          <div className="card p-3">
            <p className="text-[22px] font-bold tabular tracking-tight">
              {review.cost.ceiling_usd != null
                ? money(review.cost.ceiling_usd)
                : review.cost.worst_case_usd != null
                  ? `≤ ${money(review.cost.worst_case_usd)}`
                  : "unbounded"}
            </p>
            <p className="hint mt-1">{review.cost.basis}</p>
          </div>
        </section>

        {review.plan && (
          <section>
            <h3 className="label mb-2">How it will run</h3>
            {review.plan.error ? (
              <Note tone="error">{review.plan.error}</Note>
            ) : (
              <div className="card p-3">
                <div className="mb-3 flex gap-4">
                  <div>
                    <p className="text-[11px] uppercase tracking-wide text-faint">Workers</p>
                    <p className="text-[18px] font-bold tabular">{review.plan.concurrency}</p>
                  </div>
                  <div>
                    <p className="text-[11px] uppercase tracking-wide text-faint">Speedup</p>
                    <p className="text-[18px] font-bold tabular">×{review.plan.predicted_speedup.toFixed(2)}</p>
                  </div>
                  <div>
                    <p className="text-[11px] uppercase tracking-wide text-faint">Ceiling</p>
                    <p className="text-[18px] font-bold tabular text-faint">×{review.plan.speedup_ceiling.toFixed(2)}</p>
                  </div>
                </div>
                <div className="space-y-1.5">
                  {review.plan.waves.map((w, i) => (
                    <div key={i} className="flex items-baseline gap-2">
                      <span className="w-12 shrink-0 font-mono text-[10.5px] text-faint">wave {i + 1}</span>
                      <span className="flex flex-wrap gap-1">
                        {w.map((n) => (
                          <span key={n} className="chip font-mono">{n}</span>
                        ))}
                      </span>
                    </div>
                  ))}
                </div>
                {review.plan.critical_path.length > 0 && (
                  <p className="hint mt-3">
                    <span className="font-semibold">Longest chain:</span>{" "}
                    <span className="font-mono">{review.plan.critical_path.join(" → ")}</span>. No
                    number of workers can beat this, which is where the ceiling comes from.
                  </p>
                )}
                {review.plan.unisolated_parallel_writers.length > 0 && (
                  <div className="mt-3">
                    <Note tone="error">
                      <span className="font-semibold">These run at the same time and both write, with no worktree each:</span>{" "}
                      <span className="font-mono">{review.plan.unisolated_parallel_writers.join(", ")}</span>. They
                      will overwrite each other. Turn on Isolated for them, or make one depend on the other.
                    </Note>
                  </div>
                )}
              </div>
            )}
          </section>
        )}

        {review.notes.length > 0 && (
          <section>
            <h3 className="label mb-2">Worth knowing</h3>
            <div className="space-y-2">
              {review.notes.map((n, i) => <Note key={i} tone="ember">{n}</Note>)}
            </div>
          </section>
        )}

        {review.permissions.length > 0 && (
          <section>
            <h3 className="label mb-2">Permissions this needs</h3>
            <div className="flex flex-wrap gap-1">
              {review.permissions.map((p) => <span key={p} className="chip font-mono">{p}</span>)}
            </div>
            <p className="hint mt-2">
              Nothing outside this list is requested. Human checkpoints still stop and wait,
              grant or no grant.
            </p>
          </section>
        )}
      </div>
    </aside>
  );
}
