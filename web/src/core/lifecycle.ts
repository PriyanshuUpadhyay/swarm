// Workspace and chat lifecycle (S3): projects, the
// composer's place lists, switch model, archive, delete, prune and Archived chats, on the P3a, P3b
// and S2 routes. Framework-free; the dialogs read `state`, which is a new object after each change.

import { ApiError, sleep, type Snapshot } from "./api";
import type { App } from "./app";
import { isChair } from "./panes";
import { RunCatalog } from "./run-catalog";
import { chooseModel, endedCarry, savedCarry, switchOutcome, type Run, type SwitchCarry } from "./composer";
import {
  type NewOptions,
  type Issue,
  type Confirm,
  type ClosedChat,
  type ImportableChat,
  type Profile,
  type LifeState,
  type Handoff,
  isSettled,
  endConfirm,
} from "./lifecycle-state";
import { message } from "./app-types";
import { archiveWorkspace, deleteWorkspace, pruneWorkspace, type WorkspaceActions } from "./lifecycle-workspaces";

export * from "./lifecycle-state";


export class Lifecycle {
  state: LifeState = { project: null, importChoice: null, projectPath: "", existing: null, setupScript: null, switchModel: null, handoffs: {}, confirm: null, closed: false, imports: false };
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

  /** A `handoff` stream frame. */
  onHandoff(data: Record<string, unknown>) {
    const h = this.state.handoffs[String(data.old_session)];
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

  // ---- the composer's place lists ----

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
      for (const { session, agents } of live) {
        this.clearRunPick("pendingSwitch", session);
        this.clearRunPick("endedRun", session);
        for (const agent of agents) await this.api.action("/api/agent/close", { session, agent });
      }
      if (archive) {
        await this.api.action("/api/sessions/archive", { ids: sessions });
        this.app.navigation.rememberArchived(sessions);
        for (const session of sessions) this.app.leaveChat(session);
      }
    };
    const many = sessions.length > 1;
    if (!agents && !archive) return this.app.notice(many ? "These chats have already ended." : "This chat has already ended.");
    if (!agents) return this.runAction(run);
    // An agent whose turn is over still has an open pane; only a working or blocked one is "running".
    const busy = sessions.flatMap((s) => this.app.agentsOf(s).agents).filter((a) => a.alive && (a.state === "working" || a.state === "blocked")).length;
    return this.ask({ ...endConfirm(sessions.length, this.app.chatTitle(sessions[0]), archive, agents, busy), run });
  }

  openSwitchModel(session: string) {
    // The run chip's popover lives in the open chat's composer.
    if (this.app.openId !== session) this.app.openChat(session);
    // A settled switch's result stays only until its chat's popover opens again.
    const { [session]: h, ...rest } = this.state.handoffs;
    this.set({ switchModel: session, handoffs: h && !isSettled(h.phase) ? this.state.handoffs : rest });
  }

  private isCurrentSource(session: string, snapshot = this.app.snapshot) {
    return snapshot?.sessions.some((s) => s.id === session) && !snapshot.sessions.some((s) => s.continuation_of === session);
  }

  reconcileRunPicks(snapshot: Snapshot) {
    for (const id of Object.keys(this.state.pendingSwitch ?? {})) {
      if (!this.isCurrentSource(id, snapshot) || this.app.composing.chair(id).unreachable) this.clearRunPick("pendingSwitch", id);
    }
    for (const id of Object.keys(this.state.endedRun ?? {})) {
      if (!this.isCurrentSource(id, snapshot)) this.clearRunPick("endedRun", id);
    }
  }

  private clearRunPick(field: "pendingSwitch" | "endedRun", session: string) {
    const picks = this.state[field];
    if (!picks?.[session]) return;
    const { [session]: _, ...rest } = picks;
    this.set({ [field]: rest });
  }

  runBusy(session: string) {
    const chair = this.app.composing.chair(session);
    return chair.running || (!chair.unreachable && !!chair.disabled) || this.app.composing.input(session).stopping;
  }

  pickRun(session: string, run: Run) {
    const chair = this.app.composing.chair(session);
    if (!chair.provider || !run.model || !this.isCurrentSource(session) || this.state.resuming?.[session]) return;
    if (chair.unreachable) {
      this.set({ endedRun: { ...this.state.endedRun, [session]: run } });
      return;
    }
    const current = { provider: chair.provider, model: chair.model, effort: chair.effort, account: chair.account };
    const outcome = switchOutcome(current, run);
    if (outcome === "none") return this.clearRunPick("pendingSwitch", session);
    if (this.runBusy(session)) {
      this.set({ pendingSwitch: { ...this.state.pendingSwitch, [session]: run } });
      return;
    }
    this.clearRunPick("pendingSwitch", session);
    // switchModel records the failure on the handoff or posts a notice.
    void this.switchModel(session, run.provider, run.model, run.account, run.effort, outcome === "resume" ? "resume" : savedCarry(this.app.tree?.prefs));
  }

  dismissSwitch(session: string) {
    this.clearRunPick("pendingSwitch", session);
  }

  confirmSwitch(session: string) {
    const run = this.state.pendingSwitch?.[session];
    if (!run || this.app.openId !== session || !this.isCurrentSource(session) || this.app.composing.chair(session).unreachable) {
      this.clearRunPick("pendingSwitch", session);
      return;
    }
    if (!this.runBusy(session)) this.pickRun(session, run);
  }

  async nextFavorite(session: string) {
    const chair = this.app.composing.chair(session);
    const { provider, model, effort, account } = chair;
    if (!provider || chair.unreachable) return;
    const favorites = (this.app.tree?.prefs?.favoriteModels ?? []).filter((m) => m.provider === provider);
    if (!favorites.length) return;

    try {
      const [{ providers }, { models }] = await Promise.all([this.runs.providers(), this.runs.models(provider)]);
      if (this.app.openId !== session || this.app.composing.chair(session).model !== model) return;
      const cli = providers.find((p) => p.id === provider);
      const usable = favorites.flatMap((m) => models.filter((listed) => listed.id === m.model));
      const index = usable.findIndex((m) => m.id === model);
      const next = usable[(index + 1) % usable.length];
      if (!cli || !next || next.id === model || !this.isCurrentSource(session)) return;
      const current = { provider, model, effort, account };
      const favoritePick = { session, label: next.label };
      this.set({ favoritePick });
      setTimeout(() => { if (this.state.favoritePick === favoritePick) this.set({ favoritePick: undefined }); }, 1800);
      this.pickRun(session, chooseModel(current, current, cli, next));
    } catch {
      // A failed catalog read cannot prove that a favourite is available.
    }
  }

  /** Native resume or linked handoff; the server owns progress and cancel. */
  async switchModel(old: string, provider: string, model: string, account: string | null, effort: string | null = null, carry: SwitchCarry = "file") {
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
      return true;
    } catch (err) {
      const cancelled = err instanceof ApiError && err.message === "model switch cancelled";
      // Only this chat's open popover shows the error; anywhere else it is a notice.
      if (this.state.switchModel !== old) this.app.notice(cancelled ? "Model switch cancelled." : message(err), !cancelled);
      this.setHandoff({ old, phase: cancelled ? "cancelled" : "failed", canCancel: false, error: cancelled ? null : message(err), carry });
      return false;
    }
  }

  /** An ended chat starts only on Resume, never while picking its next run. */
  async resumeChat(session: string) {
    if (this.state.resuming?.[session] || !this.isCurrentSource(session)) return;
    const chair = this.app.composing.chair(session);
    if (!chair.unreachable) return;
    const run = this.state.endedRun?.[session];
    const provider = run?.provider ?? chair.provider;
    if (!provider) return;
    const foreign = provider !== chair.provider;

    this.set({ resuming: { ...this.state.resuming, [session]: true }, switchModel: null });
    try {
      if (foreign || !chair.resumable) {
        const carry = foreign ? endedCarry(savedCarry(this.app.tree?.prefs)) : "clean";
        const next = run ?? chair;
        if (!await this.switchModel(session, provider, next.model ?? "default", next.account, next.effort, carry)) return;
      } else {
        await this.api.post("/api/chat/resume", {
          session,
          ...(run?.model ? { model: run.model } : {}),
          ...(run?.effort ? { effort: run.effort } : {}),
          ...(run?.account ? { account: run.account } : {}),
        });
        await this.app.refresh();
      }
      this.clearRunPick("endedRun", session);
    } catch (err) {
      this.app.notice(message(err), true);
    } finally {
      const { [session]: _, ...remaining } = this.state.resuming ?? {};
      this.set({ resuming: remaining });
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
