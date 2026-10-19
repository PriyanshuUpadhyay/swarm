

import { type SendKey } from "./composer";
import { movePane, paneCells, type Direction, type PaneCell } from "./panes";


import { type Chat, type Pane, type Strip, type View, paneId, NO_FIND, WORKING, NO_PANE, NO_CELLS } from "./app-types";
import type { App } from "./app";

export class AppPanes {
  constructor(private readonly app: App) {}

  makeView(): View {
    const sendKey: SendKey = this.app.tree?.prefs?.sendKey === "commandReturn" ? "commandReturn" : "return";
    const chat = this.app.reading.open;
    const cells = this.cells();
    // A pane that left the strip takes its focus, its zoom and its Find with it.
    const has = (id: string | null) => (id !== null && cells.some((c) => c.id === id) ? id : null);
    const p = this.app.pane;
    p.focused = has(p.focused);
    p.zoomed = has(p.zoomed);
    p.reveal = has(p.reveal);
    if (this.app.findId && !cells.some((c) => paneId(this.app.openId!, c.id) === this.app.findId)) this.app.findId = null;
    return {
      ...(chat ? this.paneView(chat, sendKey, this.chairHasLog(chat.sessionId)) : NO_PANE),
      loaded: this.app.tree !== null,
      tree: this.app.tree,
      treeName: this.app.treeName,
      error: this.app.error,
      rows: this.app.rows,
      selected: this.app.selected,
      marked: this.app.marked,
      openId: this.app.openId,
      workspace: this.app.workspace,
      tabs: this.app.tabs?.workspace === this.app.workspace ? this.app.tabs : null,
      sidebarView: this.app.sidebarView,
      recent: this.app.sidebar.recentList(),
      recentSelected: this.app.recentSelected,
      paletteOpen: this.app.paletteOpen,
      asking: this.app.asking,
      strip: this.stripView(cells, sendKey),
    };
  }

  /** One chat as its transcript and composer see it. The same object comes back while nothing it
   *  shows has changed, so a sidebar key press does not render every column again. */
  paneView(chat: Chat, sendKey: SendKey, hasLog = true): Pane {
    const finds = chat.id === (this.app.findId ?? this.app.openId);
    if (finds) this.app.reading.refind(chat);
    const find = finds ? this.app.find : NO_FIND;
    const sig = [chat.version, chat.state, chat.loaded, chat.loadingOlder, find, this.app.snapshot, this.app.inputRev, sendKey, hasLog];
    if (chat.memo && sig.every((v, i) => v === chat.memo!.sig[i])) return chat.memo.pane;
    if (chat.built?.version !== chat.version) chat.built = { version: chat.version, lines: this.app.reading.build(chat) };
    const pane: Pane = {
      id: chat.id,
      // A child's column loads when it first shows; with no log yet it has nothing to load.
      chatState: chat.loaded ? chat.state : hasLog ? "loading" : "none",
      chatError: chat.error,
      lines: this.app.composing.chair(chat.id).running ? chat.built.lines.concat(WORKING) : chat.built.lines,
      hasOlder: chat.older > 0,
      loadingOlder: chat.loadingOlder,
      shift: chat.shift,
      md: chat.md,
      full: chat.full,
      find,
      composer: this.app.composing.composerView(chat, sendKey),
    };
    chat.memo = { sig, pane };
    return pane;
  }

  /** The open chat's child agents, in strip order. */
  cells(): PaneCell[] {
    const id = this.app.openId;
    if (!id) return NO_CELLS;
    const m = this.app.cellsMemo;
    if (m && m.snapshot === this.app.snapshot && m.openId === id) return m.cells;
    const cells = paneCells(this.app.snapshot?.sessions.find((s) => s.id === id), this.app.snapshot?.agents[id]);
    this.app.cellsMemo = { snapshot: this.app.snapshot, openId: id, cells };
    return cells;
  }

  stripView(cells: PaneCell[], sendKey: SendKey): Strip {
    const p = this.app.pane;
    const panes = new Map<string, Pane>();
    for (const c of cells) panes.set(c.id, this.paneView(this.app.reading.childChat(this.app.openId!, c.id), sendKey, c.hasLog));
    const old = this.app.strip;
    const same =
      old.cells === cells &&
      old.focused === p.focused &&
      old.zoomed === p.zoomed &&
      old.reveal === p.reveal &&
      old.revealSeq === p.revealSeq &&
      cells.every((c) => old.panes.get(c.id) === panes.get(c.id));
    if (!same) this.app.strip = { cells, panes, ...p };
    return this.app.strip;
  }

