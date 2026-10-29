

import { queueRows, type Usage } from "./composer";
import { MarkdownCache } from "./markdown";
import { FoldChoices, findMatches, lines, mergeRows, type Line } from "./transcript";
import type { Row } from "./api";
import { TAIL, LRU_SIZE, type FullText, type Chat, type Find, type Reader, paneId } from "./app-types";
import type { App } from "./app";

const letters = (text: string) => text.replace(/[^\p{L}\p{N}]/gu, "");

/** The running turn's screen text as a row, or null once a log row of this turn holds it. The
 *  screen drops markdown marks and wraps lines, so only letters and digits are compared. */
export function liveRow(rows: readonly Row[], text: string | null): Row | null {
  const head = letters(text ?? "").slice(0, 80);
  if (!head) return null;
  for (let i = rows.length - 1; i >= 0 && rows[i].kind !== "user"; i--)
    if (rows[i].kind === "assistant" && letters(rows[i].text).includes(head)) return null;
  return { id: "live", kind: "assistant", text: text!, order: [Number.MAX_SAFE_INTEGER, -1] };
}

export class AppReader {
  constructor(private readonly app: App) {}

  build(chat: Chat) {
    const live = liveRow(chat.rows, chat.live);
    const rows = live || chat.queue.length ? [...chat.rows, ...(live ? [live] : []), ...chat.queue] : chat.rows;
    return lines(rows, { folds: chat.folds, opened: chat.opened });
  }

  // ---- chats ----

  newChat(sessionId: string, agent = "chair"): Chat {
    const chat: Chat = {
      id: agent === "chair" ? sessionId : paneId(sessionId, agent),
      sessionId,
      agent,
      key: `${sessionId}:${agent}`,
      loaded: agent === "chair",
      rows: [],
      older: 0,
      end: 0,
      state: "loading",
      error: null,
      version: 0,
      shift: false,
      loadingOlder: false,
      usage: null,
      queue: [],
      live: null,
      typed: [],
      ...this.freshReaderState(),
    };
    this.app.chats.set(chat.id, chat);
    // Later snapshots update it (applySnapshot).
    chat.queue = queueRows(this.app.composing.chair(chat.id).queue);
    return chat;
  }

  /** A child agent's chat. It holds no rows until its column shows. */
  childChat(sessionId: string, agent: string): Chat {
    return this.app.chats.get(paneId(sessionId, agent)) ?? this.newChat(sessionId, agent);
  }

  freshReaderState() {
    return {
      folds: new FoldChoices(),
      opened: new Map<string, boolean>(),
      full: new Map<string, FullText>(),
      md: new MarkdownCache(),
      built: null,
      memo: null,
    };
  }

  async loadTail(chat: Chat) {
    try {
      const page = await this.app.api.rows(chat.sessionId, chat.agent, TAIL);
      chat.rows = mergeRows([], page.rows);
      chat.older = page.page.start_offset;
      chat.end = page.page.end_offset;
      chat.usage = page.usage as Usage;
      chat.state = "ready";
      chat.error = null;
    } catch (err) {
      chat.state = "error";
      chat.error = String(err);
    }
    chat.version++;
    chat.shift = false;
  }

  /** Marks the chat as recently rendered. Chats past the LRU lose their render caches (and,
   *  outside live mode, their model too). */
  touch(sessionId: string) {
    this.app.lru = [sessionId, ...this.app.lru.filter((id) => id !== sessionId)];
    for (const old of this.app.lru.slice(LRU_SIZE)) {
      const chat = this.app.chats.get(old);
      if (!chat) continue;
      if (this.app.liveAll) Object.assign(chat, this.freshReaderState());
      else this.app.chats.delete(old);
    }
    this.app.lru = this.app.lru.slice(0, LRU_SIZE);
  }

  /** The transcript shows this chat and its last row is in the DOM. */
  transcriptPainted(sessionId: string): boolean {
    const el = document.querySelector<HTMLElement>('[data-testid="transcript"]');
    if (!el || el.dataset.chat !== sessionId || el.dataset.state !== "ready") return false;
    const last = this.app.view.lines.length - 1;
    return last < 0 || el.querySelector(`[data-testid="row"][data-index="${last}"]`) !== null;
  }

  get open(): Chat | undefined {
    return this.app.openId ? this.app.chats.get(this.app.openId) : undefined;
  }

  /** The chat a reader call is for: `id`, else the open chair's. */
  chat(id?: string): Chat | undefined {
    return id === undefined ? this.open : this.app.chats.get(id);
  }

  /** The transcript calls of one child's chat. */
  reader(id: string): Reader {
    let r = this.app.readers.get(id);
    if (!r) {
      r = {
        toggleLine: (line) => this.toggleLine(line, id),
        loadFull: (previewId) => this.loadFull(previewId, id),
        fullText: (previewId) => this.fullText(previewId, id),
        imageUrl: this.imageUrl,
        loadOlder: () => this.loadOlder(id),
        retryRows: () => this.retryRows(id),
        setFindQuery: (query) => this.setFindQuery(query),
        stepFind: (dir) => this.stepFind(dir),
        closeFind: () => this.closeFind(),
        reuse: (composerId, text) => this.app.composing.reuse(composerId, text),
      };
      this.app.readers.set(id, r);
    }
    return r;
  }

  /** A reader change: lines rebuild, and the list does not treat it as a prepend. */
  changed(chat: Chat) {
    chat.version++;
    chat.shift = false;
    if (chat.sessionId === this.app.openId) this.app.notify();
  }

