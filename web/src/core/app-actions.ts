import { sleep } from "./api";
import { type Command } from "./commands";
import { whenPainted } from "./perf";
import { isChair, paneStatus } from "./panes";
import { chatRows, nextWaiting, paletteSources, recency, workspaceOf, workspaceRow, type PaletteItem } from "./nav";
import { type Ask, SIDEBAR_VIEWS, PROMPT_JUMP, message } from "./app-types";
import type { App } from "./app";

export class AppActions {
  constructor(private readonly app: App) {}

  /** ⌘T, a "+" and New chat: starts a profile (the default one unless `launch` names it) in the
   *  workspace. A notice shows while `swarm launch` runs; `draft` goes into the new chat's composer.
   *  `inTab` (⌘T and the strip's "+") keeps the chat out of the sidebar. */
  async startChat(workspace = this.app.workspace, launch: { role?: string } = {}, draft = "", inTab = false) {
    const row = workspaceRow(this.app.tree, workspace);
    if (!workspace || this.app.starting || (row && !row.newChatEnabled)) return;
    this.app.starting = true;
    this.app.notice("Starting a chat…");
    try {
      const started = await this.app.api.post<{ session: string }>("/api/chat/new", { cwd: workspace, ...launch });
      if (draft) this.app.composing.setDraft(started.session, draft);
      // The server's tree lists a new chat after its agent poll sees the chair's pane, about a
      // second later, and opening the chat writes a tab that the server checks against that tree.
      for (let i = 0; i < 20; i++) {
        await this.app.navigation.refreshSnapshot();
        if (this.app.listsChat(started.session)) break;
        await sleep(250);
      }
      this.app.navigation.openChat(started.session, undefined, { tabOnly: inTab });
    } catch (err) {
      this.app.notice(message(err), true);
    } finally {
      this.app.starting = false;
    }
  }

  /** ⌃⌘W opens the next chat whose chair or child waits for the owner. */
  openWaiting(e?: KeyboardEvent) {
    // Listed chats in sidebar order, then the chats that a collapsed workspace or "… more" hides.
    // Chats of an archived workspace are skipped, as the Dock badge does. The Workspaces view
    // leaves an archived workspace out; the Archived view and a tree not loaded yet mark nothing.
    const listed = chatRows(this.app.tree).flatMap((r) => (r.selection?.chat ? [r.selection.chat] : []));
    const archived = (s: string) => {
      const row = workspaceRow(this.app.tree, workspaceOf(this.app.tree, this.app.snapshot, s));
      return row ? row.archived : this.app.tree?.view === "workspaces" && !listed.includes(s);
    };
    const order = [...new Set([...listed, ...this.app.chatIds()])].filter((s) => !archived(s));
    const waiting = (s: string) => (this.app.snapshot?.agents[s] ?? []).some((a) => paneStatus(a) === "waiting");
    const next = nextWaiting(order, waiting, this.app.openId);
    if (next) this.app.navigation.openChat(next, e);
    else this.app.notice("No chat is waiting for you.");
  }

  // ---- palette and names ----

  setPalette(open: boolean) {
    if (this.app.paletteOpen === open) return;
    this.app.paletteOpen = open;
    this.app.notify();
  }

  /** Asks the owner for a name; the view shows one dialog. */
  ask(ask: Ask | null) {
    this.app.asking = ask;
    this.app.notify();
  }

  renameChat(session: string) {
    const workspace = workspaceOf(this.app.tree, this.app.snapshot, session);
    if (!workspace) return;
    this.ask({ title: "Rename chat", value: this.app.chatTitle(session), save: (name) => void this.app.navigation.choose({ op: "rename_chat", workspace, chat: session, name }) });
  }

  renameWorkspace(path: string) {
    const row = workspaceRow(this.app.tree, path);
    this.ask({ title: "Rename workspace", value: row?.title ?? "", save: (name) => void this.app.navigation.choose({ op: "rename_workspace", workspace: path, name }) });
  }

  renameProject(path: string, title: string) {
    this.ask({ title: "Rename project", value: title, save: (name) => void this.app.navigation.choose({ op: "rename_project", project: path, name }) });
  }

