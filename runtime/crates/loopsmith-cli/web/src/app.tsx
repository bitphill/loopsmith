/**
 * The shell.
 *
 * This started as sixteen config cards stacked in one scroll, three columns
 * wide, with nine buttons live at the bottom. Everything was reachable and
 * nothing was findable — the classic shape of a form that grew a section at a
 * time.
 *
 * It is now one step at a time. Six steps, in the order you would actually
 * think about the problem: where it lives, what does the work, what you want,
 * how it is checked, how the work is arranged, and when it runs. Only the
 * actions that make sense for the current step are on screen, and ⌘K reaches
 * anything at all — which is what makes hiding the rest reasonable rather than
 * obstructive.
 *
 * The right rail still watches everything, because the consequences of an edit
 * belong next to the edit and not three steps later.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { HelpProvider, Icon, Dialog, Note } from "./ui";
import {
  MorphPanel, StepBar, StatusIsland, PaletteProvider, usePalette, Reveal,
  CountUp, useShake, motion, type Step, type Command, type IslandState,
} from "./motion";
import { LeftRail, ReviewRail } from "./rails";
import { RunConsole, type ActionId } from "./console";
import { Location, Secrets, Preflight } from "./setup";
import { Information, PreExecution, Goals, Validations, Success, StopGatesSection } from "./sections-core";
import { Schedules, ConstraintsSection, Guidelines, Skills, Graph, Providers, Context } from "./sections-run";
import { Tour } from "./tour";
import type {
  LoopConfig, Detection, Help, SectionHelp, Review, ExampleCard, LibraryEntry,
  Format, PathFacts, JobSummary, ProviderSpec, Meta,
} from "./types";

const BLANK: LoopConfig = { name: "", version: "0.1.0", description: "", goals: [], validations: [] };

/** Which config keys each step owns, so a problem can route to a step. */
const STEP_KEYS: Record<string, string[]> = {
  place: ["name", "description", "version"],
  power: ["providers"],
  intent: ["information", "pre_execution", "goals"],
  proof: ["validations", "success", "stop_gates"],
  work: ["graph", "execution_guidelines", "default_skills", "constraints", "context", "skills"],
  ship: ["schedules"],
};

/** Actions worth offering on each step, in the order they are usually wanted. */
const STEP_ACTIONS: Record<string, ActionId[]> = {
  place: [],
  power: [],
  intent: ["validate"],
  proof: ["validate"],
  work: ["validate", "plan"],
  ship: ["validate", "plan", "create", "permissions_write", "skills_install", "dry_run", "run", "watch", "schedule_install"],
};

const ACTION_LABEL: Record<ActionId, { label: string; note: string; spends: boolean }> = {
  validate: { label: "Check config", note: "Reports every problem. Changes nothing, costs nothing.", spends: false },
  plan: { label: "Show the plan", note: "Waves, longest chain, predicted speedup. Runs nothing.", spends: false },
  create: { label: "Create loop", note: "Writes the loop and its state directory. This is the one that makes it real.", spends: false },
  dry_run: { label: "Dry run", note: "Walks the whole loop without calling a single model.", spends: false },
  run: { label: "Run once", note: "A real run. Calls models and spends money up to your ceilings.", spends: true },
  watch: { label: "Watch", note: "Stays resident and runs whenever a trigger fires.", spends: true },
  schedule_install: { label: "Install schedule", note: "Hands the schedule to launchd or cron so it survives a reboot.", spends: false },
  permissions_write: { label: "Grant permissions", note: "Merges the derived grant into .claude/settings.local.json.", spends: false },
  skills_install: { label: "Install sub-agents", note: "Installs everything section J declares. Idempotent.", spends: false },
};

