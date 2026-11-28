// The new chat composer and the chats it starts (ADR 0149). ⌘N opens the composer; ⌘Enter posts
// `/api/chat/new`, which answers with a start id at once, and `chat-start` frames report each step.

import type { App } from "./app";
import { firstLine, message } from "./app-types";
import { hasPendingImage, type Draft } from "./images";
import type { Run } from "./composer";
import type { Prefs } from "./nav";
import { setupFrame, type SetupRun } from "./lifecycle-state";
import { fileRows, type CommandList } from "./slash";
import { canStartIn, defaultPlace, defaultTarget, targets, type Place, type Preview } from "./new-chat-places";

export * from "./new-chat-places";

/** A start's profile, and a run the owner picked instead of its default; with no role, the
 *  server starts its default profile. */
export type ComposerRun = { role?: string; run: Run | null };

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

type NewChatState = {
  /** The open composer, and the app's navigation count then; any move away hides it. */
  composer: { cwd: string; place: Place; text: string; run: ComposerRun | null; seq: number; nav: number } | null;
  /** The prompt of a composer that Escape closed, which the next one starts with. */
  unsent: string;
  starts: Record<string, Starting>;
  /** The start on screen, and the app's navigation count then; any move away hides it. */
  viewing: { id: string; nav: number } | null;
};

/** The profile pill the composer picks first, else the first profile. ⌘T sends no profile, so
 *  the server's default holds there. */
export const DEFAULT_ROLE = "chat";

export function defaultRun(prefs: Pick<Prefs, "newChatModel"> | undefined): Run | null {
  return prefs?.newChatModel ? { ...prefs.newChatModel, effort: null, account: null } : null;
}

/** How long the start's POST may take. The server answers in under a second, after its checks. */
const START_TIMEOUT_MS = 10_000;

type StartOptions = {
  composer?: boolean;
  useSavedDefault?: boolean;
  submittedText?: string;
  queued?: string[];
  /** Lets an upload finish in the start's draft after the composer closes. */
  bindUpload?: (draft: Draft) => void;
};

