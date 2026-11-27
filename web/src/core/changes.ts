// The Changes and PR panel of the open workspace (Scope v1 items 8 and 9, ADR 0076) on the S2 routes,
// and diff line comments that go to the open chat's agent as one message. It follows `App`'s
// workspace and keeps its own view; `App` stays unaware of it.

import { ApiError } from "./api";
import type { App } from "./app";
import { readStore, writeStore } from "./app-types";
import { sameRange } from "./changes-range";
import type { ChangeFile, ChangeList, ChangesView, Check, CommentEditor, DiffComment, FileDiff, FilePreview, Layer, Load, Mode, PrView, Tab } from "./changes-types";
import type { Command } from "./commands";

export type { ChangeFile, ChangeList, ChangesView, Check, CommentEditor, DiffComment, FileDiff, FilePreview, Layer, Load, Mode, PrView, PullRequest, Tab } from "./changes-types";

const IDLE = { state: "idle" } as const;
const LOADING = { state: "loading" } as const;
const NO_COMMENTS: DiffComment[] = [];
const message = (err: unknown) => (err instanceof Error ? err.message : String(err));
const stringPairs = (v: unknown) => Array.isArray(v) ? v.filter((x): x is [string, string] => Array.isArray(x) && x.length === 2 && x.every((s) => typeof s === "string")) : [];

export const fileKey = (f: Pick<ChangeFile, "path" | "layer">) => `${f.layer}\n${f.path}`;

