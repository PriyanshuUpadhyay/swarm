

import { measure, whenPainted } from "./perf";
import { foldId, moveTarget, archivedSession, navRows, inOrder, recentWork, type NavRow, type RecentChat, type TreeName } from "./nav";
import { SIDEBAR_VIEWS, SIDEBAR_VIEW_KEY, message, markable, readStore, writeStore } from "./app-types";

const RECENT_ORDER = "swarm.recentOrder";
import type { App } from "./app";

export class AppSidebar {
  constructor(private readonly app: App) {}

  // ---- sidebar ----

  select(id: string, e?: KeyboardEvent) {
    this.app.selected = id;
    this.app.notify();
    const sidebarFocused = document.activeElement?.closest('[data-testid="sidebar"]') != null;
    const row = () => document.querySelector<HTMLElement>(`[data-testid="tree-row"][data-id="${CSS.escape(id)}"]`);
    if (sidebarFocused) whenPainted(() => row() !== null, () => row()?.focus());
    // A later key move in the same frame supersedes this one; its paint covers this one too.
    if (e) measure("key", id, e.timeStamp, () => this.app.selected !== id || row()?.getAttribute("aria-selected") === "true");
  }

  moveSelection(to: number | "home" | "end", e?: KeyboardEvent) {
    if (this.app.sidebarView === 2) return this.moveRecent(to, e);
    if (!this.app.rows.length) return;
    const i = this.app.rows.findIndex((r) => r.id === this.app.selected);
    const next = to === "home" ? 0 : to === "end" ? this.app.rows.length - 1 : Math.max(0, Math.min(this.app.rows.length - 1, i + to));
    if (this.app.rows[next].id !== this.app.selected) this.select(this.app.rows[next].id, e);
  }

  /** ← collapses an open row, else moves to its parent. */
  collapse(e?: KeyboardEvent) {
    if (this.app.sidebarView === 2) return;
    const row = this.app.rows.find((r) => r.id === this.app.selected);
    if (!row) return;
    if (row.expanded) this.toggle(row.id);
    else if (row.parentId) this.select(row.parentId, e);
  }

  /** → opens a closed row, else moves to its first child. */
  expand(e?: KeyboardEvent) {
    if (this.app.sidebarView === 2) return;
    const i = this.app.rows.findIndex((r) => r.id === this.app.selected);
    const row = this.app.rows[i];
    if (!row || row.expanded === null) return;
    if (!row.expanded) this.toggle(row.id);
    else if (this.app.rows[i + 1]?.parentId === row.id) this.select(this.app.rows[i + 1].id, e);
  }

  /** Folds or opens a header, workspace or chat. It shows at once; the server confirms it. */
  toggle(id: string) {
    const row = this.app.rows.find((r) => r.id === id);
    if (!row || row.expanded === null) return;
    this.app.navigation.stopKeyboardFollow();
    const open = !row.expanded;
    this.app.folds.set(foldId(row), open);
    this.app.rows = navRows(this.app.treeName === "archived" ? this.app.archivedTree : this.app.tree, this.app.folds);
    this.app.notify();
    void this.app.navigation.choose({ op: open ? "expand" : "collapse", id: foldId(row) });
  }

  /** Enter: open a chat or a workspace, fold a header, or list a workspace's other chats. */
  activate(e?: KeyboardEvent) {
    if (this.app.sidebarView === 2) {
      if (this.app.recentSelected) this.app.navigation.openChat(this.app.recentSelected, e);
      return;
    }
    const row = this.app.rows.find((r) => r.id === this.app.selected);
    if (row) this.runRow(row, e);
  }

  /** Unarchives an archived chat and opens it. A second press while one runs does nothing, and a
   *  failure shows as a notice. */
  unarchive(session: string) {
    if (this.app.unarchiving.has(session)) return;
    this.app.unarchiving.add(session);
    this.app.life
      .reopen(session)
      .then(() => { this.app.closedTabs = this.app.closedTabs.filter((id) => id !== session); })
      .catch((err) => this.app.notice(`Unarchive failed: ${message(err)}`, true))
      .finally(() => this.app.unarchiving.delete(session));
  }

