// Framework-free app state. Each view layer reads `view`, which is a new object after every change.

import { Api, ApiError, type Row, type RowFrame, type Snapshot, type SubscribeKey } from "./api";
import { installKeys, Registry, type Command } from "./commands";
import {
  agentInput,
  confirmSent,
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
import { isChair, movePane, paneCells, paneStatus, type Direction, type PaneCell } from "./panes";
import { FoldChoices, findMatches, lines, mergeRows, type Line, type Match } from "./transcript";
import {
  chatRows,
  foldId,
  moveTarget,
  navRows,
  paletteSources,
  recency,
  recentWork,
  shareTree,
  workspaceOf,
  workspaceRow,
  type ChoiceOp,
  type NavRow,
  type PaletteItem,
  type RecentChat,
  type Tab,
  type Tabs,
  type TreeName,
  type TreeView,
} from "./nav";

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
  tree: TreeView | null;
  treeName: TreeName;
  error: string | null;
  /** The sidebar's lines; `selected` is the keyboard row among them. */
  rows: NavRow[];
  selected: string | null;
  openId: string | null;
  /** The workspace whose tabs show; with no chat open, its landing shows. Null is Home. */
  workspace: string | null;
  /** The tab strip of `workspace`, once loaded. */
  tabs: Tabs | null;
  /** 1 Workspaces, 2 Recent (⌥⌘1-2). */
  sidebarView: number;
  /** Every listed chat across projects, newest activity first; `recentSelected` is its keyboard row. */
  recent: RecentChat[];
  recentSelected: string | null;
  paletteOpen: boolean;
  asking: Ask | null;
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

/** A name the owner types: a rename. */
export type Ask = { title: string; value: string; save: (name: string) => void };

/** The sidebar views in ⌥⌘ order. */
export const SIDEBAR_VIEWS = ["Workspaces", "Recent"];

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
  private tree: TreeView | null = null;
  private treeName: TreeName = "workspaces";
  private rows: NavRow[] = [];
  /** Fold presses the server has not confirmed yet, by fold id. */
  private folds = new Map<string, boolean>();
  private selected: string | null = null;
  private openId: string | null = null;
  private workspace: string | null = null;
  private tabs: Tabs | null = null;
  private sidebarView = 1;
  private recentMemo: { tree: TreeView | null; snapshot: Snapshot | null; list: RecentChat[] } | null = null;
  private recentSelected: string | null = null;
  /** The choices revision of the last read or write. Writes go one at a time, each on the last. */
  private revision: string | null = null;
  private writes: Promise<unknown> = Promise.resolve();
  private writing = 0;
  /** Bumped by each write, so a read that started before it does not take back its revision. */
  private writeSeq = 0;
  private restored = false;
  /** Bumped by each move between chats, so a slow workspace step does not undo a newer one. */
  private navSeq = 0;
  private starting = false;
  private paletteOpen = false;
  private asking: Ask | null = null;
  private error: string | null = null;
  private chats = new Map<string, Chat>();
  private lru: string[] = [];
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
  /** Shows a short message to the owner; the view layer sets it (a toast). */
  notice: (text: string, error?: boolean) => void = () => {};

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
      loaded: this.tree !== null,
      tree: this.tree,
      treeName: this.treeName,
      error: this.error,
      rows: this.rows,
      selected: this.selected,
      openId: this.openId,
      workspace: this.workspace,
      tabs: this.tabs?.workspace === this.workspace ? this.tabs : null,
      sidebarView: this.sidebarView,
      recent: this.recentList(),
      recentSelected: this.recentSelected,
      paletteOpen: this.paletteOpen,
      asking: this.asking,
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

  // ---- snapshot, tree and tabs ----

  private refreshSnapshot(): Promise<void> {
    if (this.snapshotBusy) {
      this.snapshotAgain = true;
      return this.snapshotBusy;
    }
    this.snapshotBusy = (async () => {
      do {
        this.snapshotAgain = false;
        try {
          const [snap] = await Promise.all([this.api.snapshot(), this.loadNav()]);
          this.applySnapshot(snap);
          this.afterNav();
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

  /** Reads the tree and the current workspace's tab strip. */
  private async loadNav() {
    const seq = this.writeSeq;
    const name = this.treeName;
    const workspace = this.workspace;
    const [tree, tabs] = await Promise.all([
      this.api.tree(name),
      // An unknown workspace is a 404; the strip stays empty.
      workspace ? this.api.tabs(workspace).catch(() => null) : null,
    ]);
    if (name !== this.treeName) return;
    // A write that landed while these were in flight holds the newer revision and folds.
    if (this.writing === 0 && seq === this.writeSeq) {
      this.revision = tree.revision;
      this.folds.clear();
    }
    this.tree = shareTree(this.tree?.view === name ? this.tree : null, tree);
    if (tabs && workspace === this.workspace) this.tabs = tabs.view;
    this.rows = navRows(this.tree, this.folds);
  }

  /** After each tree: the first one restores the owner's last chat. A chat is marked seen only when
   *  it is selected (`selected_chat`), as in Swift's WorkspaceNavigation.select. */
  private afterNav() {
    const tree = this.tree;
    if (!tree) return;
    if (!this.selected || !this.rows.some((r) => r.id === this.selected)) this.selected = this.rows[0]?.id ?? null;
    if (!this.restored) {
      this.restored = true;
      const row = tree.sections.flatMap((s) => s.rows).find((r) => r.id === tree.selectedRow);
      if (row && !this.openId && !this.workspace) {
        if (row.selection.chat) this.openChat(row.selection.chat, undefined, { write: false });
        else if (row.kind === "workspace") this.workspace = row.id;
        void this.refreshSnapshot();
      }
    }
  }

  private applySnapshot(snap: Snapshot) {
    this.snapshot = snap;
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
    if (this.liveAll) void this.loadAllForLive();
  }

  /** Writes one owner choice after the writes before it, on the newest revision. A refused write
   *  (a stale revision from another writer, or a bad target) shows its error and reloads the tree;
   *  nothing retries, so a choice never lands on a state the owner did not see. */
  choose(op: ChoiceOp): Promise<boolean> {
    this.writing++;
    const run = this.writes.then(async () => {
      try {
        if (this.revision === null) throw new Error("The sidebar has not loaded yet.");
        const done = await this.api.choice(this.revision, op);
        this.revision = done.revision;
        this.writeSeq++;
        return true;
      } catch (err) {
        this.folds.clear();
        this.rows = navRows(this.tree, this.folds);
        this.notice(
          err instanceof ApiError && err.status === 409 ? "Your choices changed elsewhere. The list was reloaded; try again." : message(err),
          true,
        );
        void this.refreshSnapshot();
        return false;
      } finally {
        this.writing--;
      }
    });
    this.writes = run;
    return run;
  }

  // ---- sidebar ----

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
    if (this.sidebarView === 2) return this.moveRecent(to, e);
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

  /** Folds or opens a header, workspace or chat. It shows at once; the server confirms it. */
  toggle(id: string) {
    const row = this.rows.find((r) => r.id === id);
    if (!row || row.expanded === null) return;
    const open = !row.expanded;
    this.folds.set(foldId(row), open);
    this.rows = navRows(this.tree, this.folds);
    this.notify();
    void this.choose({ op: open ? "expand" : "collapse", id: foldId(row) });
  }

  /** Enter: open a chat or a workspace, fold a header, or list a workspace's other chats. */
  activate(e?: KeyboardEvent) {
    if (this.sidebarView === 2) {
      if (this.recentSelected) this.openChat(this.recentSelected, e);
      return;
    }
    const row = this.rows.find((r) => r.id === this.selected);
    if (row) this.runRow(row, e);
  }

  clickRow(id: string) {
    const row = this.rows.find((r) => r.id === id);
    if (!row) return;
    this.selected = id;
    this.runRow(row);
  }

  private runRow(nav: NavRow, e?: KeyboardEvent) {
    const row = nav.row;
    if (!row) return this.toggle(nav.id);
    const sel = row.selection;
    // The archive view only shows; Restore comes in S3, and an archived workspace takes no selection.
    if (this.treeName === "archived") {
      if (row.kind === "chat" && sel.chat) this.openChat(sel.chat, e, { write: false });
      return;
    }
    if (row.kind === "more") {
      const workspace = sel.workspace ?? row.parentId;
      if (workspace) void this.choose({ op: "expand_list", workspace });
    } else if (row.kind === "workspace") void this.selectWorkspace(row.id, e);
    else if (row.kind === "child") {
      const session = sel.agentSession ?? sel.chat;
      if (session) this.openChat(session, e);
      if (sel.agent) this.focusPane(sel.agent);
    } else if (sel.chat) this.openChat(sel.chat, e);
  }

  /** Every chat the tree lists, across projects, newest activity first. Home shows the first ones. */
  private recentList(): RecentChat[] {
    const m = this.recentMemo;
    if (m && m.tree === this.tree && m.snapshot === this.snapshot) return m.list;
    const list = recentWork(this.tree, this.snapshot, Infinity);
    this.recentMemo = { tree: this.tree, snapshot: this.snapshot, list };
    return list;
  }

  private moveRecent(to: number | "home" | "end", e?: KeyboardEvent) {
    const list = this.recentList();
    if (!list.length) return;
    const i = list.findIndex((r) => r.session === this.recentSelected);
    const next = to === "home" ? 0 : to === "end" ? list.length - 1 : Math.max(0, Math.min(list.length - 1, i + to));
    const id = list[next].session;
    if (id === this.recentSelected) return;
    this.recentSelected = id;
    this.notify();
    const row = () => document.querySelector<HTMLElement>(`[data-testid="recent-row"][data-session="${CSS.escape(id)}"]`);
    whenPainted(() => row() !== null, () => row()?.focus());
    if (e) measure("key", id, e.timeStamp, () => this.recentSelected !== id || row()?.getAttribute("aria-selected") === "true");
  }

  /** A click on a Recent row: it becomes the keyboard row, and its chat opens. */
  clickRecent(session: string) {
    this.recentSelected = session;
    this.openChat(session);
  }

  /** Pin from the menu or a drop on Pinned; a folded Pinned header opens to show it. */
  async pin(workspace: string) {
    if (!(await this.choose({ op: "pin", workspace }))) return;
    const pinned = this.tree?.sections.find((s) => s.kind === "pinned");
    if (pinned && !pinned.expanded) void this.choose({ op: "expand", id: pinned.collapseId });
  }

  /** Move Up / Move Down's target, or null when the row cannot move that way. */
  workspaceMoveTarget(path: string, offset: -1 | 1): string | null {
    return moveTarget(this.tree, this.snapshot, path, offset);
  }

  /** The Workspaces / Archived switch at the foot of the sidebar. */
  setTreeName(name: TreeName) {
    if (name === this.treeName) return;
    this.treeName = name;
    this.tree = null;
    this.rows = [];
    this.notify();
    void this.refreshSnapshot();
  }

  /** ⌥⌘1 Workspaces, ⌥⌘2 Recent. The list takes the keyboard, on the open chat's row. */
  setSidebarView(n: number) {
    if (n < 1 || n > SIDEBAR_VIEWS.length) return;
    if (n !== this.sidebarView) {
      this.sidebarView = n;
      if (n === 2) this.recentSelected = this.openId ?? this.recentList()[0]?.session ?? null;
      this.notify();
    }
    const list = n === 2 ? '[data-testid="recent"]' : '[data-testid="sidebar"]';
    whenPainted(() => document.querySelector(list) !== null, () => document.querySelector<HTMLElement>(`${list} [tabindex="0"]`)?.focus() ?? document.querySelector<HTMLElement>(list)?.focus());
  }

  /** Home: no workspace and no chat. It is not saved; a reload opens the last chat. */
  showHome() {
    this.navSeq++;
    this.leavePanes("");
    this.openId = null;
    this.workspace = null;
    this.notify();
  }

  /** A workspace row, ⌃⌘↓ or ⌃⌘↑: the workspace's remembered chat, else its landing
   *  (SessionsWindowModel.selectWorkspace). */
  async selectWorkspace(path: string, e?: KeyboardEvent) {
    const seq = ++this.navSeq;
    if (this.tree && !(await this.choose({ op: "selected_chat", workspace: path, chat: null }))) return;
    let tabs: Tabs | null = null;
    try {
      tabs = (await this.api.tabs(path)).view;
    } catch {
      // No strip: the landing shows.
    }
    if (seq !== this.navSeq) return;
    if (tabs) this.tabs = tabs;
    if (tabs?.selectedChat) return this.openChat(tabs.selectedChat, e);
    this.leavePanes("");
    this.openId = null;
    this.workspace = path;
    if (this.rows.some((r) => r.id === path)) this.selected = path;
    this.notify();
  }

  /** ⌘T, a "+" and New chat: starts the default profile in the workspace. The pending tab and the
   *  profile menu come in P3c; until then a notice shows while `swarm launch` runs. */
  async startChat(workspace = this.workspace) {
    const row = workspaceRow(this.tree, workspace);
    if (!workspace || this.starting || (row && !row.newChatEnabled)) return;
    this.starting = true;
    this.notice("Starting a chat…");
    try {
      const started = await this.api.post<{ session: string }>("/api/chat/new", { cwd: workspace });
      await this.refreshSnapshot();
      this.openChat(started.session);
    } catch (err) {
      this.notice(message(err), true);
    } finally {
      this.starting = false;
    }
  }

  // ---- tabs ----

  private stripTabs(): Tab[] {
    return (this.tabs?.workspace === this.workspace ? this.tabs?.tabs : null)?.filter((t) => t.pending === null) ?? [];
  }

  /** ⌘1-8 pick a tab by strip order; ⌘9 picks the last. */
  pickChat(n: number, e?: KeyboardEvent) {
    const tabs = this.stripTabs();
    const tab = n === 9 ? tabs[tabs.length - 1] : tabs[n - 1];
    if (tab && tab.session !== this.openId) this.openChat(tab.session, e);
  }

  /** ⇧⌘] / ⇧⌘[ step through the tabs, wrapping (PaneSearch.step). */
  stepChat(delta: number, e?: KeyboardEvent) {
    const tabs = this.stripTabs();
    if (!tabs.length) return;
    const i = tabs.findIndex((t) => t.session === this.openId);
    const next = i < 0 ? (delta < 0 ? tabs.length - 1 : 0) : (i + (delta % tabs.length) + tabs.length) % tabs.length;
    if (tabs[next].session !== this.openId) this.openChat(tabs[next].session, e);
  }

  /** ⌃Tab / ⌃⇧Tab go back and forth in the workspace's tab history. Cycling keeps the history
   *  order, or repeated presses would only toggle two chats. */
  stepRecent(delta: number, e?: KeyboardEvent) {
    const strip = this.tabs?.workspace === this.workspace ? this.tabs : null;
    const key = delta < 0 ? strip?.previousRecent : strip?.nextRecent;
    const tab = strip?.tabs.find((t) => t.id === key && t.pending === null);
    if (tab && tab.session !== this.openId) this.openChat(tab.session, e, { history: false });
  }

  /** ⌘W and a tab's ×. Closing the open tab shows the chat the server picks, else the landing. */
  async closeTab(key?: string) {
    const workspace = this.workspace;
    const tab = this.stripTabs().find((t) => (key ? t.id === key : t.session === this.openId));
    if (!workspace || !tab?.canHide) return;
    const wasOpen = tab.session === this.openId;
    const seq = ++this.navSeq;
    if (!(await this.choose({ op: "close_tab", workspace, chat: tab.id })) || !wasOpen) return;
    let next: Tabs | null = null;
    try {
      next = (await this.api.tabs(workspace)).view;
    } catch {
      // The landing shows.
    }
    if (seq !== this.navSeq) return;
    if (next) this.tabs = next;
    if (next?.selectedChat) return this.openChat(next.selectedChat);
    this.leavePanes("");
    this.openId = null;
    this.notify();
  }

  // ---- chats ----

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

  /** Opens a chat: its tab, the owner's selected chat, and its transcript. `write: false` only
   *  shows it (a restore at start); `history: false` keeps the tab history order (⌃Tab). */
  openChat(sessionId: string, e?: KeyboardEvent, opts: { write?: boolean; history?: boolean } = {}) {
    this.navSeq++;
    const kind: PerfKind = this.lru.includes(sessionId) ? "switch" : "open";
    if (this.openId !== sessionId) this.leavePanes(sessionId);
    this.openId = sessionId;
    const workspace = workspaceOf(this.tree, this.snapshot, sessionId);
    const moved = workspace !== null && workspace !== this.workspace;
    if (workspace) this.workspace = workspace;
    const row = chatRows(this.tree).find((r) => r.selection.chat === sessionId);
    // A restore leaves the keyboard row where it was.
    if (row && opts.write !== false) this.selected = row.id;
    if (workspace && this.revision !== null && opts.write !== false) {
      // Swift's select: open the tab, then select; only a move to another workspace opens its project.
      const section = this.tree?.sections.find((s) => s.rows.some((r) => r.selection.chat === sessionId));
      if (moved && section && !section.expanded) void this.choose({ op: "expand", id: section.collapseId });
      void this.choose({ op: "open_tab", workspace, chat: sessionId });
      void this.choose({ op: "selected_chat", workspace, chat: sessionId, recording_history: opts.history !== false });
    } else if (moved) void this.refreshSnapshot();
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

  /** ⌃⌘↓ / ⌃⌘↑ go to the next or previous workspace row in the sidebar, wrapping. */
  stepWorkspace(delta: number, e?: KeyboardEvent) {
    const list = this.rows.filter((r) => r.kind === "workspace").map((r) => r.id);
    if (!list.length) return;
    const i = this.workspace ? list.indexOf(this.workspace) : -1;
    const next = i < 0 ? (delta < 0 ? list.length - 1 : 0) : (i + delta + list.length) % list.length;
    if (list[next] !== this.workspace) void this.selectWorkspace(list[next], e);
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

  /** ↑ on an empty draft: Claude's queued messages back into the draft. The server presses the
   *  CLI's own keys and confirms them from the log (ADR 0038, `/api/chat/pull-back`). */
  async pullBack(sessionId: string) {
    const chat = this.chats.get(sessionId);
    const st = this.input(sessionId);
    const chair = this.chair(sessionId);
    if (!chat || !chair.pullBack || chair.disabled || st.pulling || st.sending) return;
    if (!chat.rows.some((r) => r.kind === "queued")) return;
    const result: { pulled: string | null } = { pulled: null };
    await this.act(
      sessionId,
      { pulling: true, alreadySent: false },
      { pulling: false },
      async () => {
        const done = await this.api.post<{ outcome: "pulled" | "alreadySent" | "unconfirmed"; text: string | null }>("/api/chat/pull-back", this.target(sessionId));
        if (done.outcome === "unconfirmed") throw new Error("Could not confirm the pull-back. Check the agent's input box.");
        if (done.outcome === "pulled") result.pulled = done.text ?? "";
      },
      () => void this.pullBack(sessionId),
    );
    if (result.pulled === null) {
      if (this.input(sessionId).banner) return;
      // Only the caption waits, so Send works again at once. A newer caption keeps its own timer.
      const seq = ++this.alreadySentSeq;
      this.update(sessionId, { alreadySent: true });
      setTimeout(() => seq === this.alreadySentSeq && this.update(sessionId, { alreadySent: false }), 2000);
      return;
    }
    // Keep anything typed while the CLI let go of the text.
    const typed = this.draft(sessionId);
    const text = result.pulled;
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

  // ---- palette and names ----

  setPalette(open: boolean) {
    if (this.paletteOpen === open) return;
    this.paletteOpen = open;
    this.notify();
  }

  /** Asks the owner for a name; the view shows one dialog. */
  ask(ask: Ask | null) {
    this.asking = ask;
    this.notify();
  }

  renameChat(session: string) {
    const workspace = workspaceOf(this.tree, this.snapshot, session);
    const row = chatRows(this.tree).find((r) => r.selection.chat === session);
    if (!workspace) return;
    this.ask({ title: "Rename chat", value: row?.title ?? "", save: (name) => void this.choose({ op: "rename_chat", workspace, chat: session, name }) });
  }

  renameWorkspace(path: string) {
    const row = workspaceRow(this.tree, path);
    this.ask({ title: "Rename workspace", value: row?.title ?? "", save: (name) => void this.choose({ op: "rename_workspace", workspace: path, name }) });
  }

  renameProject(path: string, title: string) {
    this.ask({ title: "Rename project", value: title, save: (name) => void this.choose({ op: "rename_project", project: path, name }) });
  }

  /** The palette's items in Swift order: actions, chat actions, workspaces, chats, agents
   *  (PaletteItems.build). Pure in the view, so the palette can memoize it on the view. */
  paletteItems(): PaletteItem[] {
    const items: PaletteItem[] = this.registry
      .list()
      .filter((c) => c.palette !== false)
      .map((c) => ({ id: `action:${c.id}`, title: c.title, group: "action", shortcut: c.chord, disabledReason: c.phase ? `Comes in ${c.phase}` : null, run: () => c.run() }));
    const open = this.openId;
    const chatAction = (id: string, title: string, run: (() => void) | string): PaletteItem => ({
      id: `chatAction:${id}`,
      title,
      group: "action",
      disabledReason: typeof run === "string" ? `Comes in ${run}` : null,
      run: typeof run === "string" ? () => {} : run,
    });
    if (open)
      items.push(
        chatAction("closeTab", "Close Tab", () => void this.closeTab()),
        chatAction("endChat", "End chat", "S3"),
        chatAction("archiveChat", "Archive chat", "S3"),
        chatAction("renameChat", "Rename chat", () => this.renameChat(open)),
      );
    items.push(chatAction("reopenChat", "Reopen closed chat", "S3"));
    if (open) items.push(chatAction("switchModel", "Switch model", "S3"));
    const sources = paletteSources(this.tree, this.snapshot);
    const chats: PaletteItem[] = [];
    const agents: PaletteItem[] = [];
    for (const w of sources) {
      const title = w.project && !w.row.pinned ? `${w.project} / ${w.row.title}` : w.row.title;
      items.push({ id: `workspace:${w.row.id}`, title, subtitle: w.row.detail || undefined, group: "workspace", status: w.row.status, recency: w.at, run: () => void this.selectWorkspace(w.row.id) });
      for (const c of w.chats) {
        const session = c.selection.chat!;
        chats.push({ id: `chat:${session}`, title: c.title, subtitle: title, group: "chat", status: c.status, recency: recency(this.snapshot, session), run: () => this.openChat(session) });
        const info = this.snapshot?.sessions.find((s) => s.id === session);
        for (const a of this.snapshot?.agents[session] ?? [])
          if (info && a.alive && !isChair(a, info))
            agents.push({
              id: `agent:${session}:${a.id}`,
              title: a.id,
              subtitle: `${a.role} · ${c.title}`,
              group: "agent",
              status: paneStatus(a),
              run: () => {
                this.openChat(session);
                this.focusPane(a.id);
              },
            });
      }
    }
    return [...items, ...chats, ...agents];
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
        // Absent usage leaves the shown value; null clears it.
        if (f.usage !== undefined) {
          chat.usage = f.usage as Usage;
          this.inputRev++;
          if (chat.sessionId === this.openId) render = true;
        }
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

  /** The SwiftUI key map (AppKey.table), in the palette's action order (PaletteItems.actions).
   *  A key whose target comes later is registered and says which phase brings it. */
  private commands(): Command[] {
    const inTree = () => {
      const a = document.activeElement;
      return !a || a === document.body || a.closest('[data-testid="sidebar"], [data-testid="recent"]') !== null;
    };
    const tree = (id: string, title: string, chord: string, run: (e?: KeyboardEvent) => void): Command => ({
      id,
      title,
      chord,
      palette: false,
      when: () => !this.paletteOpen && inTree(),
      run,
    });
    const later = (id: string, title: string, chord: string, phase: string, palette = true): Command => ({
      id,
      title,
      chord,
      phase,
      palette,
      run: () => this.notice(`${title} comes in ${phase}.`),
    });
    const chat = () => this.openId !== null;
    const cmds: Command[] = [
      later("workspace.new", "New Workspace", "Meta+N", "S3"),
      {
        id: "chat.new",
        title: "New Chat",
        chord: "Meta+T",
        when: () => this.workspace !== null,
        // A held key must not start one chat per repeat.
        run: (e) => void (e?.repeat || this.startChat()),
      },
      later("project.new", "New Project…", "Shift+Meta+N", "S3"),
      { id: "workspace.next", title: "Next Workspace", chord: "Ctrl+Meta+ArrowDown", run: (e) => this.stepWorkspace(1, e) },
      { id: "workspace.prev", title: "Previous Workspace", chord: "Ctrl+Meta+ArrowUp", run: (e) => this.stepWorkspace(-1, e) },
      { id: "chat.next", title: "Next Chat", chord: "Shift+Meta+]", run: (e) => this.stepChat(1, e) },
      { id: "chat.prev", title: "Previous Chat", chord: "Shift+Meta+[", run: (e) => this.stepChat(-1, e) },
      { id: "pane.zoom", title: "Zoom Pane", chord: "Shift+Meta+Enter", when: chat, run: () => this.toggleZoom() },
      {
        id: "composer.focus",
        title: "Focus Composer",
        chord: "Meta+L",
        run: () => {
          this.revealChat();
          document.querySelector<HTMLElement>('[data-testid="composer"]')?.focus();
        },
      },
    ];
    // ⌘B and ⇧⌘B belong to the window frame, which registers them; they sit here in palette order.
    for (let n = 1; n <= SIDEBAR_VIEWS.length; n++)
      cmds.push({ id: `sidebar.view.${n}`, title: `Show ${SIDEBAR_VIEWS[n - 1]}`, chord: `Alt+Meta+${n}`, run: () => this.setSidebarView(n) });
    cmds.push(
      later("changes.show", "Show Changes", "Alt+Meta+I", "S4"),
      { id: "find.open", title: "Find", chord: "Meta+F", when: chat, run: () => this.openFind() },
      {
        id: "chat.stop",
        title: "Stop the Chair",
        chord: "Meta+.",
        when: () => this.openId !== null && !this.chair(this.openId).unreachable,
        run: () => void this.interrupt(this.openId!),
      },
      { id: "palette", title: "Search", chord: "Meta+K", palette: false, run: () => this.setPalette(!this.paletteOpen) },
      { id: "find.next", title: "Find Next", chord: "Meta+G", palette: false, when: chat, run: () => this.stepFind(1) },
      { id: "find.prev", title: "Find Previous", chord: "Shift+Meta+G", palette: false, when: chat, run: () => this.stepFind(-1) },
      { id: "tab.close", title: "Close Tab", chord: "Meta+W", palette: false, run: () => void this.closeTab() },
      later("window.close", "Close Window", "Shift+Meta+W", "S6", false),
      later("recent.closed", "Recently closed…", "Shift+Meta+T", "S3", false),
      { id: "recent.prev", title: "Previous Recent Chat", chord: "Ctrl+Tab", palette: false, run: (e) => this.stepRecent(-1, e) },
      { id: "recent.next", title: "Next Recent Chat", chord: "Ctrl+Shift+Tab", palette: false, run: (e) => this.stepRecent(1, e) },
      later("settings", "Settings…", "Meta+,", "S5", false),
      tree("tree.down", "Select next row", "j", (e) => this.moveSelection(1, e)),
      tree("tree.down2", "Select next row", "ArrowDown", (e) => this.moveSelection(1, e)),
      tree("tree.up", "Select previous row", "k", (e) => this.moveSelection(-1, e)),
      tree("tree.up2", "Select previous row", "ArrowUp", (e) => this.moveSelection(-1, e)),
      tree("tree.home", "Select first row", "Home", (e) => this.moveSelection("home", e)),
      tree("tree.end", "Select last row", "End", (e) => this.moveSelection("end", e)),
      tree("tree.collapse", "Collapse", "ArrowLeft", (e) => this.collapse(e)),
      tree("tree.expand", "Expand", "ArrowRight", (e) => this.expand(e)),
      tree("tree.open", "Open", "Enter", (e) => this.activate(e)),
    );
    for (const d of ["left", "right", "up", "down"] as const)
      cmds.push({
        id: `pane.focus.${d}`,
        title: `Focus ${d[0].toUpperCase()}${d.slice(1)}`,
        chord: `Alt+Meta+Arrow${d[0].toUpperCase()}${d.slice(1)}`,
        palette: false,
        when: chat,
        run: () => this.movePaneFocus(d),
      });
    for (let n = 1; n <= 9; n++)
      cmds.push({ id: `chat.${n}`, title: n === 9 ? "Last Chat" : `Chat ${n}`, chord: `Meta+${n}`, palette: false, run: (e) => this.pickChat(n, e) });
    return cmds;
  }
}
