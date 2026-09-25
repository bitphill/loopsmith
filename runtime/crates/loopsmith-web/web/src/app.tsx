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
import { Information, PreExecution, Goals, Validations, Success, StopGatesSection, type SectionProps } from "./sections-core";
import { Schedules, ConstraintsSection, Guidelines, Skills, Graph, Providers, Context } from "./sections-run";
import {
  EntryGates, ApprovalGates, RollbackGates, RecoverySection, Alerts, Protected, EvolutionSection,
} from "./sections-guard";
import { Tour } from "./tour";
import { SmithGate, type Smith } from "./smith-gate";
import { ExamplesPicker } from "./examples-picker";
import { Guided } from "./guided/guided";
import { SPEC_VERSION } from "./guided/wire";
import type { Answers, Assembled, Spec } from "./guided/wire";
import { at } from "./types";
import type {
  LoopConfig, Detection, Help, SectionHelp, Review, ExampleCard, LibraryEntry,
  Format, PathFacts, JobSummary, ProviderSpec, Meta,
} from "./types";

const BLANK: LoopConfig = {
  name: "",
  version: "0.1.0",
  description: "",
  intent: { goals: [] },
  safety: { checks: [] },
};

/**
 * Where the loop will actually live: a directory of its own, named after it,
 * inside the folder that was picked.
 *
 * Creating straight into the picked folder was the earlier behaviour and it is
 * a trap — the obvious thing to pick is somewhere like `~/loops`, and a loop
 * scaffolded directly into that turns the container into the loop. Every
 * subsequent loop then either refuses (not empty) or is forced on top of the
 * first one's ledger.
 */
function loopDir(parent: string, name: string): string {
    const base = parent.trim().replace(/\/+$/, "");
    const leaf = name.trim().replace(/^\/+|\/+$/g, "");
    if (!base) return leaf;
    if (!leaf) return base;
    return `${base}/${leaf}`;
}

/**
 * Which config paths each step owns, so a problem can route to a step.
 *
 * Matched as dotted prefixes rather than by first segment: in 1.0 the first
 * segment is the bundle, and `safety` alone owns the checks on one step and
 * the limits on another.
 */
const STEP_KEYS: Record<string, string[]> = {
  place: ["name", "description", "version", "environment"],
  power: ["execution.providers"],
  intent: ["intent.background", "intent.prerequisites", "intent.goals"],
  proof: ["safety.checks", "intent.success", "safety.gates"],
  work: [
    "execution.graph", "execution.phases", "execution.default_skills",
    "safety.limits", "safety.recovery", "safety.alerts", "execution.memory",
    "execution.skills",
  ],
  ship: ["execution.triggers", "evolution", "safety.protected", "features"],
};

/** The step that owns a config path, by the longest prefix that matches it. */
function stepOwning(field: string): string | undefined {
  let best: { step: string; len: number } | undefined;
  for (const [step, paths] of Object.entries(STEP_KEYS)) {
    for (const p of paths) {
      if ((field === p || field.startsWith(`${p}.`) || field.startsWith(`${p}[`)) && (!best || p.length > best.len)) {
        best = { step, len: p.length };
      }
    }
  }
  return best?.step;
}

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
  const empty = (path: string) => (at<unknown[]>(c, path)?.length ?? 0) === 0;
  return (
    !c.name.trim() && !(c.description ?? "").trim() &&
    empty("intent.goals") && empty("safety.checks") &&
    empty("intent.background") && empty("intent.prerequisites") &&
    empty("execution.graph.nodes") && empty("execution.providers.providers")
  );
}