  /** A plain click clears the picked chats. ⌘-click picks or drops one chat row, and Shift-click
   *  picks the chat rows from the selected chat to this one, or only this one when the selected row
   *  is not a chat; neither opens the chat. While chats are picked, either click on another row does
   *  nothing. */
  clickRow(id: string, pick: "toggle" | "range" | null = null) {
    const row = this.app.rows.find((r) => r.id === id);
    if (!row) return;
    if (pick && !markable(row) && this.app.marked.size) return;
    if (pick && markable(row)) {
      const marked = new Set(this.app.marked);
      const from = this.app.rows.find((r) => r.id === this.app.selected);
      if (!marked.size && from && markable(from)) marked.add(from.id);
      if (pick === "toggle") {
        if (!marked.delete(id)) marked.add(id);
      } else {
        // A header anchor, as after the selected chat left the tree, would pick from the top.
        const anchor = from && markable(from) ? from : row;
        const [a, b] = [this.app.rows.indexOf(anchor), this.app.rows.indexOf(row)].sort((x, y) => x - y);
        for (const r of this.app.rows.slice(a, b + 1)) if (markable(r)) marked.add(r.id);
      }
      this.app.marked = marked;
      this.app.selected = id;
      return this.app.notify();
    }
    if (this.app.marked.size) this.app.marked = new Set();
    this.app.selected = id;
    // A click only selects an archived chat, so a stray click does not unarchive it. Enter does.
    if (row.kind === "archived-chat") return this.app.notify();
    this.runRow(row);
  }

  runRow(nav: NavRow, e?: KeyboardEvent) {
    const row = nav.row;
    if (!row) return this.toggle(nav.id);
    const sel = row.selection ?? { workspace: null, chat: null, agentSession: null, agent: null };
    // An archived chat folds and unarchives in both views, as its button and menu do.
    if (row.kind === "archived") return this.toggle(nav.id);
    if (row.kind === "archived-chat") return this.unarchive(archivedSession(nav.id));
    // The archive view only shows; Restore is in the menu, and an archived workspace takes no selection.
    if (this.app.treeName === "archived") {
      if (row.kind === "chat" && sel.chat) this.app.navigation.openChat(sel.chat, e, { write: false });
      return;
    }
    if (row.kind === "more") {
      const workspace = sel.workspace ?? row.parentId;
      if (workspace) void this.app.navigation.choose({ op: "expand_list", workspace });
    } else if (row.kind === "workspace") void this.app.navigation.selectWorkspace(row.id, e);
    else if (row.kind === "child") {
      const session = sel.agentSession ?? sel.chat;
      if (session) this.app.navigation.openChat(session, e);
      if (sel.agent) this.app.panes.focusPane(sel.agent);
    } else if (sel.chat) this.app.navigation.openChat(sel.chat, e);
  }

  /** Every chat the tree lists, across projects, in the order the user dragged them; a new chat goes
   *  on top. Activity does not move a row. */
  recentList(): RecentChat[] {
    const m = this.app.recentMemo;
    if (m && m.tree === this.app.tree && m.snapshot === this.app.snapshot) return m.list;
    const stored = this.recentOrder();
    const { list, order } = inOrder(recentWork(this.app.tree, this.app.snapshot, Infinity), stored);
    if (order !== stored) writeStore(RECENT_ORDER, JSON.stringify(order));
    this.app.recentMemo = { tree: this.app.tree, snapshot: this.app.snapshot, list };
    return list;
  }

  private recentOrder(): string[] {
    try {
      const v = JSON.parse(readStore(RECENT_ORDER) ?? "[]");
      return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
    } catch {
      return [];
    }
  }

