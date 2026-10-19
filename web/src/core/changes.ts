// The Changes and PR panel of the open workspace (Scope v1 items 8 and 9, ADR 0076) on the S2 routes,
// and diff line comments that go to the open chat's agent as one message. It follows `App`'s
// workspace and keeps its own view; `App` stays unaware of it.

import type { TextPreview } from "./api";
import type { App } from "./app";
import type { Command } from "./commands";
import { parsePatch, type PatchLine } from "./transcript";

export type Layer = "staged" | "unstaged" | "untracked" | "conflicted" | "branch";
export type Mode = "local" | "branch";

/** One changed file. A file in both the staged and unstaged layers is two entries. Binary files and
 *  symlinks have null counts. */
export type ChangeFile = { path: string; status: string; layer: Layer; added: number | null; removed: number | null };
export type ChangeList = { files: ChangeFile[]; refs: string[]; comparison: string[]; identity: { head: string | null; branch: string | null } };
/** A capped patch (200 lines, 12,000 characters) with the full counts, or a notice for a binary file. */
export type FileDiff = {
  path: string;
  layer?: Layer;
  patch?: string;
  is_truncated?: boolean;
  full_lines?: number;
  hidden_lines?: number;
  added: number | null;
  removed: number | null;
  notice?: string;
};
export type FilePreview = { path: string; preview?: TextPreview; notice?: string };

export type Check = {
  __typename?: string;
  name?: string;
  context?: string;
  workflowName?: string;
  status?: string;
  conclusion?: string | null;
  state?: string;
  detailsUrl?: string;
  targetUrl?: string;
};
export type PullRequest = {
  number: number;
  title: string;
  state: string;
  isDraft: boolean;
  baseRefName: string;
  headRefName: string;
  reviewDecision: string | null;
  url: string;
};
export type PrView = { pullRequest: PullRequest | null; checks: Check[] | null; differsFromLocalHead?: boolean };

export type Load<T> = { state: "idle" } | { state: "loading" } | { state: "ready"; data: T } | { state: "error"; error: string };

/** One draft comment on a diff: the file, the line range label, the quoted lines with their +/-
 *  marks, and the owner's text. `at` is the display line it shows under. */
export type DiffComment = { id: number; path: string; layer: Layer; lines: string; quoted: string[]; text: string; at: number };

export type Tab = "changes" | "pr";

export type ChangesView = {
  open: boolean;
  tab: Tab;
  workspace: string | null;
  mode: Mode;
  base: string | null;
  list: Load<ChangeList>;
  /** `fileKey` of the file whose diff shows. */
  selected: string | null;
  /** Diff, or the read-only file. */
  showFile: boolean;
  diff: Load<FileDiff>;
  file: Load<FilePreview>;
  /** Drafts of this workspace, oldest first. They survive a file switch and a closed panel. */
  comments: DiffComment[];
  sending: boolean;
  pr: Load<PrView>;
};

const IDLE = { state: "idle" } as const;
const LOADING = { state: "loading" } as const;
const NO_COMMENTS: DiffComment[] = [];
const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

/** A patch's lines as the diff shows them: hunks and code, no file headers. */
export const diffLines = (patch: string) => parsePatch(patch).filter((l) => l.kind !== "meta");

export const fileKey = (f: Pick<ChangeFile, "path" | "layer">) => `${f.layer}\n${f.path}`;

/** The base a branch comparison starts on: the remote default branch, else a local main or master. */
export function defaultBase(refs: readonly string[], branch: string | null): string | null {
  const own = branch ? `refs/heads/${branch}` : null;
  for (const name of ["refs/remotes/origin/HEAD", "refs/remotes/origin/main", "refs/remotes/origin/master", "refs/heads/main", "refs/heads/master"])
    if (refs.includes(name) && name !== own) return name;
  return refs.find((r) => r !== own) ?? null;
}

/** The comment's line label and quoted lines for display lines `from`..`to` of a parsed patch. New
 *  line numbers name the range; a range of only removed lines uses the old numbers. */
