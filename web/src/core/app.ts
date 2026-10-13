// Framework-free app state. Each view layer reads `view`, which is a new object after every change.

import { Api, type Row, type RowFrame, type Snapshot, type SubscribeKey } from "./api";
import { installKeys, Registry, search, type Command } from "./commands";
import {
  chairInput,
  confirmSent,
  decidePullBack,
  outgoing,
  SEND_KEY_STORAGE,
  sentRow,
  submission,
  type ChairInput,
  type Prompt,
  type Sent,
  type SendKey,
  type Usage,
} from "./composer";
import { MarkdownCache } from "./markdown";
import { measure, whenPainted, type PerfKind } from "./perf";
import { readSse, type SseEvent } from "./sse";
import { FoldChoices, findMatches, lines, mergeRows, type Line, type Match } from "./transcript";
import { buildTree, flatten, workspaces, type TreeNode, type TreeRow } from "./tree";

export const TAIL = 200;
export const LRU_SIZE = 4;
const MAX_KEYS = 32;

/** The full text behind a capped preview, fetched on demand. */
export type FullText = { state: "loading" } | { state: "ready"; text: string; lines: string[] } | { state: "error"; error: string };

type Chat = {
  sessionId: string;
  key: string;
  rows: Row[];
  /** The next older page ends here; 0 means no older rows. */
  older: number;
  end: number;
  state: "loading" | "ready" | "error";
  error: string | null;
  version: number;
  shift: boolean;
  loadingOlder: boolean;
  // Reader state and render caches; the LRU drops them.
  showHidden: boolean;
  folds: FoldChoices;
  opened: Map<string, boolean>;
  full: Map<string, FullText>;
  md: MarkdownCache;
  built: { version: number; lines: Line[]; hidden: number } | null;
  usage: Usage;
  /** Texts typed into a running Codex or AGY chair, shown as queued rows until the log has them. */
  sent: Sent[];
};

/** A failed chat action: the server's error text, and the same action again. */
export type Banner = { text: string; retry: () => void };

/** One chat's composer actions in flight. It outlives the chat's rows in the LRU. */
export type Input = {
  sending: boolean;
  stopping: boolean;
  pulling: boolean;
  answering: { prompt: string; choice: number } | null;
  alreadySent: boolean;
  banner: Banner | null;
  /** Bumped when the app changes the draft (a send or a pull-back), so the box reloads it. */
  draftRev: number;
};

export type Composer = {
  sessionId: string;
  chair: ChairInput;
  state: Input;
  /** The owner's queued rows at the end of the transcript. */
  queued: number;
  usage: Usage;
  sendKey: SendKey;
};

export type Find = { open: boolean; query: string; matches: Match[]; index: number; seq: number };

export type View = {
  loaded: boolean;
  roots: TreeNode[];
  error: string | null;
  rows: TreeRow[];
  selected: string | null;
  openId: string | null;
  chatState: "none" | "loading" | "ready" | "error";
  chatError: string | null;
  lines: Line[];
  hidden: number;
  showHidden: boolean;
  hasOlder: boolean;
  loadingOlder: boolean;
  shift: boolean;
  md: MarkdownCache | null;
  full: ReadonlyMap<string, FullText>;
  find: Find;
  paletteOpen: boolean;
  composer: Composer | null;
};

export type PaletteEntry = { id: string; title: string; hint?: string; run: () => void };

const EMPTY: Line[] = [];
const NO_FULL: ReadonlyMap<string, FullText> = new Map();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const chatKey = (sessionId: string) => `${sessionId}:chair`;
const DRAFT_PREFIX = "composer.draft.";
const message = (err: unknown) => (err instanceof Error ? err.message : String(err));
const readStore = (key: string): string | null => {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
};
const writeStore = (key: string, value: string) => {
  try {
    if (value) localStorage.setItem(key, value);
    else localStorage.removeItem(key);
  } catch {
    // Keep working without persistence.
  }
};

export class App {
  readonly registry = new Registry();
  view: View;
  private snapshot: Snapshot | null = null;
  private roots: TreeNode[] = [];
  private rows: TreeRow[] = [];
  private collapsed = new Set<string>();
  private selected: string | null = null;
  private openId: string | null = null;
  private paletteOpen = false;
  private error: string | null = null;
  private chats = new Map<string, Chat>();
  private lru: string[] = [];
  private remembered = new Map<string, string>();
  private listeners = new Set<() => void>();
  private streamId: string | null = null;
  private sentKeys = "";
  private pending = new Map<string, RowFrame[]>();
  private find: Find = { open: false, query: "", matches: [], index: -1, seq: 0 };
  private findVersion = "";
  private frame = 0;
  private snapshotBusy: Promise<void> | null = null;
  private snapshotAgain = false;
  private inputs = new Map<string, Input>();
  private drafts = new Map<string, string>();
  private alreadySentSeq = 0;

