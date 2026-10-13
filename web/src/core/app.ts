// Framework-free app state. Each view layer reads `view`, which is a new object after every change.

import { Api, type Row, type RowFrame, type Snapshot, type SubscribeKey } from "./api";
import { installKeys, Registry, search, type Command } from "./commands";
import {
  agentInput,
  confirmSent,
  decidePullBack,
  outgoing,
  SEND_KEY_STORAGE,
  sentRow,
  submission,
  type AgentInput,
  type Prompt,
  type Sent,
  type SendKey,
  type Usage,
} from "./composer";
import { MarkdownCache } from "./markdown";
import { measure, whenPainted, type PerfKind } from "./perf";
import { readSse, type SseEvent } from "./sse";
import { movePane, paneCells, type Direction, type PaneCell } from "./panes";
import { FoldChoices, findMatches, lines, mergeRows, type Line, type Match } from "./transcript";
import { buildTree, flatten, workspaces, type TreeNode, type TreeRow } from "./tree";

export const TAIL = 200;
export const LRU_SIZE = 4;
const MAX_KEYS = 32;

/** The full text behind a capped preview, fetched on demand. */
export type FullText = { state: "loading" } | { state: "ready"; text: string; lines: string[] } | { state: "error"; error: string };

type Chat = {
  /** The session id for the chair's chat, and `paneId` for a child agent's. */
  id: string;
  sessionId: string;
  /** "chair", or the child agent's id. */
  agent: string;
  key: string;
  /** False for a child's chat until its column first shows. */
  loaded: boolean;
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
  memo: { sig: unknown[]; pane: Pane } | null;
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
  /** The chat id that every composer action takes. */
  id: string;
  agent: AgentInput;
  state: Input;
  /** The owner's queued rows at the end of the transcript. */
  queued: number;
  usage: Usage;
  sendKey: SendKey;
};

export type Find = { open: boolean; query: string; matches: Match[]; index: number; seq: number };

/** One transcript with its composer: the chair's chat, or a child agent's column. */
export type Pane = {
  /** The chat id; "" when no chat is open. */
  id: string;
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
  composer: Composer | null;
};

/** The open chat's child agents beside it (ADR 0030). `focused`, `zoomed` and `reveal` are agent ids. */
export type Strip = {
  cells: PaneCell[];
  panes: ReadonlyMap<string, Pane>;
  focused: string | null;
  zoomed: string | null;
  /** The pane a key moved focus to; null shows the chat page. The strip scrolls to it. */
  reveal: string | null;
  /** Changes on every reveal, so a repeated reveal still scrolls. */
  revealSeq: number;
};

/** The chair's chat as `Pane` fields, and the rest of the window. */
export type View = Pane & {
  loaded: boolean;
  roots: TreeNode[];
  error: string | null;
  rows: TreeRow[];
  selected: string | null;
  openId: string | null;
  paletteOpen: boolean;
  strip: Strip;
};

/** What a transcript's rows call. `App` is the reader of the chair's chat; `App.reader` gives a child's. */
export type Reader = {
  toggleLine(line: Line): void;
  toggleHidden(): void;
  loadFull(previewId: string): Promise<void>;
  fullText(previewId: string): Promise<string>;
  loadOlder(): Promise<void>;
  setFindQuery(query: string): void;
  stepFind(dir: number): void;
  closeFind(): void;
};

export type PaletteEntry = { id: string; title: string; hint?: string; run: () => void };

