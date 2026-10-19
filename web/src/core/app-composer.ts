

import { agentInput, confirmSent, outgoing, sentRow, submission, type AgentInput, type Prompt, type SendKey } from "./composer";
import { recallOrder, recordSent, type HistoryEntry } from "./history";
import { editPicks, fileRows, validPicks, type CommandList, type SkillPick } from "./slash";
import {
  type Chat,
  type Input,
  type Composer,
  DRAFT_PREFIX,
  PICKS_PREFIX,
  HISTORY_PREFIX,
  STASH_PREFIX,
  message,
  readStore,
  writeStore,
} from "./app-types";
import type { App } from "./app";

export class AppComposer {
  constructor(private readonly app: App) {}

  composerView(chat: Chat, sendKey: SendKey): Composer {
    let queued = 0;
    for (let i = chat.rows.length - 1; i >= 0 && chat.rows[i].kind === "queued"; i--) queued++;
    return {
      id: chat.id,
      agent: this.chair(chat.id),
      state: { ...this.input(chat.id) },
      queued,
      usage: chat.usage,
      sendKey,
    };
  }

  // ---- composer ----

  /** The session and agent behind a chat id. An id with no chat is a chair's session id. */
  target(id: string): { session: string; agent: string } {
    const chat = this.app.chats.get(id);
    return chat ? { session: chat.sessionId, agent: chat.agent } : { session: id, agent: "chair" };
  }

  /** The chat's agent as the composer sees it, from the last snapshot: the chair, or a child. */
  chair(id: string): AgentInput {
    const { session, agent } = this.target(id);
    return agentInput(
      this.app.snapshot?.sessions.find((s) => s.id === session),
      this.app.snapshot?.agents[session],
      agent === "chair" ? null : agent,
    );
  }

  input(id: string): Input {
    let input = this.app.inputs.get(id);
    if (!input) {
      input = { sending: false, stopping: false, pulling: false, answering: null, alreadySent: false, banner: null, draftRev: 0 };
      this.app.inputs.set(id, input);
    }
    return input;
  }

  update(id: string, change: Partial<Input>) {
    Object.assign(this.input(id), change);
    this.app.inputRev++;
    if (this.target(id).session === this.app.openId) this.app.notify();
  }

  /** Each chat keeps its draft, also across a reload (ComposerDraftStore). */
  draft(sessionId: string): string {
    let d = this.app.drafts.get(sessionId);
    if (d === undefined) this.app.drafts.set(sessionId, (d = readStore(DRAFT_PREFIX + sessionId) ?? ""));
    return d;
  }

  /** Pick ranges persist beside the draft text; old drafts need no migration. */
  draftPicks(sessionId: string): SkillPick[] {
    let picks = this.app.picks.get(sessionId);
    if (!picks) {
      try {
        const stored = JSON.parse(readStore(PICKS_PREFIX + sessionId) ?? "[]");
        picks = validPicks(this.draft(sessionId), Array.isArray(stored) ? stored : []);
      } catch { picks = []; }
      this.app.picks.set(sessionId, picks);
    }
    return picks;
  }

  /** The box calls this on each edit; it does not render. */
  setDraft(sessionId: string, text: string, picks?: readonly SkillPick[]) {
    const next = picks ? validPicks(text, picks) : editPicks(this.draft(sessionId), text, this.draftPicks(sessionId));
    this.app.drafts.set(sessionId, text);
    this.app.picks.set(sessionId, next);
    writeStore(DRAFT_PREFIX + sessionId, text);
    writeStore(PICKS_PREFIX + sessionId, next.length ? JSON.stringify(next) : "");
  }

  /** History is per workspace, so a new chat recalls the prompts of the workspace's other chats. */
  historyKey(sessionId: string): string {
    const session = this.target(sessionId).session;
    const info = this.app.snapshot?.sessions.find((s) => s.id === session);
    return HISTORY_PREFIX + (info?.workspace || info?.cwd || session);
  }

  historyList(key: string): HistoryEntry[] {
    let list = this.app.histories.get(key);
    if (!list) {
      try {
        const stored = JSON.parse(readStore(key) ?? "[]");
        list = Array.isArray(stored) ? stored.filter((e) => typeof e?.text === "string" && typeof e?.session === "string") : [];
      } catch { list = []; }
      this.app.histories.set(key, list);
    }
    return list;
  }

  /** Sent prompts for Up, ⌘K and Reuse, newest first: this chat's, then the workspace's. */
  history(sessionId: string): string[] {
    return recallOrder(this.historyList(this.historyKey(sessionId)), sessionId);
  }

  recordHistory(sessionId: string, text: string) {
    const key = this.historyKey(sessionId);
    // Read the stored list again, so a send from another tab is kept.
    this.app.histories.delete(key);
    const list = recordSent(this.historyList(key), { text, session: sessionId, at: Date.now() });
    this.app.histories.set(key, list);
    writeStore(key, JSON.stringify(list));
  }

  /** The one draft that Esc or Reuse cleared, so the owner can take it back. */
  stashed(sessionId: string): string {
    let s = this.app.stashes.get(sessionId);
    if (s === undefined) this.app.stashes.set(sessionId, (s = readStore(STASH_PREFIX + sessionId) ?? ""));
    return s;
  }

  setStash(sessionId: string, text: string) {
    this.app.stashes.set(sessionId, text);
    writeStore(STASH_PREFIX + sessionId, text);
  }

  /** Puts a past prompt in the composer. A different draft goes to the stash, so nothing is lost. */
  reuse(sessionId: string, text: string) {
    const draft = this.draft(sessionId);
    if (draft.trim() && draft !== text) this.setStash(sessionId, draft);
    this.replaceDraft(sessionId, text);
  }