function isEmpty(c: LoopConfig): boolean {
  return (
    !c.name.trim() && !(c.description ?? "").trim() &&
    (c.goals?.length ?? 0) === 0 && (c.validations?.length ?? 0) === 0 &&
    (c.information?.length ?? 0) === 0 && (c.pre_execution?.length ?? 0) === 0 &&
    (c.graph?.nodes?.length ?? 0) === 0 && (c.providers?.providers?.length ?? 0) === 0
  );
}

function fillBlanks(current: LoopConfig, incoming: LoopConfig): LoopConfig {
  const out: LoopConfig = { ...incoming, ...current };
  (Object.keys(incoming) as (keyof LoopConfig)[]).forEach((k) => {
    const mine = current[k] as unknown;
    const empty = mine == null || (typeof mine === "string" && !mine.trim()) || (Array.isArray(mine) && mine.length === 0);
    if (empty && incoming[k] != null) (out[k] as unknown) = incoming[k];
  });
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
  const [lastJob, setLastJob] = useState<JobSummary | null>(null);
  const [toast, setToast] = useState<{ tone: "good" | "bad"; text: string } | null>(null);

  const [step, setStep] = useState("place");
  const [direction, setDirection] = useState(1);
  const [railOpen, setRailOpen] = useState(true);
  const [confirm, setConfirm] = useState<ActionId | null>(null);

  const [pendingLoad, setPendingLoad] = useState<{ id: string; incoming: LoopConfig } | null>(null);
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [testing, setTesting] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, { ok: boolean; text: string }>>({});

  const [theme, setTheme] = useState<ThemeChoice>(
    () => (localStorage.getItem("loopsmith-theme") as ThemeChoice) || "system");
  const [tourDone, setTourDone] = useState(() => localStorage.getItem("loopsmith-tour") === "done");

  const { shake, shakeKey, shakeProps } = useShake();

  // Toasts clear themselves. A confirmation that sits there forever eventually
  // reads as part of the furniture, and it costs the action bar a row.
  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), 7000);
    return () => window.clearTimeout(t);
  }, [toast]);
  const patch = useCallback((p: Partial<LoopConfig>) => setCfg((c) => ({ ...c, ...p })), []);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    localStorage.setItem("loopsmith-theme", theme);
  }, [theme]);

  useEffect(() => {
    api.help().then(setHelp).catch(() => {});
    api.examples().then(setExamples).catch(() => {});
    api.library().then(setLibrary).catch(() => {});
    api.meta().then(setMeta).catch(() => {});
    setScanning(true);
    api.detect(false).then(setDetection).catch(() => {}).finally(() => setScanning(false));
  }, []);

  const reviewTimer = useRef<number>(0);
  useEffect(() => {
    window.clearTimeout(reviewTimer.current);
    reviewTimer.current = window.setTimeout(() => { api.review(cfg).then(setReview).catch(() => {}); }, 220);
    return () => window.clearTimeout(reviewTimer.current);
  }, [cfg]);

  const pathTimer = useRef<number>(0);
  useEffect(() => {
    if (!path.trim()) { setFacts(null); return; }
    window.clearTimeout(pathTimer.current);
    pathTimer.current = window.setTimeout(() => { api.pathFacts(path).then(setFacts).catch(() => {}); }, 300);
    return () => window.clearTimeout(pathTimer.current);
  }, [path]);

  useEffect(() => {
    if (cfg.name.trim()) return;
    const leaf = path.trim().replace(/\/+$/, "").split("/").pop();
    if (leaf && leaf !== "~") patch({ name: leaf });
  }, [path, cfg.name, patch]);

  const sectionHelp = useMemo(
    () => new Map<string, SectionHelp>(help.sections.map((s) => [s.key, s])), [help.sections]);

  /** Errors per step, so a tab can show that something behind it is wrong. */
  const problemsByStep = useMemo(() => {
    const out: Record<string, number> = {};
    for (const issue of review?.issues ?? []) {
      if (issue.severity !== "error") continue;
      const key = issue.field.split(/[.[]/)[0];
      const owner = Object.entries(STEP_KEYS).find(([, keys]) => keys.includes(key))?.[0] ?? "place";
      out[owner] = (out[owner] ?? 0) + 1;
    }
    return out;
  }, [review]);

  const STEPS: Step[] = [
    { id: "place", label: "Place", icon: Icon.folder({ size: 14 }), problems: problemsByStep.place, done: !!path && !!cfg.name },
    { id: "power", label: "Power", icon: Icon.bolt({ size: 14 }), problems: problemsByStep.power, done: (cfg.providers?.providers?.length ?? 0) > 0 },
    { id: "intent", label: "Intent", icon: Icon.target({ size: 14 }), problems: problemsByStep.intent, done: (cfg.goals?.length ?? 0) > 0 },
    { id: "proof", label: "Proof", icon: Icon.shield({ size: 14 }), problems: problemsByStep.proof, done: (cfg.validations?.length ?? 0) > 0 },
    { id: "work", label: "Work", icon: Icon.graph({ size: 14 }), problems: problemsByStep.work, done: (cfg.graph?.nodes?.length ?? 0) > 0 },
    { id: "ship", label: "Ship", icon: Icon.play({ size: 14 }), problems: problemsByStep.ship, done: created },
  ];

  const goStep = useCallback((id: string) => {
    setStep((current) => {
      const from = STEPS.findIndex((s) => s.id === current);
      const to = STEPS.findIndex((s) => s.id === id);
      setDirection(to >= from ? 1 : -1);
      return id;
    });
    document.getElementById("step-panel")?.scrollTo({ top: 0 });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [problemsByStep, path, cfg, created]);

  const loadExample = async (id: string) => {
    setLoadingId(id);
    try {
      const { config } = await api.example(id);
      if (isEmpty(cfg)) applyLoaded(config, id);
      else setPendingLoad({ id, incoming: config });
    } catch (e) { setToast({ tone: "bad", text: (e as Error).message }); }
    finally { setLoadingId(null); }
  };

  const applyLoaded = (config: LoopConfig, id: string) => {
    setCfg(config);
    setCreated(false);
    if (!path.trim()) setPath(`~/loops/${id}`);
    goStep("place");
    setToast({ tone: "good", text: `Loaded ${config.name}. Nothing is on disk yet — walk the steps and press Create loop.` });
  };

  const openLoop = async (p: string) => {
    try {
      const res = await api.open(p);
      setCfg(res.config); setFormat(res.format); setPath(res.dir);
      setFacts(res.facts); setCreated(true); goStep("ship");
      setToast({ tone: "good", text: `Opened ${res.config_path}.` });
    } catch (e) { setToast({ tone: "bad", text: (e as Error).message }); }
  };

  const run = async (action: ActionId) => {
    const dir = path.trim();
    const configFile = `${dir}/loop.${format === "markdown" ? "md" : "yaml"}`;
    const body: Record<string, unknown> = { cwd: dir || ".", action };

    if (action === "create") {
      Object.assign(body, {
        path: dir, name: cfg.name, purpose: cfg.description ?? "",
        config_file: "", force: false, draft: { config: cfg, format },
      });
    } else if (action === "permissions_write") {
      Object.assign(body, { config: configFile, settings: `${dir}/.claude/settings.local.json` });
    } else {
      body.config = configFile;
      if (!created && (action === "validate" || action === "plan")) body.draft = { config: cfg, format };
    }

    try {
      const { job: id } = await api.start(body);
      setJob(id);
      setRailOpen(true);
    } catch (e) {
      shake();
      setToast({ tone: "bad", text: (e as Error).message });
    }
  };

  const onFinished = useCallback((s: JobSummary) => {
    setLastJob(s);
    if (s.kind === "create" && s.state === "succeeded") {
      setCreated(true);
      api.library().then(setLibrary).catch(() => {});
      api.pathFacts(path).then(setFacts).catch(() => {});
      setToast({ tone: "good", text: "Loop created. Everything on this step is unlocked." });
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
        [p.id]: { ok: r.ok, text: r.ok ? `answered in ${(r.elapsed_ms / 1000).toFixed(1)}s` : (r.error ?? "no answer") },
      }));
    } catch (e) {
      setTestResults((all) => ({ ...all, [p.id]: { ok: false, text: (e as Error).message } }));
    } finally { setTesting(null); }
  };

  /** Send a problem to the step that owns it, then to the field. */
  const jump = (field: string) => {
    const key = field.split(/[.[]/)[0];
    const owner = Object.entries(STEP_KEYS).find(([, keys]) => keys.includes(key))?.[0];
    if (owner) goStep(owner);
    window.setTimeout(
      () => document.getElementById(`section-${key}`)?.scrollIntoView({ block: "start" }), 220);
  };

  const neededKeys = useMemo(
    () => (cfg.providers?.providers ?? []).flatMap((p) => p.requires_env ?? []), [cfg.providers]);

  const sectionProps = { cfg, patch, help: sectionHelp };

  const island: IslandState = job
    ? { tone: "busy", label: "running", detail: lastJob?.kind, onClick: () => setRailOpen(true) }
    : lastJob
      ? {
          tone: lastJob.state === "succeeded" ? "good" : lastJob.state === "cancelled" ? "idle" : "bad",
          label: `${lastJob.kind} ${lastJob.state}`,
          onClick: () => { setJob(lastJob.id); setRailOpen(true); },
        }
      : scanning
        ? { tone: "busy", label: "reading this machine" }
        : { tone: "idle", label: `${detection?.agents.length ?? 0} agent CLIs found` };

  const actions = STEP_ACTIONS[step] ?? [];
  const blocked = !review?.parsed || review.error_count > 0;

  const stepIndex = STEPS.findIndex((s) => s.id === step);
  const prevStep = stepIndex > 0 ? STEPS[stepIndex - 1] : null;
  const nextStep = stepIndex < STEPS.length - 1 ? STEPS[stepIndex + 1] : null;

  const commands: Command[] = useMemo(() => [
    ...STEPS.map((s) => ({
      id: `step-${s.id}`, group: "Go to", label: s.label,
      hint: s.problems ? `${s.problems} problem${s.problems === 1 ? "" : "s"}` : undefined,
      run: () => goStep(s.id),
    })),
    ...help.sections.map((s) => ({
      id: `sec-${s.key}`, group: "Sections", label: s.title, hint: s.letter.length === 1 ? s.letter : undefined,
      run: () => jump(s.key),
    })),
    ...(Object.keys(ACTION_LABEL) as ActionId[]).map((a) => ({
      id: `act-${a}`, group: "Actions", label: ACTION_LABEL[a].label,
      hint: ACTION_LABEL[a].spends ? "spends" : undefined,
      disabled: blocked && a !== "validate" && a !== "plan",
      run: () => (ACTION_LABEL[a].spends ? setConfirm(a) : run(a)),
    })),
    ...examples.map((e) => ({
      id: `ex-${e.id}`, group: "Load an example", label: e.name, hint: e.trigger,
      run: () => loadExample(e.id),
    })),
    { id: "theme", group: "View", label: "Switch theme", run: () => setTheme(theme === "dark" ? "light" : "dark") },
    { id: "tour", group: "View", label: "How this works", run: () => setTourDone(false) },
    // eslint-disable-next-line react-hooks/exhaustive-deps
  ], [help.sections, examples, blocked, theme, problemsByStep, cfg, path, created]);

  return (
    <PaletteProvider commands={commands}>
      <HelpProvider fields={help.fields}>
        <div className="grid h-screen grid-rows-[auto_1fr] overflow-hidden">
          <Header
            meta={meta} island={island} theme={theme} setTheme={setTheme}
            onTour={() => setTourDone(false)}
            onRail={() => setRailOpen((v) => !v)} railOpen={railOpen}
          />

          <div className={`grid min-h-0 ${railOpen ? "lg:grid-cols-[17rem_1fr_22rem]" : "lg:grid-cols-[17rem_1fr]"}`}>
            <div className="hidden min-h-0 lg:block">
              <LeftRail
                examples={examples} library={library} onLoad={loadExample} onOpen={openLoop}
                onForget={(p) => api.forget(p).then(() => api.library().then(setLibrary)).catch(() => {})}
                loadingId={loadingId}
              />
            </div>

            <div className="grid min-h-0 grid-rows-[auto_1fr_auto]">
              <div className="flex flex-wrap items-center gap-3 border-b bg-surface px-4 py-2">
                <StepBar steps={STEPS} active={step} onPick={goStep} />
                <div className="ml-auto flex items-center gap-2">
                  <button
                    type="button"
                    className="btn btn-sm"
                    disabled={!prevStep}
                    title={prevStep ? `Back to ${prevStep.label}` : "This is the first step"}
                    onClick={() => prevStep && goStep(prevStep.id)}
                  >
                    ← Previous
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm"
                    disabled={!nextStep}
                    title={nextStep ? `On to ${nextStep.label}` : "This is the last step"}
                    onClick={() => nextStep && goStep(nextStep.id)}
                  >
                    Next →
                  </button>
                  <span className="hidden text-[11.5px] text-faint xl:block">
                    press <span className="kbd">⌘K</span> to jump anywhere
                  </span>
                </div>
              </div>

              <main id="step-panel" className="min-h-0 overflow-y-auto bg-ground p-4">
                <MorphPanel view={step} direction={direction} className="space-y-4">
                  <StepView
                    step={step} sectionProps={sectionProps} detection={detection}
                    path={path} setPath={setPath} format={format} setFormat={setFormat}
                    facts={facts} review={review} neededKeys={neededKeys}
                    onTest={testProvider} testing={testing} testResults={testResults}
                    scanning={scanning}
                    onRescan={(deep) => {
                      setScanning(true);
                      api.detect(deep).then(setDetection).catch(() => {}).finally(() => setScanning(false));
                    }}
                  />
                </MorphPanel>
              </main>

              <StepActions
                actions={actions} blocked={blocked} created={created} facts={facts}
                onRun={(a) => (ACTION_LABEL[a].spends ? setConfirm(a) : run(a))}
                shakeKey={shakeKey} shakeProps={shakeProps}
                toast={toast} onDismissToast={() => setToast(null)}
              />
            </div>

            {railOpen && (
              <div className="hidden min-h-0 lg:block">
                {job ? (
                  <div className="h-full border-l bg-surface">
                    <RunConsole jobId={job} onClose={() => setJob(null)} onFinished={onFinished} />
                  </div>
                ) : (
                  <ReviewRail review={review} onJump={jump} />
                )}
              </div>
            )}
          </div>
        </div>

        {confirm && (
          <SpendConfirm
            action={confirm} review={review}
            onCancel={() => setConfirm(null)}
            onGo={() => { run(confirm); setConfirm(null); }}
          />
        )}

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
                }}>Fill blanks only</button>
                <button className="btn btn-primary" onClick={() => {
                  applyLoaded(pendingLoad.incoming, pendingLoad.id);
                  setPendingLoad(null);
                }}>Replace everything</button>
              </>
            }
          >
            <p className="hint">
              Loading <span className="font-semibold">{pendingLoad.incoming.name}</span> can either replace
              what is in the form, or only fill the parts you have left empty.
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
    </PaletteProvider>
  );
}