  constructor(
    readonly api: Api,
    readonly liveAll = false,
  ) {
    this.view = this.makeView();
    this.registry.add(...this.commands());
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private notify() {
    this.view = this.makeView();
    for (const fn of this.listeners) fn();
  }

  private makeView(): View {
    const chat = this.openId ? this.chats.get(this.openId) : undefined;
    if (chat && chat.built?.version !== chat.version) chat.built = { version: chat.version, ...this.build(chat) };
    this.refind(chat);
    return {
      loaded: this.snapshot !== null,
      roots: this.roots,
      error: this.error,
      rows: this.rows,
      selected: this.selected,
      openId: this.openId,
      chatState: chat ? chat.state : "none",
      chatError: chat?.error ?? null,
      lines: chat?.built?.lines ?? EMPTY,
      hidden: chat?.built?.hidden ?? 0,
      showHidden: chat?.showHidden ?? false,
      hasOlder: (chat?.older ?? 0) > 0,
      loadingOlder: chat?.loadingOlder ?? false,
      shift: chat?.shift ?? false,
      md: chat?.md ?? null,
      full: chat?.full ?? NO_FULL,
      find: this.find,
      paletteOpen: this.paletteOpen,
      composer: chat ? this.composerView(chat) : null,
    };
  }

  private build(chat: Chat) {
    const rows = chat.sent.length ? chat.rows.concat(chat.sent.map((s) => s.row)) : chat.rows;
    return lines(rows, { showHidden: chat.showHidden, folds: chat.folds, opened: chat.opened });
  }

  private composerView(chat: Chat): Composer {
    let queued = 0;
    for (let i = chat.rows.length - 1; i >= 0 && chat.rows[i].kind === "queued"; i--) queued++;
    return {
      sessionId: chat.sessionId,
      chair: this.chair(chat.sessionId),
      state: { ...this.input(chat.sessionId) },
      queued,
      usage: chat.usage,
      sendKey: readStore(SEND_KEY_STORAGE) === "commandReturn" ? "commandReturn" : "return",
    };
  }

  start(): () => void {
    window.__swarmPerf = [];
    const uninstall = installKeys(this.registry);
    const abort = new AbortController();
    void this.refreshSnapshot().then(() => {
      whenPainted(
        () => document.querySelector('[data-testid="sidebar"][data-loaded="true"]') !== null,
        () => (window.__swarmReady = true),
      );
    });
    void this.connect(abort.signal);
    return () => {
      uninstall();
      abort.abort();
    };
  }

  // ---- snapshot and tree ----

  private refreshSnapshot(): Promise<void> {
    if (this.snapshotBusy) {
      this.snapshotAgain = true;
      return this.snapshotBusy;
    }
    this.snapshotBusy = (async () => {
      do {
        this.snapshotAgain = false;
        try {
          this.applySnapshot(await this.api.snapshot());
          this.error = null;
        } catch (err) {
          this.error = String(err);
        }
        this.notify();
      } while (this.snapshotAgain);
      this.snapshotBusy = null;
    })();
    return this.snapshotBusy;
  }

  private applySnapshot(snap: Snapshot) {
    this.snapshot = snap;
    this.roots = buildTree(snap);
    this.rows = flatten(this.roots, this.collapsed);
    // A card stays busy until its question goes; the same question shown again is a new ask.
    for (const [id, input] of this.inputs)
      if (input.answering && this.chair(id).prompt?.id !== input.answering.prompt) input.answering = null;
    // A turn that ended took or dropped every typed text (ComposerSentMessages.update).
    for (const chat of this.chats.values())
      if (chat.sent.length && !this.chair(chat.sessionId).running) {
        chat.sent = [];
        chat.version++;
      }
    if (!this.selected || !this.rows.some((r) => r.id === this.selected)) this.selected = this.rows[0]?.id ?? null;
    if (this.liveAll) void this.loadAllForLive();
  }

  private reflatten() {
    this.rows = flatten(this.roots, this.collapsed);
  }

  private select(id: string, e?: KeyboardEvent) {
    this.selected = id;
    this.notify();
    const sidebarFocused = document.activeElement?.closest('[data-testid="sidebar"]') != null;
    const row = () => document.querySelector<HTMLElement>(`[data-testid="tree-row"][data-id="${CSS.escape(id)}"]`);
    if (sidebarFocused) whenPainted(() => row() !== null, () => row()?.focus());
    // A later key move in the same frame supersedes this one; its paint covers this one too.
    if (e) measure("key", id, e.timeStamp, () => this.selected !== id || row()?.getAttribute("aria-selected") === "true");
  }

  moveSelection(to: number | "home" | "end", e?: KeyboardEvent) {
    if (!this.rows.length) return;
    const i = this.rows.findIndex((r) => r.id === this.selected);
    const next = to === "home" ? 0 : to === "end" ? this.rows.length - 1 : Math.max(0, Math.min(this.rows.length - 1, i + to));
    if (this.rows[next].id !== this.selected) this.select(this.rows[next].id, e);
  }

  /** ← collapses an open row, else moves to its parent. */
  collapse(e?: KeyboardEvent) {
    const row = this.rows.find((r) => r.id === this.selected);
    if (!row) return;
    if (row.expanded) this.toggle(row.id);
    else if (row.parentId) this.select(row.parentId, e);
  }

  /** → opens a closed row, else moves to its first child. */
  expand(e?: KeyboardEvent) {
    const i = this.rows.findIndex((r) => r.id === this.selected);
    const row = this.rows[i];
    if (!row || row.expanded === null) return;
    if (!row.expanded) this.toggle(row.id);
    else if (this.rows[i + 1]?.parentId === row.id) this.select(this.rows[i + 1].id, e);
  }

  toggle(id: string) {
    if (!this.collapsed.delete(id)) this.collapsed.add(id);
    this.reflatten();
    this.notify();
  }

  /** Enter: open a chat, or fold a group. */
  activate(e?: KeyboardEvent) {
    const row = this.rows.find((r) => r.id === this.selected);
    if (!row) return;
    if (row.sessionId) this.openChat(row.sessionId, e);
    else this.toggle(row.id);
  }

  clickRow(id: string) {
    const row = this.rows.find((r) => r.id === id);
    if (!row) return;
    this.selected = id;
    if (row.sessionId) this.openChat(row.sessionId);
    else this.toggle(id);
  }

  // ---- chats ----

  private workspaceOf(sessionId: string): TreeNode | undefined {
    return workspaces(this.roots).find((w) => w.children.some((c) => c.sessionId === sessionId));
  }

  private newChat(sessionId: string): Chat {
    const chat: Chat = {
      sessionId,
      key: chatKey(sessionId),
      rows: [],
      older: 0,
      end: 0,
      state: "loading",
      error: null,
      version: 0,
      shift: false,
      loadingOlder: false,
      usage: null,
      sent: [],
      ...this.freshReaderState(),
    };
    this.chats.set(sessionId, chat);
    return chat;
  }

  private freshReaderState() {
    return {
      showHidden: false,
      folds: new FoldChoices(),
      opened: new Map<string, boolean>(),
      full: new Map<string, FullText>(),
      md: new MarkdownCache(),
      built: null,
    };
  }

  private async loadTail(chat: Chat) {
    try {
      const page = await this.api.rows(chat.sessionId, "chair", TAIL);
      chat.rows = mergeRows([], page.rows);
      chat.older = page.page.start_offset;
      chat.end = page.page.end_offset;
      chat.usage = page.usage as Usage;
      chat.sent = confirmSent(chat.sent, chat.rows);
      chat.state = "ready";
      chat.error = null;
    } catch (err) {
      chat.state = "error";
      chat.error = String(err);
    }
    chat.version++;
    chat.shift = false;
  }

  /** Marks the chat as recently rendered. Chats past the LRU lose their render caches (and,
   *  outside live mode, their model too). */
  private touch(sessionId: string) {
    this.lru = [sessionId, ...this.lru.filter((id) => id !== sessionId)];
    for (const old of this.lru.slice(LRU_SIZE)) {
      const chat = this.chats.get(old);
      if (!chat) continue;
      if (this.liveAll) Object.assign(chat, this.freshReaderState());
      else this.chats.delete(old);
    }
    this.lru = this.lru.slice(0, LRU_SIZE);
  }

  openChat(sessionId: string, e?: KeyboardEvent) {
    const kind: PerfKind = this.lru.includes(sessionId) ? "switch" : "open";
    this.openId = sessionId;
    const ws = this.workspaceOf(sessionId);
    if (ws) {
      this.remembered.set(ws.id, sessionId);
      const project = this.roots.find((p) => p.children.includes(ws));
      if (this.collapsed.has(ws.id) || (project && this.collapsed.has(project.id))) {
        this.collapsed.delete(ws.id);
        if (project) this.collapsed.delete(project.id);
        this.reflatten();
      }
    }
    this.selected = `c:${sessionId}`;
    this.touch(sessionId);
    let chat = this.chats.get(sessionId);
    if (!chat) {
      chat = this.newChat(sessionId);
      const c = chat;
      void this.loadTail(c).then(() => {
        this.syncSubscriptions();
        if (this.openId === c.sessionId) this.notify();
      });
    } else if (!this.liveAll) this.syncSubscriptions();
    this.notify();
    if (e) measure(kind, sessionId, e.timeStamp, () => this.transcriptPainted(sessionId));
  }

  /** The transcript shows this chat and its last row is in the DOM. */
  private transcriptPainted(sessionId: string): boolean {
    const el = document.querySelector<HTMLElement>('[data-testid="transcript"]');
    if (!el || el.dataset.chat !== sessionId || el.dataset.state !== "ready") return false;
    const last = this.view.lines.length - 1;
    return last < 0 || el.querySelector(`[data-testid="row"][data-index="${last}"]`) !== null;
  }

  private currentWorkspace(): TreeNode | undefined {
    if (this.openId) return this.workspaceOf(this.openId);
    const id = this.selected;
    return workspaces(this.roots).find((w) => w.id === id || w.children.some((c) => c.id === id));
  }

  /** ⌃⌘↓ / ⌃⌘↑ open the next or previous workspace at its remembered chat. */
  stepWorkspace(delta: number, e?: KeyboardEvent) {
    const all = workspaces(this.roots);
    if (!all.length) return;
    const cur = this.currentWorkspace();
    const i = cur ? all.indexOf(cur) : -1;
    const next = all[Math.max(0, Math.min(all.length - 1, i + delta))];
    if (next === cur) return;
    const sessionId = this.remembered.get(next.id) ?? next.children[0]?.sessionId;
    if (sessionId) this.openChat(sessionId, e);
  }

  /** ⌘1..⌘8 pick a chat in the workspace; ⌘9 picks the last. */
  pickChat(n: number, e?: KeyboardEvent) {
    const chats = this.currentWorkspace()?.children ?? [];
    const target = n === 9 ? chats[chats.length - 1] : chats[n - 1];
    if (target?.sessionId) this.openChat(target.sessionId, e);
  }

  /** ⇧⌘] / ⇧⌘[ step through the workspace's chats, wrapping. */
  stepChat(delta: number, e?: KeyboardEvent) {
    const chats = this.currentWorkspace()?.children ?? [];
    if (!chats.length) return;
    const i = chats.findIndex((c) => c.sessionId === this.openId);
    const target = chats[(i + delta + chats.length) % chats.length];
    if (target.sessionId && target.sessionId !== this.openId) this.openChat(target.sessionId, e);
  }

  private get open(): Chat | undefined {
    return this.openId ? this.chats.get(this.openId) : undefined;
  }

  /** A reader change: lines rebuild, and the list does not treat it as a prepend. */
  private changed(chat: Chat) {
    chat.version++;
    chat.shift = false;
    if (chat.sessionId === this.openId) this.notify();
  }

  /** Opens or closes a fold, a tool card, a thought, or a long system row. */
  toggleLine(line: Line) {
    const chat = this.open;
    if (!chat) return;
    if (line.row.kind === "fold") chat.folds.set(line.row, !line.open);
    else chat.opened.set(line.id, !line.open);
    this.changed(chat);
  }

  toggleHidden() {
    const chat = this.open;
    if (!chat) return;
    chat.showHidden = !chat.showHidden;
    this.changed(chat);
  }

  /** Fetches the full text behind a capped preview once and shows it; the LRU drops it. */
  async loadFull(previewId: string) {
    const chat = this.open;
    if (!chat || (chat.full.has(previewId) && chat.full.get(previewId)!.state !== "error")) return;
    chat.full = new Map(chat.full).set(previewId, { state: "loading" });
    this.changed(chat);
    let next: FullText;
    try {
      const text = await this.api.toolOutput(chat.sessionId, "chair", previewId);
      next = { state: "ready", text, lines: text.split("\n") };
    } catch (err) {
      next = { state: "error", error: String(err) };
    }
    chat.full = new Map(chat.full).set(previewId, next);
    this.changed(chat);
  }

  /** The full text for Copy, without showing it. */
  async fullText(previewId: string): Promise<string> {
    const chat = this.open;
    const have = chat?.full.get(previewId);
    if (have?.state === "ready") return have.text;
    if (!chat) throw new Error("no chat open");
    return this.api.toolOutput(chat.sessionId, "chair", previewId);
  }

  /** Pages older rows in above the current ones. */
  async loadOlder() {
    const chat = this.open;
    if (!chat || chat.state !== "ready" || chat.loadingOlder || chat.older <= 0) return;
    chat.loadingOlder = true;
    try {
      const page = await this.api.rows(chat.sessionId, "chair", TAIL, chat.older);
      chat.rows = mergeRows(chat.rows, page.rows, page.remove);
      chat.older = page.page.start_offset;
      chat.version++;
      chat.shift = true;
    } catch (err) {
      chat.error = String(err);
    } finally {
      chat.loadingOlder = false;
    }
    if (this.openId === chat.sessionId) this.notify();
  }

  // ---- find ----

  /** Matches follow the rows; the current match stays on its row when rows change. */
  private refind(chat: Chat | undefined) {
    const f = this.find;
    const sig = `${chat?.sessionId}:${chat?.version}:${chat?.showHidden}:${f.open}:${f.query}`;
    if (sig === this.findVersion) return;
    this.findVersion = sig;
    if (!f.open || !chat) {
      if (f.matches.length) this.find = { ...f, matches: [], index: -1 };
      return;
    }
    const current = f.matches[f.index]?.id;
    const matches = findMatches(chat.rows, f.query, chat.showHidden);
    const kept = current === undefined ? -1 : matches.findIndex((m) => m.id === current);
    this.find = { ...f, matches, index: kept >= 0 ? kept : matches.length ? matches.length - 1 : -1 };
    // A new current match must come into view.
    if (kept < 0 && matches.length) this.reveal(chat, this.find);
  }

  /** Opens the fold or card that holds the current match, and asks the list to scroll to it. */
  private reveal(chat: Chat, f: Find) {
    const m = f.matches[f.index];
    if (!m) return;
    if (m.fold && !chat.folds.isOpen(m.fold)) chat.folds.set(m.fold, true);
    chat.opened.set(m.id, true);
    chat.version++;
    chat.shift = false;
    chat.built = { version: chat.version, ...this.build(chat) };
    this.findVersion = `${chat.sessionId}:${chat.version}:${chat.showHidden}:${f.open}:${f.query}`;
    this.find = { ...f, seq: f.seq + 1 };
  }

  openFind() {
    if (!this.open) return;
    this.find = { ...this.find, open: true, seq: this.find.seq + 1 };
    this.notify();
    requestAnimationFrame(() => document.querySelector<HTMLInputElement>('[data-testid="find-input"]')?.select());
  }

  closeFind() {
    if (!this.find.open) return;
    this.find = { ...this.find, open: false };
    this.notify();
  }

  setFindQuery(query: string) {
    this.find = { ...this.find, query, matches: [], index: -1 };
    this.notify();
  }

  /** Find starts at the newest match. Next (+1) goes to the older one, as up the chat; it wraps. */
  stepFind(dir: number) {
    const chat = this.open;
    const f = this.find;
    if (!chat) return;
    if (!f.open) return this.openFind();
    if (!f.matches.length) return;
    this.find = { ...f, index: (f.index - dir + f.matches.length) % f.matches.length };
    this.reveal(chat, this.find);
    this.notify();
  }

  // ---- composer ----

  /** The chat's chair as the composer sees it, from the last snapshot. */
  chair(sessionId: string): ChairInput {
    return chairInput(
      this.snapshot?.sessions.find((s) => s.id === sessionId),
      this.snapshot?.agents[sessionId],
    );
  }

  private input(sessionId: string): Input {
    let input = this.inputs.get(sessionId);
    if (!input) {
      input = { sending: false, stopping: false, pulling: false, answering: null, alreadySent: false, banner: null, draftRev: 0 };
      this.inputs.set(sessionId, input);
    }
    return input;
  }

  private update(sessionId: string, change: Partial<Input>) {
    Object.assign(this.input(sessionId), change);
    if (sessionId === this.openId) this.notify();
  }

  /** Each chat keeps its draft, also across a reload (ComposerDraftStore). */
  draft(sessionId: string): string {
    let d = this.drafts.get(sessionId);
    if (d === undefined) this.drafts.set(sessionId, (d = readStore(DRAFT_PREFIX + sessionId) ?? ""));
    return d;
  }

  /** The box calls this on each edit; it does not render. */
  setDraft(sessionId: string, text: string) {
    this.drafts.set(sessionId, text);
    writeStore(DRAFT_PREFIX + sessionId, text);
  }

  private replaceDraft(sessionId: string, text: string) {
    this.setDraft(sessionId, text);
    this.update(sessionId, { draftRev: this.input(sessionId).draftRev + 1 });
  }

  /** Runs one chat action. A failure shows the server's text in the banner; only Retry runs it again. */
  private async act(sessionId: string, busy: Partial<Input>, idle: Partial<Input>, run: () => Promise<void>, retry: () => void): Promise<boolean> {
    this.update(sessionId, { ...busy, banner: null });
    try {
      await run();
      this.update(sessionId, idle);
      return true;
    } catch (err) {
      this.update(sessionId, { ...idle, banner: { text: message(err), retry } });
      return false;
    }
  }

  /** Types `text` into the chair. The draft clears only if it still holds the sent text. */
  async send(sessionId: string, text: string) {
    const chair = this.chair(sessionId);
    const st = this.input(sessionId);
    const out = outgoing(text);
    // A send typed while Up and C-u presses run would be cut or garbled.
    if (!out || st.sending || st.pulling || chair.disabled) return;
    const typed = submission(out);
    const ok = await this.act(
      sessionId,
      { sending: true },
      { sending: false },
      () => this.api.action("/api/chat/send", { session: sessionId, agent: "chair", text: typed }),
      () => void this.send(sessionId, text),
    );
    if (!ok) return;
    const chat = this.chats.get(sessionId);
    if (chat && chair.running && (chair.provider === "codex" || chair.provider === "agy")) {
      chat.sent = [...chat.sent, sentRow(typed, chair.provider, chat.rows)];
      this.changed(chat);
    }
    if (this.draft(sessionId) === text) this.replaceDraft(sessionId, "");
  }

  /** ⌘. and Stop: interrupt the chair's turn. */
  async interrupt(sessionId: string) {
    if (this.input(sessionId).stopping || this.chair(sessionId).unreachable) return;
    await this.act(
      sessionId,
      { stopping: true },
      { stopping: false },
      () => this.api.action("/api/chat/interrupt", { session: sessionId, agent: "chair" }),
      () => void this.interrupt(sessionId),
    );
  }

  /** Picks choice `choice` (zero-based); swarm refuses it when the question changed. */
  async answer(sessionId: string, prompt: Prompt, choice: number) {
    if (this.input(sessionId).answering?.prompt === prompt.id) return;
    const ok = await this.act(
      sessionId,
      { answering: { prompt: prompt.id, choice } },
      {},
      () => this.api.action("/api/chat/answer", { session: sessionId, agent: "chair", prompt_id: prompt.id, choice }),
      () => void this.answer(sessionId, prompt, choice),
    );
    // The card stays busy until the next snapshot drops this prompt.
    if (!ok) this.update(sessionId, { answering: null });
  }

  dismissBanner(sessionId: string) {
    this.update(sessionId, { banner: null });
  }

  /** ↑ on an empty draft: Claude's queued messages back into the draft, with the CLI's own keys
   *  (ADR 0038). Up pops the queue into the CLI box, and C-u presses empty that box again. */
  async pullBack(sessionId: string) {
    const chat = this.chats.get(sessionId);
    const st = this.input(sessionId);
    const chair = this.chair(sessionId);
    if (!chat || !chair.pullBack || chair.disabled || st.pulling || st.sending) return;
    const queued = chat.rows.filter((r) => r.kind === "queued");
    if (!queued.length) return;
    const before = new Set(chat.rows.map((r) => r.id));
    const key = (k: string) => this.api.action("/api/chat/key", { session: sessionId, agent: "chair", key: k });
    const result: { pulled: string[] | null } = { pulled: null };
    await this.act(
      sessionId,
      { pulling: true, alreadySent: false },
      { pulling: false },
      async () => {
        await key("Up");
        const deadline = performance.now() + 1500;
        let decision = decidePullBack(queued, before, chat.rows);
        while (decision.state === "waiting" && performance.now() < deadline) {
          await sleep(50);
          decision = decidePullBack(queued, before, chat.rows);
        }
        // C-u could wipe text Up pulled, so an unconfirmed box gets none.
        if (decision.state === "waiting") throw new Error("Could not confirm the pull-back. Check the agent's input box.");
        for (let i = 0; i < decision.presses; i++) {
          if (i) await sleep(50);
          await key("C-u");
        }
        if (decision.state === "pulled") result.pulled = decision.texts;
      },
      () => void this.pullBack(sessionId),
    );
    if (!result.pulled) {
      if (this.input(sessionId).banner) return;
      // Only the caption waits, so Send works again at once. A newer caption keeps its own timer.
      const seq = ++this.alreadySentSeq;
      this.update(sessionId, { alreadySent: true });
      setTimeout(() => seq === this.alreadySentSeq && this.update(sessionId, { alreadySent: false }), 2000);
      return;
    }
    // Keep anything typed while the CLI let go of the text.
    const typed = this.draft(sessionId);
    const text = result.pulled.join("\n");
    this.replaceDraft(sessionId, typed ? `${text}\n${typed}` : text);
  }

  // ---- palette ----

  setPalette(open: boolean) {
    if (this.paletteOpen === open) return;
    this.paletteOpen = open;
    this.notify();
  }

  /** Pure in (roots, query), so a memoizing view can cache it on those two. */
  paletteEntries(roots: TreeNode[], query: string): PaletteEntry[] {
    const cmds: PaletteEntry[] = this.registry
      .list()
      .filter((c) => c.palette !== false)
      .map((c) => ({ id: c.id, title: c.title, hint: c.chord, run: () => c.run() }));
    const chats: PaletteEntry[] = workspaces(roots).flatMap((w) =>
      w.children.map((c) => ({ id: c.id, title: c.title, run: () => this.openChat(c.sessionId!) })),
    );
    return search([...cmds, ...chats], query);
  }

  // ---- stream ----

  private async connect(signal: AbortSignal) {
    while (!signal.aborted) {
      try {
        await readSse(this.api.fetch, "/api/stream", this.api.headers(), (e) => this.onSse(e), signal);
      } catch {
        // Reconnect below.
      }
      this.streamId = null;
      this.sentKeys = "";
      if (!signal.aborted) await sleep(1000);
    }
  }

  private onSse(e: SseEvent) {
    let data: Record<string, unknown>;
    try {
      data = JSON.parse(e.data);
    } catch {
      return;
    }
    switch (e.event) {
      case "hello":
        this.streamId = String(data.stream);
        this.syncSubscriptions();
        void this.refreshSnapshot();
        break;
      case "snapshot":
        if (data.version !== this.snapshot?.version) void this.refreshSnapshot();
        break;
      case "rows": {
        const frame = data as unknown as RowFrame;
        const slot = this.pending.get(frame.key);
        if (slot) slot.push(frame);
        else this.pending.set(frame.key, [frame]);
        this.frame ||= requestAnimationFrame(() => this.flush());
        break;
      }
      case "reset":
        void this.reset(String(data.key));
        break;
    }
  }

  /** Applies all buffered row frames once per animation frame. Only the open chat renders. */
  private flush() {
    this.frame = 0;
    let render = false;
    for (const [key, frames] of this.pending) {
      const chat = this.chats.get(key.slice(0, key.lastIndexOf(":")));
      if (!chat || chat.state !== "ready") continue;
      const before = chat.rows;
      for (const f of frames) {
        chat.rows = mergeRows(chat.rows, f.upsert, f.remove);
        chat.end = Math.max(chat.end, Number(f.end_offset));
      }
      if (chat.rows !== before) {
        chat.sent = confirmSent(chat.sent, chat.rows);
        chat.version++;
        chat.shift = false;
        if (chat.sessionId === this.openId) render = true;
      }
    }
    this.pending.clear();
    if (render) this.notify();
  }

  private async reset(key: string) {
    this.pending.delete(key);
    const chat = this.chats.get(key.slice(0, key.lastIndexOf(":")));
    if (!chat) return;
    Object.assign(chat, this.freshReaderState());
    await this.loadTail(chat);
    this.sentKeys = "";
    this.syncSubscriptions();
    if (chat.sessionId === this.openId) this.notify();
  }

  private async loadAllForLive() {
    const fresh = (this.snapshot?.sessions ?? []).filter((s) => !this.chats.has(s.id)).map((s) => this.newChat(s.id));
    if (!fresh.length) return;
    await Promise.all(fresh.map((c) => this.loadTail(c)));
    this.syncSubscriptions();
    if (fresh.some((c) => c.sessionId === this.openId)) this.notify();
  }

  /** Sends the full key set when it changes. Live mode holds every session; otherwise the LRU. */
  private syncSubscriptions() {
    if (!this.streamId) return;
    const ids = this.liveAll ? [...this.chats.keys()] : this.lru;
    const keys: SubscribeKey[] = [];
    for (const id of ids) {
      const chat = this.chats.get(id);
      if (chat?.state === "ready") keys.push({ key: chat.key, from: chat.end, view: "rows" });
      if (keys.length === MAX_KEYS) break;
    }
    const sig = keys.map((k) => k.key).sort().join(",");
    if (sig === this.sentKeys) return;
    this.sentKeys = sig;
    this.api.subscribe(this.streamId, keys).catch(() => (this.sentKeys = ""));
  }

  // ---- commands ----

  private commands(): Command[] {
    const inTree = () => {
      const a = document.activeElement;
      return !a || a === document.body || a.closest('[data-testid="sidebar"]') !== null;
    };
    const tree = (id: string, title: string, chord: string, run: (e?: KeyboardEvent) => void): Command => ({
      id,
      title,
      chord,
      palette: false,
      when: () => !this.paletteOpen && inTree(),
      run,
    });
    const cmds: Command[] = [
      { id: "palette", title: "Command palette", chord: "Meta+K", palette: false, run: () => this.setPalette(!this.paletteOpen) },
      { id: "workspace.next", title: "Next workspace", chord: "Ctrl+Meta+ArrowDown", run: (e) => this.stepWorkspace(1, e) },
      { id: "workspace.prev", title: "Previous workspace", chord: "Ctrl+Meta+ArrowUp", run: (e) => this.stepWorkspace(-1, e) },
      { id: "chat.next", title: "Next chat", chord: "Shift+Meta+]", run: (e) => this.stepChat(1, e) },
      { id: "chat.prev", title: "Previous chat", chord: "Shift+Meta+[", run: (e) => this.stepChat(-1, e) },
      { id: "find.open", title: "Find in chat", chord: "Meta+F", when: () => this.openId !== null, run: () => this.openFind() },
      { id: "find.next", title: "Find next", chord: "Meta+G", when: () => this.openId !== null, run: () => this.stepFind(1) },
      { id: "find.prev", title: "Find previous", chord: "Shift+Meta+G", when: () => this.openId !== null, run: () => this.stepFind(-1) },
      {
        id: "composer.focus",
        title: "Focus composer",
        chord: "Meta+L",
        run: () => document.querySelector<HTMLElement>('[data-testid="composer"]')?.focus(),
      },
      {
        id: "chat.stop",
        title: "Stop the chair",
        chord: "Meta+.",
        when: () => this.openId !== null && !this.chair(this.openId).unreachable,
        run: () => void this.interrupt(this.openId!),
      },
      tree("tree.down", "Select next row", "j", (e) => this.moveSelection(1, e)),
      tree("tree.down2", "Select next row", "ArrowDown", (e) => this.moveSelection(1, e)),
      tree("tree.up", "Select previous row", "k", (e) => this.moveSelection(-1, e)),
      tree("tree.up2", "Select previous row", "ArrowUp", (e) => this.moveSelection(-1, e)),
      tree("tree.home", "Select first row", "Home", (e) => this.moveSelection("home", e)),
      tree("tree.end", "Select last row", "End", (e) => this.moveSelection("end", e)),
      tree("tree.collapse", "Collapse", "ArrowLeft", (e) => this.collapse(e)),
      tree("tree.expand", "Expand", "ArrowRight", (e) => this.expand(e)),
      tree("tree.open", "Open", "Enter", (e) => this.activate(e)),
    ];
    for (let n = 1; n <= 9; n++)
      cmds.push({ id: `chat.${n}`, title: n === 9 ? "Last chat" : `Chat ${n}`, chord: `Meta+${n}`, palette: false, run: (e) => this.pickChat(n, e) });
    return cmds;
  }
}
