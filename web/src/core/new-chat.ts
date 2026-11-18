// The new chat composer and the chats it starts (ADR 0147). ⌘N opens the composer; ⌘Enter posts
// `/api/chat/new`, which answers with a start id at once, and `chat-start` frames report each step.

import type { App } from "./app";
import { firstLine, message } from "./app-types";
import type { Run } from "./composer";
import { setupFrame, type SetupRun } from "./lifecycle-state";
import { workspaceRow, type TreeView } from "./nav";

/** Where a new chat runs; `new` makes the chat its own worktree. */
export type Place = { kind: "new" } | { kind: "here" } | { kind: "branch"; branch: string } | { kind: "pr"; number: number } | { kind: "issue"; number: number };

/** A project the composer can start in: the folder a new chat names, and whether it is a git
 *  project, which can make a new worktree; a plain folder starts the chat in itself. */
export type Target = { cwd: string; project: string; title: string; git: boolean };

/** What a new worktree from this prompt gets, for the composer's card. */
export type Preview = { branch: string | null; folder: string | null; base: string | null; fetchedUnixS: number | null; setup: string | null };

/** The composer's run: a profile, and a run the owner picked instead of its default. */
export type ComposerRun = { role: string; run: Run | null };

/** One start step; `words` is the server's text for it. */
export type StartStep = { step: string; words: string | null; state: "running" | "completed" | "failed"; error: string | null };

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
  /** The profile and run it posted, so Edit and retry keeps them. */
  run: ComposerRun;
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
  composer: { cwd: string; place: Place; text: string; run: ComposerRun | null; seq: number; nav: number } | null;
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

/** How long the start's POST may take. The server answers in under a second, after its checks. */
const START_TIMEOUT_MS = 10_000;

