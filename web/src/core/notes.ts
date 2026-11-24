// Chat notes (ADR 0161): the owner marks agent reply text, one item, or a whole reply, and writes a
// note on it. The notes are browser drafts per chat; one send types them into the agent.

import { readStore, writeStore, type Input } from "./app-types";
import type { App } from "./app";
import { plural } from "./lifecycle-state";

const KINDS = ["text", "item", "reply"] as const;
/** `rowId` only finds the row on screen; the agent never sees it. `quote` is the full text. */
export type Note = { id: number; kind: (typeof KINDS)[number]; rowId: string; quote: string; text: string; at: number };

/** The saved drafts: this key holds `savedNotes` of every chat with notes. */
export const STORE_KEY = "swarm.notes.v1";
export const savedNotes = (chats: [string, readonly Note[]][]) => JSON.stringify({ v: 1, chats });
/** Notes per chat, a chosen limit. It counts notes, not bytes: a quote is never cut, so long
 *  quotes can still fill the browser's storage, and then a notice says the save failed. */
const CAP = 200;

const isNote = (n: unknown): n is Note => {
  if (typeof n !== "object" || n === null) return false;
  const r = n as Record<string, unknown>;
  return typeof r.id === "number" && (KINDS as readonly unknown[]).includes(r.kind) && typeof r.rowId === "string"
    && typeof r.quote === "string" && typeof r.text === "string" && typeof r.at === "number";
};

/** The saved drafts; a value of any other shape gives none. */
function readNotes(): Map<string, Note[]> {
  try {
    const saved: unknown = JSON.parse(readStore(STORE_KEY) ?? "null");
    if (typeof saved !== "object" || saved === null || (saved as { v?: unknown }).v !== 1) return new Map();
    const chats = (saved as { chats?: unknown }).chats;
    if (!Array.isArray(chats)) return new Map();
    const valid = chats.filter((c: unknown): c is [string, Note[]] => Array.isArray(c) && c.length === 2 && typeof c[0] === "string" && Array.isArray(c[1]) && c[1].every(isNote));
    return new Map(valid);
  } catch {
    return new Map();
  }
}

/** The one message the agent gets: each note numbered under its full quote. */
export function noteMessage(notes: readonly Note[]): string {
  const parts = notes.map((n, i) => {
    const head = `${i + 1}. `;
    const pad = " ".repeat(head.length);
    const quote = (n.kind === "reply" ? ["[whole reply]", ...n.quote.split("\n")] : n.quote.split("\n")).map((l) => (l ? `> ${l}` : ">"));
    const text = n.text.trim() ? n.text.trim().split("\n") : [];
    return [...quote, ...text].map((l, j) => (j === 0 ? head : pad) + l).join("\n");
  });
  return [`Notes on your reply (${notes.length}):`, ...parts].join("\n\n");
}

/** True while a chat's notes stay as sent: their send runs, or the banner still offers its
 *  Retry. So a Retry never sends text other than the drafts that it clears. */
export const notesHeld = (sending: boolean, failed: (() => void) | null, input: Input | undefined) =>
  sending || (!!failed && !!input && (input.sending || input.banner?.onSent === failed));

const NO_NOTES: readonly Note[] = [];

export class Notes {
  private chats: Map<string, Note[]>;
  private seq: number;
  private sending = new Set<string>();
  /** Per chat, the callback of its failed send, which the banner's Retry still holds. */
  private failed = new Map<string, () => void>();
  private saveRefused = false;
  private listeners = new Set<() => void>();
  /** The card whose note box takes the caret once it shows. */
  private focusId: number | null = null;
  /** Annotate mode: reply items and whole replies take a click. Not saved. */
  private mode = false;

  constructor(private readonly app: App) {
    this.chats = readNotes();
    this.seq = Math.max(0, ...[...this.chats.values()].flat().map((n) => n.id));
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  /** Tells the views; with `chat`, also saves the notes of that chat. */
  private changed(chat?: string) {
    if (chat !== undefined) this.save(chat);
    for (const fn of this.listeners) fn();
  }

  /** Writes one chat's notes over the saved value, read again first: another tab can have saved
   *  its own chats since this one read it. */
  private save(chat: string) {
    const saved = readNotes();
    const notes = this.list(chat);
    if (notes.length) saved.set(chat, [...notes]);
    else saved.delete(chat);
    const chats = [...saved];
    const ok = writeStore(STORE_KEY, chats.length ? savedNotes(chats) : "");
    if (!ok && !this.saveRefused) this.app.notice("The browser did not save the notes, maybe because its storage is full. They stay in this tab until it closes.", true);
    this.saveRefused = !ok;
  }

  list(chat: string): readonly Note[] {
    return this.chats.get(chat) ?? NO_NOTES;
  }

  annotating(): boolean {
    return this.mode;
  }

  setMode(on: boolean) {
    if (this.mode === on) return;
    this.mode = on;
    this.changed();
  }

  toggleMode() {
    this.setMode(!this.mode);
  }

  isSending(chat: string): boolean {
    return this.sending.has(chat);
  }

  failedSend(chat: string): (() => void) | null {
    return this.failed.get(chat) ?? null;
  }

  held(chat: string): boolean {
    return notesHeld(this.sending.has(chat), this.failedSend(chat), this.app.composing.input(chat));
  }

  /** True once for a new card, so only it takes the caret. */
  takeFocus(id: number): boolean {
    if (this.focusId !== id) return false;
    this.focusId = null;
    return true;
  }

  add(chat: string, note: Pick<Note, "kind" | "rowId" | "quote">): Note | null {
    if (!note.quote.trim()) return null;
    if (this.list(chat).length >= CAP) {
      this.app.notice(`This chat has ${CAP} notes, the most it keeps. Send or remove some first.`, true);
      return null;
    }
    const added: Note = { ...note, id: ++this.seq, text: "", at: Date.now() };
    this.chats.set(chat, [...this.list(chat), added]);
    this.focusId = added.id;
    this.changed(chat);
    return added;
  }

  setText(chat: string, id: number, text: string) {
    if (this.held(chat)) return;
    this.chats.set(chat, this.list(chat).map((n) => (n.id === id ? { ...n, text } : n)));
    this.changed(chat);
  }

  remove(chat: string, id: number) {
    if (this.held(chat)) return;
    this.chats.set(chat, this.list(chat).filter((n) => n.id !== id));
    this.changed(chat);
  }

  /** Sends every note of the chat as one message. A failed send keeps them, held as sent while
   *  the composer's banner offers Retry; the notes clear once, when a send goes. */
  async send(chat: string): Promise<boolean> {
    const notes = this.list(chat);
    if (!notes.length || this.sending.has(chat)) return false;

    const text = noteMessage(notes);
    const ids = new Set(notes.map((n) => n.id));
    const onSent = () => {
      this.failed.delete(chat);
      this.chats.set(chat, this.list(chat).filter((n) => !ids.has(n.id)));
      this.changed(chat);
      this.app.notice(`Sent ${plural(notes.length, "note")} to the agent.`);
    };

    this.failed.delete(chat);
    this.sending.add(chat);
    this.changed();
    try {
      const ok = await this.app.send(chat, text, text, false, onSent);
      if (!ok) this.failed.set(chat, onSent);
      return ok;
    } finally {
      this.sending.delete(chat);
      this.changed();
    }
  }
}
