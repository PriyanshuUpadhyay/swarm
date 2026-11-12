// Workspace and chat lifecycle (S3): new workspace in four modes with its setup script, projects,
// new chat with a profile, switch model, archive, delete, prune and Archived chats, on the P3a, P3b
// and S2 routes. Framework-free; the dialogs read `state`, which is a new object after each change.

import { ApiError, sleep } from "./api";
import type { App } from "./app";
import { isChair } from "./panes";
import { RunCatalog } from "./run-catalog";
import type { SwitchCarry } from "./composer";
import {
  type NewOptions,
  type Existing,
  type Mode,
  type Issue,
  type NewRequest,
  type SetupRun,
  type Confirm,
  type ClosedChat,
  type ImportableChat,
  type Profile,
  type LifeState,
  type Handoff,
  issuePrompt,
  setupFrame,
  setupFailure,
  isSettled,
  endConfirm,
} from "./lifecycle-state";
import { message } from "./app-types";
import { archiveWorkspace, deleteWorkspace, pruneWorkspace, type WorkspaceActions } from "./lifecycle-workspaces";

export * from "./lifecycle-state";


export class Lifecycle {
  state: LifeState = { newWorkspace: null, create: null, project: null, importChoice: null, projectPath: "", existing: null, setupScript: null, switchModel: null, handoffs: {}, confirm: null, closed: false, imports: false };
  private listeners = new Set<() => void>();
  /** The CLIs, models and accounts that the run chip and Switch model offer. */
  readonly runs: RunCatalog;
  private roles: Promise<Profile[]> | null = null;
  private confirmDone: (() => void) | null = null;

