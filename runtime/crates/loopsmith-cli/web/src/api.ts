/**
 * Everything that talks to the Rust side.
 *
 * Kept in one file so the whole surface the browser depends on is readable in
 * one sitting, and so an endpoint that changes shape has exactly one place to
 * be fixed.
 */
import type {
  Detection, Review, ExampleCard, LibraryEntry, Help, SecretStatus, SecretStore,
  JobSummary, JobLine, LoopConfig, Format, PathFacts, HandshakeResult, Meta,
} from "./types";

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    headers: init?.body ? { "content-type": "application/json" } : undefined,
    ...init,
  });
  if (!res.ok) {
    // The server sends `{ error }` for everything it refuses; falling back to
    // the status line matters for the cases it does not reach, like a dev
    // proxy with nothing behind it.
    const body = await res.json().catch(() => null);
    throw new Error(body?.error ?? `${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

const post = <T,>(path: string, body: unknown) =>
  call<T>(path, { method: "POST", body: JSON.stringify(body) });

export const api = {
  meta: () => call<Meta>("/api/meta"),

  /** `deep` costs money — it puts a real prompt through each CLI. */
  detect: (deep = false) => call<Detection>(`/api/detect?deep=${deep}`),

  handshake: (command: string, args: string[], promptOnStdin: boolean) =>
    post<HandshakeResult>("/api/handshake", {
      command,
      args,
      prompt_on_stdin: promptOnStdin,
    }),

  pathFacts: (path: string) =>
    call<PathFacts>(`/api/path?path=${encodeURIComponent(path)}`),

  /**
   * Open the OS folder chooser. Resolves when the user picks or cancels —
   * `path` null and `unavailable` null means they cancelled, which is fine.
   */
  pickFolder: (startIn?: string) =>
    post<{ path: string | null; unavailable: string | null }>("/api/pick-folder", {
      start_in: startIn ?? null,
    }),

  help: () => call<Help>("/api/help"),

  examples: () => call<ExampleCard[]>("/api/examples"),
  example: (id: string) =>
    call<{ id: string; yaml: string; config: LoopConfig }>(`/api/examples/${id}`),

  library: () => call<LibraryEntry[]>("/api/library"),
  forget: (path: string) => post<{ ok: true }>("/api/library/forget", { path }),

  open: (path: string) =>
    post<{
      config: LoopConfig;
      format: Format;
      config_path: string;
      dir: string;
      facts: PathFacts;
    }>("/api/open", { path }),

  review: (config: unknown) => post<Review>("/api/review", config),

  render: (config: unknown, format: Format) =>
    post<{ text: string; file_name: string }>("/api/render", { config, format }),

  secrets: () => call<SecretStatus[]>("/api/secrets"),
  setSecret: (name: string, value: string | null, store: SecretStore) =>
    post<SecretStatus>("/api/secrets", { name, value, store }),
  revealSecret: (name: string, store: SecretStore) =>
    post<{ name: string; value: string }>("/api/secrets/reveal", { name, store }),

  jobs: () => call<JobSummary[]>("/api/jobs"),
  job: (id: string) =>
    call<{ summary: JobSummary; lines: JobLine[] }>(`/api/jobs/${id}`),
  cancel: (id: string) => post<{ ok: true }>(`/api/jobs/${id}/cancel`, {}),

  /**
   * Start one of the actions in `exec::Action`. The browser names a verb and
   * its parameters; it never names a program.
   */
  start: (body: Record<string, unknown>) => post<{ job: string }>("/api/jobs", body),
};

/**
 * Follow a job's output.
 *
 * The socket replays what has already been printed before going live, so this
 * hands back every line from the beginning regardless of when it is opened.
 */
export function streamJob(
  id: string,
  on: {
    line: (l: JobLine) => void;
    state: (s: JobSummary) => void;
    lagged?: (skipped: number, message: string) => void;
  },
): () => void {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const ws = new WebSocket(`${proto}//${location.host}/api/jobs/${id}/stream`);
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data as string);
    if (msg.type === "line") on.line(msg.line);
    else if (msg.type === "state") on.state(msg.summary);
    else if (msg.type === "lagged") on.lagged?.(msg.skipped, msg.message);
  };
  return () => ws.close();
}
