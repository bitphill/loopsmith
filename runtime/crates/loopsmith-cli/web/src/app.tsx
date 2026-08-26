/**
 * The shell.
 *
 * Layout is three columns and a footer: examples and saved loops on the left,
 * the A–J form down the middle, live review on the right, actions along the
 * bottom. The run console slides in over the right rail when a job starts,
 * because during a run that is the only thing worth looking at.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { HelpProvider, Icon, Dialog, Note } from "./ui";
import { LeftRail, ReviewRail } from "./rails";
import { ActionBar, RunConsole, type ActionId } from "./console";
import { Location, Secrets, Preflight } from "./setup";
import {
  Information, PreExecution, Goals, Validations, Success, StopGatesSection,
} from "./sections-core";
import {
  Schedules, ConstraintsSection, Guidelines, Skills, Graph, Providers, Context,
} from "./sections-run";
import { Tour } from "./tour";
import type {
  LoopConfig, Detection, Help, SectionHelp, Review, ExampleCard, LibraryEntry,
  Format, PathFacts, JobSummary, ProviderSpec, Meta,
} from "./types";

const BLANK: LoopConfig = {
  name: "",
  version: "0.1.0",
  description: "",
  goals: [],
  validations: [],
};

/** Has the user put anything in yet? Decides whether Load needs a warning. */
function isEmpty(c: LoopConfig): boolean {
  return (
    !c.name.trim() &&
    !(c.description ?? "").trim() &&
    (c.goals?.length ?? 0) === 0 &&
    (c.validations?.length ?? 0) === 0 &&
    (c.information?.length ?? 0) === 0 &&
    (c.pre_execution?.length ?? 0) === 0 &&
    (c.graph?.nodes?.length ?? 0) === 0 &&
    (c.providers?.providers?.length ?? 0) === 0
  );
}

/** Merge that keeps whatever the user has already typed. */
function fillBlanks(current: LoopConfig, incoming: LoopConfig): LoopConfig {
  const out: LoopConfig = { ...incoming, ...current };
  const keep = <K extends keyof LoopConfig>(k: K) => {
    const mine = current[k] as unknown;
    const theirs = incoming[k] as unknown;
    const empty =
      mine == null ||
      (typeof mine === "string" && !mine.trim()) ||
      (Array.isArray(mine) && mine.length === 0);
    if (empty && theirs != null) (out[k] as unknown) = theirs;
  };
  (Object.keys(incoming) as (keyof LoopConfig)[]).forEach(keep);
  return out;
}

type ThemeChoice = "system" | "light" | "dark";