export function commentRange(lines: readonly PatchLine[], from: number, to: number): { lines: string; quoted: string[] } {
  const picked = lines.slice(Math.min(from, to), Math.max(from, to) + 1).filter((l) => l.kind === "add" || l.kind === "del" || l.kind === "ctx");
  const news = picked.flatMap((l) => (l.new === undefined || l.kind === "del" ? [] : [l.new]));
  const olds = picked.flatMap((l) => (l.old === undefined ? [] : [l.old]));
  const [nums, prefix] = news.length ? [news, "line"] : [olds, "old line"];
  const lo = Math.min(...nums);
  const hi = Math.max(...nums);
  return {
    lines: !nums.length ? "" : lo === hi ? `${prefix} ${lo}` : `${prefix}s ${lo}-${hi}`,
    quoted: picked.map((l) => `${l.kind === "add" ? "+" : l.kind === "del" ? "-" : " "}${l.text}`),
  };
}

/** The one message that carries every draft: file, line range, quoted lines, then the comment. */
export function commentMessage(comments: readonly DiffComment[]): string {
  const parts = comments.map((c) => [`${c.path} (${c.lines}):`, ...c.quoted.map((q) => `> ${q}`), c.text].join("\n"));
  return [`Review comments on the diff (${comments.length}):`, ...parts].join("\n\n");
}

/** A check's state as one word, from a GitHub CheckRun or StatusContext. */
export function checkState(c: Check): "passed" | "failed" | "pending" | "skipped" {
  const s = (c.conclusion || c.state || c.status || "").toUpperCase();
  if (["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STARTUP_FAILURE", "STALE"].includes(s)) return "failed";
  if (["SUCCESS"].includes(s)) return "passed";
  if (["NEUTRAL", "SKIPPED"].includes(s)) return "skipped";
  return "pending";
}

