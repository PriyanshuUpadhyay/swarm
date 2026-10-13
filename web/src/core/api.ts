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

export type Page = { start_offset: number; end_offset: number };

// Row wire types from `src/view` (P0-U1). An absent field means null, false or [].
export type RowKind =
  | "user"
  | "assistant"
  | "thought"
  | "toolUse"
  | "toolResult"
  | "diff"
  | "question"
  | "error"
  | "notice"
  | "system"
  | "result"
  | "shell"
  | "divider"
  | "fold"
  | "queued";

export type ToolState = "waiting" | "finished" | "failed" | "interrupted" | "unreported";

export type TextPreview = { id: string; text: string; is_truncated?: boolean; full_lines: number; full_bytes: number; hidden_lines: number };

export type DiffPreview = {
  id: string;
  path: string;
  patch: string;
  full_lines: number;
  full_bytes: number;
  omitted_lines: number;
  shortened_lines: number;
  added: number;
  removed: number;
};

export type ToolActivity = {
  name: string;
  input: unknown;
  output?: TextPreview;
  diffs?: DiffPreview[];
  state: ToolState;
  command?: string;
  path?: string;
  duration_seconds?: number;
  exit_code?: number;
  header_title: string;
  skill_body?: TextPreview;
};

export type CommandChip = { name: string; arguments: string; skill_body?: TextPreview; output?: TextPreview };

export type ShellRun = { command?: string; output: TextPreview; exit_code?: number };

export type FoldSummary = {
  commands: number;
  waits: number;
  other_tools?: { name: string; count: number }[];
  rings: number;
  failed: number;
  interrupted: number;
  unreported: number;
  is_running?: boolean;
  latest_title: string;
  duration_seconds?: number;
  state: ToolState;
  text: string;
};

export type Row = {
  id: string;
  kind: RowKind;
  text: string;
  detail?: string;
  diff?: DiffPreview;
  tool?: ToolActivity;
  tool_status?: string;
  system_kind?: string;
  shell?: ShellRun;
  command?: CommandChip;
  source_ids?: string[];
  ends_turn?: boolean;
  starts_turn?: boolean;
  arrives_mid_turn?: boolean;
  hidden?: boolean;
  output?: TextPreview;
  fold?: FoldSummary;
  steps?: Row[];
  default_open?: boolean;
  /** Source byte offset and event ordinal. */
  order: [number, number];
};

export type RowPage = { page: Page; rows: Row[]; usage: unknown; remove?: string[] };

/** One `rows` stream frame. */
export type RowFrame = { key: string; upsert?: Row[]; remove?: string[]; end_offset: number };

export type SubscribeKey = { key: string; from: number; view: "rows" };

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

  rows(session: string, agent: string, tail: number, before?: number): Promise<RowPage> {
    const q = new URLSearchParams({ session, agent, tail: String(tail) });
    if (before !== undefined) q.set("before", String(before));
    return this.json(`/api/rows?${q}`);
  }

  /** The full text behind a capped preview: a tool or shell output, or a diff's whole patch. */
  async toolOutput(session: string, agent: string, row: string): Promise<string> {
    const q = new URLSearchParams({ session, agent, row });
    return (await this.json<{ text: string }>(`/api/tool-output?${q}`)).text;
  }

  subscribe(stream: string, keys: SubscribeKey[]): Promise<unknown> {
    return this.json("/api/subscribe", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ stream, keys }),
    });
  }
}