  /** Opens or closes a fold, a tool card, a thought, or a long system row. */
  toggleLine(line: Line, id?: string) {
    const chat = this.chat(id);
    if (!chat) return;
    if (line.row.kind === "fold") chat.folds.set(line.row, !line.open);
    else chat.opened.set(line.id, !line.open);
    this.changed(chat);
  }

  /** Fetches the full text behind a capped preview once and shows it; the LRU drops it. */
  async loadFull(previewId: string, id?: string) {
    const chat = this.chat(id);
    if (!chat || (chat.full.has(previewId) && chat.full.get(previewId)!.state !== "error")) return;
    chat.full = new Map(chat.full).set(previewId, { state: "loading" });
    this.changed(chat);
    let next: FullText;
    try {
      const text = await this.app.api.toolOutput(chat.sessionId, chat.agent, previewId);
      next = { state: "ready", text, lines: text.split("\n") };
    } catch (err) {
      next = { state: "error", error: String(err) };
    }
    chat.full = new Map(chat.full).set(previewId, next);
    this.changed(chat);
  }

  /** One blob URL per pasted image for the page's life; a failed read is tried again next time.
   *  An arrow, so a row can pass it on unbound. */
  imageUrl = (path: string): Promise<string> => {
    let url = this.app.images.get(path);
    if (!url) {
      url = this.app.api.imageUrl(path);
      url.catch(() => this.app.images.delete(path));
      this.app.images.set(path, url);
    }
    return url;
  };

  /** The full text for Copy, without showing it. */
  async fullText(previewId: string, id?: string): Promise<string> {
    const chat = this.chat(id);
    const have = chat?.full.get(previewId);
    if (have?.state === "ready") return have.text;
    if (!chat) throw new Error("no chat open");
    return this.app.api.toolOutput(chat.sessionId, chat.agent, previewId);
  }

  async retryRows(id?: string) {
    const chat = this.chat(id);
    if (!chat || chat.state === "loading") return;
    chat.state = "loading";
    chat.error = null;
    this.app.notify();
    await this.loadTail(chat);
    this.app.streaming.syncSubscriptions();
    this.app.notify();
  }

  /** Pages older rows in above the current ones. */
  async loadOlder(id?: string) {
    const chat = this.chat(id);
    if (!chat || chat.state !== "ready" || chat.loadingOlder || chat.older <= 0) return;
    chat.loadingOlder = true;
    chat.error = null;
    this.app.notify();
    try {
      const page = await this.app.api.rows(chat.sessionId, chat.agent, TAIL, chat.older);
      chat.rows = mergeRows(chat.rows, page.rows, page.remove);
      chat.older = page.page.start_offset;
      chat.version++;
      chat.shift = true;
    } catch (err) {
      chat.error = String(err);
    } finally {
      chat.loadingOlder = false;
    }
    if (this.app.openId === chat.sessionId) this.app.notify();
  }

  // ---- find ----

  /** Matches follow the rows; the current match stays on its row when rows change. */
  refind(chat: Chat | undefined) {
    const f = this.app.find;
    const sig = `${chat?.id}:${chat?.version}:${f.open}:${f.query}`;
    if (sig === this.app.findVersion) return;
    this.app.findVersion = sig;
    if (!f.open || !chat) {
      if (f.matches.length) this.app.find = { ...f, matches: [], index: -1 };
      return;
    }
    const current = f.matches[f.index];
    const matches = findMatches(chat.rows, f.query, chat.full);
    const kept = current === undefined ? -1 : matches.findIndex((m) => m.id === current.id && m.occurrence === current.occurrence);
    this.app.find = { ...f, matches, index: kept >= 0 ? kept : matches.length ? matches.length - 1 : -1 };
    // A new current match must come into view.
    if (kept < 0 && matches.length) this.reveal(chat, this.app.find);
  }

  /** Opens the fold or card that holds the current match, and asks the list to scroll to it. */
  reveal(chat: Chat, f: Find) {
    const m = f.matches[f.index];
    if (!m) return;
    if (m.fold && !chat.folds.isOpen(m.fold)) chat.folds.set(m.fold, true);
    chat.opened.set(m.id, true);
    chat.version++;
    chat.shift = false;
    chat.built = { version: chat.version, lines: this.build(chat) };
    this.app.findVersion = `${chat.id}:${chat.version}:${f.open}:${f.query}`;
    this.app.find = { ...f, seq: f.seq + 1 };
  }

  /** ⌘F finds in the focused pane, else in the chat. */
  openFind() {
    const surface = document.activeElement?.closest<HTMLElement>("[data-find-surface]");
    if (surface) { surface.dispatchEvent(new Event("swarm:find")); return; }
    if (!this.open) return;
    this.app.findId = this.app.pane.focused ? paneId(this.app.openId!, this.app.pane.focused) : null;
    this.app.find = { ...this.app.find, open: true, seq: this.app.find.seq + 1 };
    this.app.notify();
    requestAnimationFrame(() => document.querySelector<HTMLInputElement>('[data-testid="find-input"]')?.select());
  }

  closeFind() {
    if (!this.app.find.open) return;
    this.app.find = { ...this.app.find, open: false };
    this.app.notify();
  }

  setFindQuery(query: string) {
    this.app.find = { ...this.app.find, query, matches: [], index: -1 };
    this.app.notify();
  }

  /** Find starts at the newest match. Next (+1) goes to the older one, as up the chat; it wraps. */
  stepFind(dir: number) {
    const chat = this.app.chats.get(this.app.findId ?? this.app.openId ?? "");
    const f = this.app.find;
    if (!chat) return;
    if (!f.open) return this.openFind();
    if (!f.matches.length) return;
    this.app.find = { ...f, index: (f.index - dir + f.matches.length) % f.matches.length };
    this.reveal(chat, this.app.find);
    this.app.notify();
  }
}