  /** A Recent row dropped on another: it takes that row's place. */
  dropRecent(session: string, target: string) {
    const order = this.recentOrder().filter((id) => id !== session);
    const i = order.indexOf(target);
    if (i < 0) return;
    const from = this.recentList().findIndex((r) => r.session === session);
    const to = this.recentList().findIndex((r) => r.session === target);
    // Dragged down, it lands after the target; dragged up, before it.
    order.splice(from < to ? i + 1 : i, 0, session);
    writeStore(RECENT_ORDER, JSON.stringify(order));
    this.app.recentMemo = null;
    this.app.notify();
  }

  moveRecent(to: number | "home" | "end", e?: KeyboardEvent) {
    const list = this.recentList();
    if (!list.length) return;
    const i = list.findIndex((r) => r.session === this.app.recentSelected);
    const next = to === "home" ? 0 : to === "end" ? list.length - 1 : Math.max(0, Math.min(list.length - 1, i + to));
    const id = list[next].session;
    if (id === this.app.recentSelected) return;
    this.app.recentSelected = id;
    this.app.notify();
    const row = () => document.querySelector<HTMLElement>(`[data-testid="recent-row"][data-session="${CSS.escape(id)}"]`);
    whenPainted(() => row() !== null, () => row()?.focus());
    if (e) measure("key", id, e.timeStamp, () => this.app.recentSelected !== id || row()?.getAttribute("aria-selected") === "true");
  }

  /** A click on a Recent row: it becomes the keyboard row, and its chat opens. */
  clickRecent(session: string) {
    this.app.recentSelected = session;
    this.app.navigation.openChat(session);
  }

  /** Pin from the menu or a drop on Pinned; a folded Pinned header opens to show it. */
  async pin(workspace: string) {
    if (!(await this.app.navigation.choose({ op: "pin", workspace }))) return;
    const pinned = this.app.tree?.sections.find((s) => s.kind === "pinned");
    if (pinned && !pinned.expanded) void this.app.navigation.choose({ op: "expand", id: pinned.collapseId });
  }

  /** Move Up / Move Down's target, or null when the row cannot move that way. */
  workspaceMoveTarget(path: string, offset: -1 | 1): string | null {
    return moveTarget(this.app.tree, this.app.snapshot, path, offset);
  }

  /** The Workspaces / Archived switch at the foot of the sidebar. */
  setTreeName(name: TreeName) {
    if (name === this.app.treeName) return;
    this.app.treeName = name;
    this.app.rows = navRows(name === "archived" ? this.app.archivedTree : this.app.tree, this.app.folds);
    this.app.notify();
    void this.app.navigation.refreshSnapshot();
  }

  /** ⌥⌘1 Workspaces, ⌥⌘2 Recent. The list takes the keyboard, on the open chat's row. */
  setSidebarView(n: number) {
    if (n < 1 || n > SIDEBAR_VIEWS.length) return;
    if (n !== this.app.sidebarView) {
      this.app.sidebarView = n;
      writeStore(SIDEBAR_VIEW_KEY, String(n));
      if (n === 2) this.app.recentSelected = this.app.openId ?? this.recentList()[0]?.session ?? null;
      this.app.notify();
    }
    const list = n === 2 ? '[data-testid="recent"]' : '[data-testid="sidebar"]';
    whenPainted(() => document.querySelector(list) !== null, () => {
      const target = document.querySelector<HTMLElement>(`${list} [tabindex="0"]`) ?? document.querySelector<HTMLElement>(list);
      target?.focus();
    });
  }

  /** The sessions of the picked chat rows that the tree still lists. */
  markedChats(): string[] {
    return this.app.rows.filter((r) => this.app.marked.has(r.id) && markable(r)).map((r) => r.row!.selection!.chat!);
  }

  clearMarked() {
    if (!this.app.marked.size) return;
    this.app.marked = new Set();
    this.app.notify();
  }
}