export default function App() {
  const [cfg, setCfg] = useState<LoopConfig>(BLANK);
  const [path, setPath] = useState("");
  const [format, setFormat] = useState<Format>("yaml");
  const [created, setCreated] = useState(false);

  const [detection, setDetection] = useState<Detection | null>(null);
  const [scanning, setScanning] = useState(false);
  const [help, setHelp] = useState<Help>({ sections: [], fields: [] });
  const [examples, setExamples] = useState<ExampleCard[]>([]);
  const [library, setLibrary] = useState<LibraryEntry[]>([]);
  const [meta, setMeta] = useState<Meta | null>(null);

  const [review, setReview] = useState<Review | null>(null);
  const [facts, setFacts] = useState<PathFacts | null>(null);
  const [job, setJob] = useState<string | null>(null);
  const [toast, setToast] = useState<{ tone: "good" | "bad"; text: string } | null>(null);

  const [pendingLoad, setPendingLoad] = useState<{ id: string; incoming: LoopConfig } | null>(null);
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [testing, setTesting] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, { ok: boolean; text: string }>>({});

  const [theme, setTheme] = useState<ThemeChoice>(
    () => (localStorage.getItem("loopsmith-theme") as ThemeChoice) || "system",
  );
  const [tourDone, setTourDone] = useState(() => localStorage.getItem("loopsmith-tour") === "done");

  const patch = useCallback(
    (p: Partial<LoopConfig>) => setCfg((c) => ({ ...c, ...p })),
    [],
  );

  /* --- theme ------------------------------------------------------------- */
  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    localStorage.setItem("loopsmith-theme", theme);
  }, [theme]);

  /* --- first load -------------------------------------------------------- */
  useEffect(() => {
    api.help().then(setHelp).catch(() => {});
    api.examples().then(setExamples).catch(() => {});
    api.library().then(setLibrary).catch(() => {});
    api.meta().then(setMeta).catch(() => {});
    setScanning(true);
    api.detect(false).then(setDetection).catch(() => {}).finally(() => setScanning(false));
  }, []);

  /* --- live review, debounced ------------------------------------------- */
  const reviewTimer = useRef<number>(0);
  useEffect(() => {
    window.clearTimeout(reviewTimer.current);
    reviewTimer.current = window.setTimeout(() => {
      api.review(cfg).then(setReview).catch(() => {});
    }, 220);
    return () => window.clearTimeout(reviewTimer.current);
  }, [cfg]);

  /* --- path facts, debounced -------------------------------------------- */
  const pathTimer = useRef<number>(0);
  useEffect(() => {
    if (!path.trim()) { setFacts(null); return; }
    window.clearTimeout(pathTimer.current);
    pathTimer.current = window.setTimeout(() => {
      api.pathFacts(path).then(setFacts).catch(() => {});
    }, 300);
    return () => window.clearTimeout(pathTimer.current);
  }, [path]);

  /* --- name follows the folder, until the user says otherwise ----------- */
  useEffect(() => {
    if (cfg.name.trim()) return;
    const leaf = path.trim().replace(/\/+$/, "").split("/").pop();
    if (leaf && leaf !== "~") patch({ name: leaf });
  }, [path, cfg.name, patch]);

  const sectionHelp = useMemo(
    () => new Map<string, SectionHelp>(help.sections.map((s) => [s.key, s])),
    [help.sections],
  );

  /* --- loading an example ----------------------------------------------- */
  const loadExample = async (id: string) => {
    setLoadingId(id);
    try {
      const { config } = await api.example(id);
      if (isEmpty(cfg)) applyLoaded(config, id);
      else setPendingLoad({ id, incoming: config });
    } catch (e) {
      setToast({ tone: "bad", text: (e as Error).message });
    } finally {
      setLoadingId(null);
    }
  };

  const applyLoaded = (config: LoopConfig, id: string) => {
    setCfg(config);
    setCreated(false);
    if (!path.trim()) setPath(`~/loops/${id}`);
    setToast({ tone: "good", text: `Loaded ${config.name}. Nothing is on disk yet — press Create loop when it looks right.` });
  };

  const openLoop = async (p: string) => {
    try {
      const res = await api.open(p);
      setCfg(res.config);
      setFormat(res.format);
      setPath(res.dir);
      setFacts(res.facts);
      setCreated(true);
      setToast({ tone: "good", text: `Opened ${res.config_path}.` });
    } catch (e) {
      setToast({ tone: "bad", text: (e as Error).message });
    }
  };

  /* --- actions ----------------------------------------------------------- */
  const run = async (action: ActionId, opts?: { force?: boolean }) => {
    const dir = path.trim();
    const configFile = `${dir}/loop.${format === "markdown" ? "md" : "yaml"}`;
    const body: Record<string, unknown> = { cwd: dir || ".", action };

    switch (action) {
      case "create":
        body.action = "create";
        body.path = dir;
        body.name = cfg.name;
        body.purpose = cfg.description ?? "";
        body.config_file = "";
        body.force = opts?.force ?? false;
        body.draft = { config: cfg, format };
        break;
      case "permissions_write":
        body.config = configFile;
        body.settings = `${dir}/.claude/settings.local.json`;
        break;
      case "validate":
      case "plan":
      case "dry_run":
      case "run":
      case "watch":
      case "schedule_install":
      case "skills_install":
        body.config = configFile;
        break;
    }

    // Check and Plan work on the draft before anything is on disk, so they get
    // a scratch file instead of a path that does not exist yet.
    if (!created && (action === "validate" || action === "plan")) {
      body.draft = { config: cfg, format };
      body.config = configFile;
    }

    try {
      const { job: id } = await api.start(body);
      setJob(id);
    } catch (e) {
      setToast({ tone: "bad", text: (e as Error).message });
    }
  };

  const onFinished = useCallback((s: JobSummary) => {
    if (s.kind === "create" && s.state === "succeeded") {
      setCreated(true);
      api.library().then(setLibrary).catch(() => {});
      api.pathFacts(path).then(setFacts).catch(() => {});
      setToast({ tone: "good", text: "Loop created. The run buttons are unlocked." });
    }
    if (s.state === "failed") {
      setToast({ tone: "bad", text: `${s.kind} exited ${s.exit_code ?? "?"} — the console has the detail.` });
    }
  }, [path]);

  const testProvider = async (p: ProviderSpec) => {
    setTesting(p.id);
    try {
      const r = await api.handshake(p.command, p.args ?? [], p.prompt_on_stdin ?? false);
      setTestResults((all) => ({
        ...all,
        [p.id]: {
          ok: r.ok,
          text: r.ok ? `answered in ${(r.elapsed_ms / 1000).toFixed(1)}s` : (r.error ?? "no answer"),
        },
      }));
    } catch (e) {
      setTestResults((all) => ({ ...all, [p.id]: { ok: false, text: (e as Error).message } }));
    } finally {
      setTesting(null);
    }
  };

  const jump = (field: string) => {
    const key = field.split(/[.[]/)[0];
    document.getElementById(`section-${key}`)?.scrollIntoView({ block: "start" });
  };

  const neededKeys = useMemo(
    () => (cfg.providers?.providers ?? []).flatMap((p) => p.requires_env ?? []),
    [cfg.providers],
  );

  const sectionProps = { cfg, patch, help: sectionHelp };

  return (
    <HelpProvider fields={help.fields}>
      <div className="grid h-screen grid-rows-[auto_1fr] overflow-hidden">
        {/* --- top bar --- */}
        <header className="flex items-center gap-3 border-b bg-surface px-4 py-2.5">
          <img
            src="/logo.png"
            alt=""
            width={30}
            height={30}
            className="shrink-0 select-none"
            // Decorative: the wordmark beside it already carries the name, so
            // announcing the image too would just say "loopsmith" twice.
            aria-hidden="true"
            draggable={false}
          />
          <h1 className="forge-mark text-[18px] leading-none">loopsmith</h1>
          <span className="chip font-mono">{meta?.version ?? "…"}</span>
          <span className="hidden text-[12px] text-faint md:block">
            build a loop, check it, then let it run
          </span>
          <div className="ml-auto flex items-center gap-2">
            {detection && (
              <span className="chip" title={detection.agents.map((a) => a.label).join(", ")}>
                {detection.agents.length} agent CLI{detection.agents.length === 1 ? "" : "s"}
              </span>
            )}
            <button type="button" className="btn btn-sm btn-ghost" onClick={() => setTourDone(false)}>
              How this works
            </button>
            <div className="flex rounded-[10px] border p-0.5" role="group" aria-label="Theme">
              {([
                { id: "light", label: "Light theme" },
                // The visible text is "auto", so the accessible name has to
                // contain "auto" — a control that reads one way and announces
                // another is unusable by voice control (WCAG 2.5.3).
                { id: "system", label: "Auto theme, follow the system" },
                { id: "dark", label: "Dark theme" },
              ] as const).map((opt) => (
                <button key={opt.id} type="button" aria-label={opt.label} aria-pressed={theme === opt.id}
                  className={`btn btn-sm ${theme === opt.id ? "btn-primary" : "btn-ghost"} px-2`}
                  onClick={() => setTheme(opt.id)}>
                  {opt.id === "light" ? Icon.sun({ size: 13 })
                    : opt.id === "dark" ? Icon.moon({ size: 13 })
                    : <span className="text-[11px]">auto</span>}
                </button>
              ))}
            </div>
          </div>
        </header>

        {/* --- body --- */}
        <div className="grid min-h-0 grid-cols-1 lg:grid-cols-[19rem_1fr_23rem]">
          <div className="hidden min-h-0 lg:block">
            <LeftRail
              examples={examples}
              library={library}
              onLoad={loadExample}
              onOpen={openLoop}
              onForget={(p) => api.forget(p).then(() => api.library().then(setLibrary)).catch(() => {})}
              loadingId={loadingId}
            />
          </div>

          <div className="grid min-h-0 grid-rows-[1fr_auto]">
            <main className="min-h-0 space-y-4 overflow-y-auto bg-ground p-4">
              <Location
                path={path} onPath={setPath} cfg={cfg} patch={patch}
                format={format} onFormat={setFormat} facts={facts}
                permissions={review?.permissions ?? []}
              />
              <Providers
                {...sectionProps} detection={detection}
                onTest={testProvider} testing={testing} results={testResults}
              />
              <Secrets detection={detection} needed={neededKeys} />
              <Information {...sectionProps} />
              <PreExecution {...sectionProps} />
              <Goals {...sectionProps} />
              <Validations {...sectionProps} />
              <Success {...sectionProps} />
              <StopGatesSection {...sectionProps} />
              <Graph {...sectionProps} />
              <Schedules {...sectionProps} />
              <ConstraintsSection {...sectionProps} />
              <Guidelines {...sectionProps} />
              <Skills {...sectionProps} detection={detection} />
              <Context {...sectionProps} />
              <Preflight
                detection={detection}
                scanning={scanning}
                onRescan={(deep) => {
                  setScanning(true);
                  api.detect(deep).then(setDetection).catch(() => {}).finally(() => setScanning(false));
                }}
              />
            </main>
            <div>
              {/* Above the action bar, in the flow. Fixed positioning put it on
                  top of the buttons the moment the bar wrapped to two rows. */}
              {toast && (
                <div className="rise border-t bg-surface px-4 pt-3">
                  <div className="card flex items-start gap-2.5 p-3">
                    <span className={toast.tone === "good" ? "text-good" : "text-bad"}>
                      {toast.tone === "good" ? Icon.check({ size: 16 }) : Icon.x({ size: 16 })}
                    </span>
                    <p className="hint flex-1 text-text">{toast.text}</p>
                    <button className="btn btn-ghost btn-sm btn-icon" aria-label="Dismiss" onClick={() => setToast(null)}>
                      {Icon.x({ size: 13 })}
                    </button>
                  </div>
                </div>
              )}
              <ActionBar review={review} facts={facts} created={created} onRun={run} running={false} />
            </div>
          </div>

          <div className="hidden min-h-0 lg:block">
            {job ? (
              <div className="h-full border-l bg-surface">
                <RunConsole jobId={job} onClose={() => setJob(null)} onFinished={onFinished} />
              </div>
            ) : (
              <ReviewRail review={review} onJump={jump} />
            )}
          </div>
        </div>
      </div>

      {/* --- load confirmation (Q18: replace all, or fill blanks only) --- */}
      {pendingLoad && (
        <Dialog
          title="You have already filled some of this in"
          onClose={() => setPendingLoad(null)}
          actions={
            <>
              <button className="btn" onClick={() => setPendingLoad(null)}>Cancel</button>
              <button className="btn" onClick={() => {
                setCfg((c) => fillBlanks(c, pendingLoad.incoming));
                setToast({ tone: "good", text: "Filled the empty sections. What you had typed is untouched." });
                setPendingLoad(null);
              }}>
                Fill blanks only
              </button>
              <button className="btn btn-primary" onClick={() => {
                applyLoaded(pendingLoad.incoming, pendingLoad.id);
                setPendingLoad(null);
              }}>
                Replace everything
              </button>
            </>
          }
        >
          <p className="hint">
            Loading <span className="font-semibold">{pendingLoad.incoming.name}</span> can either
            replace what is in the form, or only fill the parts you have left empty.
          </p>
          <div className="mt-3">
            <Note tone="warning">
              Replacing discards everything currently in the form. Nothing on disk changes either
              way — this is only the draft in front of you.
            </Note>
          </div>
        </Dialog>
      )}

      {!tourDone && <Tour onClose={() => { setTourDone(true); localStorage.setItem("loopsmith-tour", "done"); }} />}

    </HelpProvider>
  );
}