  /** The palette's items in Swift order: actions, chat actions, workspaces, chats, agents
   *  (PaletteItems.build). Pure in the view, so the palette can memoize it on the view. */
  paletteItems(): PaletteItem[] {
    const items: PaletteItem[] = this.app.registry
      .list()
      .filter((c) => c.palette !== false)
      .map((c) => ({ id: `action:${c.id}`, title: c.title, group: "action", shortcut: c.chord, disabledReason: c.phase ? `Comes in ${c.phase}` : null, run: () => c.run() }));
    const open = this.app.openId;
    const chatAction = (id: string, title: string, run: (() => void) | string): PaletteItem => ({
      id: `chatAction:${id}`,
      title,
      group: "action",
      disabledReason: typeof run === "string" ? `Comes in ${run}` : null,
      run: typeof run === "string" ? () => {} : run,
    });
    if (open)
      items.push(
        chatAction("closeTab", "Close Tab", () => void this.app.navigation.closeTab()),
        chatAction("endChat", "End chat", () => void this.app.life.endChat(open, false)),
        { ...chatAction("archiveChat", "Archive chat", () => void this.app.life.endChat(open, true)), shortcut: "Shift+Meta+W" },
        chatAction("renameChat", "Rename chat", () => this.renameChat(open)),
      );
    items.push(chatAction("reopenChat", "Archived chats", () => this.app.life.openClosed()));
    if (open) items.push(chatAction("switchModel", "Switch model", () => this.app.life.openSwitchModel(open)));
    const sources = paletteSources(this.app.tree, this.app.snapshot);
    const chats: PaletteItem[] = [];
    const agents: PaletteItem[] = [];
    for (const w of sources) {
      const title = w.project && !w.row.pinned ? `${w.project} / ${w.row.title}` : w.row.title;
      items.push({ id: `workspace:${w.row.id}`, title, subtitle: w.row.detail || undefined, group: "workspace", status: w.row.status, recency: w.at, run: () => void this.app.navigation.selectWorkspace(w.row.id) });
      for (const c of w.chats) {
        const session = c.selection?.chat!;
        chats.push({ id: `chat:${session}`, title: c.title, subtitle: title, group: "chat", status: c.status, recency: recency(this.app.snapshot, session), run: () => this.app.navigation.openChat(session) });
        const info = this.app.snapshot?.sessions.find((s) => s.id === session);
        for (const a of this.app.snapshot?.agents[session] ?? [])
          if (info && a.alive && !isChair(a, info))
            agents.push({
              id: `agent:${session}:${a.id}`,
              title: a.id,
              subtitle: `${a.role} · ${c.title}`,
              group: "agent",
              status: paneStatus(a),
              run: () => {
                this.app.navigation.openChat(session);
                this.app.panes.focusPane(a.id);
              },
            });
      }
    }
    // Shown only for a query, as the empty palette lists Recent and Actions.
    const prompts: PaletteItem[] = open
      ? this.app.composing.history(open).map((text, i) => ({
          id: `prompt:${i}`,
          // By code point, so an emoji at the cut stays whole.
          title: Array.from(text.split("\n")[0]).slice(0, 120).join(""),
          group: "prompt",
          run: () => {
            this.app.composing.reuse(open, text);
            // The closing dialog gives the focus back to where it was, so focus after it is gone.
            whenPainted(() => !document.querySelector('[role="dialog"]'), () => document.querySelector<HTMLElement>('[data-testid="composer"]')?.focus());
          },
        }))
      : [];
    return [...items, ...chats, ...agents, ...prompts];
  }

  // ---- commands ----

