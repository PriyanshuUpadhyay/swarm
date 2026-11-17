// The new chat composer and the chats it starts (ADR 0147). ⌘N opens the composer; ⌘Enter posts
// `/api/chat/new`, which answers with a start id at once, and `chat-start` frames report each step.

import type { App } from "./app";
import { message } from "./app-types";
import type { Run } from "./composer";
import { setupFrame, type SetupRun } from "./lifecycle-state";
import type { TreeView } from "./nav";

/** Where a new chat runs; `new` makes the chat its own worktree. */
export type Place = { kind: "new" } | { kind: "here" } | { kind: "branch"; branch: string } | { kind: "pr"; number: number } | { kind: "issue"; number: number; title?: string };

/** A project the composer can start in: the folder a new chat names, and what it offers. */
export type Target = { cwd: string; project: string; title: string; git: boolean; worktree: boolean };

/** The composer's run: a profile, and a run the owner picked instead of its default. */
export type ComposerRun = { role: string; run: Run | null };

export type StartStep = { step: string; state: "running" | "completed" | "failed"; error: string | null };

/** A chat that is starting: its steps, its setup output, and text to send after the first prompt. */
export type Starting = {
  id: string;
  /** The workspace whose tab strip shows it. */
  from: string | null;
  title: string;
  prompt: string;
  steps: StartStep[];
  workspace: string | null;
  branch: string | null;
  setup: SetupRun | null;
  error: string | null;
  queued: string[];
};

export type NewChatState = {
  composer: { cwd: string; place: Place; seq: number } | null;
  starts: Record<string, Starting>;
  /** The start on screen, and the app's navigation count then; any move away hides it. */
  viewing: { id: string; nav: number } | null;
};

export const STEP_WORDS: Record<string, string> = { worktree: "Make the worktree", setup: "Run the setup script", start: "Start the agent", send: "Send your prompt" };

/** The projects the composer offers, each with the folder a new chat in it names: a hub's main
 *  checkout, else the open workspace, else the project's own folder. A git project can make a new worktree; a plain
 *  folder starts the chat in itself. */
export function targets(tree: TreeView | null, workspace: string | null): Target[] {
  return (tree?.sections ?? [])
    .filter((s) => s.kind === "project")
    .flatMap((s) => {
      const rows = s.rows.filter((r) => r.kind === "workspace" && r.newChatEnabled);
      const main = rows.find((r) => r.startsChatWorktree);
      const cwd = main?.id ?? rows.find((r) => r.id === workspace)?.id ?? (s.folder ? rows[0]?.id : s.path) ?? null;
      return cwd ? [{ cwd, project: s.path ?? cwd, title: s.title, git: !s.folder, worktree: !s.folder }] : [];
    });
}

/** The project the composer opens on: the one of `workspace`, else the first. */
export function defaultTarget(list: Target[], tree: TreeView | null, workspace: string | null): Target | null {
  const section = tree?.sections.find((s) => s.kind === "project" && s.rows.some((r) => r.id === workspace));
  return list.find((t) => section?.rows.some((r) => r.id === t.cwd)) ?? list[0] ?? null;
}

/** The place a target starts on unless the owner picks another. */
export const defaultPlace = (target: Target | null): Place => ({ kind: target?.worktree ? "new" : "here" });

/** The `/api/chat/new` body for a composer send. An issue's own text goes first on the server. */
export function startBody(cwd: string, prompt: string, place: Place, run: ComposerRun) {
  const text = prompt.trim();
  const { kind } = place;
  const where = kind === "branch" ? { kind, branch: place.branch } : kind === "pr" || kind === "issue" ? { kind, number: place.number } : { kind };
  return {
    cwd,
    role: run.role,
    ...(text ? { prompt: text } : {}),
    place: where,
    ...(run.run ? { provider: run.run.provider, model: run.run.model ?? undefined, effort: run.run.effort ?? undefined, account: run.run.account ?? undefined } : {}),
  };
}

/** The tab title of a start: the prompt's first line, else New chat. */
export const startTitle = (prompt: string) => prompt.trim().split("\n")[0].slice(0, 60) || "New chat";

export class NewChat {
  state: NewChatState = { composer: null, starts: {}, viewing: null };
  private listeners = new Set<() => void>();
  private seq = 0;

  constructor(private readonly app: App) {}

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private set(change: Partial<NewChatState>) {
    this.state = { ...this.state, ...change };
    for (const fn of this.listeners) fn();
  }

