// HTTP client for `swarm serve`. The token lives only in memory.

export type Session = {
  id: string;
  cwd: string;
  created_at: number;
  chair_provider: string | null;
  adapter: string | null;
  project: string | null;
  workspace: string | null;
  [field: string]: unknown;
};

export type Agent = { id: string; role: string; state: string | null; alive: boolean | null; [field: string]: unknown };

export type Snapshot = { version: number; sessions: Session[]; agents: Record<string, Agent[]> };

export type TranscriptEvent = { type: string; [field: string]: unknown };

export type Page = { start_offset: number; end_offset: number };

export type TranscriptPage = { page: Page; events: TranscriptEvent[] };

export type SubscribeKey = { key: string; from: number };

type Loc = Pick<Location, "hash" | "pathname" | "search">;
type Hist = Pick<History, "replaceState" | "state">;

/** Reads `#token=...`, then removes the hash so the token does not stay in the URL or history. */
export function takeToken(loc: Loc, hist: Hist): string | null {
  const token = new URLSearchParams(loc.hash.slice(1)).get("token");
  if (token !== null) hist.replaceState(hist.state, "", loc.pathname + loc.search);
  return token || null;
}

export class ApiError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
  }
}

export class Api {
  constructor(
    private readonly token: string,
    readonly fetch: typeof globalThis.fetch = globalThis.fetch.bind(globalThis),
  ) {}

  headers(extra?: Record<string, string>): Record<string, string> {
    return { Authorization: `Bearer ${this.token}`, ...extra };
  }

  private async json<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await this.fetch(path, { ...init, headers: this.headers(init?.headers as Record<string, string>) });
    const body = await res.text();
    if (!res.ok) throw new ApiError(res.status, `${path}: ${res.status} ${body}`);
    return (body ? JSON.parse(body) : null) as T;
  }

  snapshot(): Promise<Snapshot> {
    return this.json("/api/snapshot");
  }

  transcript(session: string, agent: string, tail: number, before?: number): Promise<TranscriptPage> {
    const q = new URLSearchParams({ session, agent, tail: String(tail) });
    if (before !== undefined) q.set("before", String(before));
    return this.json(`/api/transcript?${q}`);
  }

  subscribe(stream: string, keys: SubscribeKey[]): Promise<unknown> {
    return this.json("/api/subscribe", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ stream, keys }),
    });
  }
}
