import { ApiError } from "./api";
import { measure, type PerfKind } from "./perf";
import { chatPath, resolveChat, shortId } from "./route";
import { chatRows, currentRow, navRows, shareTree, workspaceOf, type ChoiceOp, type Tab, type Tabs } from "./nav";
import { message, markable } from "./app-types";
import type { App } from "./app";

const CLOSE_READ_TIMEOUT_MS = 5_000;

type NavRead = { workspace: string; writeSeq: number; navSeq: number };

export class AppNavigation {
  private closes: Promise<void> = Promise.resolve();
  private keyboardFollow: { session: string; row: string } | null = null;
  // A fresh strip can still list an archived chat until the server prunes it.
  private leftChat: string | null = null;
  /** Puts the keyboard in a chat that opened. App.start sets it (focusOpenedChat), so the node
   *  tests run with no DOM. */
  focusOpened: (session: string, byKey: boolean, current: () => boolean) => void = () => {};

  constructor(private readonly app: App) {}

  stopKeyboardFollow() {
    this.keyboardFollow = null;
  }

  /** Points the URL at the open chat once the first tree has restored one. */
  followUrl() {
    if (typeof location === "undefined" || !this.app.restored) return;
    // `/setup` names the Setup & doctor page while it is open.
    if (location.pathname === "/setup" && this.app.settings.state.open && this.app.settings.state.page === "setup") return;
    const path = chatPath(this.app.openId);
    if (location.pathname !== path) {
      if (this.app.urlFollowed) history.pushState(null, "", path + location.search);
      else history.replaceState(history.state, "", path + location.search);
    }
    this.app.urlFollowed = true;
  }

  refreshSnapshot(): Promise<void> {
    if (this.app.snapshotBusy) {
      this.app.snapshotAgain = true;
      return this.app.snapshotBusy;
    }
    this.app.snapshotBusy = (async () => {
      do {
        this.app.snapshotAgain = false;
        try {
          const [snap, read] = await Promise.all([this.app.api.snapshot(), this.loadNav()]);
          this.app.streaming.applySnapshot(snap);
          this.afterNav(read);
          this.app.error = null;
        } catch (err) {
          this.app.error = String(err);
          if (err instanceof ApiError && err.status === 401) this.app.unauthorized();
        }
        this.app.notify();
      } while (this.app.snapshotAgain);
      this.app.snapshotBusy = null;
    })();
    return this.app.snapshotBusy;
  }

  /** Reads the tree and strip, with the sequences that show whether the strip is still fresh. */
  async loadNav(): Promise<NavRead | null> {
    const seq = this.app.writeSeq;
    const navSeq = this.app.navSeq;
    const name = this.app.treeName;
    const workspace = this.app.workspace;
    const [tree, tabs] = await Promise.all([
      this.app.api.tree(name),
      // An unknown workspace is a 404; the strip stays empty.
      workspace ? this.app.api.tabs(workspace).catch(() => null) : null,
    ]);
    if (name !== this.app.treeName) return null;
    // A write that landed while these were in flight holds the newer revision and folds.
    if (this.app.writing === 0 && seq === this.app.writeSeq) {
      this.app.revision = tree.revision;
      this.app.folds.clear();
    }
    this.app.tree = shareTree(this.app.tree?.view === name ? this.app.tree : null, tree);
    if (tabs && workspace === this.app.workspace) this.app.tabs = tabs.view;
    this.app.rows = navRows(this.app.tree, this.app.folds);
    return tabs ? { workspace: tabs.view.workspace, writeSeq: seq, navSeq } : null;
  }