  replaceDraft(sessionId: string, text: string) {
    this.setDraft(sessionId, text);
    this.update(sessionId, { draftRev: this.input(sessionId).draftRev + 1 });
  }

  /** Runs one chat action. A failure shows the server's text in the banner; only Retry runs it again. */
  async act(sessionId: string, busy: Partial<Input>, idle: Partial<Input>, run: () => Promise<void>, retry: () => void): Promise<boolean> {
    this.update(sessionId, { ...busy, banner: null });
    try {
      await run();
      this.update(sessionId, idle);
      return true;
    } catch (err) {
      this.update(sessionId, { ...idle, banner: { text: message(err), retry } });
      return false;
    }
  }

  /** Types `text` into the chair. Clear only the source draft, which can differ after a skill moves.
   *  `owner` is false for text that code composes, which stays out of the prompt history. */
  async send(sessionId: string, text: string, draftText = text, owner = true): Promise<boolean> {
    const chair = this.chair(sessionId);
    const st = this.input(sessionId);
    const out = outgoing(text);
    // A send typed while Up and C-u presses run would be cut or garbled.
    if (!out || st.sending || st.pulling || chair.disabled) return false;
    const typed = submission(out);
    const chat = this.app.chats.get(sessionId);
    const sent = chat && sentRow(typed, chair.provider, chat.rows, chair.running);
    const ok = await this.act(
      sessionId,
      { sending: true },
      { sending: false },
      () => this.app.api.action("/api/chat/send", { ...this.target(sessionId), text: typed }),
      () => void this.send(sessionId, text, draftText, owner),
    );
    if (!ok) return false;
    if (chat && sent) {
      // The log can show the text while the send is in flight.
      chat.sent = confirmSent([...chat.sent, sent], chat.rows);
      this.app.reading.changed(chat);
    }
    // The owner's words, before a pi skill moved to the front.
    if (owner) this.recordHistory(sessionId, outgoing(draftText) ?? out);
    if (this.draft(sessionId) === draftText) this.replaceDraft(sessionId, "");
    return true;
  }

  /** The `/` menu rows of the chat's agent, as its CLI lists them (`swarm commands`). */
  menuCommands(sessionId: string): Promise<CommandList> {
    return this.app.api.get<CommandList>("/api/chat/commands", this.target(sessionId));
  }

  /** The `@` menu rows: the files of the chat's folder (`/api/chat/files`). */
  async menuFiles(sessionId: string): Promise<CommandList> {
    const { files, note } = await this.app.api.get<{ files: string[]; note: string | null }>("/api/chat/files", this.target(sessionId));
    return fileRows(files, note);
  }

  /** ⌘. and Stop: interrupt the chair's turn. */
  async interrupt(sessionId: string) {
    if (this.input(sessionId).stopping || this.chair(sessionId).unreachable) return;
    await this.act(
      sessionId,
      { stopping: true },
      { stopping: false },
      () => this.app.api.action("/api/chat/interrupt", this.target(sessionId)),
      () => void this.interrupt(sessionId),
    );
  }

  /** Picks choice `choice` (zero-based); swarm refuses it when the question changed. */
  async answer(sessionId: string, prompt: Prompt, choice: number) {
    if (this.input(sessionId).answering?.prompt === prompt.id) return;
    const ok = await this.act(
      sessionId,
      { answering: { prompt: prompt.id, choice } },
      {},
      () => this.app.api.action("/api/chat/answer", { ...this.target(sessionId), prompt_id: prompt.id, choice }),
      () => void this.answer(sessionId, prompt, choice),
    );
    // The card stays busy until the next snapshot drops this prompt.
    if (!ok) this.update(sessionId, { answering: null });
  }

  dismissBanner(sessionId: string) {
    this.update(sessionId, { banner: null });
  }

  /** ↑ on an empty draft: Claude's queued messages back into the draft. The server presses the
   *  CLI's own keys and confirms them from the log (ADR 0038, `/api/chat/pull-back`). */
  async pullBack(sessionId: string) {
    const chat = this.app.chats.get(sessionId);
    const st = this.input(sessionId);
    const chair = this.chair(sessionId);
    if (!chat || !chair.pullBack || chair.disabled || st.pulling || st.sending) return;
    if (!chat.rows.some((r) => r.kind === "queued")) return;
    const result: { pulled: string | null } = { pulled: null };
    await this.act(
      sessionId,
      { pulling: true, alreadySent: false },
      { pulling: false },
      async () => {
        const done = await this.app.api.post<{ outcome: "pulled" | "alreadySent" | "unconfirmed"; text: string | null }>("/api/chat/pull-back", this.target(sessionId));
        if (done.outcome === "unconfirmed") throw new Error("Could not confirm the pull-back. Check the agent's input box.");
        if (done.outcome === "pulled") result.pulled = done.text ?? "";
      },
      () => void this.pullBack(sessionId),
    );
    if (result.pulled === null) {
      if (this.input(sessionId).banner) return;
      // Only the caption waits, so Send works again at once. A newer caption keeps its own timer.
      const seq = ++this.app.alreadySentSeq;
      this.update(sessionId, { alreadySent: true });
      setTimeout(() => seq === this.app.alreadySentSeq && this.update(sessionId, { alreadySent: false }), 2000);
      return;
    }
    // Keep anything typed while the CLI let go of the text.
    const typed = this.draft(sessionId);
    const text = result.pulled;
    this.replaceDraft(sessionId, typed ? `${text}\n${typed}` : text);
  }
}