  constructor(private readonly app: App) {
    this.runs = new RunCatalog(() => app.api);
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private setHandoff(h: Handoff, change: Partial<LifeState> = {}) {
    this.set({ ...change, handoffs: { ...this.state.handoffs, [h.old]: h } });
  }

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
    const h = event === "handoff" ? this.state.handoffs[String(data.old_session)] : undefined;
    if (h && !isSettled(h.phase)) {
      const phase = String(data.phase);
      // A resume closes the chair's pane from `starting` on; its popover goes with the composer
      // now, so the ended-chat dialog does not open for it.
      const gone = h.carry === "resume" && phase === "starting" && this.state.switchModel === h.old;
      this.setHandoff({ ...h, phase, canCancel: data.canCancel === true, error: data.error ? String(data.error) : null }, gone ? { switchModel: null } : {});
    }
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
  /** The projects that New Workspace can use: a plain folder has no Git branches. */
  projects(): { path: string; title: string }[] {
    return (this.app.view.tree?.sections ?? []).filter((s) => s.kind === "project" && s.path && !s.folder).map((s) => ({ path: s.path!, title: s.title }));
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

  async saveSetupScript(project: string, script: string): Promise<boolean> {
    const saved = await this.app.choose({ op: "setup_script", project, setup_script: script });
    if (saved && this.state.setupScript?.project === project) this.set({ setupScript: null });
    return saved;
  }

  // ---- new workspace ----

  /** ⌘N, a project's "+" and New Workspace…: the dialog on this project, else the current one.
   *  "N more worktrees" opens it on Existing branch, where each worktree is marked open. */
  newWorkspace(project?: string, mode?: Mode) {
    const repos = this.projects().map((p) => p.path);
    const target = [project, this.projectOf(this.app.view.workspace)].find((p) => p && repos.includes(p)) ?? repos[0];
    if (!target) return this.app.notice("New Workspace needs a Git project. Import or create one first.");
    // A create still running keeps its progress, so the dialog shows it again.
    this.set({ newWorkspace: { project: target, mode }, create: this.state.create?.state === "working" ? this.state.create : null });
  }

  options(project: string): Promise<NewOptions> {
    return this.api.get("/api/workspace/new-options", { project });
  }

  /** `git fetch --prune origin`, so the next options read lists the branches origin has now. */
  fetchOrigin(project: string): Promise<unknown> {
    return this.api.post("/api/workspace/fetch", { project });
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
    let made: { workspace: string; created?: boolean; setup?: Record<string, unknown>; issue?: Issue; existing?: Existing };
    try {
      made = await this.api.post("/api/workspace/new", req);
    } catch (err) {
      if (!this.state.newWorkspace) this.app.notice(message(err), true);
      this.set({ create: { state: "failed", error: message(err), workspace: null, setup: null } });
      return;
    }
    // The name is taken, and nothing changed: the dialog offers to open or reopen that branch.
    if (made.existing) {
      if (!this.state.newWorkspace) this.app.notice(`The branch ${made.existing.branch} already exists. Open it from New Workspace, Existing branch.`, true);
      this.set({ create: { state: "exists", error: null, workspace: made.existing.path, setup: null, existing: made.existing } });
      return;
    }
    await this.app.refresh();
    // An existing worktree was opened, not made, so no setup ran and no chat starts.
    if (made.created === false) {
      this.set({ newWorkspace: null, create: null });
      return this.app.selectWorkspace(made.workspace);
    }
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

  /** A new name clears the taken-name offer, so Create comes back. */
  forgetExisting() {
    if (this.state.create?.state === "exists") this.set({ create: null });
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
  endChat(session: string, archive: boolean) {
    return this.endChats([session], archive);
  }

  /** End or Archive for the sidebar's selected chats, with one confirm for all of them. */
  async endChats(sessions: string[], archive: boolean): Promise<void> {
    const live = sessions.map((session) => ({ session, agents: this.liveAgents(session) }));
    const agents = live.reduce((n, l) => n + l.agents.length, 0);
    const run = async () => {
      for (const { session, agents } of live) for (const agent of agents) await this.api.action("/api/agent/close", { session, agent });
      if (archive) {
        await this.api.action("/api/sessions/archive", { ids: sessions });
        this.app.navigation.rememberArchived(sessions);
        for (const session of sessions) this.app.leaveChat(session);
      }
    };
    const many = sessions.length > 1;
    if (!agents && !archive) return this.app.notice(many ? "These chats have already ended." : "This chat has already ended.");
    if (!agents) return this.runAction(run);
    // An agent whose turn is over still has an open pane; only a working or waiting one is "running".
    const busy = sessions.flatMap((s) => this.app.agentsOf(s).agents).filter((a) => a.alive && (a.state === "working" || a.state === "waiting")).length;
    return this.ask({ ...endConfirm(sessions.length, this.app.chatTitle(sessions[0]), archive, agents, busy), run });
  }

  openSwitchModel(session: string) {
    // The run chip's popover lives in the open chat's composer.
    if (this.app.openId !== session) this.app.openChat(session);
    // A settled switch's result stays only until its chat's popover opens again.
    const { [session]: h, ...rest } = this.state.handoffs;
    this.set({ switchModel: session, handoffs: h && !isSettled(h.phase) ? this.state.handoffs : rest });
  }

  /** Switch model (ADR 0110). `resume` starts the chat's own CLI session again on the new run; a
   *  new chat takes this chat's summary, or nothing on `clean`. Progress comes from `handoff`
   *  frames; on success the chat that runs now opens. */
  async switchModel(old: string, provider: string, model: string, account: string | null, effort: string | null = null, carry: SwitchCarry = "summary") {
    const running = this.state.handoffs[old];
    if (running && !isSettled(running.phase)) return;
    // The server's frames own canCancel.
    this.setHandoff({ old, phase: "preparing", canCancel: true, error: null, carry });
    try {
      const done = await this.api.post<{ session: string }>("/api/chat/continue", {
        old_session: old,
        provider,
        model,
        carry,
        ...(account ? { account } : {}),
        ...(effort ? { effort } : {}),
      });
      const { [old]: _, ...rest } = this.state.handoffs;
      this.set({ handoffs: rest, ...(this.state.switchModel === old ? { switchModel: null } : {}) });
      await this.app.refresh();
      // The owner may have moved to another chat, also during the refresh; it stays open.
      if (this.state.switchModel === null && this.app.openId === old) this.app.openChat(done.session);
    } catch (err) {
      const cancelled = err instanceof ApiError && err.message === "model switch cancelled";
      // Only this chat's open popover shows the error; anywhere else it is a notice.
      if (this.state.switchModel !== old) this.app.notice(cancelled ? "Model switch cancelled." : message(err), !cancelled);
      this.setHandoff({ old, phase: cancelled ? "cancelled" : "failed", canCancel: false, error: cancelled ? null : message(err), carry });
    }
  }

  /** Resume: the chat's own CLI session starts again in this chat, with its whole context. */
  async resumeChat(session: string) {
    try {
      await this.api.post("/api/chat/resume", { session });
      await this.app.refresh();
    } catch (err) {
      this.app.notice(message(err), true);
    }
  }

  async cancelSwitch(old: string) {
    const h = this.state.handoffs[old];
    if (!h?.canCancel) return;
    try {
      await this.api.post("/api/chat/continue/cancel", { old_session: h.old });
    } catch (err) {
      this.app.notice(message(err), true);
    }
  }

  // ---- workspaces (lifecycle-workspaces.ts) ----

  private get workspaceActions(): WorkspaceActions {
    return { api: this.api, app: this.app, ask: (c) => this.ask(c), runAction: (run, leave) => this.runAction(run, null, leave) };
  }

  archiveWorkspace(workspace: string) {
    return archiveWorkspace(this.workspaceActions, workspace);
  }

  restoreWorkspace(workspace: string) {
    void this.runAction(() => this.api.post("/api/workspace/restore", { workspace }));
  }

  deleteWorkspace(workspace: string) {
    return deleteWorkspace(this.workspaceActions, workspace);
  }

  pruneWorkspace(workspace: string, project: string) {
    return pruneWorkspace(this.workspaceActions, workspace, project);
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

  ask(confirm: Confirm): Promise<void> {
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