  // ---- agent panes ----

  /** Leaving a chat frees its child columns, so they stop reading (AgentPaneStore.stop). */
  leavePanes(next: string) {
    for (const chat of [...this.app.chats.values()])
      if (chat.agent !== "chair" && chat.sessionId !== next) {
        this.app.chats.delete(chat.id);
        this.app.inputs.delete(chat.id);
        this.app.readers.delete(chat.id);
        this.app.pending.delete(chat.key);
      }
    this.app.visible.clear();
    this.app.findId = null;
    this.app.pane = { focused: null, zoomed: null, reveal: null, revealSeq: this.app.pane.revealSeq };
  }

  /** A new chat's CLI writes its log on the first turn; until then the rows route has nothing. */
  chairHasLog(sessionId: string): boolean {
    const log = this.app.snapshot?.sessions.find((s) => s.id === sessionId)?.chair_log;
    return log !== null && log !== "";
  }

  /** Loads a chair's chat once its log exists. */
  loadChair(chat: Chat) {
    if (chat.loaded || !this.chairHasLog(chat.sessionId)) return;
    chat.loaded = true;
    void this.app.reading.loadTail(chat).then(() => {
      this.app.streaming.syncSubscriptions();
      if (this.app.openId === chat.sessionId) this.app.notify();
    });
  }

  /** Loads a child's chat once its hooks have reported a log. */
  loadPane(id: string) {
    const chat = this.app.chats.get(id);
    if (!chat || chat.loaded || !this.cells().find((c) => c.id === chat.agent)?.hasLog) return;
    chat.loaded = true;
    void this.app.reading.loadTail(chat).then(() => {
      this.app.streaming.syncSubscriptions();
      if (this.app.openId === chat.sessionId) this.app.notify();
    });
  }

  /** A column scrolled into or out of view. Only a column on screen follows its log. */
  setPaneVisible(agent: string, visible: boolean) {
    if (!this.app.openId) return;
    const id = paneId(this.app.openId, agent);
    if (visible === this.app.visible.has(id)) return;
    if (visible) {
      this.app.visible.add(id);
      this.loadPane(id);
    } else this.app.visible.delete(id);
    this.app.streaming.syncSubscriptions();
  }

  /** A key or "Show column" moved focus to this pane: the strip scrolls to it and its composer takes the keyboard. */
  focusPane(agent: string) {
    this.app.pane = { ...this.app.pane, focused: agent, reveal: agent, revealSeq: this.app.pane.revealSeq + 1 };
    this.app.notify();
  }

  /** A click or a focus inside a pane, or on the chat page (null). The strip does not scroll. */
  paneFocused(agent: string | null) {
    if (this.app.pane.focused === agent) return;
    this.app.pane = { ...this.app.pane, focused: agent };
    this.app.notify();
  }

  /** The strip shows the chat page again, unzoomed, as when focus moves to it. */
  revealChat() {
    const zoomed = this.app.pane.zoomed !== null;
    this.app.pane = { focused: null, zoomed: null, reveal: null, revealSeq: this.app.pane.revealSeq + 1 };
    if (zoomed) this.app.streaming.syncSubscriptions();
    this.app.notify();
  }

  /** ⌥⌘ arrows. Returns false when focus lands on the chat page. */
  movePaneFocus(direction: Direction): boolean {
    const cells = this.cells();
    const from = cells.findIndex((c) => c.id === this.app.pane.focused);
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
    const id = agent ?? this.app.pane.zoomed ?? this.app.pane.focused;
    if (!id || !this.app.openId) return;
    this.app.pane = { ...this.app.pane, zoomed: this.app.pane.zoomed === id ? null : id, focused: id };
    this.loadPane(paneId(this.app.openId, id));
    this.app.streaming.syncSubscriptions();
    this.app.notify();
  }

  /** The header's Stop: interrupts a child's turn. A failure throws the server's text. */
  stopPane(agent: string): Promise<void> {
    return this.app.api.action("/api/chat/interrupt", { session: this.app.openId, agent });
  }

  /** The header's Close agent: ends the child's pane. A failure throws the server's text. */
  closePane(agent: string): Promise<void> {
    return this.app.api.action("/api/agent/close", { session: this.app.openId, agent });
  }
}