  /** After each tree: the first one restores the owner's last chat. A chat is marked seen only when
   *  it is selected (`selected_chat`), as in Swift's WorkspaceNavigation.select. */
  afterNav(read: NavRead | null) {
    const tree = this.app.tree;
    if (!tree) return;
    const follow = this.keyboardFollow;
    if (follow) {
      if (this.app.openId !== follow.session || this.app.selected !== follow.row) this.keyboardFollow = null;
      else {
        const chat = this.app.rows.find((row) => row.kind === "chat" && row.row?.selection?.chat === follow.session);
        if (chat) {
          this.app.selected = chat.id;
          this.keyboardFollow = null;
        }
      }
    }
    if (!this.app.selected || !this.app.rows.some((r) => r.id === this.app.selected)) this.app.selected = currentRow(this.app.rows, this.app.openId, this.app.workspace) ?? this.app.rows[0]?.id ?? null;
    // A picked chat that left the tree (archived, ended elsewhere) must not come back picked.
    const kept = new Set(this.app.rows.filter((r) => this.app.marked.has(r.id) && markable(r)).map((r) => r.id));
    if (kept.size !== this.app.marked.size) this.app.marked = kept;
    if (!this.app.restored) {
      this.app.restored = true;
      const asked = this.app.urlChat && resolveChat(this.app.urlChat, this.app.chatIds());
      const row = tree.sections.flatMap((s) => s.rows).find((r) => r.id === tree.selectedRow);
      if (asked && !this.app.openId) {
        this.openChat(asked);
        void this.refreshSnapshot();
      } else if (row && !this.app.openId && !this.app.workspace) {
        if (row.selection?.chat) this.openChat(row.selection?.chat, undefined, { write: false });
        else if (row.kind === "workspace") this.app.workspace = row.id;
        void this.refreshSnapshot();
      }
    }
    const tabs = this.app.tabs;
    // The snapshot can finish after the strip, so freshness is checked after both reads.
    if (read && this.app.writing === 0 && read.writeSeq === this.app.writeSeq && read.navSeq === this.app.navSeq
      && tree.view === "workspaces" && this.app.workspace && read.workspace === this.app.workspace
      && tabs?.workspace === this.app.workspace && tabs.selectedChat && tabs.selectedChat !== this.app.openId
      && tabs.selectedChat !== this.leftChat
      && tabs.tabs.some((tab) => tab.session === tabs.selectedChat)
      && !tabs.tabs.some((tab) => tab.session === this.app.openId)) {
      this.openChat(tabs.selectedChat, undefined, { write: false });
    }
  }

  /** Writes one owner choice after the writes before it, on the newest revision. A refused write
   *  (a stale revision from another writer, or a bad target) shows its error and reloads the tree;
   *  nothing retries, so a choice never lands on a state the owner did not see. */
  choose(op: ChoiceOp): Promise<boolean> {
    this.app.writing++;
    const run = this.app.writes.then(async () => {
      try {
        if (this.app.revision === null) throw new Error("The sidebar has not loaded yet.");
        const done = await this.app.api.choice(this.app.revision, op);
        this.app.revision = done.revision;
        this.app.writeSeq++;
        return true;
      } catch (err) {
        this.app.folds.clear();
        this.app.rows = navRows(this.app.tree, this.app.folds);
        this.app.notice(
          err instanceof ApiError && err.status === 409 ? "Your choices changed elsewhere. The list was reloaded; try again." : message(err),
          true,
        );
        void this.refreshSnapshot();
        return false;
      } finally {
        this.app.writing--;
      }
    });
    this.app.writes = run;
    return run;
  }

  /** Home: no workspace and no chat. It is not saved; a reload opens the last chat. */
  showHome() {
    this.app.navSeq++;
    this.app.panes.leavePanes("");
    this.app.openId = null;
    this.app.workspace = null;
    this.app.notify();
  }

  /** A workspace row, ⌃⌘↓ or ⌃⌘↑: the workspace's remembered chat, else its landing
   *  (SessionsWindowModel.selectWorkspace). */
  async selectWorkspace(path: string, e?: KeyboardEvent) {
    const seq = ++this.app.navSeq;
    if (this.app.tree && !(await this.choose({ op: "selected_chat", workspace: path, chat: null }))) return;
    let tabs: Tabs | null = null;
    try {
      tabs = (await this.app.api.tabs(path)).view;
    } catch {
      // No strip: the landing shows.
    }
    if (seq !== this.app.navSeq) return;
    if (tabs) this.app.tabs = tabs;
    if (tabs?.selectedChat) return this.openChat(tabs.selectedChat, e);
    this.app.panes.leavePanes("");
    this.app.openId = null;
    this.app.workspace = path;
    if (this.app.rows.some((r) => r.id === path)) this.app.selected = path;
    this.app.notify();
  }