/** Nothing the author put there: absent, blank, or an empty list. */
function blank(v: unknown): boolean {
  if (v == null) return true;
  if (typeof v === "string") return !v.trim();
  if (Array.isArray(v)) return v.length === 0;
  return false;
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

/**
 * "Fill blanks only": take from `incoming` wherever the author has left a gap.
 *
 * Recursive, because 1.0 nests. A top-level merge would look at `intent`,
 * find an object, decide it is filled in, and leave every goal inside it
 * untouched — which is the opposite of what the button says.
 */
function fillBlanks<T>(current: T, incoming: T): T {
  if (!isRecord(current) || !isRecord(incoming)) {
    return blank(current) && !blank(incoming) ? incoming : current;
  }
  const out: Record<string, unknown> = { ...incoming, ...current };
  for (const k of Object.keys(incoming)) {
    out[k] = fillBlanks(current[k], incoming[k]);
  }
  return out as T;
}

type ThemeChoice = "system" | "light" | "dark";

export default function App() {
  const [cfg, setCfg] = useState<LoopConfig>(BLANK);
  /** The container the loop's own directory is created inside. */
  const [parent, setParent] = useState("");
  /** Initialise a repo in the new directory, so `isolated` nodes isolate. */
  const [initGit, setInitGit] = useState(true);
  const [format, setFormat] = useState<Format>("yaml");
  const [created, setCreated] = useState(false);

  const [detection, setDetection] = useState<Detection | null>(null);
  const [scanning, setScanning] = useState(false);
  const [help, setHelp] = useState<Help>({ sections: [], fields: [] });
  /** The model's own defaults, so a section can show what a blank field does. */
  const [defaults, setDefaults] = useState<LoopConfig | null>(null);
  const [examples, setExamples] = useState<ExampleCard[]>([]);
  const [library, setLibrary] = useState<LibraryEntry[]>([]);
  const [meta, setMeta] = useState<Meta | null>(null);

  const [review, setReview] = useState<Review | null>(null);

  /**
   * The guided wizard's draft.
   *
   * Answers are the wizard's own currency — strings keyed by the config path
   * each one fills — and the server is what turns them into a config. `base`
   * is whatever config they were unpacked from, so an edit does not drop the
   * sections the wizard has no question for.
   */
  const [spec, setSpec] = useState<Spec | null>(null);
  const [answers, setAnswers] = useState<Answers>({});
  const [assembled, setAssembled] = useState<Assembled | null>(null);
  const [answerBase, setAnswerBase] = useState<unknown | null>(null);
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

  /**
   * Which door was taken on the way in. Remembered, because the answer does not
   * change from one launch to the next and asking again would be noise.
   */
  const [smith, setSmith] = useState<Smith | null>(
    () => (localStorage.getItem("loopsmith-smith") as Smith | null) || null);
  /** Where a new smith is in the walk-through: the explanation, then the examples. */
  const [onboarding, setOnboarding] = useState<"tour" | "examples" | "done">("done");
  /**
   * The guided wizard and the six-step editor are two views of the same draft.
   * Remembered, so a reload halfway through the walk-through comes back to the
   * walk-through rather than dropping someone into the form they were being
   * walked through in the first place.
   */
  const [mode, setMode] = useState<"expert" | "guided">(
    () => (localStorage.getItem("loopsmith-mode") as "expert" | "guided") || "expert");
  /** The tour is only ever opened deliberately — from onboarding, or the header. */
  const [tourOpen, setTourOpen] = useState(false);

  const { shake, shakeKey, shakeProps } = useShake();

  // Toasts clear themselves. A confirmation that sits there forever eventually
  // reads as part of the furniture, and it costs the action bar a row.
  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), 7000);
    return () => window.clearTimeout(t);
  }, [toast]);
  const patch = useCallback((p: Partial<LoopConfig>) => setCfg((c) => ({ ...c, ...p })), []);

  /** Where the loop actually lands: its own directory inside the container. */
  const path = useMemo(() => loopDir(parent, cfg.name), [parent, cfg.name]);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    localStorage.setItem("loopsmith-theme", theme);
  }, [theme]);

  useEffect(() => { localStorage.setItem("loopsmith-mode", mode); }, [mode]);

  useEffect(() => {
    api.help().then(setHelp).catch(() => {});
    api.defaults().then(setDefaults).catch(() => {});
    api.examples().then(setExamples).catch(() => {});
    api.library().then(setLibrary).catch(() => {});
    api.meta().then(setMeta).catch(() => {});
    setScanning(true);
    api.detect(false).then(setDetection).catch(() => {}).finally(() => setScanning(false));

    // Reattach to whatever is already going.
    //
    // A run is a subprocess of the *server*, not of this page, so closing the
    // tab never stopped it — but the page forgot which job it had been
    // watching, which looked exactly like it had. The server is the source of
    // truth here rather than localStorage: it knows what is genuinely still
    // running, and a remembered id from a previous session would be a lie.
    //
    // The socket replays every retained line before going live, so a run
    // rejoined halfway through arrives whole rather than mid-sentence.
    api.jobs()
      .then((all) => {
        const live = all.find((j) => j.state === "running");
        if (live) {
          setJob(live.id);
          setRailOpen(true);
          setToast({
            tone: "good",
            text: `Still running: ${live.kind}. It kept going while this page was closed — picking the log back up.`,
          });
          return;
        }
        // Nothing running, but the most recent outcome is still worth showing
        // in the header rather than starting blank.
        if (all.length > 0) setLastJob(all[0]);
      })
      .catch(() => {});
  }, []);

  const reviewTimer = useRef<number>(0);
  useEffect(() => {
    window.clearTimeout(reviewTimer.current);
    reviewTimer.current = window.setTimeout(() => { api.review(cfg).then(setReview).catch(() => {}); }, 220);
    return () => window.clearTimeout(reviewTimer.current);
  }, [cfg]);

  /**
   * The wizard's answers, typed by the server.
   *
   * Short debounce rather than none: the call is in-process on loopback, and
   * the inputs read the local answer map, so what waits for the reply is only
   * the part the server owns — which questions apply, what the dynamic
   * selects offer, and the config itself.
   */
  const answerTimer = useRef<number>(0);
  useEffect(() => {
    if (mode !== "guided") return;
    window.clearTimeout(answerTimer.current);
    answerTimer.current = window.setTimeout(() => {
      api.wizardAssemble(answers, answerBase)
        .then((out) => {
          setAssembled(out);
          // A refused draft leaves the last good config in place, so the rail
          // keeps showing something true rather than blanking mid-keystroke.
          if (out.config) setCfg(out.config as LoopConfig);
        })
        .catch(() => {});
    }, 120);
    return () => window.clearTimeout(answerTimer.current);
  }, [answers, answerBase, mode]);

  // The question list never changes while the server is up, so it is fetched
  // once, the first time the wizard is opened.
  useEffect(() => {
    if (mode !== "guided" || spec) return;
    api.wizardSpec().then((s) => {
      setSpec(s);
      // This page and that server normally ship together, in one binary. A
      // tab left open across an upgrade is the case where they do not.
      if (s.version !== SPEC_VERSION) {
        setToast({
          tone: "bad",
          text: "This page was loaded before loopsmith was upgraded. Reload it to get the current questions.",
        });
      }
    }).catch(() => {});
  }, [mode, spec]);

  const pathTimer = useRef<number>(0);
  useEffect(() => {
    if (!parent.trim() || !cfg.name.trim()) { setFacts(null); return; }
    window.clearTimeout(pathTimer.current);
    pathTimer.current = window.setTimeout(() => { api.pathFacts(path).then(setFacts).catch(() => {}); }, 300);
    return () => window.clearTimeout(pathTimer.current);
  }, [path]);

  const sectionHelp = useMemo(
    () => new Map<string, SectionHelp>(help.sections.map((s) => [s.key, s])), [help.sections]);

  /** Errors per step, so a tab can show that something behind it is wrong. */
  const problemsByStep = useMemo(() => {
    const out: Record<string, number> = {};
    for (const issue of review?.issues ?? []) {
      if (issue.severity !== "error") continue;
      const owner = stepOwning(issue.field) ?? "place";
      out[owner] = (out[owner] ?? 0) + 1;
    }
    return out;
  }, [review]);

  const filled = (path: string) => (at<unknown[]>(cfg, path)?.length ?? 0) > 0;

  const STEPS: Step[] = [
    { id: "place", label: "Place", icon: Icon.folder({ size: 14 }), problems: problemsByStep.place, done: !!path && !!cfg.name },
    { id: "power", label: "Power", icon: Icon.bolt({ size: 14 }), problems: problemsByStep.power, done: filled("execution.providers.providers") },
    { id: "intent", label: "Intent", icon: Icon.target({ size: 14 }), problems: problemsByStep.intent, done: filled("intent.goals") },
    { id: "proof", label: "Proof", icon: Icon.shield({ size: 14 }), problems: problemsByStep.proof, done: filled("safety.checks") },
    { id: "work", label: "Work", icon: Icon.graph({ size: 14 }), problems: problemsByStep.work, done: filled("execution.graph.nodes") },
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
      if (isEmpty(cfg)) applyLoaded(config);
      else setPendingLoad({ id, incoming: config });
    } catch (e) { setToast({ tone: "bad", text: (e as Error).message }); }
    finally { setLoadingId(null); }
  };

  const applyLoaded = (config: LoopConfig) => {
    setCfg(config);
    setCreated(false);
    if (!parent.trim()) setParent("~/loops");
    goStep("place");
    setToast({ tone: "good", text: `Loaded ${config.name}. Nothing is on disk yet — walk the steps and press Create loop.` });
  };

  const pickSmith = (s: Smith) => {
    localStorage.setItem("loopsmith-smith", s);
    setSmith(s);
    // A new smith gets the explanation and then a working loop to start from.
    // An experienced one is dropped straight into the editor, which is what
    // they came for.
    if (s === "new") setOnboarding("tour");
  };

  /**
   * The one door into the wizard.
   *
   * Whatever is in the editor becomes the answers, so the two modes stay two
   * views of one draft rather than two drafts. The same config is kept as the
   * base, because the wizard asks about a subset of it and an assemble that
   * saw only the answers would drop the rest.
   */
  const enterGuided = async (from: LoopConfig) => {
    setAnswerBase(from);
    try {
      const { answers: recovered } = await api.wizardUnpack(from);
      setAnswers(recovered);
    } catch {
      // A draft the loader will not read yet is not a reason to refuse the
      // wizard: start it empty rather than stranding someone in the editor.
      setAnswers({});
    }
    setMode("guided");
  };

  /**
   * Leave the wizard for the editor, carrying the last keystroke with it.
   *
   * The answers are posted on a short debounce, so switching straight away
   * would drop whatever was typed in the last fraction of a second — and the
   * editor would open on a draft that is one field behind.
   */
  const exitGuided = async () => {
    try {
      const out = await api.wizardAssemble(answers, answerBase);
      if (out.config) setCfg(out.config as LoopConfig);
    } catch {
      // Keep the last config that did assemble rather than refusing to leave.
    }
    setMode("expert");
  };

  /** Leave the examples picker for the wizard, with or without a loaded example. */
  const startGuided = async (id: string | null) => {
    let start = cfg;
    if (id) {
      setLoadingId(id);
      try {
        const { config } = await api.example(id);
        setCfg(config);
        setCreated(false);
        start = config;
      } catch (e) {
        setToast({ tone: "bad", text: (e as Error).message });
      } finally {
        setLoadingId(null);
      }
    }
    if (!parent.trim()) setParent("~/loops");
    setOnboarding("done");
    await enterGuided(start);
  };

  const openLoop = async (p: string) => {
    try {
      const res = await api.open(p);
      // An existing loop's directory IS the loop, so its container is the
      // parent — otherwise the derived path would gain a second copy of the
      // name and point at somewhere that does not exist.
      const dir = res.dir.replace(/\/+$/, "");
      setParent(dir.slice(0, dir.lastIndexOf("/")) || "/");
      setCfg(res.config); setFormat(res.format);
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
        config_file: "", force: false, git: initGit,
        draft: { config: cfg, format },
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

  /** Send a problem to the step that owns it, then to the section. */
  const jump = (field: string) => {
    const owner = stepOwning(field);
    if (owner) goStep(owner);
    // The anchor is the section's own path, so the longest prefix that names
    // a section is the one to scroll to.
    const section = Object.values(STEP_KEYS)
      .flat()
      .filter((p) => field === p || field.startsWith(`${p}.`) || field.startsWith(`${p}[`))
      .sort((a, b) => b.length - a.length)[0];
    window.setTimeout(
      () => document.getElementById(`section-${section ?? field}`)?.scrollIntoView({ block: "start" }), 220);
  };

  const neededKeys = useMemo(
    () => (at<ProviderSpec[]>(cfg, "execution.providers.providers") ?? []).flatMap((p) => p.requires_env ?? []),
    [cfg]);

  const sectionProps = { cfg, patch, help: sectionHelp, defaults };

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
      id: `sec-${s.key}`, group: "Sections", label: s.title, hint: s.bundle,
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
    { id: "tour", group: "View", label: "How this works", run: () => setTourOpen(true) },
    {
      id: "mode",
      group: "View",
      label: mode === "guided" ? "Switch to the expert editor" : "Walk me through it, one field at a time",
      run: () => { if (mode === "guided") void exitGuided(); else void enterGuided(cfg); },
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
  ], [help.sections, examples, blocked, theme, problemsByStep, cfg, path, created, mode]);

  return (
    <PaletteProvider commands={commands}>
      <HelpProvider fields={help.fields}>
        <div className="grid h-screen grid-rows-[auto_1fr] overflow-hidden">
          <Header
            meta={meta} island={island} theme={theme} setTheme={setTheme}
            onTour={() => setTourOpen(true)}
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

            {mode === "guided" ? (
              <div className="grid min-h-0">
                <Guided
                  spec={spec} answers={answers} setAnswers={setAnswers}
                  assembled={assembled} review={review}
                  detection={detection} scanning={scanning}
                  onRescan={(deep) => {
                    setScanning(true);
                    api.detect(deep).then(setDetection).catch(() => {}).finally(() => setScanning(false));
                  }}
                  onTest={testProvider} testing={testing} testResults={testResults}
                  parent={parent} setParent={setParent} loopPath={path}
                  initGit={initGit} setInitGit={setInitGit}
                  format={format} setFormat={setFormat} facts={facts}
                  onExit={() => void exitGuided()}
                  onCreate={() => run("create")}
                  createDisabled={blocked || !path.trim() || (!!facts && !facts.writable)}
                  onJump={(field) => { void exitGuided().then(() => jump(field)); }}
                />
              </div>
            ) : (
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
                    parent={parent} setParent={setParent} loopPath={path}
                    initGit={initGit} setInitGit={setInitGit}
                    format={format} setFormat={setFormat}
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
            )}

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
                  applyLoaded(pendingLoad.incoming);
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

        {/* The way in: pick a door, then (for a new smith) the explanation and
            a working loop to start from. Each is shown once and remembered. */}
        {smith === null && <SmithGate onPick={pickSmith} />}

        {(tourOpen || onboarding === "tour") && (
          <Tour
            onClose={() => {
              setTourOpen(false);
              if (onboarding === "tour") setOnboarding("examples");
            }}
          />
        )}

        {smith !== null && onboarding === "examples" && (
          <ExamplesPicker
            examples={examples}
            loading={loadingId !== null}
            onGo={startGuided}
          />
        )}
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

/**
 * One card on a step, with the bundle it belongs to.
 *
 * A step is a stage of the work — where it lives, what powers it, what you
 * want — and the bundles are how the config itself is organised. Most steps
 * span two of them, so the card carries its bundle and [`Bundled`] draws the
 * line. `null` is for the cards that are not config at all: the folder
 * picker, the secrets table, the preflight report.
 */
type Card = { bundle: string | null; node: React.ReactNode };

/** What each bundle is for, in the six words that fit above a group of cards. */
const BUNDLE_BLURB: Record<string, string> = {
  intent: "what this loop is for",
  execution: "how the work gets done",
  safety: "what must not happen, and when to stop",
  evolution: "how the loop may change itself",
};

/**
 * Draw a step's cards, grouped under the bundle each one edits.
 *
 * This is the whole of "the layout follows the four bundles": a step is still
 * a stage of the work, but inside it the cards are gathered under the part of
 * the config they write, with one line saying what that part is for. The same
 * word used to sit on every card as a badge, which said where a card lived
 * without ever saying what the place was.
 */
function Bundled({ cards }: { cards: Card[] }) {
  const groups: { bundle: string | null; nodes: React.ReactNode[] }[] = [];
  for (const c of cards) {
    const last = groups[groups.length - 1];
    if (last && last.bundle === c.bundle) last.nodes.push(c.node);
    else groups.push({ bundle: c.bundle, nodes: [c.node] });
  }

  let index = 0;
  return (
    <div className="space-y-4">
      {groups.map((g, gi) => (
        <div key={gi} className="space-y-4">
          {g.bundle && (
            <div className="flex flex-wrap items-baseline gap-2 pt-1">
              <span className="chip chip-ember font-mono">{g.bundle}</span>
              <span className="hint">{BUNDLE_BLURB[g.bundle]}</span>
            </div>
          )}
          {g.nodes.map((n) => <Reveal key={index} index={index++}>{n}</Reveal>)}
        </div>
      ))}
    </div>
  );
}

function StepView(props: {
  step: string;
  sectionProps: SectionProps;
  detection: Detection | null;
  parent: string; setParent: (v: string) => void; loopPath: string;
  initGit: boolean; setInitGit: (v: boolean) => void;
  format: Format; setFormat: (f: Format) => void;
  facts: PathFacts | null; review: Review | null; neededKeys: string[];
  onTest: (p: ProviderSpec) => void; testing: string | null;
  testResults: Record<string, { ok: boolean; text: string }>;
  scanning: boolean; onRescan: (deep: boolean) => void;
}) {
  const { step, sectionProps: sp } = props;

  switch (step) {
    case "place":
      return <Bundled cards={[
        { bundle: null, node: (
          <Location
            parent={props.parent} onParent={props.setParent} loopPath={props.loopPath}
            initGit={props.initGit} onInitGit={props.setInitGit}
            cfg={sp.cfg} patch={sp.patch}
            format={props.format} onFormat={props.setFormat} facts={props.facts}
            permissions={props.review?.permissions ?? []}
          />
        ) },
      ]} />;

    case "power":
      return <Bundled cards={[
        { bundle: "execution", node: (
          <Providers {...sp} detection={props.detection} onTest={props.onTest}
            testing={props.testing} results={props.testResults} />
        ) },
        { bundle: null, node: <Secrets detection={props.detection} needed={props.neededKeys} /> },
      ]} />;

    case "intent":
      return <Bundled cards={[
        { bundle: "intent", node: <Goals {...sp} /> },
        { bundle: "intent", node: <Information {...sp} /> },
        { bundle: "intent", node: <PreExecution {...sp} /> },
      ]} />;

    // Proof is the step that is deliberately two bundles: what counts as done
    // (`intent.success`) and what is allowed to decide it (`safety`). Keeping
    // them on one step is what stops someone writing a success scenario with
    // nothing checking it.
    case "proof":
      return <Bundled cards={[
        { bundle: "safety", node: <Validations {...sp} /> },
        { bundle: "intent", node: <Success {...sp} /> },
        { bundle: "safety", node: <StopGatesSection {...sp} /> },
        { bundle: "safety", node: <EntryGates {...sp} /> },
        { bundle: "safety", node: <ApprovalGates {...sp} /> },
        { bundle: "safety", node: <RollbackGates {...sp} /> },
      ]} />;

    case "work":
      return <Bundled cards={[
        { bundle: "execution", node: <Graph {...sp} /> },
        { bundle: "execution", node: <Guidelines {...sp} /> },
        { bundle: "execution", node: <Skills {...sp} detection={props.detection} /> },
        { bundle: "execution", node: <Context {...sp} /> },
        { bundle: "safety", node: <ConstraintsSection {...sp} /> },
        { bundle: "safety", node: <RecoverySection {...sp} /> },
        { bundle: "safety", node: <Alerts {...sp} /> },
      ]} />;

    default:
      return <Bundled cards={[
        { bundle: "execution", node: <Schedules {...sp} /> },
        { bundle: "evolution", node: <EvolutionSection {...sp} /> },
        { bundle: "safety", node: <Protected {...sp} /> },
        { bundle: null, node: (
          <Preflight detection={props.detection} scanning={props.scanning} onRescan={props.onRescan} />
        ) },
      ]} />;
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
