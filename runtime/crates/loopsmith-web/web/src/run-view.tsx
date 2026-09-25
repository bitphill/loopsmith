/**
 * What a run is doing, above the log of what it has said.
 *
 * A console answers "what happened" and is bad at "where is it now". Both
 * questions have answers in the same stream — the run mirrors its ledger to
 * stderr and the server reads it back into [`RunEvent`]s — so this draws the
 * two things the text is worst at showing.
 *
 * **The trail** is the run's actual path through the lifecycle, built from the
 * transitions it reported. Deliberately not a drawing of the state machine
 * with the current state lit: that would mean a second copy of `RunState`'s
 * successor table living in TypeScript, and a run cannot take a path it did
 * not take.
 *
 * **The waves** come from the plan the review rail already has — the same
 * schedule the engine derived from the graph — with each node coloured by the
 * last thing the run said about it. Which is the point of the whole panel: a
 * wave of four builders running at once is the thing loopsmith does that is
 * hardest to see in a log, because the four of them interleave.
 */
import type { PlanView, RunEvent } from "./types";

/** How a node is doing, as of the last event that mentioned it. */
type NodeState = "waiting" | "running" | "done" | "failed";

const NODE_TONE: Record<NodeState, string> = {
  waiting: "border-dashed text-faint",
  running: "border-[var(--ember)] text-text running",
  done: "border-[var(--good)] text-good",
  failed: "border-[var(--bad)] text-bad",
};

/** The states that mean the run is over, for colouring the end of the trail. */
const ENDED: Record<string, string> = {
  succeeded: "chip-good",
  failed: "chip-bad",
  blocked: "chip-bad",
  rolled_back: "chip-bad",
  escalated: "chip-warn",
  paused: "chip-warn",
  closed: "chip",
};

/**
 * Fold the event stream into what the panel draws.
 *
 * One pass, because the events arrive in order and every question here is
 * about the latest thing said rather than about the history.
 */
export function digest(events: RunEvent[]) {
  const trail: { state: string; why: string | null }[] = [];
  const nodes = new Map<string, NodeState>();
  let iteration = 0;
  let satisfied: string | null = null;
  let stopped: string | null = null;

  for (const e of events) {
    if (e.iteration > iteration) iteration = e.iteration;
    if (e.state) {
      const why = e.detail.includes(":") ? e.detail.slice(e.detail.indexOf(":") + 1).trim() : null;
      // The first transition is the only one that says where the run began,
      // and `created →` is half of it. Without this the trail opens on
      // `validating`, which reads as though something was missed.
      if (trail.length === 0) {
        const from = e.detail.split("→")[0].trim();
        if (from) trail.push({ state: from, why: null });
      }
      trail.push({ state: e.state, why });
      // A new iteration re-dispatches everything, so the previous pass's
      // verdicts are not this pass's.
      if (e.state === "running") nodes.clear();
    }
    if (e.node) {
      if (e.kind === "NodeDispatched") nodes.set(e.node, "running");
      else if (e.kind === "NodeSucceeded") nodes.set(e.node, "done");
      else if (e.kind === "NodeFailed") nodes.set(e.node, "failed");
    }
    if (e.kind === "IterationStarted") nodes.clear();
    // The gate's own closing line for an iteration: "… 2/4 target(s)
    // satisfied." It is the one number that says whether the run is getting
    // anywhere, and it is buried in a sentence.
    const count = /(\d+)\/(\d+) target\(s\) satisfied/.exec(e.detail);
    if (count) satisfied = `${count[1]}/${count[2]}`;
    if (e.kind === "StopGateTriggered") stopped = e.detail;
  }

  return { trail, nodes, iteration, satisfied, stopped };
}

export function RunView({
  events, plan, live,
}: {
  events: RunEvent[];
  plan: PlanView | null;
  /** Whether the subprocess is still going. */
  live: boolean;
}) {
  const { trail, nodes, iteration, satisfied, stopped } = digest(events);
  const waves = plan?.waves ?? [];
  const critical = new Set(plan?.critical_path ?? []);

  // Nothing has been said yet. A panel of empty boxes reads as broken, and the
  // console underneath is already saying "waiting for output".
  if (trail.length === 0 && waves.length === 0) return null;

  return (
    <section className="shrink-0 space-y-2.5 border-b p-2.5" aria-label="Run progress">
      {trail.length > 0 && (
        <div>
          <p className="label">Where it is</p>
          <ol className="mt-1 flex flex-wrap items-center gap-1">
            {trail.map((t, i) => {
              const last = i === trail.length - 1;
              const tone = last ? ENDED[t.state] ?? "chip-ember" : "chip";
              return (
                <li key={`${t.state}-${i}`} className="flex items-center gap-1">
                  {i > 0 && <span aria-hidden="true" className="text-faint">→</span>}
                  <span className={`chip ${tone} font-mono`}>{t.state.replace(/_/g, " ")}</span>
                </li>
              );
            })}
          </ol>
          {trail[trail.length - 1]?.why && (
            <p className="hint mt-1">{trail[trail.length - 1].why}</p>
          )}
        </div>
      )}

      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        {iteration > 0 && (
          <span className="hint">
            iteration <span className="tabular font-semibold text-text">{iteration}</span>
          </span>
        )}
        {satisfied && (
          <span className="hint">
            <span className="tabular font-semibold text-text">{satisfied}</span> satisfied
          </span>
        )}
        {plan && plan.concurrency > 1 && (
          <span className="hint">
            up to <span className="tabular font-semibold text-text">{plan.concurrency}</span> at once
          </span>
        )}
      </div>

      {waves.length > 0 && (
        <div>
          <p className="label">
            Waves
            <span className="ml-1.5 font-normal text-faint">
              everything in one row runs together
            </span>
          </p>
          <div className="mt-1 space-y-1">
            {waves.map((wave, i) => (
              <div key={i} className="flex items-start gap-1.5">
                <span className="mt-1 w-4 shrink-0 text-right font-mono text-[10px] text-faint">
                  {i + 1}
                </span>
                <div className="flex flex-wrap gap-1">
                  {wave.map((id) => {
                    let state = nodes.get(id) ?? "waiting";
                    // A node that was dispatched and never reported back is
                    // not still working once the process has exited — a dry
                    // run says "would dispatch" and stops there, and leaving
                    // it pulsing would claim work that never happened.
                    if (!live && state === "running") state = "waiting";
                    return (
                      <span
                        key={id}
                        // The critical path is the chain that decides how long
                        // the whole run takes; a node on it being slow is the
                        // one worth looking at.
                        title={critical.has(id) ? `${id} — on the critical path` : id}
                        className={`chip border font-mono ${NODE_TONE[state]} ${
                          critical.has(id) ? "font-bold" : ""
                        }`}
                      >
                        {id}
                      </span>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {stopped && (
        <p className="stripe stripe-note py-1 text-[11.5px]">
          <span className="font-semibold">Stopped: </span>
          {stopped}
        </p>
      )}
    </section>
  );
}
