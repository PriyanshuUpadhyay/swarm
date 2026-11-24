// Chat notes (ADR 0161): the owner marks agent reply text, one item, or a whole reply, and writes a
// note on it. The notes are browser drafts per chat; one send types them into the agent.

import { readStore, writeStore } from "./app-types";
import type { App } from "./app";

export type NoteKind = "text" | "item" | "reply";
/** `rowId` only finds the row on screen; the agent never sees it. `quote` is the full text. */
export type Note = { id: number; kind: NoteKind; rowId: string; quote: string; text: string; at: number };

const STORE_KEY = "swarm.notes.v1";
const KINDS: readonly string[] = ["text", "item", "reply"] satisfies NoteKind[];

const isNote = (n: unknown): n is Note => {
  if (typeof n !== "object" || n === null) return false;
  const r = n as Record<string, unknown>;
  return typeof r.id === "number" && typeof r.kind === "string" && KINDS.includes(r.kind) && typeof r.rowId === "string"
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

const NO_NOTES: readonly Note[] = [];

export class Notes {
  private chats: Map<string, Note[]>;
  private seq: number;
  private sending = new Set<string>();
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

  private changed(save = true) {
    if (save) {
      const chats = [...this.chats].filter(([, notes]) => notes.length);
      writeStore(STORE_KEY, chats.length ? JSON.stringify({ v: 1, chats }) : "");
    }
    for (const fn of this.listeners) fn();
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
    this.changed(false);
  }

  toggleMode() {
    this.setMode(!this.mode);
  }

  isSending(chat: string): boolean {
    return this.sending.has(chat);
  }

  /** True once for a new card, so only it takes the caret. */
  takeFocus(id: number): boolean {
    if (this.focusId !== id) return false;
    this.focusId = null;
    return true;
  }

  add(chat: string, note: Pick<Note, "kind" | "rowId" | "quote">): Note | null {
    if (!note.quote.trim()) return null;
    const added: Note = { ...note, id: ++this.seq, text: "", at: Date.now() };
    this.chats.set(chat, [...this.list(chat), added]);
    this.focusId = added.id;
    this.changed();
    return added;
  }

  setText(chat: string, id: number, text: string) {
    this.chats.set(chat, this.list(chat).map((n) => (n.id === id ? { ...n, text } : n)));
    this.changed();
  }

  remove(chat: string, id: number) {
    this.chats.set(chat, this.list(chat).filter((n) => n.id !== id));
    this.changed();
  }

  /** Sends every note of the chat as one message. A failed send keeps them; the composer's banner
   *  offers Retry, and the notes clear once, when a send goes. */
  async send(chat: string): Promise<boolean> {
    const notes = this.list(chat);
    if (!notes.length || this.sending.has(chat)) return false;
    const text = noteMessage(notes);
    const ids = new Set(notes.map((n) => n.id));
    const onSent = () => {
      this.chats.set(chat, this.list(chat).filter((n) => !ids.has(n.id)));
      this.changed();
      this.app.notice(`Sent ${notes.length === 1 ? "1 note" : `${notes.length} notes`} to the agent.`);
    };
    this.sending.add(chat);
    this.changed(false);
    try {
      return await this.app.send(chat, text, text, false, onSent);
    } finally {
      this.sending.delete(chat);
      this.changed(false);
    }
  }
}
