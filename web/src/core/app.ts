// Framework-free app state. Each view layer reads `view`, which is a new object after every change.

import { Api, type Snapshot, type SubscribeKey, type TranscriptEvent } from "./api";
import { installKeys, Registry, search, type Command } from "./commands";
import { MarkdownCache } from "./markdown";
import { measure, whenPainted, type PerfKind } from "./perf";
import { readSse, type SseEvent } from "./sse";
import { Transcript, items, type BlockItem, type Item } from "./transcript";
import { buildTree, flatten, workspaces, type TreeNode, type TreeRow } from "./tree";

export const TAIL = 200;
export const LRU_SIZE = 4;
const MAX_KEYS = 32;

type Chat = {
  sessionId: string;
  key: string;
  model: Transcript;
  state: "loading" | "ready" | "error";
  error: string | null;
  version: number;
  shift: boolean;
  loadingOlder: boolean;
  // Render caches; the LRU drops them.
  expanded: Set<string>;
  blockCache: Map<string, { src: string; items: BlockItem[] }>;
  md: MarkdownCache;
  items: { version: number; list: Item[] } | null;
};

export type View = {
  loaded: boolean;
  roots: TreeNode[];
  error: string | null;
  rows: TreeRow[];
  selected: string | null;
  openId: string | null;
  chatState: "none" | "loading" | "ready" | "error";
  chatError: string | null;
  items: Item[];
  shift: boolean;
  md: MarkdownCache | null;
  expanded: ReadonlySet<string>;
  paletteOpen: boolean;
};

export type PaletteEntry = { id: string; title: string; hint?: string; run: () => void };

const EMPTY: Item[] = [];
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
  private pending = new Map<string, { events: TranscriptEvent[]; end: number }>();
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
    let list = EMPTY;
    if (chat) {
      if (chat.items?.version !== chat.version)
        chat.items = { version: chat.version, list: items(chat.model.rows, chat.expanded, chat.blockCache) };
      list = chat.items.list;
    }
    return {
      loaded: this.snapshot !== null,
      roots: this.roots,
      error: this.error,
      rows: this.rows,
      selected: this.selected,
      openId: this.openId,
      chatState: chat ? chat.state : "none",
      chatError: chat?.error ?? null,
      items: list,
      shift: chat?.shift ?? false,
      md: chat?.md ?? null,
      expanded: chat?.expanded ?? new Set(),
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
      model: new Transcript(),
      state: "loading",
      error: null,
      version: 0,
      shift: false,
      loadingOlder: false,
      expanded: new Set(),
      blockCache: new Map(),
      md: new MarkdownCache(),
      items: null,
    };
    this.chats.set(sessionId, chat);
    return chat;
  }

  private async loadTail(chat: Chat) {
    try {
      const page = await this.api.transcript(chat.sessionId, "chair", TAIL);
      chat.model = new Transcript();
      chat.model.apply(page.events);
      chat.model.page = page.page;
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
      if (this.liveAll) {
        chat.md = new MarkdownCache();
        chat.blockCache = new Map();
        chat.expanded = new Set();
        chat.items = null;
      } else this.chats.delete(old);
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
    const last = this.view.items.length - 1;
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

  toggleOutput(rowId: string) {
    const chat = this.openId ? this.chats.get(this.openId) : undefined;
    if (!chat) return;
    chat.expanded = new Set(chat.expanded);
    if (!chat.expanded.delete(rowId)) chat.expanded.add(rowId);
    chat.version++;
    chat.shift = false;
    this.notify();
  }

  /** Pages older rows in above the current ones. */
  async loadOlder() {
    const chat = this.openId ? this.chats.get(this.openId) : undefined;
    if (!chat || chat.state !== "ready" || chat.loadingOlder || chat.model.page.start_offset <= 0) return;
    chat.loadingOlder = true;
    try {
      const page = await this.api.transcript(chat.sessionId, "chair", TAIL, chat.model.page.start_offset);
      chat.model.prepend(page.events, page.page);
      chat.version++;
      chat.shift = true;
      if (this.openId === chat.sessionId) this.notify();
    } catch (err) {
      chat.error = String(err);
    } finally {
      chat.loadingOlder = false;
    }
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
      case "transcript": {
        const key = String(data.key);
        const slot = this.pending.get(key) ?? { events: [], end: 0 };
        for (const ev of (data.events as TranscriptEvent[]) ?? []) slot.events.push(ev);
        slot.end = Number(data.end_offset);
        this.pending.set(key, slot);
        this.frame ||= requestAnimationFrame(() => this.flush());
        break;
      }
      case "reset":
        void this.reset(String(data.key));
        break;
    }
  }

  /** Applies all buffered stream events once per animation frame. Only the open chat renders. */
  private flush() {
    this.frame = 0;
    let render = false;
    for (const [key, slot] of this.pending) {
      const chat = this.chats.get(key.slice(0, key.lastIndexOf(":")));
      if (!chat || chat.state !== "ready") continue;
      if (chat.model.apply(slot.events)) {
        chat.version++;
        chat.shift = false;
        if (chat.sessionId === this.openId) render = true;
      }
      chat.model.page = { ...chat.model.page, end_offset: slot.end };
    }
    this.pending.clear();
    if (render) this.notify();
  }

  private async reset(key: string) {
    this.pending.delete(key);
    const chat = this.chats.get(key.slice(0, key.lastIndexOf(":")));
    if (!chat) return;
    chat.expanded = new Set();
    chat.blockCache = new Map();
    chat.md = new MarkdownCache();
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
      if (chat?.state === "ready") keys.push({ key: chat.key, from: chat.model.page.end_offset });
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
