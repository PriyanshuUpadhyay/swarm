// Workspace and chat lifecycle (S3): new workspace in four modes with its setup script, projects,
// new chat with a profile, switch model, archive, delete, prune and Archived chats, on the P3a, P3b
// and S2 routes. Framework-free; the dialogs read `state`, which is a new object after each change.

import { ApiError, sleep } from "./api";
import type { App } from "./app";
import { isChair } from "./panes";
import {
  type NewOptions,
  type Issue,
  type NewRequest,
  type SetupRun,
  type Confirm,
  type ClosedChat,
  type ImportableChat,
  type Profile,
  type LifeState,
  issuePrompt,
  setupFrame,
  setupFailure,
} from "./lifecycle-state";

export * from "./lifecycle-state";

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export class Lifecycle {
  state: LifeState = { newWorkspace: null, create: null, project: null, importChoice: null, projectPath: "", existing: null, setupScript: null, switchModel: null, handoff: null, confirm: null, closed: false, imports: false };
  private listeners = new Set<() => void>();
  private roles: Promise<Profile[]> | null = null;
  private confirmDone: (() => void) | null = null;

  constructor(private readonly app: App) {}

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private set(change: Partial<LifeState>) {
    this.state = { ...this.state, ...change };
    for (const fn of this.listeners) fn();
  }

  private get api() {
    return this.app.api;
  }

  /** `setup` and `handoff` stream frames. */
  onFrame(event: string, data: Record<string, unknown>) {
    const c = this.state.create;
    if (event === "setup" && c?.state === "working") {
      const setup = setupFrame(c.setup, data);
      if (setup !== c.setup) this.set({ create: { ...c, setup } });
    }
    const h = this.state.handoff;
    if (event === "handoff" && h && data.old_session === h.old && !["completed", "failed", "cancelled"].includes(h.phase))
      this.set({ handoff: { ...h, phase: String(data.phase), canCancel: data.canCancel === true, error: data.error ? String(data.error) : null } });
  }

  close<K extends keyof LifeState>(key: K) {
    if (key === "confirm") {
      this.confirmDone?.();
      this.confirmDone = null;
    }
    this.set({ [key]: key === "closed" || key === "imports" ? false : null } as Partial<LifeState>);
  }

  // ---- projects ----

  /** Project sections of the tree, for the project picker. */
  projects(): { path: string; title: string }[] {
    return (this.app.view.tree?.sections ?? []).filter((s) => s.kind === "project" && s.path).map((s) => ({ path: s.path!, title: s.title }));
  }

  /** The project of a workspace: the section that lists it. */
  projectOf(workspace: string | null): string | null {
    const s = this.app.view.tree?.sections.find((s) => s.kind === "project" && s.rows.some((r) => r.id === workspace));
    return s?.path ?? null;
  }

  openProject(mode: "create" | "import", path = "") {
    this.set({ project: mode, importChoice: null, projectPath: path, existing: null });
  }

  /** Create (a full destination path and a name) or Import (a path, and later Git or plain folder). */
  async saveProject(mode: "create" | "import", path: string, name: string, as?: "git" | "folder"): Promise<string | null> {
    this.set({ existing: null });
    try {
      if (mode === "create") {
        const made = await this.api.post<{ notice?: string | null }>("/api/project/create", { path, name });
        if (made.notice) this.app.notice(made.notice);
      } else {
        const done = await this.api.post<{ needsChoice?: boolean }>("/api/project/import", as ? { path, as } : { path });
        if (done.needsChoice) {
          this.set({ importChoice: path });
          return null;
        }
      }
      this.set({ project: null, importChoice: null });
      await this.app.refresh();
      return null;
    } catch (err) {
      if (mode === "create" && err instanceof ApiError && err.status === 409) this.set({ existing: path });
      return message(err);
    }
  }

  removeProject(path: string, title: string) {
    void this.ask({
      title: "Remove project?",
      body: `Hide ${title} from the sidebar? Its folder and chats stay on disk.`,
      action: "Remove Project",
      run: () => this.api.post("/api/project/remove", { project: path }),
    });
  }

  editSetupScript(project: string, title: string) {
    this.set({ setupScript: { project, title } });
  }

  saveSetupScript(project: string, script: string): Promise<boolean> {
    this.set({ setupScript: null });
    return this.app.choose({ op: "setup_script", project, setup_script: script });
  }

  // ---- new workspace ----

  /** ⌘N, a project's "+" and New Workspace…: the dialog on this project, else the current one. */
  newWorkspace(project?: string) {
    const target = project ?? this.projectOf(this.app.view.workspace) ?? this.projects()[0]?.path;
    if (!target) return this.app.notice("Import or create a project first.");
    // A create still running keeps its progress, so the dialog shows it again.
    this.set({ newWorkspace: { project: target }, create: this.state.create?.state === "working" ? this.state.create : null });
  }

  options(project: string): Promise<NewOptions> {
    return this.api.get("/api/workspace/new-options", { project });
  }

  async issues(project: string): Promise<Issue[]> {
    return (await this.api.get<{ issues: Issue[] }>("/api/issues", { project })).issues;
  }

  /** Creates the workspace and waits for its setup script. A failed setup keeps the workspace and
   *  the dialog, which says so; otherwise the dialog closes on the new workspace and starts a chat,
   *  with the issue in its draft for a workspace from an issue. With the dialog closed, a notice reports. */
  async create(req: NewRequest) {
    if (this.state.create?.state === "working") return;
    this.set({ create: { state: "working", error: null, workspace: null, setup: null } });
    let made: { workspace: string; setup?: Record<string, unknown>; issue?: Issue };
    try {
      made = await this.api.post("/api/workspace/new", req);
    } catch (err) {
      if (!this.state.newWorkspace) this.app.notice(message(err), true);
      this.set({ create: { state: "failed", error: message(err), workspace: null, setup: null } });
      return;
    }
    await this.app.refresh();
    const closed = !this.state.newWorkspace;
    // The reply's terminal frame is the result; the streamed output stays.
    let setup: SetupRun | null = null;
    if (made.setup) {
      const streamed = this.state.create?.setup;
      const output = streamed?.workspace === made.workspace ? streamed.output : "";
      setup = setupFrame({ workspace: made.workspace, state: "running", output, exitCode: null, error: null }, { ...made.setup, workspace: made.workspace });
    }
    if (setup && setup.state !== "completed") {
      if (closed) this.app.notice(setupFailure(setup), true);
      this.set({ create: closed ? null : { state: "kept", error: null, workspace: made.workspace, setup } });
      return;
    }
    this.set({ newWorkspace: null, create: null });
    if (setup) this.app.notice("The setup script finished.");
    if (closed && !made.issue) return this.app.notice(`Created the workspace ${made.workspace}.`);
    await this.app.selectWorkspace(made.workspace);
    await this.app.startChat(made.workspace, {}, made.issue ? issuePrompt(made.issue) : "");
  }

  /** The kept workspace after a failed setup. */
  async openKept() {
    const workspace = this.state.create?.workspace;
    this.set({ newWorkspace: null, create: null });
    if (workspace) await this.app.selectWorkspace(workspace);
  }

  // ---- chats ----

  /** After a profile write in Settings, so the next menu reads the new list. */
  forgetProfiles() {
    this.roles = null;
  }

  /** The profiles for the new-chat menu, read once. */
  profiles(): Promise<Profile[]> {
    this.roles ??= this.api.get<{ profiles: Profile[] }>("/api/roles").then(
      (r) => r.profiles,
      (err) => {
        this.roles = null;
        throw err;
      },
    );
    return this.roles;
  }

  /** The session's live agents, children before the chair, as `/api/agent/close` takes them. */
  private liveAgents(session: string): string[] {
    const { info, agents } = this.app.agentsOf(session);
    const live = agents.filter((a) => a.alive);
    const chair = (a: (typeof live)[number]) => info !== undefined && isChair(a, info);
    return [...live.filter((a) => !chair(a)).map((a) => a.id), ...(live.some(chair) ? ["chair"] : [])];
  }

  /** End chat… and Archive chat. Live agents ask first (SwarmSessionCloser); Archive then moves the
   *  chat to Archived chats. */
  endChat(session: string, archive: boolean, done?: () => void) {
    return this.endChats([session], archive, done);
  }

  /** End or Archive for the sidebar's selected chats, with one confirm for all of them. */
  async endChats(sessions: string[], archive: boolean, done?: () => void): Promise<void> {
    const live = sessions.map((session) => ({ session, agents: this.liveAgents(session) }));
    const agents = live.reduce((n, l) => n + l.agents.length, 0);
    const run = async () => {
      for (const { session, agents } of live) for (const agent of agents) await this.api.action("/api/agent/close", { session, agent });
      if (archive) {
        await this.api.action("/api/sessions/archive", { ids: sessions });
        for (const session of sessions) this.app.leaveChat(session);
      }
      done?.();
    };
    const many = sessions.length > 1;
    if (!agents && !archive) return this.app.notice(many ? "These chats have already ended." : "This chat has already ended.");
    if (!agents) return this.runAction(run);
    // An agent whose turn is over still has an open pane; only a working or waiting one is "running".
    const busy = sessions.flatMap((s) => this.app.agentsOf(s).agents).filter((a) => a.alive && (a.state === "working" || a.state === "waiting")).length;
    const verb = archive ? "Archiving" : "Ending";
    return this.ask({
      title: `${archive ? "Archive" : "End"} ${many ? `${sessions.length} chats` : `“${this.app.chatTitle(sessions[0])}”`}?`,
      body: `${busy ? `${plural(busy, "agent")} still working. ${verb} stops them.` : `${verb} closes ${many ? "their" : "its"} agent${agents > 1 ? "s" : ""}.`}${archive ? (many ? " The chats move to Archived." : " The chat moves to Archived.") : ""}`,
      action: `${archive ? "Archive" : "End"} ${many ? `${sessions.length} Chats` : "Chat"}`,
      run,
    });
  }

  openSwitchModel(session: string) {
    const h = this.state.handoff;
    // A switch still running for this chat keeps its progress.
    this.set({ switchModel: session, handoff: h?.old === session && !["completed", "failed", "cancelled"].includes(h.phase) ? h : null });
  }

  providers(): Promise<{ providers: { id: string; label: string; accounts: boolean }[] }> {
    return this.api.get("/api/providers");
  }

  models(provider: string): Promise<{ models: { id: string; label: string }[] }> {
    return this.api.get("/api/models", { provider });
  }

  accounts(provider: string): Promise<{ accounts: { name: string; email: string | null; auth_state?: string }[] }> {
    return this.api.get("/api/accounts", { provider });
  }

  /** Switch model: a new chat on the chosen model takes this chat's summary (P3b). Progress comes
   *  from `handoff` frames; on success the new chat opens. */
  async switchModel(old: string, provider: string, model: string, account: string | null) {
    if (this.state.handoff && !["completed", "failed", "cancelled"].includes(this.state.handoff.phase)) return;
    this.set({ handoff: { old, phase: "preparing", canCancel: true, error: null } });
    try {
      const done = await this.api.post<{ session: string }>("/api/chat/continue", account ? { old_session: old, provider, model, account } : { old_session: old, provider, model });
      this.set({ switchModel: null, handoff: null });
      await this.app.refresh();
      this.app.openChat(done.session);
    } catch (err) {
      const cancelled = err instanceof ApiError && err.message === "model switch cancelled";
      if (!this.state.switchModel) this.app.notice(cancelled ? "Model switch cancelled." : message(err), !cancelled);
      this.set({ handoff: { old, phase: cancelled ? "cancelled" : "failed", canCancel: false, error: cancelled ? null : message(err) } });
    }
  }

  async cancelSwitch() {
    const h = this.state.handoff;
    if (!h?.canCancel) return;
    try {
      await this.api.post("/api/chat/continue/cancel", { old_session: h.old });
    } catch (err) {
      this.app.notice(message(err), true);
    }
  }

  // ---- workspaces ----

  private impact(workspace: string) {
    return this.api.get<{
      liveAgents: number;
      confirmationRequired: boolean;
      dirtyFiles: string[];
      ignoredCount: number;
      firstIgnoredPath: string | null;
      canDelete: boolean;
      deleteBlocker: string | null;
      prunePaths: string[];
    }>("/api/workspace/impact", { workspace });
  }

  /** Archive asks only while agents run, as Swift does; it keeps the folder. */
  async archiveWorkspace(workspace: string) {
    let impact;
    try {
      impact = await this.impact(workspace);
    } catch (err) {
      return this.app.notice(message(err), true);
    }
    const run = () => this.api.post("/api/workspace/archive", { workspace });
    if (!impact.confirmationRequired) return void this.runAction(run, null, workspace);
    void this.ask({
      title: "Archive workspace?",
      body: `${plural(impact.liveAgents, "agent")} still running. Archiving stops them and keeps the workspace folder.`,
      action: "Archive Workspace",
      run: async () => {
        await run();
        this.app.leaveWorkspace(workspace);
      },
    });
  }

  restoreWorkspace(workspace: string) {
    void this.runAction(() => this.api.post("/api/workspace/restore", { workspace }));
  }

  async deleteWorkspace(workspace: string) {
    let impact;
    try {
      impact = await this.impact(workspace);
    } catch (err) {
      return this.app.notice(message(err), true);
    }
    if (!impact.canDelete) return this.app.notice(impact.deleteBlocker ?? "This workspace cannot be deleted.", true);
    const lines = [`Delete the workspace folder at ${workspace}? Git refuses uncommitted changes or unpushed commits.`];
    if (impact.confirmationRequired) lines.push(`${plural(impact.liveAgents, "agent")} still running. Deleting stops them before the folder is removed.`);
    if (impact.ignoredCount) lines.push(`${plural(impact.ignoredCount, "ignored file")} will be deleted too, such as ${impact.firstIgnoredPath}.`);
    if (impact.deleteBlocker) lines.push(impact.deleteBlocker);
    void this.ask({
      title: "Delete workspace?",
      body: lines.join("\n\n"),
      action: "Delete Workspace",
      blocked: impact.deleteBlocker !== null,
      run: async () => {
        await this.api.post("/api/workspace/delete", { workspace });
        this.app.leaveWorkspace(workspace);
      },
    });
  }

  async pruneWorkspace(workspace: string, project: string) {
    let impact;
    try {
      impact = await this.impact(workspace);
    } catch (err) {
      return this.app.notice(message(err), true);
    }
    void this.ask({
      title: "Prune missing worktrees?",
      body: `Prune removes the worktree records for these missing folders in ${project}:\n\n${impact.prunePaths.join("\n")}`,
      action: "Prune",
      run: () => this.api.post("/api/workspace/prune", { workspace }),
    });
  }

  // ---- Archived chats ----

  openClosed() {
    this.set({ closed: true });
  }

  async closedChats(): Promise<ClosedChat[]> {
    const { sessions } = await this.api.get<{ sessions: ClosedChat[] }>("/api/sessions/archived");
    return sessions.sort((a, b) => b.archivedAt - a.archivedAt);
  }

  async reopen(session: string) {
    await this.api.action("/api/sessions/unarchive", { ids: [session] });
    this.set({ closed: false });
    // The server's tree shows the chat again about a second later. A chat from another adapter
    // loses its import when archived (ADR 0085), so import it again.
    for (let i = 0; i < 20; i++) {
      await this.app.refresh();
      if (this.app.listsChat(session)) break;
      const imported = (await this.importableChats()).find((c) => c.id === session);
      if (imported) return this.importChat(imported);
      await sleep(250);
    }
    this.app.openChat(session);
  }

  // ---- Import chats ----

  openImports() {
    this.set({ imports: true });
  }

  async importableChats(): Promise<ImportableChat[]> {
    return (await this.api.get<{ chats: ImportableChat[] }>("/api/chats/importable")).chats;
  }

  /** Lists a chat from another adapter in the sidebar, then opens it. A refused write shows its
   *  notice and keeps the dialog. */
  async importChat(chat: ImportableChat) {
    if (!(await this.app.choose({ op: "import_chat", workspace: chat.workspace, chat: chat.id }))) return;
    this.set({ imports: false });
    await this.app.refresh();
    this.app.openChat(chat.id);
  }

  // ---- confirm ----

  private ask(confirm: Confirm): Promise<void> {
    this.confirmDone?.();
    return new Promise((resolve) => {
      this.confirmDone = resolve;
      this.set({ confirm });
    });
  }

  /** The confirm dialog's action. A failure keeps the dialog and shows the server's text. */
  async confirmed(): Promise<string | null> {
    const c = this.state.confirm;
    if (!c) return null;
    const done = this.confirmDone;
    try {
      await c.run();
    } catch (err) {
      return message(err);
    }
    if (this.state.confirm === c) {
      this.set({ confirm: null });
      this.confirmDone = null;
    }
    await this.app.refresh();
    done?.();
    return null;
  }

  /** Runs an action with no dialog; a failure shows the server's text. */
  private async runAction(run: () => Promise<unknown>, leaveChat: string | null = null, leaveWorkspace: string | null = null) {
    try {
      await run();
    } catch (err) {
      return this.app.notice(message(err), true);
    }
    if (leaveChat) this.app.leaveChat(leaveChat);
    if (leaveWorkspace) this.app.leaveWorkspace(leaveWorkspace);
    await this.app.refresh();
  }
}
