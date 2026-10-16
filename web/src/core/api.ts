// HTTP client for `swarm serve`. The page saves the token in local storage (ADR 0088).

import type { ChoiceOp, TreeName, TreeView, TabsView } from "./nav";

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

/** One `rows` stream frame. `usage` is there only when it changed; null clears it. */
export type RowFrame = { key: string; upsert?: Row[]; remove?: string[]; end_offset: number; usage?: unknown };

export type SubscribeKey = { key: string; from: number; view: "rows" };

type Loc = Pick<Location, "hash" | "pathname" | "search">;
type Hist = Pick<History, "replaceState" | "state">;

type Saved = Pick<Storage, "getItem" | "setItem" | "removeItem">;

export const TOKEN_KEY = "swarm.token";

/** Runs a storage call; a blocked or full store only loses the saved token. */
function quietly<T>(call: () => T): T | undefined {
  try {
    return call();
  } catch {
    return undefined;
  }
}

/** Reads `#token=...`, saves it in `saved`, then removes the hash so the token does not stay in the
 *  URL or history. Without a hash it returns the saved token, so a reload or a new tab needs no
 *  link (ADR 0088). */
export function takeToken(loc: Loc, hist: Hist, saved?: Saved): string | null {
  const token = new URLSearchParams(loc.hash.slice(1)).get("token");
  if (token !== null) {
    hist.replaceState(hist.state, "", loc.pathname + loc.search);
    if (token) quietly(() => saved?.setItem(TOKEN_KEY, token));
    return token || null;
  }
  return quietly(() => saved?.getItem(TOKEN_KEY)) ?? null;
}

/** Drops the saved token, after the server refused it. */
export function forgetToken(saved?: Saved) {
  quietly(() => saved?.removeItem(TOKEN_KEY));
}

/** The server's `error` text, else the body, else the status. */
function errorText(text: string, res: Response): string {
  try {
    return String(JSON.parse(text).error ?? text);
  } catch {
    return text || `${res.status} ${res.statusText}`;
  }
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

  tree(view: TreeName): Promise<TreeView> {
    return this.json(`/api/tree?view=${view}`);
  }

  tabs(workspace: string): Promise<TabsView> {
    return this.json(`/api/tabs?${new URLSearchParams({ workspace })}`);
  }

  /** One owner choice. A stale revision is a 409 `ApiError`. */
  choice(revision: string, op: ChoiceOp): Promise<{ revision: string; version: number }> {
    return this.post("/api/choices", { revision, ...op });
  }

  /** A GET with query values. A failure throws the server's `error` text as the message. */
  async get<T>(path: string, query: Record<string, string> = {}): Promise<T> {
    const q = new URLSearchParams(query).toString();
    const res = await this.fetch(q ? `${path}?${q}` : path, { headers: this.headers() });
    const text = await res.text();
    if (res.ok) return JSON.parse(text) as T;
    throw new ApiError(res.status, errorText(text, res));
  }

  /** A chat action (P0-U3). A failure throws the server's `error` text as the message. */
  async action(path: string, body: Record<string, unknown>): Promise<void> {
    await this.post(path, body);
  }

  /** A POST with a JSON body and reply. A failure throws the server's `error` text as the message. */
  async post<T>(path: string, body: Record<string, unknown>): Promise<T> {
    const res = await this.fetch(path, {
      method: "POST",
      headers: this.headers({ "Content-Type": "application/json" }),
      body: JSON.stringify(body),
    });
    const text = await res.text();
    if (res.ok) return (text ? JSON.parse(text) : null) as T;
    throw new ApiError(res.status, errorText(text, res));
  }

  /** A pasted image. The server saves it and returns its path, which the agent opens. */
  async image(file: Blob): Promise<string> {
    const res = await this.fetch("/api/chat/image", { method: "POST", headers: this.headers({ "Content-Type": file.type }), body: file });
    const text = await res.text();
    if (res.ok) return (JSON.parse(text) as { path: string }).path;
    throw new ApiError(res.status, errorText(text, res));
  }

  /** A pasted image as a blob URL, for its preview; an <img> cannot send the bearer token. */
  async imageUrl(path: string): Promise<string> {
    const name = path.slice(path.lastIndexOf("/") + 1);
    const res = await this.fetch(`/api/chat/image?name=${encodeURIComponent(name)}`, { headers: this.headers() });
    if (!res.ok) throw new ApiError(res.status, errorText(await res.text(), res));
    return URL.createObjectURL(await res.blob());
  }

  subscribe(stream: string, keys: SubscribeKey[]): Promise<unknown> {
    return this.json("/api/subscribe", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ stream, keys }),
    });
  }
}

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