/** The base a branch comparison starts on: the remote default branch, else a local main or master. */
export function defaultBase(refs: readonly string[], branch: string | null): string | null {
  const own = branch ? `refs/heads/${branch}` : null;
  for (const name of ["refs/remotes/origin/HEAD", "refs/remotes/origin/main", "refs/remotes/origin/master", "refs/heads/main", "refs/heads/master"])
    if (refs.includes(name) && name !== own) return name;
  return refs.find((r) => r !== own) ?? null;
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

  // A failed automatic read can hide the list without closing the full diff.
  private listedFiles: ChangeFile[] = [];

  private selected: string | null = null;
  private showFile = false;
  private diff: Load<FileDiff> = IDLE;
  private full: Load<FileDiff> = IDLE;
  private file: Load<FilePreview> = IDLE;
  private drafts = new Map<string, DiffComment[]>();
  private commentSeq = 0;
  private sending = false;
  private adding = false;
  private pr: Load<PrView> = IDLE;
  /** Bumped by each list, diff, file or PR read; an older reply is dropped. */
  private seq = { list: 0, diff: 0, file: 0, pr: 0 };

  private editors = new Map<string, CommentEditor>();
  private unfinished = new Map<string, string>();

  constructor(private readonly app: App) {
    try {
      const saved = JSON.parse(readStore("swarm.changes.v1") ?? "null");
      if (saved) {
        this.open = saved.open === true;
        this.tab = saved.tab === "pr" ? "pr" : "changes";
        this.mode = saved.mode === "branch" ? "branch" : "local";

        this.bases = new Map(stringPairs(saved.bases));
        this.unfinished = new Map(stringPairs(saved.unfinished));
        if (Array.isArray(saved.drafts)) this.drafts = new Map(saved.drafts.filter((x: unknown) => Array.isArray(x) && typeof x[0] === "string" && Array.isArray(x[1]) && x[1].every((c) => typeof c.id === "number" && typeof c.path === "string" && typeof c.text === "string" && typeof c.lines === "string" && typeof c.layer === "string" && typeof c.at === "number" && Array.isArray(c.quoted) && c.quoted.every((q: unknown) => typeof q === "string"))));
        this.commentSeq = Math.max(0, ...[...this.drafts.values()].flat().map((c) => c.id));
      }
    } catch { /* A bad saved value leaves the panel usable. */ }
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
    writeStore("swarm.changes.v1", JSON.stringify({ open: this.open, tab: this.tab, mode: this.mode, bases: [...this.bases], drafts: [...this.drafts], unfinished: [...this.unfinished] }));
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
      diff: this.full.state === "ready" ? this.full : this.diff,
      full: this.full,
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
    this.seq.list++;
    this.seq.pr++;
    this.workspace = this.app.view.workspace;
    this.list = IDLE;
    this.pr = IDLE;
    this.clearFile();
    if (this.open) this.reload();
    else this.notify();
  }

  private clearFile() {
    this.listedFiles = [];
    this.selected = null;
    this.diff = IDLE;
    this.full = IDLE;
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
    const before = document.activeElement;
    requestAnimationFrame(() => {
      if (document.activeElement === before || document.activeElement === document.body) document.querySelector<HTMLElement>(list)?.focus();
    });
  }

  close() {
    if (!this.open) return;
    this.open = false;
    this.notify();
  }

  /** Reads the shown tab again. */
  reload(automatic = false) {
    if (this.tab === "pr") void this.loadPr();
    else {
      if (!automatic) {
        if (this.full.state !== "idle") this.seq.diff++;
        this.full = IDLE;
      }
      void this.loadList(automatic);
    }
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

  private query(extra: Record<string, string> = {}) {
    const base = this.base();
    return { workspace: this.workspace ?? "", mode: this.mode, ...extra, ...(this.mode === "branch" && base ? { base } : {}) };
  }

  private async loadList(automatic = false) {
    if (!this.workspace) return;
    const workspace = this.workspace;
    const seq = ++this.seq.list;
    const keep = this.list.state === "ready";
    if (!keep) this.list = LOADING;
    this.notify();

    // Branch mode needs a base; the first read in local mode lists the refs.
    if (this.mode === "branch" && !this.base()) {
      try {
        const local = await this.app.api.get<ChangeList>("/api/changes", { workspace: this.workspace, mode: "local" });
        if (seq !== this.seq.list || workspace !== this.workspace) return;
        if (local.refs.length === 0) throw new Error("This workspace has no branch to compare with.");
        this.list = { state: "ready", data: { ...local, files: [] } };
      } catch (err) {
        if (seq !== this.seq.list || workspace !== this.workspace) return;
        this.list = { state: "error", error: message(err) };
        return this.notify();
      }
    }

    let next: Load<ChangeList>;
    try {
      next = { state: "ready", data: await this.app.api.get<ChangeList>("/api/changes", this.query()) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    if (seq !== this.seq.list || workspace !== this.workspace) return;
    const previous = this.listedFiles;
    const previousIndex = previous.findIndex((file) => fileKey(file) === this.selected);
    this.list = next;
    const files = next.state === "ready" ? next.data.files : [];
    if (next.state === "ready") this.listedFiles = files;

    const still = files.find((f) => fileKey(f) === this.selected);
    if (automatic && (still || next.state !== "ready") && (this.full.state === "loading" || this.full.state === "ready")) return this.notify();

    if (automatic && (still || next.state !== "ready") && this.full.state === "error" && this.full.rangeMismatch && this.diff.state === "ready") {
      const { path, layer = "branch" } = this.diff.data;
      if (this.editor(path, layer)?.open) return this.notify();
    }

    if (still) void this.select(still, true, automatic);
    else if (files.length) void this.select(files[Math.max(0, Math.min(previousIndex, files.length - 1))]);
    else {
      this.clearFile();
      this.notify();
    }
  }

  /** Shows a file's diff (or its contents, in file mode). `again` reads it even if it shows. */
  async select(f: ChangeFile, again = false, automatic = false) {
    const key = fileKey(f);
    if (key === this.selected && !again) return;
    this.selected = key;
    this.file = IDLE;
    if (!automatic) this.full = IDLE;
    const seq = ++this.seq.diff;
    if (this.diff.state !== "ready" || !again) this.diff = LOADING;
    this.notify();
    let next: Load<FileDiff>;
    try {
      next = { state: "ready", data: await this.app.api.get<FileDiff>("/api/changes/diff", this.query({ path: f.path, layer: f.layer })) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    if (seq !== this.seq.diff) return;
    this.diff = next;
    if (this.showFile) void this.loadFile(f.path);
    this.notify();
  }

  async expandDiff() {
    if (this.diff.state !== "ready" || !this.diff.data.is_truncated || (this.full.state !== "idle" && !(this.full.state === "error" && !this.full.capped))) return;
    const preview = this.diff.data;
    const { path, layer = "branch" } = preview;
    const seq = ++this.seq.diff;
    this.full = LOADING;
    this.notify();

    try {
      const data = await this.app.api.get<FileDiff>("/api/changes/diff", this.query({ path, layer, full: "1" }));
      if (seq !== this.seq.diff) return;
      const editor = this.editor(path, layer);
      this.full = sameRange(preview, data, editor) ? { state: "ready", data } : {
        state: "error", rangeMismatch: true, error: "The selected lines changed. The preview stays open.",
      };
      this.notify();
    } catch (err) {
      if (seq !== this.seq.diff) return;

      const capped = err instanceof ApiError && err.status === 413;
      const detail = err instanceof ApiError && (capped || err.status === 500) ? message(err) : "";
      this.full = { state: "error", capped, error: detail || "Could not load the full diff." };
      this.notify();
    }
  }

  /** j/k and the arrows in the file list. */
  move(by: number | "home" | "end", query = "") {
    const files = this.list.state === "ready" ? this.list.data.files.filter((f) => f.path.toLowerCase().includes(query.trim().toLowerCase())) : [];
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
      next = { state: "ready", data: await this.app.api.get<FilePreview>("/api/changes/file", { workspace: this.workspace ?? "", path }) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    if (seq !== this.seq.file) return;
    this.file = next;
    this.notify();
  }

  private async loadPr() {
    if (!this.workspace) return;
    const workspace = this.workspace;
    const seq = ++this.seq.pr;
    if (this.pr.state !== "ready") this.pr = LOADING;
    this.notify();
    let next: Load<PrView>;
    try {
      next = { state: "ready", data: await this.app.api.get<PrView>("/api/pr", { workspace: this.workspace }) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    if (seq !== this.seq.pr || workspace !== this.workspace) return;
    this.pr = next;
    this.notify();
  }

  private editorKey(path: string, layer: Layer) {
    return `${this.workspace}\n${fileKey({ path, layer })}`;
  }

  editor(path: string, layer: Layer): CommentEditor | null {
    return this.editors.get(this.editorKey(path, layer)) ?? null;
  }

  unfinishedText(path: string, layer: Layer) {
    return this.unfinished.get(this.editorKey(path, layer)) ?? "";
  }

  setEditor(path: string, layer: Layer, value: CommentEditor | null, keepText = false) {
    const key = this.editorKey(path, layer);
    if (value) this.editors.set(key, value);
    else this.editors.delete(key);

    if (value?.open && value.text) this.unfinished.set(key, value.text);
    else if (value?.open || (!value && !keepText)) this.unfinished.delete(key);
    this.notify();
  }

  // ---- diff line comments ----

  /** Adds a draft for display lines `from`..`to` of the shown diff. */
  async addComment(from: number, to: number, text: string, rangePatch?: string): Promise<boolean> {
    const ws = this.workspace;
    const d = this.view.diff.state === "ready" ? this.view.diff.data : null;
    if (!ws || !d?.patch || !d.display || !text.trim() || this.adding) return false;
    const layer = d.layer ?? "branch";
    const editor = this.editor(d.path, layer);
    const seq = this.seq.diff;
    this.adding = true;

    try {
      const range = await this.app.api.post<{ lines: string; quoted: string[] }>("/api/changes/comment-range", { op: "range", ...(rangePatch ? { patch: rangePatch, from: 0, to: rangePatch.split("\n").length } : { patch: d.patch, from, to }) });
      if (ws !== this.workspace || ws !== this.app.view.workspace) return false;
      if (seq !== this.seq.diff || this.view.diff.state !== "ready" || this.view.diff.data !== d) {
        this.app.notice("Could not add the comment. The diff changed. Try again.", true);
        return false;
      }
      if (this.editor(d.path, layer) !== editor) return false;
      if (!range.lines) return false;

      const comment: DiffComment = { id: ++this.commentSeq, path: d.path, layer, ...range, text: text.trim(), at: Math.max(from, to) };
      this.drafts.set(ws, [...(this.drafts.get(ws) ?? []), comment]);
      this.notify();
      return true;
    } catch (err) {
      this.app.notice(err instanceof ApiError && err.status === 413 ? "Could not add the comment. Select fewer lines." : message(err), true);
      return false;
    } finally {
      this.adding = false;
    }
  }

  confirmRemoveComment(id: number) {
    // The dialog handles cancel and action errors, so the caller does not wait for it.
    void this.app.life.ask({
      title: "Remove this comment?",
      body: "The saved comment will not be sent.",
      action: "Remove",
      cancelFirst: true,
      run: async () => this.removeComment(id),
    });
  }

  private removeComment(id: number) {
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
    try {
      const { message: text } = await this.app.api.post<{ message: string }>("/api/changes/comment-range", {
        op: "message", comments: comments.map(({ path, lines, quoted, text }) => ({ path, lines, quoted, text })),
      });
      if (ws !== this.workspace || ws !== this.app.view.workspace || chat !== this.app.view.openId || this.sendBlocked()) return;
      const sent = await this.app.send(chat, text, text, false);
      if (sent) {
        const left = (this.drafts.get(ws) ?? []).filter((c) => !comments.includes(c));
        this.drafts.set(ws, left);
        this.app.notice(`Sent ${comments.length === 1 ? "1 comment" : `${comments.length} comments`} to the agent.`);
      } else this.app.notice("The comments were not sent. The chat shows why.", true);
    } catch (err) {
      this.app.notice(message(err), true);
    } finally {
      this.sending = false;
      this.notify();
    }
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