  /** An archived chat leaves the page; its workspace's landing shows. */
  leaveChat(session: string) {
    if (this.app.openId !== session) return;
    this.leftChat = session;
    this.app.navSeq++;
    this.app.panes.leavePanes("");
    this.app.openId = null;
    this.app.notify();
  }

  /** An archived or deleted workspace leaves the page; Home shows. */
  leaveWorkspace(workspace: string) {
    if (this.app.workspace === workspace) this.showHome();
  }

  /** A chat's title as the tree lists it. */
  chatTitle(session: string): string {
    // A chat under a folded row or "… N more" has no sidebar row, so its tab names it.
    return chatRows(this.app.tree).find((r) => r.selection?.chat === session)?.title ?? this.app.tabs?.tabs.find((t) => t.session === session)?.title ?? shortId(session, this.app.chatIds());
  }

  // ---- tabs ----

  stripTabs(): Tab[] {
    return (this.app.tabs?.workspace === this.app.workspace ? this.app.tabs?.tabs : null)?.filter((t) => t.pending === null) ?? [];
  }

  /** ⌘1-8 pick a tab by strip order; ⌘9 picks the last. */
  pickChat(n: number, e?: KeyboardEvent) {
    const tabs = this.stripTabs();
    const tab = n === 9 ? tabs[tabs.length - 1] : tabs[n - 1];
    if (tab && tab.session !== this.app.openId) this.openChat(tab.session, e);
  }

  /** ⇧⌘] / ⇧⌘[ step through the tabs, wrapping (PaneSearch.step). */
  stepChat(delta: number, e?: KeyboardEvent) {
    const tabs = this.stripTabs();
    if (!tabs.length) return;
    const i = tabs.findIndex((t) => t.session === this.app.openId);
    const next = i < 0 ? (delta < 0 ? tabs.length - 1 : 0) : (i + (delta % tabs.length) + tabs.length) % tabs.length;
    if (tabs[next].session !== this.app.openId) this.openChat(tabs[next].session, e);
  }

  /** ⇧⌘T reopens the last closed tab, as a browser does. A chat archived since its tab closed
   *  is unarchived. With no closed tab, the archived chats list opens. */
  reopenTab() {
    const session = this.app.closedTabs.pop();
    if (!session) return this.app.life.openClosed();
    if (this.app.listsChat(session)) return this.openChat(session);
    this.app.sidebar.unarchive(session);
  }

  /** ⌃Tab / ⌃⇧Tab go back and forth in the workspace's tab history. Cycling keeps the history
   *  order, or repeated presses would only toggle two chats. */
  stepRecent(delta: number, e?: KeyboardEvent) {
    const strip = this.app.tabs?.workspace === this.app.workspace ? this.app.tabs : null;
    const key = delta < 0 ? strip?.previousRecent : strip?.nextRecent;
    const tab = strip?.tabs.find((t) => t.id === key && t.pending === null);
    if (tab && tab.session !== this.app.openId) this.openChat(tab.session, e, { history: false });
  }

  /** ⌘W and a tab's ×. Closing the open tab shows the chat the server picks, else the landing. */
  closeTab(key?: string): Promise<void> {
    if (this.app.life.state.confirm) return Promise.resolve();
    const run = this.closes.then(() => this.closeTabNow(key));
    this.closes = run.catch(() => {});
    return run;
  }

