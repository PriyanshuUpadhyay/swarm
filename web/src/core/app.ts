// Framework-free app state. Each view layer reads `view`, which is a new object after every change.

import { Api, type Row, type RowFrame, type Snapshot, type SubscribeKey } from "./api";
import { installKeys, Registry, search, type Command } from "./commands";
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
};

export type PaletteEntry = { id: string; title: string; hint?: string; run: () => void };

const EMPTY: Line[] = [];
const NO_FULL: ReadonlyMap<string, FullText> = new Map();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const chatKey = (sessionId: string) => `${sessionId}:chair`;

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
    if (chat && chat.built?.version !== chat.version)
      chat.built = { version: chat.version, ...lines(chat.rows, { showHidden: chat.showHidden, folds: chat.folds, opened: chat.opened }) };
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
    chat.built = { version: chat.version, ...lines(chat.rows, { showHidden: chat.showHidden, folds: chat.folds, opened: chat.opened }) };
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