  private setStart(id: string, change: Partial<Starting>) {
    const start = this.state.starts[id];
    if (start) this.set({ starts: { ...this.state.starts, [id]: { ...start, ...change } } });
  }

  /** ⌘N, New chat, a row "+" and the tab "+": the composer on the project of `at` (a workspace or
   *  a project path), else on the open one. */
  open(at?: string, place?: Place) {
    const list = targets(this.app.view.tree, at ?? this.app.view.workspace);
    const target = (at && list.find((t) => t.cwd === at || t.project === at)) || defaultTarget(list, this.app.view.tree, at ?? this.app.view.workspace);
    if (!target) return this.app.notice("Import or create a project first.");
    // The composer replaces a start's view; the start keeps its tab.
    this.set({ composer: { cwd: target.cwd, place: place ?? defaultPlace(target), seq: ++this.seq }, viewing: null });
  }

  close() {
    if (this.state.composer) this.set({ composer: null });
  }

  /** Posts the start and shows its tab at once; the steps come as `chat-start` frames. */
  async start(cwd: string, prompt: string, place: Place, run: ComposerRun): Promise<boolean> {
    let id: string;
    try {
      ({ start: id } = await this.app.api.post<{ start: string }>("/api/chat/new", startBody(cwd, prompt, place, run)));
    } catch (err) {
      this.app.notice(message(err), true);
      return false;
    }
    const from = this.app.view.workspace ?? cwd;
    const start: Starting = { id, from, title: startTitle(prompt), prompt: prompt.trim(), steps: [], workspace: null, branch: null, setup: null, error: null, queued: [] };
    this.set({ composer: null, starts: { ...this.state.starts, [id]: start }, viewing: { id, nav: this.app.navSeq } });
    return true;
  }

  /** Back to the open chat, from a start's view. */
  hide() {
    if (this.state.viewing) this.set({ viewing: null });
  }

  /** Shows a start, from its tab. */
  view(id: string) {
    if (this.state.starts[id]) this.set({ viewing: { id, nav: this.app.navSeq } });
  }

  /** The start on screen, while the owner has not moved away from it. */
  shown(): Starting | null {
    const v = this.state.viewing;
    if (!v || v.nav !== this.app.navSeq) return null;
    return this.state.starts[v.id] ?? null;
  }

  /** Text typed while the chat starts; it goes after the first prompt. */
  queue(id: string, text: string) {
    const start = this.state.starts[id];
    if (start && text.trim()) this.setStart(id, { queued: [...start.queued, text.trim()] });
  }

  /** Drops a failed start's tab. */
  dismiss(id: string) {
    const { [id]: _, ...starts } = this.state.starts;
    this.set({ starts, viewing: this.state.viewing?.id === id ? null : this.state.viewing });
  }

  /** A `setup` or `chat-start` frame; the promise ends when a started chat has opened. */
  onFrame(event: string, data: Record<string, unknown>): Promise<void> | undefined {
    if (event === "setup") {
      for (const start of Object.values(this.state.starts)) {
        if (start.workspace !== data.workspace) continue;
        const setup = setupFrame(start.setup, data);
        if (setup !== start.setup) this.setStart(start.id, { setup });
      }
      return;
    }
    const id = String(data.start);
    const start = this.state.starts[id];
    if (event !== "chat-start" || !start) return;
    const step = String(data.step);
    const state = data.state === "completed" || data.state === "failed" ? data.state : "running";
    const error = typeof data.error === "string" ? data.error : null;
    if (step === "done") {
      if (state === "failed") {
        this.setStart(id, { error });
        this.app.notice(error ?? "The chat did not start.", true);
        return;
      }
      return this.opened(start, String(data.session));
    }
    const steps = [...start.steps.filter((s) => s.step !== step), { step, state, error } as StartStep];
    this.setStart(id, {
      steps,
      ...(typeof data.workspace === "string" ? { workspace: data.workspace } : {}),
      ...(typeof data.branch === "string" ? { branch: data.branch } : {}),
    });
  }

  /** The chat runs: open it where the owner still looks at its start, then send the queued text. */
  private async opened(start: Starting, session: string) {
    const shown = this.shown()?.id === start.id;
    const queued = this.state.starts[start.id]?.queued ?? [];
    this.dismiss(start.id);
    await this.app.actions.openNew(session, shown);
    for (const text of queued) await this.app.api.action("/api/chat/send", { session, agent: "chair", text });
  }
}
