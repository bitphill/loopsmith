/**
 * The panels that are about this machine rather than about the config:
 * where the loop will live, what secrets are set, and what the host can do.
 */
import { useEffect, useState } from "react";
import { api } from "./api";
import { Card, Field, Text, Select, Toggle, Note, Icon } from "./ui";
import { Skeleton } from "./motion";
import type { Detection, PathFacts, SecretStatus, SecretStore, Format, LoopConfig } from "./types";

/**
 * The folder icon, wired to the operating system's own chooser.
 *
 * The browser cannot help here — `showDirectoryPicker()` returns a handle with
 * no filesystem path, by design — but the server is a local process on this
 * machine, so it can open the real dialog. Typing an absolute path from memory
 * was always the worst ask in this form.
 *
 * The text box keeps working regardless: a machine with no dialog (a bare Linux
 * box with neither zenity nor kdialog) says so rather than leaving a button
 * that does nothing.
 */
export function PickFolder({
  startIn, onPick, label = "Browse for a folder",
}: {
  startIn?: string;
  onPick: (path: string) => void;
  label?: string;
}) {
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const open = async () => {
    setBusy(true);
    setProblem(null);
    try {
      const res = await api.pickFolder(startIn);
      if (res.path) onPick(res.path);
      else if (res.unavailable) setProblem(res.unavailable);
      // Neither: the dialog was cancelled, which needs no comment.
    } catch (e) {
      setProblem((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <button
        type="button"
        className="btn btn-icon shrink-0"
        onClick={open}
        disabled={busy}
        aria-label={label}
        title={label}
      >
        {busy ? Icon.refresh({ size: 15 }) : Icon.folder({ size: 15 })}
      </button>
      {problem && <p className="hint mt-1 w-full text-warn">{problem}</p>}
    </>
  );
}

/* --- where the loop lives ------------------------------------------------ */

export function Location({
  parent, onParent, loopPath, initGit, onInitGit,
  cfg, patch, format, onFormat, facts, permissions,
}: {
  /** The folder the loop's own directory is created inside. */
  parent: string;
  onParent: (v: string) => void;
  /** `parent/name` — where the loop will actually live. */
  loopPath: string;
  initGit: boolean;
  onInitGit: (v: boolean) => void;
  cfg: LoopConfig;
  patch: (p: Partial<LoopConfig>) => void;
  format: Format;
  onFormat: (f: Format) => void;
  facts: PathFacts | null;
  permissions: string[];
}) {
  return (
    <Card
      title="Where this loop lives"
      summary="A loop owns durable state — a ledger, checkpoints, its own memory — so it needs a directory of its own."
      detail="The path is mandatory by design. Leaving it implicit is how three half-finished loops end up writing into each other's state. Pick an empty directory, or a new one; loopsmith creates it."
      failure="Pointing two loops at one directory corrupts both ledgers, and there is no way to tell them apart afterwards."
      actions={
        facts && (
          <span className={`chip ${facts.writable ? "chip-good" : "chip-bad"}`}>
            {facts.writable ? "writable" : "not writable"}
          </span>
        )
      }
    >
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Field
          label="Put it in this folder"
          hint="A container, not the loop itself. The loop gets its own directory inside it, named after the loop."
          required
          wide
        >
          {(id) => (
            <div className="flex items-start gap-2">
              {/* min-w-0 lets the input actually shrink; without it the flex
                  base size wins and the button is pushed to the next line. */}
              <div className="min-w-0 flex-1">
                <Text id={id} mono value={parent} onChange={onParent} placeholder="~/loops" invalid={!!facts && !facts.writable} />
              </div>
              <PickFolder startIn={parent || undefined} onPick={onParent} label="Choose the folder for this loop" />
            </div>
          )}
        </Field>
        <Field label="Loop name" helpPath="name" required>
          {(id) => <Text id={id} mono value={cfg.name} onChange={(name) => patch({ name })} placeholder="blog-pipeline" />}
        </Field>
        <Field label="Config grammar" hint="The same model either way. YAML is compact; Markdown reads like a document.">
          {(id) => (
            <Select id={id} value={format} onChange={onFormat}
              options={[
                { value: "yaml", label: "loop.yaml — compact" },
                { value: "markdown", label: "loop.md — reads like prose" },
              ]} />
          )}
        </Field>
        <Field label="Purpose" helpPath="description" wide>
          {(id) => <Text id={id} value={cfg.description ?? ""} onChange={(description) => patch({ description })} placeholder="Draft, fact-check, and publish one post a week in the house voice." />}
        </Field>
      </div>

      {parent.trim() && cfg.name.trim() && (
        <p className="hint mt-3 stripe stripe-ember py-1">
          Will be created at <span className="font-mono text-text">{loopPath}</span>
        </p>
      )}

      {facts && (
        <div className="mt-4 space-y-2">
          <div className="flex flex-wrap gap-1.5">
            <span className={`chip ${facts.exists ? "chip-good" : ""}`}>
              {facts.exists ? "exists" : "will be created"}
            </span>
            {facts.exists && !facts.empty && <span className="chip chip-warn">not empty</span>}
            <span className={`chip ${facts.in_git_repo ? "chip-quench" : ""}`}>
              {facts.in_git_repo ? "inside a git repo" : "not a git repo"}
            </span>
            {facts.existing_loop && <span className="chip chip-warn">already has {facts.existing_loop}</span>}
          </div>

          {!facts.writable && (
            <Note tone="error">
              This process cannot write to {facts.path}. Pick another directory, or fix the
              permissions on that one. Nothing will be created until it is writable.
            </Note>
          )}
          {facts.existing_loop && (
            <Note tone="warning">
              {facts.path} already holds a {facts.existing_loop}. Creating here needs the
              overwrite option, and that replaces the config that is already there. Use Open
              instead if you meant to edit it.
            </Note>
          )}
          {!facts.in_git_repo && !initGit && (
            <Note tone="warning">
              Without a repository, nodes marked <span className="font-mono">isolated</span> cannot have
              their own worktree — they will all share one directory and say so. That is fine for a single
              builder and destructive for two running at once, because they overwrite each other's files.
              Turn the setting below back on, or run <span className="font-mono">git init</span> there
              yourself later.
            </Note>
          )}
        </div>
      )}

      <hr className="rule my-4" />
      <Toggle
        checked={initGit}
        onChange={onInitGit}
        label="Make it a git repository"
        hint="Initialises a repo with one commit in the new directory. This is what lets nodes marked `isolated` get a worktree each instead of silently sharing one — the scaffold already writes a .gitignore expecting it."
      />

      <hr className="rule my-4" />
      <p className="label mb-1.5">What this loop will be allowed to do</p>
      <p className="hint mb-2.5">
        Derived from the config, not guessed: a loop that only reads files never asks for write
        access. An agent CLI working in this folder needs these granted, or it will stop and ask
        partway through a run that was supposed to be hands-off.
      </p>
      <div className="flex flex-wrap gap-1.5">
        {permissions.length === 0 ? (
          <span className="hint">Nothing yet — add a provider or a script check.</span>
        ) : (
          permissions.map((p) => <span key={p} className="chip font-mono">{p}</span>)
        )}
      </div>
      {facts && !facts.has_claude_settings && permissions.length > 0 && (
        <div className="mt-3">
          <Note tone="warning">
            There is no <span className="font-mono">.claude/settings.local.json</span> in this
            folder yet, so a Claude Code node would be asked to approve each of these mid-run.
            The <span className="font-semibold">Grant permissions</span> button below writes them
            in, merging with anything already there.
          </Note>
        </div>
      )}
    </Card>
  );
}

/* --- secrets ------------------------------------------------------------- */

export function Secrets({ detection, needed }: { detection: Detection | null; needed: string[] }) {
  const [statuses, setStatuses] = useState<SecretStatus[]>([]);
  const [store, setStore] = useState<SecretStore>("profile");
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [shown, setShown] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Wrapped rather than passed straight to useEffect: an effect that returns
  // a promise looks to React like it returned a cleanup function.
  useEffect(() => {
    api.secrets().then(setStatuses).catch(() => {});
  }, []);

  const keychain = statuses.find((s) => s.keychain_kind)?.keychain_kind ?? null;
  const purposes = new Map((detection?.env_keys ?? []).map((k) => [k.name, k.purpose]));

  const save = async (name: string) => {
    setBusy(name);
    setError(null);
    try {
      const next = await api.setSecret(name, drafts[name] ?? "", store);
      setStatuses((all) => all.map((s) => (s.name === name ? next : s)));
      setDrafts((d) => ({ ...d, [name]: "" }));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(null);
    }
  };

  const reveal = async (name: string) => {
    if (shown[name]) {
      setShown((s) => { const n = { ...s }; delete n[name]; return n; });
      return;
    }
    try {
      const { value } = await api.revealSecret(name, store);
      setShown((s) => ({ ...s, [name]: value }));
    } catch (e) {
      setError((e as Error).message);
    }
  };

  // Keys this config actually needs come first; the rest are available but
  // folded away, so the panel is short for the common case.
  const ordered = [...statuses].sort((a, b) => {
    const an = needed.includes(a.name) ? 0 : 1;
    const bn = needed.includes(b.name) ? 0 : 1;
    return an - bn || a.name.localeCompare(b.name);
  });
  const [showAll, setShowAll] = useState(false);
  const visible = showAll ? ordered : ordered.filter((s) => needed.includes(s.name) || s.in_env || s.in_profile || s.in_keychain);

  return (
    <Card
      title="API keys"
      summary="Saved to your machine, never into the config. Only the variable name is ever written to a loop file."
      detail="Two stores. The shell profile sets a real environment variable, which is what most CLIs need and what every other tool on the machine can see — the cost is a plaintext secret in a dotfile. The OS secret store keeps nothing in a dotfile, but only loopsmith-started runs see the value. Either way, loopsmith writes the key name into `requires_env` and nothing else."
      failure="A key pasted into a config file is a key that ends up in version control."
      actions={<span className="chip">{statuses.filter((s) => s.in_env || s.in_profile || s.in_keychain).length} set</span>}
    >
      <div className="mb-4 grid grid-cols-1 gap-4 md:grid-cols-2">
        <Field label="Where to save" hint={keychain ? `This machine has ${keychain}.` : "No OS secret store found on this machine."}>
          {(id) => (
            <Select id={id} value={store} onChange={setStore}
              options={[
                { value: "profile", label: "Shell profile — every tool sees it" },
                ...(keychain ? [{ value: "keychain" as const, label: `${keychain} — encrypted at rest` }] : []),
              ]} />
          )}
        </Field>
      </div>

      {error && <div className="mb-3"><Note tone="error">{error}</Note></div>}

      <div className="space-y-2">
        {visible.map((s) => {
          const set = s.in_env || s.in_profile || s.in_keychain;
          return (
            <div key={s.name} className="rounded-[10px] border bg-raised p-3">
              <div className="mb-2 flex flex-wrap items-center gap-2">
                <span className="font-mono text-[12.5px] font-semibold">{s.name}</span>
                {needed.includes(s.name) && <span className="chip chip-ember">this loop needs it</span>}
                {set ? <span className="chip chip-good">{Icon.check({ size: 11 })} set</span> : <span className="chip">not set</span>}
                {s.in_profile && <span className="chip">profile</span>}
                {s.in_keychain && <span className="chip chip-quench">{s.keychain_kind}</span>}
              </div>
              <p className="hint mb-2">{purposes.get(s.name) ?? "An API key."}</p>
              <div className="flex flex-wrap gap-2">
                <input
                  type={shown[s.name] ? "text" : "password"}
                  className="input mono flex-1 min-w-[12rem]"
                  autoComplete="off"
                  spellCheck={false}
                  placeholder={set ? "leave blank to keep the current value" : "paste the key"}
                  value={shown[s.name] ?? drafts[s.name] ?? ""}
                  onChange={(e) => setDrafts((d) => ({ ...d, [s.name]: e.target.value }))}
                  aria-label={s.name}
                />
                {set && (
                  <button type="button" className="btn btn-sm btn-icon" onClick={() => reveal(s.name)}
                    aria-label={shown[s.name] ? `Hide ${s.name}` : `Show ${s.name}`}>
                    {shown[s.name] ? Icon.eyeOff({ size: 14 }) : Icon.eye({ size: 14 })}
                  </button>
                )}
                <button type="button" className="btn btn-sm btn-primary" disabled={busy === s.name || !drafts[s.name]}
                  onClick={() => save(s.name)}>
                  {busy === s.name ? "Saving…" : "Save"}
                </button>
                {set && (
                  <button type="button" className="btn btn-sm btn-danger" disabled={busy === s.name}
                    onClick={async () => {
                      setBusy(s.name);
                      try {
                        const next = await api.setSecret(s.name, null, store);
                        setStatuses((all) => all.map((x) => (x.name === s.name ? next : x)));
                      } catch (e) { setError((e as Error).message); } finally { setBusy(null); }
                    }}>
                    Remove
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>

      <button type="button" className="btn btn-ghost btn-sm mt-3" onClick={() => setShowAll((v) => !v)}>
        {showAll ? "Show fewer" : `Show all ${statuses.length} known keys`}
      </button>

      <div className="mt-3">
        <Note>
          Saved keys are exported into runs loopsmith starts from here, so a key set a moment ago
          works on the next run without restarting anything. A shell you already had open will
          not see it until you open a new one.
        </Note>
      </div>
    </Card>
  );
}

/* --- preflight ----------------------------------------------------------- */

export function Preflight({ detection, onRescan, scanning }: {
  detection: Detection | null;
  onRescan: (deep: boolean) => void;
  scanning: boolean;
}) {
  const [deep, setDeep] = useState(false);
  if (!detection) {
    return (
      <Card title="This machine" summary="Checking what is installed…">
        <Skeleton ready={false} lines={4}>
          <span />
        </Skeleton>
      </Card>
    );
  }
  const p = detection.platform;
  return (
    <Card
      title="This machine"
      summary="What loopsmith found, and what it stops you doing."
      detail="Most of what goes wrong with a loop on a new machine is environmental and reported far from its cause: a detector written against GNU sed editing a file called `-e` on a Mac, a schedule handed to a host with no cron. These are the probes that make those obvious up front."
      failure="Skipping this is how a schedule silently never fires."
      defaultOpen={false}
      actions={
        <button type="button" className="btn btn-sm" disabled={scanning} onClick={() => onRescan(deep)}>
          {Icon.refresh({ size: 13 })} {scanning ? "Scanning…" : "Rescan"}
        </button>
      }
    >
      <dl className="grid grid-cols-2 gap-x-6 gap-y-2 md:grid-cols-4">
        {[
          ["Operating system", p.os],
          ["Userland", p.userland],
          ["bash", p.bash ?? "not on PATH"],
          ["Scheduler", p.scheduler ?? "none installed"],
          ["git", detection.git.version ?? "not on PATH"],
          ["Agent CLIs", `${detection.agents.length} found`],
          ["MCP servers", `${detection.mcp_servers.length} configured`],
          ["Sub-agents", `${detection.skills.length} installed`],
        ].map(([k, v]) => (
          <div key={k}>
            <dt className="text-[11px] uppercase tracking-wide text-faint">{k}</dt>
            <dd className="font-mono text-[12.5px]">{v}</dd>
          </div>
        ))}
      </dl>

      {detection.mcp_servers.length > 0 && (
        <>
          <hr className="rule my-4" />
          <p className="label mb-2">MCP servers already configured on this machine</p>
          <div className="flex flex-wrap gap-1.5">
            {detection.mcp_servers.map((m) => (
              <span key={`${m.origin}-${m.name}`} className="chip font-mono" title={`${m.command} ${m.args.join(" ")} — from ${m.origin}`}>
                {m.name}
              </span>
            ))}
          </div>
          <p className="hint mt-2">
            Found in your editor and desktop app configs. Add one as a provider with family
            <span className="font-mono"> mcp</span> to let a node call it.
          </p>
        </>
      )}

      {detection.notes.length > 0 && (
        <>
          <hr className="rule my-4" />
          <div className="space-y-2">
            {detection.notes.map((n, i) => <Note key={i} tone="warning">{n}</Note>)}
          </div>
        </>
      )}

      <hr className="rule my-4" />
      <Toggle checked={deep} onChange={setDeep}
        label="Probe harder on the next rescan"
        hint="Falls back to asking each CLI for help output when it will not report a version. Still free — the paid check is the Test button on each provider." />
    </Card>
  );
}