  private async closeTabNow(key?: string) {
    const workspace = this.app.workspace;
    const tab = this.stripTabs().find((t) => (key ? t.id === key : t.session === this.app.openId));
    if (!workspace || !tab?.canHide) return;
    const closed = () => { this.app.closedTabs = [...this.app.closedTabs.filter((s) => s !== tab.session), tab.session]; };
    const wasOpen = tab.session === this.app.openId;
    const seq = wasOpen ? ++this.app.navSeq : this.app.navSeq;
    if (!(await this.choose({ op: "close_tab", workspace, chat: tab.id }))) return;
    closed();
    if (!wasOpen) return;
    let next: Tabs | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      next = await Promise.race([
        this.app.api.tabs(workspace).then((tabs) => tabs.view),
        // A stalled strip read must release the close queue.
        new Promise<null>((resolve) => { timer = setTimeout(() => resolve(null), CLOSE_READ_TIMEOUT_MS); }),
      ]);
    } catch {
      // The landing shows.
    } finally {
      clearTimeout(timer);
    }
    if (seq !== this.app.navSeq) return;
    if (next) this.app.tabs = next;
    if (next?.selectedChat) return this.openChat(next.selectedChat);
    this.app.panes.leavePanes("");
    this.app.openId = null;
    this.app.notify();
  }

  /** Opens a chat: its tab, the owner's selected chat, and its transcript. `write: false` only
   *  shows it (a restore at start); `history: false` keeps the tab history order (⌃Tab). */
  openChat(sessionId: string, e?: KeyboardEvent, opts: { write?: boolean; history?: boolean } = {}) {
    if (this.leftChat === sessionId) this.leftChat = null;
    const seq = ++this.app.navSeq;
    const kind: PerfKind = this.app.lru.includes(sessionId) ? "switch" : "open";
    if (this.app.openId !== sessionId) this.app.panes.leavePanes(sessionId);
    this.app.openId = sessionId;
    const workspace = workspaceOf(this.app.tree, this.app.snapshot, sessionId);
    const moved = workspace !== null && workspace !== this.app.workspace;
    if (workspace) this.app.workspace = workspace;
    // A restore leaves the keyboard row where it was.
    this.keyboardFollow = null;
    if (opts.write !== false) {
      const row = currentRow(this.app.rows, sessionId, this.app.workspace);
      if (row !== null) {
        this.app.selected = row;
        this.keyboardFollow = { session: sessionId, row };
      }
    }
    if (workspace && this.app.revision !== null && opts.write !== false) {
      // Swift's select: open the tab, then select; only a move to another workspace opens its project.
      const section = this.app.tree?.sections.find((s) => s.rows.some((r) => r.selection?.chat === sessionId));
      if (moved && section && !section.expanded) void this.choose({ op: "expand", id: section.collapseId });
      void this.choose({ op: "open_tab", workspace, chat: sessionId });
      void this.choose({ op: "selected_chat", workspace, chat: sessionId, recording_history: opts.history !== false });
    } else if (moved) void this.refreshSnapshot();
    this.app.reading.touch(sessionId);
    let chat = this.app.chats.get(sessionId);
    if (!chat) {
      chat = this.app.reading.newChat(sessionId);
      chat.loaded = false;
      this.app.panes.loadChair(chat);
    } else if (!this.app.liveAll) this.app.streaming.syncSubscriptions();
    this.app.notify();
    // A restore leaves the keyboard on the page, where j and k walk the sidebar.
    if (opts.write !== false) this.focusOpened(sessionId, e !== undefined, () => this.app.navSeq === seq && this.app.openId === sessionId);
    if (e) measure(kind, sessionId, e.timeStamp, () => this.app.reading.transcriptPainted(sessionId));
  }

  /** ⌃⌘↓ / ⌃⌘↑ go to the next or previous workspace row in the sidebar, wrapping. */
  stepWorkspace(delta: number, e?: KeyboardEvent) {
    const list = this.app.rows.filter((r) => r.kind === "workspace").map((r) => r.id);
    if (!list.length) return;
    const i = this.app.workspace ? list.indexOf(this.app.workspace) : -1;
    const next = i < 0 ? (delta < 0 ? list.length - 1 : 0) : (i + delta + list.length) % list.length;
    if (list[next] !== this.app.workspace) void this.selectWorkspace(list[next], e);
  }
}