/* ------------------------------------------------------------------ header */

function Header({
  meta, island, theme, setTheme, onTour, onRail, railOpen,
}: {
  meta: Meta | null; island: IslandState; theme: ThemeChoice;
  setTheme: (t: ThemeChoice) => void; onTour: () => void;
  onRail: () => void; railOpen: boolean;
}) {
  const palette = usePalette();
  return (
    <header className="flex items-center gap-3 border-b bg-surface px-4 py-2.5">
      {/* The plate is the point. The mark's own outlines are near-black, so a
          transparent PNG on the dark theme loses its silhouette against the
          ground. White is invisible against the light theme's white header, so
          one treatment serves both rather than branching on theme. */}
      <span className="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-[6px] bg-white">
        <img src="/logo.png" alt="" width={26} height={26} className="select-none"
          aria-hidden="true" draggable={false} />
      </span>
      <h1 className="forge-mark text-[18px] leading-none">loopsmith</h1>
      <span className="chip font-mono">{meta?.version ?? "…"}</span>

      <div className="ml-3 hidden md:block"><StatusIsland state={island} /></div>

      <div className="ml-auto flex items-center gap-2">
        <button type="button" className="btn btn-sm btn-ghost" onClick={palette.open} aria-label="Open the command palette">
          {Icon.command({ size: 13 })}<span className="hidden lg:inline">Jump to…</span>
        </button>
        <button type="button" className="btn btn-sm btn-ghost" onClick={onTour}>How this works</button>
        <button type="button" className="btn btn-sm btn-ghost btn-icon" onClick={onRail}
          aria-pressed={railOpen} aria-label={railOpen ? "Hide the side panel" : "Show the side panel"}>
          {Icon.panel({ size: 14 })}
        </button>
        <div className="flex rounded-[10px] border p-0.5" role="group" aria-label="Theme">
          {([
            { id: "light", label: "Light theme" },
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
  );
}

/* -------------------------------------------------------------- step views */

function StepView(props: {
  step: string;
  sectionProps: { cfg: LoopConfig; patch: (p: Partial<LoopConfig>) => void; help: Map<string, SectionHelp> };
  detection: Detection | null;
  path: string; setPath: (v: string) => void;
  format: Format; setFormat: (f: Format) => void;
  facts: PathFacts | null; review: Review | null; neededKeys: string[];
  onTest: (p: ProviderSpec) => void; testing: string | null;
  testResults: Record<string, { ok: boolean; text: string }>;
  scanning: boolean; onRescan: (deep: boolean) => void;
}) {
  const { step, sectionProps: sp } = props;
  const wrap = (nodes: React.ReactNode[]) => nodes.map((n, i) => <Reveal key={i} index={i}>{n}</Reveal>);

  switch (step) {
    case "place":
      return <div className="space-y-4">{wrap([
        <Location
          path={props.path} onPath={props.setPath} cfg={sp.cfg} patch={sp.patch}
          format={props.format} onFormat={props.setFormat} facts={props.facts}
          permissions={props.review?.permissions ?? []}
        />,
      ])}</div>;

    case "power":
      return <div className="space-y-4">{wrap([
        <Providers {...sp} detection={props.detection} onTest={props.onTest}
          testing={props.testing} results={props.testResults} />,
        <Secrets detection={props.detection} needed={props.neededKeys} />,
      ])}</div>;

    case "intent":
      return <div className="space-y-4">{wrap([
        <Goals {...sp} />, <Information {...sp} />, <PreExecution {...sp} />,
      ])}</div>;

    case "proof":
      return <div className="space-y-4">{wrap([
        <Validations {...sp} />, <Success {...sp} />, <StopGatesSection {...sp} />,
      ])}</div>;

    case "work":
      return <div className="space-y-4">{wrap([
        <Graph {...sp} />, <ConstraintsSection {...sp} />, <Guidelines {...sp} />,
        <Skills {...sp} detection={props.detection} />, <Context {...sp} />,
      ])}</div>;

    default:
      return <div className="space-y-4">{wrap([
        <Schedules {...sp} />,
        <Preflight detection={props.detection} scanning={props.scanning} onRescan={props.onRescan} />,
      ])}</div>;
  }
}

/* ------------------------------------------------------------ step actions */

function StepActions({
  actions, blocked, created, facts, onRun, shakeKey, shakeProps, toast, onDismissToast,
}: {
  actions: ActionId[];
  blocked: boolean; created: boolean; facts: PathFacts | null;
  onRun: (a: ActionId) => void;
  shakeKey: number; shakeProps: Record<string, unknown>;
  toast: { tone: "good" | "bad"; text: string } | null;
  onDismissToast: () => void;
}) {
  const cantWrite = !!facts && !facts.writable;

  // Steps that only collect input have no actions of their own, and an empty
  // bar is worse than no bar — it reads as something that failed to load.
  if (actions.length === 0 && !toast) return null;

  return (
    <div>
      {toast && (
        <Reveal>
          <div className="border-t bg-surface px-4 pt-3">
            <div className="card flex items-start gap-2.5 p-3">
              <span className={toast.tone === "good" ? "text-good" : "text-bad"}>
                {toast.tone === "good" ? Icon.check({ size: 16 }) : Icon.x({ size: 16 })}
              </span>
              <p className="hint flex-1 text-text">{toast.text}</p>
              <button className="btn btn-ghost btn-sm btn-icon" aria-label="Dismiss" onClick={onDismissToast}>
                {Icon.x({ size: 13 })}
              </button>
            </div>
          </div>
        </Reveal>
      )}

      <motion.div key={shakeKey} {...shakeProps}
        className="flex flex-wrap items-center gap-2 border-t bg-surface px-4 py-3">
        {actions.map((a) => {
          const m = ACTION_LABEL[a];
          const needsLoop = a !== "create" && a !== "validate" && a !== "plan";
          const disabled =
            (a !== "validate" && a !== "plan" && blocked) ||
            (a === "create" && cantWrite) ||
            (needsLoop && !created);
          return (
            <button
              key={a}
              className={`btn btn-sm ${a === "create" ? "btn-primary" : m.spends ? "btn-quench" : ""}`}
              disabled={disabled}
              title={disabled
                ? needsLoop && !created ? "Create the loop first — this acts on a loop that exists on disk."
                  : blocked ? "Fix the errors in the side panel first."
                    : cantWrite ? "That folder is not writable." : "Not available yet"
                : m.note}
              onClick={() => onRun(a)}
            >
              {m.spends ? Icon.play({ size: 13 }) : a === "create" ? Icon.bolt({ size: 13 }) : null}
              {m.label}
            </button>
          );
        })}
      </motion.div>
    </div>
  );
}

/* ------------------------------------------------------------ spend dialog */

function SpendConfirm({
  action, review, onCancel, onGo,
}: { action: ActionId; review: Review | null; onCancel: () => void; onGo: () => void }) {
  const m = ACTION_LABEL[action];
  return (
    <Dialog
      title={`${m.label} — this spends money`}
      onClose={onCancel}
      actions={
        <>
          <button className="btn" onClick={onCancel}>Cancel</button>
          <button className="btn btn-primary" onClick={onGo}>{m.label}</button>
        </>
      }
    >
      <p className="hint">{m.note}</p>
      <div className="mt-4 card p-3">
        <p className="text-[11px] uppercase tracking-wide text-faint">Ceiling for this run</p>
        <p className="mt-0.5 text-[26px] font-bold leading-none">
          {review?.cost.ceiling_usd != null
            ? <CountUp value={review.cost.ceiling_usd} prefix="$" />
            : <span className="text-warn">unbounded</span>}
        </p>
        <p className="hint mt-1.5">{review?.cost.basis}</p>
      </div>
      <p className="hint mt-3">
        You can stop a run at any time from the console, and Dry run walks the same path without
        calling a model.
      </p>
    </Dialog>
  );
}