/** Options of a start: whether the composer sent it, and the text that goes after its prompt. */
type StartOptions = { composer?: boolean; queued?: string[] };

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
  return {
    cwd,
    role: run.role,
    ...(text ? { prompt: text } : {}),
    place,
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

/** The most characters of a prompt's first line that a start's tab shows. */
const TITLE_CHARS = 60;

/** The tab title of a start: the prompt's first line, else New chat. It cuts whole characters. */
const startTitle = (prompt: string) => firstLine(prompt.trim(), TITLE_CHARS) || "New chat";

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
  open(at?: string, place?: Place, text = "", run: ComposerRun | null = null) {
    const list = targets(this.app.view.tree, at ?? this.app.view.workspace);
    const target = (at && list.find((t) => t.cwd === at || t.project === at)) || defaultTarget(list, this.app.view.tree, at ?? this.app.view.workspace);
    if (!target) return this.app.notice("Import or create a project first.");
    // ⌘N again on the same open composer keeps it, with its typed text.
    const open = this.composing();
    if (open && open.cwd === target.cwd && !place && !text) return;
    // The new composer gets every text that waits: the one passed, `unsent`, and the typed text of
    // a composer that it replaces, open or hidden by a move away.
    const waiting = [text, this.state.unsent, this.state.composer?.text ?? ""].filter((t) => t.trim());
    // The composer replaces a start's view; the start keeps its tab.
    const composer = { cwd: target.cwd, place: place ?? defaultPlace(target), text: [...new Set(waiting)].join("\n\n"), run, seq: ++this.seq, nav: this.app.navSeq };
    this.set({ composer, unsent: "", viewing: null });
  }

  /** The composer while the owner has not moved to another view since it opened. */
  composing() {
    const c = this.state.composer;
    return c && c.nav === this.app.navSeq ? c : null;
  }

  /** A failed start's Edit and retry: the composer again, with its text, folder and place. A ⌘T
   *  start's folder is no composer target, so it starts again as ⌘T did, with its queued text. */
  retry(id: string) {
    const start = this.state.starts[id];
    if (!start) return;
    const composed = targets(this.app.view.tree, start.cwd).some((t) => t.cwd === start.cwd);
    if (composed) {
      // The start's text waits in `unsent`, and open() gives it to the composer.
      this.dismiss(id);
      return this.open(start.cwd, start.place, "", start.run);
    }

    const rest = [...start.queued, start.draft.trim()].filter(Boolean);
    this.remove(id, false);
    // Fire-and-forget is safe: start() shows a notice for a failed POST, and the text then waits
    // in `unsent`, joined to any text there.
    void this.start(start.cwd, start.prompt, start.place, start.run, { queued: rest }).then((ok) => {
      if (!ok) this.set({ unsent: [this.state.unsent, start.prompt, ...rest].filter(Boolean).join("\n\n") });
    });
  }

  /** The composer's text as the owner types, so a move away does not lose it; no render needs it.
   *  The start page's composer has no `composer` state, so its text waits in `unsent`. */
  typed(text: string) {
    // A hidden composer keeps its own text; open() joins it to `unsent` later.
    const open = this.composing();
    if (open) open.text = text;
    else this.state.unsent = text;
  }

  /** The card's read of what place `new` in `cwd` makes from `prompt`. */
  preview(cwd: string, prompt: string) {
    return this.app.api.get<Preview>("/api/chat/new-preview", { cwd, prompt });
  }

  /** Closes the composer; its `text` waits for the next one. */
  close(text = "") {
    if (this.state.composer) this.set({ composer: null, unsent: text.trim() ? text : "" });
  }

  /** Posts the start and shows its tab at once; the steps come as `chat-start` frames. Only a
   *  start that the composer sent closes it and takes its text. */
  async start(cwd: string, prompt: string, place: Place, run: ComposerRun, { composer: fromComposer = false, queued = [] }: StartOptions = {}): Promise<boolean> {
    // A second press while one start is posted would start a second chat.
    if (this.posting) return false;
    this.posting = true;
    // Read before the wait: a move during the POST then shows the start in the old workspace's
    // strip and not over the view the owner moved to.
    const from = this.app.view.workspace ?? cwd;
    const nav = this.app.navSeq;
    const sent = this.state.composer?.seq;

    let id: string;
    try {
      ({ start: id } = await this.app.api.post<{ start: string }>("/api/chat/new", startBody(cwd, prompt, place, run), AbortSignal.timeout(START_TIMEOUT_MS)));
    } catch (err) {
      // The server may have started the chat before the wait ended, so the notice says to look first.
      const timedOut = err instanceof DOMException && err.name === "TimeoutError";
      this.app.notice(timedOut ? `The server did not answer in ${START_TIMEOUT_MS / 1000} s. Check the sidebar for the chat before you press ⌘Enter again.` : message(err), true);
      return false;
    } finally {
      this.posting = false;
    }

    const start: Starting = { id, cwd, place, from, title: startTitle(prompt), prompt: prompt.trim(), run, steps: [], workspace: null, branch: null, setup: null, error: null, queued, draft: "" };
    // A composer the owner opened while this one posted stays open, with its text.
    const closes = fromComposer && this.state.composer?.seq === sent;
    const composer = closes ? null : this.state.composer;
    // The page composer has no `composer` state, and its text in `unsent` is the prompt it sent.
    const unsent = fromComposer && !composer ? "" : this.state.unsent;
    this.set({ composer, unsent, starts: { ...this.state.starts, [id]: start }, viewing: { id, nav } });
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

  /** Drops a start's tab. A failed start's prompt, queued text and draft wait for the next
   *  composer, so none is lost. */
  dismiss(id: string) {
    this.remove(id, Boolean(this.state.starts[id]?.error));
  }

  /** Drops a start's tab; with `keep`, its text waits in `unsent`. */
  private remove(id: string, keep: boolean) {
    const { [id]: gone, ...starts } = this.state.starts;
    const kept = keep && gone ? [gone.prompt, ...gone.queued, gone.draft.trim()] : [];
    const unsent = [this.state.unsent, ...kept].filter(Boolean).join("\n\n");
    this.set({ starts, unsent, viewing: this.state.viewing?.id === id ? null : this.state.viewing });
  }

  /** A `setup` or `chat-start` frame; the promise ends when a started chat has opened and its
   *  queued text went. It never rejects: a failure shows a notice. */
  async onFrame(event: "setup" | "chat-start", data: Record<string, unknown>): Promise<void> {
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
        // A failed start always has an error text, as the page tells a failed start by it.
        const text = error || "The chat did not start.";
        this.setStart(id, { error: text });
        this.app.notice(text, true);
        return;
      }
      return this.opened(start, String(data.session));
    }

    const words = typeof data.words === "string" ? data.words : null;
    const steps = [...start.steps.filter((s) => s.step !== step), { step, words, state, error } as StartStep];
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
    // The chat runs, but its first prompt or the issue's text did not go: nothing goes before it,
    // and the chat's text box keeps the rest.
    const failed = start.steps.find((s) => s.step === "send" && s.state === "failed");
    this.dismiss(start.id);
    await this.app.actions.openNew(session, shown).catch((err) => this.app.notice(message(err), true));

    if (failed) {
      const kept = [start.prompt, ...texts].filter(Boolean).join("\n\n");
      if (kept) this.app.composing.setDraft(session, kept);
      const what = start.prompt ? "Your first prompt" : "The issue's text";
      const where = kept ? " Your text is in the chat's text box." : "";
      return this.app.notice(`${what} did not send: ${failed.error ?? "the send failed"}.${where}`, true);
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
