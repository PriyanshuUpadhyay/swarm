// The new chat composer and the chats it starts (ADR 0147). ⌘N opens the composer; ⌘Enter posts
// `/api/chat/new`, which answers with a start id at once, and `chat-start` frames report each step.

import type { App } from "./app";
import { message } from "./app-types";
import type { Run } from "./composer";
import { setupFrame, type SetupRun } from "./lifecycle-state";
import { workspaceRow, type TreeView } from "./nav";

/** Where a new chat runs; `new` makes the chat its own worktree. */
export type Place = { kind: "new" } | { kind: "here" } | { kind: "branch"; branch: string } | { kind: "pr"; number: number } | { kind: "issue"; number: number; title?: string };

/** A project the composer can start in: the folder a new chat names, and whether it is a git
 *  project, which can make a new worktree; a plain folder starts the chat in itself. */
export type Target = { cwd: string; project: string; title: string; git: boolean };

/** The composer's run: a profile, and a run the owner picked instead of its default. */
export type ComposerRun = { role: string; run: Run | null };

export type StartStep = { step: string; state: "running" | "completed" | "failed"; error: string | null };

/** A chat that is starting: its steps, its setup output, and text to send after the first prompt. */
export type Starting = {
  id: string;
  /** The folder and place the start posted, so Edit and retry opens the composer on them. */
  cwd: string;
  place: Place;
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
  /** Text in the start view's box that the owner has not queued yet; it goes too. */
  draft: string;
};

export type NewChatState = {
  /** The open composer, and the app's navigation count then; any move away hides it. */
  composer: { cwd: string; place: Place; text: string; seq: number; nav: number } | null;
  /** The prompt of a composer that Escape closed, which the next one starts with. */
  unsent: string;
  starts: Record<string, Starting>;
  /** The start on screen, and the app's navigation count then; any move away hides it. */
  viewing: { id: string; nav: number } | null;
};

/** Whether the view shows the composer in its page: Home, and a workspace's landing that takes
 *  new chats. Elsewhere ⌘N opens it over the view. */
export const composerInPage = (view: { openId: string | null; workspace: string | null; tree: TreeView | null }) =>
  !view.openId && (!view.workspace || workspaceRow(view.tree, view.workspace)?.newChatEnabled === true);

/** The profile a new chat starts on unless the owner picks another. */
export const DEFAULT_ROLE = "chat";

/** The words of each start step that the server sends. */
export const STEP_WORDS: Record<string, string> = { worktree: "Make the worktree", setup: "Run the setup script", start: "Start the agent", send: "Send your prompt" };

/** How long the start's POST may take. The server answers in under a second, after its checks. */
const START_TIMEOUT_MS = 10_000;

/** How many frames of unknown starts wait for their POST reply. */
const EARLY_KEPT = 50;

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
      return cwd ? [{ cwd, project: s.path ?? cwd, title: s.title, git: !s.folder }] : [];
    });
}

/** The project the composer opens on: the one of `workspace`, else the first. */
export function defaultTarget(list: Target[], tree: TreeView | null, workspace: string | null): Target | null {
  const section = tree?.sections.find((s) => s.kind === "project" && s.rows.some((r) => r.id === workspace));
  return list.find((t) => section?.rows.some((r) => r.id === t.cwd)) ?? list[0] ?? null;
}

/** The place a target starts on unless the owner picks another. */
export const defaultPlace = (target: Target): Place => ({ kind: target.git ? "new" : "here" });

/** The `/api/chat/new` body for a composer send. An issue's own text goes first on the server. */
function startBody(cwd: string, prompt: string, place: Place, run: ComposerRun) {
  const text = prompt.trim();
  // An issue's title is only for the chip; the server reads the issue itself.
  const where = place.kind === "issue" ? { kind: place.kind, number: place.number } : place;
  return {
    cwd,
    role: run.role,
    ...(text ? { prompt: text } : {}),
    place: where,
    ...(run.run ? { provider: run.run.provider, model: run.run.model ?? undefined, effort: run.run.effort ?? undefined, account: run.run.account ?? undefined } : {}),
  };
}

/** The chip's words for a place. */
export function placeLabel(place: Place, target: Target): string {
  if (!target.git) return "In folder";

  switch (place.kind) {
    case "new":
      return "New worktree";
    case "here":
      return "In this folder";
    case "branch":
      return place.branch ? `Branch ${place.branch}` : "Pick a branch";
    case "pr":
      return place.number ? `PR #${place.number}` : "Pick a pull request";
    case "issue":
      return place.number ? `Issue #${place.number}` : "Pick an issue";
    default: {
      const _exhaustive: never = place;
      return _exhaustive;
    }
  }
}

/** Whether a send can go with this place: a branch, PR or issue needs its pick. */
export function placeReady(place: Place): boolean {
  switch (place.kind) {
    case "new":
    case "here":
      return true;
    case "branch":
      return place.branch !== "";
    case "pr":
    case "issue":
      return place.number > 0;
    default: {
      const _exhaustive: never = place;
      return _exhaustive;
    }
  }
}

/** The tab title of a start: the prompt's first line, else New chat. It cuts whole characters. */
const startTitle = (prompt: string) => Array.from(prompt.trim().split("\n")[0]).slice(0, 60).join("") || "New chat";