const EMPTY: Line[] = [];
const NO_FULL: ReadonlyMap<string, FullText> = new Map();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** The chat id of a child agent's column. It is also the agent's stream key. */
export const paneId = (sessionId: string, agent: string) => `${sessionId}:${agent}`;
const DRAFT_PREFIX = "composer.draft.";
const DISMISSED_STORAGE = "swarm.panes.dismissed";
const NO_FIND: Find = { open: false, query: "", matches: [], index: -1, seq: 0 };
const NO_PANE: Pane = {
  id: "",
  chatState: "none",
  chatError: null,
  lines: EMPTY,
  hidden: 0,
  showHidden: false,
  hasOlder: false,
  loadingOlder: false,
  shift: false,
  md: null,
  full: NO_FULL,
  find: NO_FIND,
  composer: null,
};
const NO_CELLS: PaneCell[] = [];
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
  private find: Find = NO_FIND;
  /** The chat that Find searches: the focused pane's when Find opened, else the chair's. */
  private findId: string | null = null;
  private findVersion = "";
  private frame = 0;
  private snapshotBusy: Promise<void> | null = null;
  private snapshotAgain = false;
  private inputs = new Map<string, Input>();
  private drafts = new Map<string, string>();
  private alreadySentSeq = 0;
  private inputRev = 0;
  private pane = { focused: null as string | null, zoomed: null as string | null, reveal: null as string | null, revealSeq: 0 };
  /** Chat ids of the child columns on screen. Only these follow their logs. */
  private visible = new Set<string>();
  /** Finished children the owner dismissed, per session (ADR 0056). P2 moves this to view state. */
  private dismissed: Record<string, string[]>;
  private cellsMemo: { snapshot: Snapshot | null; openId: string | null; dismissed: string[] | undefined; cells: PaneCell[] } | null = null;
  private strip: Strip = { cells: NO_CELLS, panes: new Map(), focused: null, zoomed: null, reveal: null, revealSeq: 0 };
  private readers = new Map<string, Reader>();

  constructor(
    readonly api: Api,
    readonly liveAll = false,
  ) {
    try {
      this.dismissed = JSON.parse(readStore(DISMISSED_STORAGE) ?? "{}");
    } catch {
      this.dismissed = {};
    }
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
    const sendKey: SendKey = readStore(SEND_KEY_STORAGE) === "commandReturn" ? "commandReturn" : "return";
    const chat = this.open;
    const cells = this.cells();
    // A pane that left the strip takes its focus, its zoom and its Find with it.
    const has = (id: string | null) => (id !== null && cells.some((c) => c.id === id) ? id : null);
    const p = this.pane;
    p.focused = has(p.focused);
    p.zoomed = has(p.zoomed);
    p.reveal = has(p.reveal);
    if (this.findId && !cells.some((c) => paneId(this.openId!, c.id) === this.findId)) this.findId = null;
    return {
      ...(chat ? this.paneView(chat, sendKey) : NO_PANE),
      loaded: this.snapshot !== null,
      roots: this.roots,
      error: this.error,
      rows: this.rows,
      selected: this.selected,
      openId: this.openId,
      paletteOpen: this.paletteOpen,
      strip: this.stripView(cells, sendKey),
    };
  }

  /** One chat as its transcript and composer see it. The same object comes back while nothing it
   *  shows has changed, so a sidebar key press does not render every column again. */
  private paneView(chat: Chat, sendKey: SendKey, hasLog = true): Pane {
    const finds = chat.id === (this.findId ?? this.openId);
    if (finds) this.refind(chat);
    const find = finds ? this.find : NO_FIND;
    const sig = [chat.version, chat.state, chat.loaded, chat.loadingOlder, find, this.snapshot, this.inputRev, sendKey, hasLog];
    if (chat.memo && sig.every((v, i) => v === chat.memo!.sig[i])) return chat.memo.pane;
    if (chat.built?.version !== chat.version) chat.built = { version: chat.version, ...this.build(chat) };
    const pane: Pane = {
      id: chat.id,
      // A child's column loads when it first shows; with no log yet it has nothing to load.
      chatState: chat.loaded ? chat.state : hasLog ? "loading" : "none",
      chatError: chat.error,
      lines: chat.built.lines,
      hidden: chat.built.hidden,
      showHidden: chat.showHidden,
      hasOlder: chat.older > 0,
      loadingOlder: chat.loadingOlder,
      shift: chat.shift,
      md: chat.md,
      full: chat.full,
      find,
      composer: this.composerView(chat, sendKey),
    };
    chat.memo = { sig, pane };
    return pane;
  }

  /** The open chat's child agents, in strip order. */
  private cells(): PaneCell[] {
    const id = this.openId;
    if (!id) return NO_CELLS;
    const dismissed = this.dismissed[id];
    const m = this.cellsMemo;
    if (m && m.snapshot === this.snapshot && m.openId === id && m.dismissed === dismissed) return m.cells;
    const cells = paneCells(this.snapshot?.sessions.find((s) => s.id === id), this.snapshot?.agents[id], dismissed);
    this.cellsMemo = { snapshot: this.snapshot, openId: id, dismissed, cells };
    return cells;
  }

  private stripView(cells: PaneCell[], sendKey: SendKey): Strip {
    const p = this.pane;
    const panes = new Map<string, Pane>();
    for (const c of cells) panes.set(c.id, this.paneView(this.childChat(this.openId!, c.id), sendKey, c.hasLog));
    const old = this.strip;
    const same =
      old.cells === cells &&
      old.focused === p.focused &&
      old.zoomed === p.zoomed &&
      old.reveal === p.reveal &&
      old.revealSeq === p.revealSeq &&
      cells.every((c) => old.panes.get(c.id) === panes.get(c.id));
    if (!same) this.strip = { cells, panes, ...p };
    return this.strip;
  }

  private build(chat: Chat) {
    const rows = chat.sent.length ? chat.rows.concat(chat.sent.map((s) => s.row)) : chat.rows;
    return lines(rows, { showHidden: chat.showHidden, folds: chat.folds, opened: chat.opened });
  }

  private composerView(chat: Chat, sendKey: SendKey): Composer {
    let queued = 0;
    for (let i = chat.rows.length - 1; i >= 0 && chat.rows[i].kind === "queued"; i--) queued++;
    return {
      id: chat.id,
      agent: this.chair(chat.id),
      state: { ...this.input(chat.id) },
      queued,
      usage: chat.usage,
      sendKey,
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
      if (chat.sent.length && !this.chair(chat.id).running) {
        chat.sent = [];
        chat.version++;
      }
    // A child's hooks report its log after its column shows.
    for (const id of this.visible) this.loadPane(id);
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

  private newChat(sessionId: string, agent = "chair"): Chat {
    const chat: Chat = {
      id: agent === "chair" ? sessionId : paneId(sessionId, agent),
      sessionId,
      agent,
      key: `${sessionId}:${agent}`,
      loaded: agent === "chair",
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
    this.chats.set(chat.id, chat);
    return chat;
  }

  /** A child agent's chat. It holds no rows until its column shows. */
  private childChat(sessionId: string, agent: string): Chat {
    return this.chats.get(paneId(sessionId, agent)) ?? this.newChat(sessionId, agent);
  }

  private freshReaderState() {
    return {
      showHidden: false,
      folds: new FoldChoices(),
      opened: new Map<string, boolean>(),
      full: new Map<string, FullText>(),
      md: new MarkdownCache(),
      built: null,
      memo: null,
    };
  }

  private async loadTail(chat: Chat) {
    try {
      const page = await this.api.rows(chat.sessionId, chat.agent, TAIL);
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
    if (this.openId !== sessionId) this.leavePanes(sessionId);
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

  /** The chat a reader call is for: `id`, else the open chair's. */
  private chat(id?: string): Chat | undefined {
    return id === undefined ? this.open : this.chats.get(id);
  }

  /** The transcript calls of one child's chat. */
  reader(id: string): Reader {
    let r = this.readers.get(id);
    if (!r) {
      r = {
        toggleLine: (line) => this.toggleLine(line, id),
        toggleHidden: () => this.toggleHidden(id),
        loadFull: (previewId) => this.loadFull(previewId, id),
        fullText: (previewId) => this.fullText(previewId, id),
        loadOlder: () => this.loadOlder(id),
        setFindQuery: (query) => this.setFindQuery(query),
        stepFind: (dir) => this.stepFind(dir),
        closeFind: () => this.closeFind(),
      };
      this.readers.set(id, r);
    }
    return r;
  }

  /** A reader change: lines rebuild, and the list does not treat it as a prepend. */
  private changed(chat: Chat) {
    chat.version++;
    chat.shift = false;
    if (chat.sessionId === this.openId) this.notify();
  }

  /** Opens or closes a fold, a tool card, a thought, or a long system row. */
  toggleLine(line: Line, id?: string) {
    const chat = this.chat(id);
    if (!chat) return;
    if (line.row.kind === "fold") chat.folds.set(line.row, !line.open);
    else chat.opened.set(line.id, !line.open);
    this.changed(chat);
  }

  toggleHidden(id?: string) {
    const chat = this.chat(id);
    if (!chat) return;
    chat.showHidden = !chat.showHidden;
    this.changed(chat);
  }

  /** Fetches the full text behind a capped preview once and shows it; the LRU drops it. */
  async loadFull(previewId: string, id?: string) {
    const chat = this.chat(id);
    if (!chat || (chat.full.has(previewId) && chat.full.get(previewId)!.state !== "error")) return;
    chat.full = new Map(chat.full).set(previewId, { state: "loading" });
    this.changed(chat);
    let next: FullText;
    try {
      const text = await this.api.toolOutput(chat.sessionId, chat.agent, previewId);
      next = { state: "ready", text, lines: text.split("\n") };
    } catch (err) {
      next = { state: "error", error: String(err) };
    }
    chat.full = new Map(chat.full).set(previewId, next);
    this.changed(chat);
  }

  /** The full text for Copy, without showing it. */
  async fullText(previewId: string, id?: string): Promise<string> {
    const chat = this.chat(id);
    const have = chat?.full.get(previewId);
    if (have?.state === "ready") return have.text;
    if (!chat) throw new Error("no chat open");
    return this.api.toolOutput(chat.sessionId, chat.agent, previewId);
  }

  /** Pages older rows in above the current ones. */
  async loadOlder(id?: string) {
    const chat = this.chat(id);
    if (!chat || chat.state !== "ready" || chat.loadingOlder || chat.older <= 0) return;
    chat.loadingOlder = true;
    try {
      const page = await this.api.rows(chat.sessionId, chat.agent, TAIL, chat.older);
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
    const sig = `${chat?.id}:${chat?.version}:${chat?.showHidden}:${f.open}:${f.query}`;
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
    this.findVersion = `${chat.id}:${chat.version}:${chat.showHidden}:${f.open}:${f.query}`;
    this.find = { ...f, seq: f.seq + 1 };
  }

  /** ⌘F finds in the focused pane, else in the chat. */
  openFind() {
    if (!this.open) return;
    this.findId = this.pane.focused ? paneId(this.openId!, this.pane.focused) : null;
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
    const chat = this.chats.get(this.findId ?? this.openId ?? "");
    const f = this.find;
    if (!chat) return;
    if (!f.open) return this.openFind();
    if (!f.matches.length) return;
    this.find = { ...f, index: (f.index - dir + f.matches.length) % f.matches.length };
    this.reveal(chat, this.find);
    this.notify();
  }

  // ---- composer ----

  /** The session and agent behind a chat id. An id with no chat is a chair's session id. */
  private target(id: string): { session: string; agent: string } {
    const chat = this.chats.get(id);
    return chat ? { session: chat.sessionId, agent: chat.agent } : { session: id, agent: "chair" };
  }

  /** The chat's agent as the composer sees it, from the last snapshot: the chair, or a child. */
  chair(id: string): AgentInput {
    const { session, agent } = this.target(id);
    return agentInput(
      this.snapshot?.sessions.find((s) => s.id === session),
      this.snapshot?.agents[session],
      agent === "chair" ? null : agent,
    );
  }

  private input(id: string): Input {
    let input = this.inputs.get(id);
    if (!input) {
      input = { sending: false, stopping: false, pulling: false, answering: null, alreadySent: false, banner: null, draftRev: 0 };
      this.inputs.set(id, input);
    }
    return input;
  }

  private update(id: string, change: Partial<Input>) {
    Object.assign(this.input(id), change);
    this.inputRev++;
    if (this.target(id).session === this.openId) this.notify();
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
      () => this.api.action("/api/chat/send", { ...this.target(sessionId), text: typed }),
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
      () => this.api.action("/api/chat/interrupt", this.target(sessionId)),
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
      () => this.api.action("/api/chat/answer", { ...this.target(sessionId), prompt_id: prompt.id, choice }),
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
    const key = (k: string) => this.api.action("/api/chat/key", { ...this.target(sessionId), key: k });
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

  // ---- agent panes ----

  /** Leaving a chat frees its child columns, so they stop reading (AgentPaneStore.stop). */
  private leavePanes(next: string) {
    for (const chat of [...this.chats.values()])
      if (chat.agent !== "chair" && chat.sessionId !== next) {
        this.chats.delete(chat.id);
        this.inputs.delete(chat.id);
        this.readers.delete(chat.id);
        this.pending.delete(chat.key);
      }
    this.visible.clear();
    this.findId = null;
    this.pane = { focused: null, zoomed: null, reveal: null, revealSeq: this.pane.revealSeq };
  }

  /** Loads a child's chat once its hooks have reported a log. */
  private loadPane(id: string) {
    const chat = this.chats.get(id);
    if (!chat || chat.loaded || !this.cells().find((c) => c.id === chat.agent)?.hasLog) return;
    chat.loaded = true;
    void this.loadTail(chat).then(() => {
      this.syncSubscriptions();
      if (this.openId === chat.sessionId) this.notify();
    });
  }

  /** A column scrolled into or out of view. Only a column on screen follows its log. */
  setPaneVisible(agent: string, visible: boolean) {
    if (!this.openId) return;
    const id = paneId(this.openId, agent);
    if (visible === this.visible.has(id)) return;
    if (visible) {
      this.visible.add(id);
      this.loadPane(id);
    } else this.visible.delete(id);
    this.syncSubscriptions();
  }

  /** A key or "Show column" moved focus to this pane: the strip scrolls to it and its composer takes the keyboard. */
  focusPane(agent: string) {
    this.pane = { ...this.pane, focused: agent, reveal: agent, revealSeq: this.pane.revealSeq + 1 };
    this.notify();
  }

  /** A click or a focus inside a pane, or on the chat page (null). The strip does not scroll. */
  paneFocused(agent: string | null) {
    if (this.pane.focused === agent) return;
    this.pane = { ...this.pane, focused: agent };
    this.notify();
  }

  /** The strip shows the chat page again, unzoomed, as when focus moves to it. */
  revealChat() {
    const zoomed = this.pane.zoomed !== null;
    this.pane = { focused: null, zoomed: null, reveal: null, revealSeq: this.pane.revealSeq + 1 };
    if (zoomed) this.syncSubscriptions();
    this.notify();
  }

  /** ⌥⌘ arrows. Returns false when focus lands on the chat page. */
  movePaneFocus(direction: Direction): boolean {
    const cells = this.cells();
    const from = cells.findIndex((c) => c.id === this.pane.focused);
    const to = movePane(from < 0 ? null : from, cells.length, direction);
    if (to === null) {
      this.revealChat();
      return false;
    }
    this.focusPane(cells[to].id);
    return true;
  }

  /** ⇧⌘↩ and the header button: zooms `agent` (by default the focused pane), or returns the zoomed pane to the strip. */
  toggleZoom(agent?: string) {
    const id = agent ?? this.pane.zoomed ?? this.pane.focused;
    if (!id || !this.openId) return;
    this.pane = { ...this.pane, zoomed: this.pane.zoomed === id ? null : id, focused: id };
    this.loadPane(paneId(this.openId, id));
    this.syncSubscriptions();
    this.notify();
  }

  /** Removes finished children from the strip: one pane's ×, or "Clear finished" (ADR 0056). */
  dismissPanes(agents: string[]) {
    const id = this.openId;
    if (!id) return;
    const finished = this.cells().filter((c) => c.status === "ended" && agents.includes(c.id)).map((c) => c.id);
    if (!finished.length) return;
    this.dismissed = { ...this.dismissed, [id]: [...(this.dismissed[id] ?? []), ...finished] };
    writeStore(DISMISSED_STORAGE, JSON.stringify(this.dismissed));
    for (const agent of finished) this.visible.delete(paneId(id, agent));
    if (finished.includes(this.pane.focused!) || finished.includes(this.pane.zoomed!)) this.revealChat();
    else this.notify();
    this.syncSubscriptions();
  }

  /** The header's Stop: interrupts a child's turn. A failure throws the server's text. */
  stopPane(agent: string): Promise<void> {
    return this.api.action("/api/chat/interrupt", { session: this.openId, agent });
  }

  /** The header's Close agent: ends the child's pane. A failure throws the server's text. */
  closePane(agent: string): Promise<void> {
    return this.api.action("/api/agent/close", { session: this.openId, agent });
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
      const chat = this.byKey(key);
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

  /** The chat of a stream key. A child's key is its chat id; the chair's ends in `:chair`. */
  private byKey(key: string): Chat | undefined {
    return this.chats.get(key) ?? (key.endsWith(":chair") ? this.chats.get(key.slice(0, -6)) : undefined);
  }

  private async reset(key: string) {
    this.pending.delete(key);
    const chat = this.byKey(key);
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
    const chairs = this.liveAll ? [...this.chats.values()].filter((c) => c.agent === "chair").map((c) => c.id) : this.lru;
    // A zoomed pane covers the strip, so only it follows its log.
    const panes = this.pane.zoomed && this.openId ? [paneId(this.openId, this.pane.zoomed)] : this.visible;
    const keys: SubscribeKey[] = [];
    for (const id of [...chairs, ...panes]) {
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
        run: () => {
          this.revealChat();
          document.querySelector<HTMLElement>('[data-testid="composer"]')?.focus();
        },
      },
      { id: "pane.zoom", title: "Zoom Pane", chord: "Shift+Meta+Enter", when: () => this.openId !== null, run: () => this.toggleZoom() },
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
    for (const d of ["left", "right", "up", "down"] as const)
      cmds.push({
        id: `pane.focus.${d}`,
        title: `Focus ${d[0].toUpperCase()}${d.slice(1)}`,
        chord: `Alt+Meta+Arrow${d[0].toUpperCase()}${d.slice(1)}`,
        when: () => this.openId !== null,
        run: () => this.movePaneFocus(d),
      });
    for (let n = 1; n <= 9; n++)
      cmds.push({ id: `chat.${n}`, title: n === 9 ? "Last chat" : `Chat ${n}`, chord: `Meta+${n}`, palette: false, run: (e) => this.pickChat(n, e) });
    return cmds;
  }
}