  /** The SwiftUI key map (AppKey.table), in the palette's action order (PaletteItems.actions). */
  commands(): Command[] {
    const inTree = () => {
      const a = document.activeElement;
      return !a || a === document.body || a.closest('[data-testid="sidebar"], [data-testid="recent"]') !== null;
    };
    const tree = (id: string, title: string, chord: string, run: (e?: KeyboardEvent) => void): Command => ({
      id,
      title,
      chord,
      palette: false,
      when: () => !this.app.paletteOpen && inTree(),
      run,
    });
    const chat = () => this.app.openId !== null;
    const cmds: Command[] = [
      { id: "workspace.new", title: "New Workspace", chord: "Meta+N", run: () => this.app.life.newWorkspace() },
      {
        id: "chat.new",
        title: "New Chat",
        chord: "Meta+T",
        when: () => this.app.workspace !== null,
        // A held key must not start one chat per repeat.
        run: (e) => void (e?.repeat || this.startChat(undefined, {}, "", true)),
      },
      { id: "project.new", title: "New Project…", chord: "Shift+Meta+N", run: () => this.app.life.openProject("create") },
      { id: "workspace.next", title: "Next Workspace", chord: "Ctrl+Meta+ArrowDown", run: (e) => this.app.navigation.stepWorkspace(1, e) },
      { id: "workspace.prev", title: "Previous Workspace", chord: "Ctrl+Meta+ArrowUp", run: (e) => this.app.navigation.stepWorkspace(-1, e) },
      { id: "chat.next", title: "Next Chat", chord: "Shift+Meta+]", run: (e) => this.app.navigation.stepChat(1, e) },
      { id: "chat.prev", title: "Previous Chat", chord: "Shift+Meta+[", run: (e) => this.app.navigation.stepChat(-1, e) },
      { id: "pane.zoom", title: "Zoom Pane", chord: "Shift+Meta+Enter", when: chat, run: () => this.app.panes.toggleZoom() },
      {
        id: "composer.focus",
        title: "Focus Composer",
        chord: "Meta+L",
        run: () => {
          this.app.panes.revealChat();
          document.querySelector<HTMLElement>('[data-testid="composer"]')?.focus();
        },
      },
    ];
    // ⌘B and ⇧⌘B belong to the window frame, which registers them; they sit here in palette order.
    for (let n = 1; n <= SIDEBAR_VIEWS.length; n++)
      cmds.push({ id: `sidebar.view.${n}`, title: `Show ${SIDEBAR_VIEWS[n - 1]}`, chord: `Alt+Meta+${n}`, run: () => this.app.sidebar.setSidebarView(n) });
    cmds.push(
      { id: "find.open", title: "Find", chord: "Meta+F", when: chat, run: () => this.app.reading.openFind() },
      { id: "prompt.prev", title: "Previous Prompt", chord: "Ctrl+Alt+ArrowUp", when: chat, run: () => void window.dispatchEvent(new CustomEvent(PROMPT_JUMP, { detail: -1 })) },
      { id: "prompt.next", title: "Next Prompt", chord: "Ctrl+Alt+ArrowDown", when: chat, run: () => void window.dispatchEvent(new CustomEvent(PROMPT_JUMP, { detail: 1 })) },
      { id: "chat.waiting", title: "Next Waiting Chat", chord: "Ctrl+Meta+W", run: (e) => this.openWaiting(e) },
      {
        id: "chat.stop",
        title: "Stop the Chair",
        chord: "Meta+.",
        when: () => this.app.openId !== null && !this.app.composing.chair(this.app.openId).unreachable,
        run: () => void this.app.composing.interrupt(this.app.openId!),
      },
      { id: "palette", title: "Search", chord: "Meta+K", palette: false, run: () => this.setPalette(!this.app.paletteOpen) },
      { id: "palette.alt", title: "Search", chord: "Shift+Meta+P", palette: false, run: () => this.setPalette(!this.app.paletteOpen) },
      { id: "find.next", title: "Find Next", chord: "Meta+G", palette: false, when: chat, run: () => this.app.reading.stepFind(1) },
      { id: "find.prev", title: "Find Previous", chord: "Shift+Meta+G", palette: false, when: chat, run: () => this.app.reading.stepFind(-1) },
      { id: "tab.close", title: "Close Tab", chord: "Meta+W", palette: false, run: () => void this.app.navigation.closeTab() },
      // The palette lists Archive chat with this key, so the command stays out of it.
      { id: "chat.archive", title: "Archive Chat", chord: "Shift+Meta+W", palette: false, when: chat, run: (e) => void (e?.repeat || this.app.life.endChat(this.app.openId!, true)) },
      { id: "recent.closed", title: "Reopen Closed Tab", chord: "Shift+Meta+T", palette: false, run: () => this.app.navigation.reopenTab() },
      { id: "recent.prev", title: "Previous Recent Chat", chord: "Ctrl+Tab", palette: false, run: (e) => this.app.navigation.stepRecent(-1, e) },
      { id: "recent.next", title: "Next Recent Chat", chord: "Ctrl+Shift+Tab", palette: false, run: (e) => this.app.navigation.stepRecent(1, e) },
      tree("tree.down", "Select next row", "j", (e) => this.app.sidebar.moveSelection(1, e)),
      tree("tree.down2", "Select next row", "ArrowDown", (e) => this.app.sidebar.moveSelection(1, e)),
      tree("tree.up", "Select previous row", "k", (e) => this.app.sidebar.moveSelection(-1, e)),
      tree("tree.up2", "Select previous row", "ArrowUp", (e) => this.app.sidebar.moveSelection(-1, e)),
      tree("tree.home", "Select first row", "Home", (e) => this.app.sidebar.moveSelection("home", e)),
      tree("tree.end", "Select last row", "End", (e) => this.app.sidebar.moveSelection("end", e)),
      tree("tree.collapse", "Collapse", "ArrowLeft", (e) => this.app.sidebar.collapse(e)),
      tree("tree.expand", "Expand", "ArrowRight", (e) => this.app.sidebar.expand(e)),
      tree("tree.open", "Open", "Enter", (e) => this.app.sidebar.activate(e)),
      { ...tree("tree.unmark", "Clear selected chats", "Escape", () => this.app.sidebar.clearMarked()), when: () => !this.app.paletteOpen && inTree() && this.app.marked.size > 0 },
    );
    for (const d of ["left", "right", "up", "down"] as const)
      cmds.push({
        id: `pane.focus.${d}`,
        title: `Focus ${d[0].toUpperCase()}${d.slice(1)}`,
        chord: `Alt+Meta+Arrow${d[0].toUpperCase()}${d.slice(1)}`,
        palette: false,
        when: chat,
        run: () => this.app.panes.movePaneFocus(d),
      });
    for (let n = 1; n <= 9; n++)
      cmds.push({ id: `chat.${n}`, title: n === 9 ? "Last Chat" : `Chat ${n}`, chord: `Meta+${n}`, palette: false, run: (e) => this.app.navigation.pickChat(n, e) });
    return cmds;
  }
}