export class NewChat {
  state: NewChatState = { composer: null, unsent: "", starts: {}, viewing: null };
  private listeners = new Set<() => void>();
  private seq = 0;
  private posting = false;
  /** `chat-start` frames that came before the POST reply named their start. */
  private early: Record<string, unknown>[] = [];

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
  open(at?: string, place?: Place, text = "") {
    const list = targets(this.app.view.tree, at ?? this.app.view.workspace);
    const target = (at && list.find((t) => t.cwd === at || t.project === at)) || defaultTarget(list, this.app.view.tree, at ?? this.app.view.workspace);
    if (!target) return this.app.notice("Import or create a project first.");
    // ⌘N again on the same open composer keeps it, with its typed text.
    const open = this.composing();
    if (open && open.cwd === target.cwd && !place && !text) return;
    // The composer replaces a start's view; the start keeps its tab.
    const composer = { cwd: target.cwd, place: place ?? defaultPlace(target), text: text || this.state.unsent, seq: ++this.seq, nav: this.app.navSeq };
    this.set({ composer, unsent: "", viewing: null });
  }

  /** The composer while the owner has not moved to another view since it opened. */
  composing() {
    const c = this.state.composer;
    return c && c.nav === this.app.navSeq ? c : null;
  }

  /** A failed start's Edit and retry: the composer again, with its prompt, folder and place. */
  retry(id: string) {
    const start = this.state.starts[id];
    if (!start) return;
    this.dismiss(id);
    this.open(start.cwd, start.place, start.prompt);
  }

  /** Closes the composer; its `text` waits for the next one. */
  close(text = "") {
    if (this.state.composer) this.set({ composer: null, unsent: text.trim() ? text : "" });
  }

  /** Posts the start and shows its tab at once; the steps come as `chat-start` frames. */
  async start(cwd: string, prompt: string, place: Place, run: ComposerRun): Promise<boolean> {
    // A second press while one start is posted would start a second chat.
    if (this.posting) return false;
    this.posting = true;
    let id: string;
    try {
      ({ start: id } = await this.app.api.post<{ start: string }>("/api/chat/new", startBody(cwd, prompt, place, run), AbortSignal.timeout(START_TIMEOUT_MS)));
    } catch (err) {
      // The server may have started the chat before the wait ended, so the notice says to look first.
      const timedOut = err instanceof DOMException && err.name === "TimeoutError";
      this.app.notice(timedOut ? "The server did not answer in 10 s. Check the sidebar for the chat before you press ⌘Enter again." : message(err), true);
      return false;
    } finally {
      this.posting = false;
    }

    const from = this.app.view.workspace ?? cwd;
    const start: Starting = { id, cwd, place, from, title: startTitle(prompt), prompt: prompt.trim(), steps: [], workspace: null, branch: null, setup: null, error: null, queued: [], draft: "" };
    this.set({ composer: null, starts: { ...this.state.starts, [id]: start }, viewing: { id, nav: this.app.navSeq } });
    const early = this.early.filter((data) => data.start === id);
    this.early = this.early.filter((data) => data.start !== id);
    for (const data of early) void this.onFrame("chat-start", data);
    return true;
  }

  /** After the stream reconnects: a start whose `done` frame was lost reads it again. */
  async recheck() {
    for (const start of Object.values(this.state.starts)) {
      if (start.error) continue;
      try {
        const { done, running } = await this.app.api.get<{ done?: Record<string, unknown>; running?: boolean }>("/api/chat/start", { id: start.id });
        if (done) void this.onFrame("chat-start", done);
        // A restarted server, or one past its last 64 starts, no longer knows the start.
        else if (!running) this.setStart(start.id, { error: "The server no longer knows this start. Look for the chat in the sidebar." });
      } catch {
        // The next reconnect reads it again.
      }
    }
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
  queue(id: string) {
    const start = this.state.starts[id];
    if (start?.draft.trim()) this.setStart(id, { queued: [...start.queued, start.draft.trim()], draft: "" });
  }

  setDraft(id: string, draft: string) {
    this.setStart(id, { draft });
  }

  /** Drops a start's tab. A failed start's prompt waits for the next composer, so it is not lost. */
  dismiss(id: string) {
    const { [id]: gone, ...starts } = this.state.starts;
    const unsent = gone?.error && gone.prompt ? gone.prompt : this.state.unsent;
    this.set({ starts, unsent, viewing: this.state.viewing?.id === id ? null : this.state.viewing });
  }

  /** A `setup` or `chat-start` frame; the promise ends when a started chat has opened and its
   *  queued text went. It never rejects: a failure shows a notice. */
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
    if (event !== "chat-start") return;

    if (!start) {
      // The frame can come before the POST reply; start() takes it then.
      this.early = [...this.early, data].slice(-EARLY_KEPT);
      return;
    }

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

  /** The chat runs: open it where the owner still looks at its start, then send the queued text
   *  and the box's draft. Text that did not send goes in the chat's text box, so none is lost. */
  private async opened(start: Starting, session: string) {
    const shown = this.shown()?.id === start.id;
    const texts = [...start.queued, start.draft.trim()].filter(Boolean);
    // The chat runs, but its first prompt did not go: nothing goes before it, and a notice keeps it all.
    const failed = start.prompt ? start.steps.find((s) => s.step === "send" && s.state === "failed") : undefined;
    this.dismiss(start.id);
    await this.app.actions.openNew(session, shown).catch((err) => this.app.notice(message(err), true));

    if (failed) {
      this.app.composing.setDraft(session, [start.prompt, ...texts].join("\n\n"));
      return this.app.notice(`Your first prompt did not send: ${failed.error ?? "the send failed"}. It is in the chat's text box.`, true);
    }

    for (const [i, text] of texts.entries()) {
      try {
        await this.app.api.action("/api/chat/send", { session, agent: "chair", text });
      } catch (err) {
        this.app.composing.setDraft(session, texts.slice(i).join("\n\n"));
        this.app.notice(`Your next message did not send: ${message(err)}. It is in the chat's text box.`, true);
        return;
      }
    }
  }
}