/** How many frames of unknown starts wait for their POST reply. */
const EARLY_KEPT = 50;

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
  /** Starts whose chat opens now; a second `done`, as a reconnect's recheck reads, is dropped. */
  private opening = new Set<string>();

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
  open(at?: string, place?: Place, run: ComposerRun | null = null) {
    const list = targets(this.app.view.tree, at ?? this.app.view.workspace);
    // A project of the tree with no folder to start in must not open the composer on another one.
    const project = this.app.view.tree?.projects.some((p) => p.path === at);
    if (project && !canStartIn(this.app.view.tree, at ?? null)) return this.app.notice("This project has no folder that a new chat can start in.");
    const target = (at && list.find((t) => t.cwd === at || t.project === at)) || defaultTarget(list, this.app.view.tree, at ?? this.app.view.workspace);
    if (!target) return this.app.notice("Import or create a project first.");
    // ⌘N again on the same open composer keeps it, with its typed text.
    const open = this.composing();
    if (open && open.cwd === target.cwd && !place) return;
    // The new composer gets every text that waits: `unsent`, and the typed text of a composer
    // that it replaces, open or hidden by a move away.
    const waiting = [this.state.unsent, this.state.composer?.text ?? ""].filter((t) => t.trim());
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
      return this.open(start.cwd, start.place, start.run);
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
  preview(cwd: string) {
    return this.app.api.get<Preview>("/api/chat/new-preview", { cwd });
  }

  /** The `/` menu rows that `provider` offers in `cwd`, before a chat exists (ADR 0155). */
  commands(cwd: string, provider: string) {
    return this.app.api.get<CommandList>("/api/chat/new-commands", { cwd, provider });
  }

  /** The `@` menu rows: the files of `cwd`. */
  async files(cwd: string) {
    const { files, note } = await this.app.api.get<{ files: string[]; note: string | null }>("/api/chat/new-files", { cwd });
    return fileRows(files, note);
  }

  /** Closes the composer; its `text` waits for the next one. A page composer's Escape also ends
   *  a composer that a move away hid, so that one's text waits too. */
  close(text = "") {
    const composer = this.state.composer;
    if (!composer) return;
    const hidden = this.composing() ? "" : composer.text;
    this.set({ composer: null, unsent: [text, hidden].filter((t) => t.trim()).join("\n\n") });
  }

  /** Posts the start and shows its tab at once; the steps come as `chat-start` frames. Only a
   *  start that the composer sent closes it and takes its text. */
  async start(cwd: string, prompt: string, place: Place, run: ComposerRun, { composer: fromComposer = false, useSavedDefault = !run.run && run.role === undefined, submittedText, queued = [], bindUpload }: StartOptions = {}): Promise<boolean> {
    const saved = useSavedDefault ? defaultRun(this.app.tree?.prefs) : null;
    if (useSavedDefault) run = { ...run, run: saved };
    // A second press while one start is posted would start a second chat.
    if (this.posting) return false;
    const models = saved?.model ? this.app.life.runs.loadedModels(saved.provider) : undefined;
    if (models?.length && !models.some((model) => model.id === saved?.model)) {
      if (!fromComposer) this.open(cwd, place);
      this.app.notice("The default model for new chats is no longer listed. Reset it in the run picker or pick a new model.", true);
      return false;
    }

    this.posting = true;
    // Read before the wait: a move during the POST then shows the start in the old workspace's
    // strip and not over the view the owner moved to.
    const from = this.app.view.workspace ?? cwd;
    const nav = this.app.navSeq;
    // The composer on screen; a send from the page's composer leaves a hidden one and its text.
    const sent = this.composing();
    const submitted = submittedText ?? sent?.text ?? this.state.unsent;

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

    // A composer the owner opened while this one posted keeps its own text.
    const closes = fromComposer && sent !== null && this.state.composer?.seq === sent.seq;
    const fromPage = fromComposer && sent === null;
    const latest = closes ? sent.text : fromPage ? this.state.unsent : submitted;
    const added = latest.startsWith(submitted) ? latest.slice(submitted.length).trimStart() : latest;
    const start: Starting = { id, cwd, place, from, title: startTitle(prompt), prompt: prompt.trim(), run, steps: [], workspace: null, branch: null, setup: null, error: null, queued, draft: added };
    const composer = closes ? null : this.state.composer;
    const unsent = fromPage ? "" : this.state.unsent;

    if (hasPendingImage(added)) {
      bindUpload?.({
        read: () => this.state.starts[id]?.draft ?? "",
        change: (next) => this.setDraft(id, next),
      });
    }

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
      if (this.opening.has(id)) return;
      this.opening.add(id);
      const sendError = typeof data.sendError === "string" ? data.sendError : undefined;
      return this.opened(start, String(data.session), sendError);
    }

    const words = typeof data.words === "string" ? data.words : null;
    const steps = [...start.steps.filter((s) => s.step !== step), { step, words, state, error } as StartStep];
    this.setStart(id, {
      steps,
      ...(typeof data.workspace === "string" ? { workspace: data.workspace } : {}),
      ...(typeof data.branch === "string" ? { branch: data.branch } : {}),
    });
  }

  /** Resolves when no image upload waits in the start's box. An upload always ends by putting its
   *  path or nothing in place of its marker, so the send never carries the marker. */
  private uploaded(id: string) {
    const waits = () => hasPendingImage(this.state.starts[id]?.draft ?? "");
    return new Promise<void>((done) => {
      if (!waits()) return done();
      const stop = this.subscribe(() => {
        if (waits()) return;
        stop();
        done();
      });
    });
  }

  /** The chat runs: open it where the owner still looks at its start, then send the queued text
   *  and the box's draft. Text that did not send goes in the chat's text box, so none is lost. */
  private async opened(start: Starting, session: string, sendError?: string) {
    // The chat runs, but its first prompt or the issue's text did not go: nothing goes before it,
    // and the chat's text box keeps the rest.
    const failed = start.steps.find((s) => s.step === "send" && s.state === "failed");
    // The start's view stays while the tree lists the chat, so the page behind does not show.
    await this.app.actions.openNew(session, () => this.shown()?.id === start.id).catch((err) => this.app.notice(message(err), true));
    await this.uploaded(start.id);
    const now = this.state.starts[start.id] ?? start;
    const texts = [...now.queued, now.draft.trim()].filter(Boolean);
    this.dismiss(start.id);
    this.opening.delete(start.id);

    if (sendError || failed) {
      const kept = [start.prompt, ...texts].filter(Boolean).join("\n\n");
      if (kept) this.app.composing.setDraft(session, kept);
      const what = start.prompt ? "Your first prompt" : "The issue's text";
      const where = kept ? " Your text is in the chat's text box." : "";
      return this.app.notice(`${what} did not send: ${sendError ?? failed?.error ?? "the send failed"}.${where}`, true);
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