/** Only an http(s) link from GitHub opens; anything else shows as text. */
export const safeUrl = (url: string | undefined) => (url && /^https?:\/\//i.test(url) ? url : null);

export class Changes {
  view: ChangesView;
  private listeners = new Set<() => void>();
  private open = false;
  private tab: Tab = "changes";
  private workspace: string | null = null;
  private mode: Mode = "local";
  private bases = new Map<string, string>();
  private list: Load<ChangeList> = IDLE;
  private selected: string | null = null;
  private showFile = false;
  private diff: Load<FileDiff> = IDLE;
  private file: Load<FilePreview> = IDLE;
  private drafts = new Map<string, DiffComment[]>();
  private commentSeq = 0;
  private sending = false;
  private pr: Load<PrView> = IDLE;
  /** Bumped by each list, diff, file or PR read; an older reply is dropped. */
  private seq = { list: 0, diff: 0, file: 0, pr: 0 };

  constructor(private readonly app: App) {
    this.view = this.makeView();
    // Only a workspace change reloads; a rows frame in the chat costs one comparison.
    app.subscribe(() => {
      if (app.view.workspace !== this.workspace) this.follow();
    });
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private notify() {
    this.view = this.makeView();
    for (const fn of this.listeners) fn();
  }

  private makeView(): ChangesView {
    return {
      open: this.open,
      tab: this.tab,
      workspace: this.workspace,
      mode: this.mode,
      base: this.base(),
      list: this.list,
      selected: this.selected,
      showFile: this.showFile,
      diff: this.diff,
      file: this.file,
      comments: (this.workspace && this.drafts.get(this.workspace)) || NO_COMMENTS,
      sending: this.sending,
      pr: this.pr,
    };
  }

  private base(): string | null {
    if (!this.workspace) return null;
    const chosen = this.bases.get(this.workspace);
    if (chosen) return chosen;
    return this.list.state === "ready" ? defaultBase(this.list.data.refs, this.list.data.identity.branch) : null;
  }

  private follow() {
    this.workspace = this.app.view.workspace;
    this.list = IDLE;
    this.pr = IDLE;
    this.clearFile();
    if (this.open) this.reload();
    else this.notify();
  }

  private clearFile() {
    this.selected = null;
    this.diff = IDLE;
    this.file = IDLE;
    this.seq.diff++;
    this.seq.file++;
  }

  /** ⌥⌘I and the palette: shows the panel on `tab`, or hides it when that tab already shows. */
  toggle(tab: Tab = "changes") {
    if (this.open && this.tab === tab) return this.close();
    this.show(tab);
  }

  show(tab: Tab) {
    const wasOpen = this.open;
    this.open = true;
    this.tab = tab;
    if (this.workspace !== this.app.view.workspace) this.follow();
    else if (!wasOpen || (tab === "changes" ? this.list : this.pr).state === "idle") this.reload();
    else this.notify();
    const list = tab === "changes" ? '[data-testid="changes-list"]' : '[data-testid="pr-panel"]';
    requestAnimationFrame(() => document.querySelector<HTMLElement>(list)?.focus());
  }

  close() {
    if (!this.open) return;
    this.open = false;
    this.notify();
  }

  /** Reads the shown tab again. */
  reload() {
    if (this.tab === "pr") void this.loadPr();
    else void this.loadList();
    this.notify();
  }

  setMode(mode: Mode) {
    if (mode === this.mode) return;
    this.mode = mode;
    this.list = IDLE;
    this.clearFile();
    void this.loadList();
  }

  setBase(base: string) {
    if (!this.workspace) return;
    this.bases.set(this.workspace, base);
    if (this.mode === "branch") {
      this.clearFile();
      void this.loadList();
    } else this.notify();
  }

  private query(extra: Record<string, string> = {}): URLSearchParams {
    const q = new URLSearchParams({ workspace: this.workspace ?? "", mode: this.mode, ...extra });
    const base = this.base();
    if (this.mode === "branch" && base) q.set("base", base);
    return q;
  }

  private async read<T>(path: string, q: URLSearchParams): Promise<T> {
    const res = await this.app.api.fetch(`${path}?${q}`, { headers: this.app.api.headers() });
    const text = await res.text();
    if (res.ok) return JSON.parse(text) as T;
    let error = text || `${res.status} ${res.statusText}`;
    try {
      error = String(JSON.parse(text).error ?? error);
    } catch {
      // Not JSON: keep the body text.
    }
    throw new Error(error);
  }

  private async loadList() {
    if (!this.workspace) return;
    const seq = ++this.seq.list;
    const keep = this.list.state === "ready";
    if (!keep) this.list = LOADING;
    this.notify();
    // Branch mode needs a base; the first read in local mode lists the refs.
    if (this.mode === "branch" && !this.base()) {
      try {
        const local = await this.read<ChangeList>("/api/changes", new URLSearchParams({ workspace: this.workspace, mode: "local" }));
        if (seq !== this.seq.list) return;
        if (local.refs.length === 0) throw new Error("This workspace has no branch to compare with.");
        this.list = { state: "ready", data: { ...local, files: [] } };
      } catch (err) {
        if (seq !== this.seq.list) return;
        this.list = { state: "error", error: message(err) };
        return this.notify();
      }
    }
    let next: Load<ChangeList>;
    try {
      next = { state: "ready", data: await this.read<ChangeList>("/api/changes", this.query()) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    if (seq !== this.seq.list) return;
    this.list = next;
    const files = next.state === "ready" ? next.data.files : [];
    // The shown file stays when it is still changed; else the first file shows.
    const still = files.find((f) => fileKey(f) === this.selected);
    if (still) void this.select(still, true);
    else if (files.length) void this.select(files[0]);
    else {
      this.clearFile();
      this.notify();
    }
  }

  /** Shows a file's diff (or its contents, in file mode). `again` reads it even if it shows. */
  async select(f: ChangeFile, again = false) {
    const key = fileKey(f);
    if (key === this.selected && !again) return;
    this.selected = key;
    this.file = IDLE;
    const seq = ++this.seq.diff;
    if (this.diff.state !== "ready" || !again) this.diff = LOADING;
    this.notify();
    let next: Load<FileDiff>;
    try {
      next = { state: "ready", data: await this.read<FileDiff>("/api/changes/diff", this.query({ path: f.path, layer: f.layer })) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    if (seq !== this.seq.diff) return;
    this.diff = next;
    if (this.showFile) void this.loadFile(f.path);
    this.notify();
  }

  /** j/k and the arrows in the file list. */
  move(by: number | "home" | "end") {
    const files = this.list.state === "ready" ? this.list.data.files : [];
    if (!files.length) return;
    const i = files.findIndex((f) => fileKey(f) === this.selected);
    const next = by === "home" ? 0 : by === "end" ? files.length - 1 : Math.max(0, Math.min(files.length - 1, i + by));
    void this.select(files[next]);
  }

  setShowFile(show: boolean) {
    if (show === this.showFile) return;
    this.showFile = show;
    const path = this.selected?.slice(this.selected.indexOf("\n") + 1);
    if (show && path && this.file.state === "idle") void this.loadFile(path);
    this.notify();
  }

  private async loadFile(path: string) {
    const seq = ++this.seq.file;
    this.file = LOADING;
    this.notify();
    let next: Load<FilePreview>;
    try {
      next = { state: "ready", data: await this.read<FilePreview>("/api/changes/file", new URLSearchParams({ workspace: this.workspace ?? "", path })) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    if (seq !== this.seq.file) return;
    this.file = next;
    this.notify();
  }

  private async loadPr() {
    if (!this.workspace) return;
    const seq = ++this.seq.pr;
    if (this.pr.state !== "ready") this.pr = LOADING;
    this.notify();
    let next: Load<PrView>;
    try {
      next = { state: "ready", data: await this.read<PrView>("/api/pr", new URLSearchParams({ workspace: this.workspace })) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    if (seq !== this.seq.pr) return;
    this.pr = next;
    this.notify();
  }

  // ---- diff line comments ----

  /** Adds a draft for display lines `from`..`to` of the shown diff. */
  addComment(from: number, to: number, text: string) {
    const ws = this.workspace;
    const d = this.diff.state === "ready" ? this.diff.data : null;
    if (!ws || !d?.patch || !text.trim()) return;
    const range = commentRange(diffLines(d.patch), from, to);
    if (!range.lines) return;
    const comment: DiffComment = { id: ++this.commentSeq, path: d.path, layer: d.layer ?? "branch", ...range, text: text.trim(), at: Math.max(from, to) };
    this.drafts.set(ws, [...(this.drafts.get(ws) ?? []), comment]);
    this.notify();
  }

  removeComment(id: number) {
    const ws = this.workspace;
    if (!ws) return;
    this.drafts.set(ws, (this.drafts.get(ws) ?? []).filter((c) => c.id !== id));
    this.notify();
  }

  /** Why the drafts cannot go now, or null. They go to the open chat's chair. */
  sendBlocked(): string | null {
    const chat = this.app.view.openId;
    if (!chat) return "Open a chat in this workspace to send comments.";
    return this.app.chair(chat).disabled;
  }

  /** Sends every draft as one message through the composer's send; the drafts clear once it went. */
  async sendComments() {
    const ws = this.workspace;
    const chat = this.app.view.openId;
    const comments = ws ? this.drafts.get(ws) : undefined;
    if (!ws || !chat || !comments?.length || this.sending || this.sendBlocked()) return;
    this.sending = true;
    this.notify();
    const text = commentMessage(comments);
    const sent = await this.app.send(chat, text, text, false);
    this.sending = false;
    if (sent) {
      const left = (this.drafts.get(ws) ?? []).filter((c) => !comments.includes(c));
      this.drafts.set(ws, left);
      this.app.notice(`Sent ${comments.length === 1 ? "1 comment" : `${comments.length} comments`} to the agent.`);
    } else this.app.notice("The comments were not sent. The chat shows why.", true);
    this.notify();
  }

  commands(): Command[] {
    const ws = () => this.app.view.workspace !== null;
    return [
      { id: "changes.show", title: "Show Changes", chord: "Alt+Meta+I", when: ws, run: () => this.toggle("changes") },
      { id: "pr.show", title: "Show Pull Request", chord: "Alt+Meta+P", when: ws, run: () => this.toggle("pr") },
      { id: "changes.refresh", title: "Refresh Changes", when: () => this.open, run: () => this.reload() },
      { id: "changes.send", title: "Send Diff Comments to Agent", when: () => this.view.comments.length > 0, run: () => void this.sendComments() },
    ];
  }
}
