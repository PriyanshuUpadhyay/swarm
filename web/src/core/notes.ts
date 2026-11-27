// Chat notes (ADR 0162): the owner marks agent reply text, one item, or a whole reply, and writes a
// note on it. The notes are browser drafts per chat; one step puts them in the chat's message box.
// A note on a child agent's column belongs to its main chat and names the agent.

import { readStore, writeStore } from "./app-types";
import type { App } from "./app";
import { scopedRowId } from "./transcript";

const KINDS = ["text", "item", "reply"] as const;
/** `rowId` only finds the row on screen; the agent never sees it. `quote` is the full text.
 *  `agent` is the child agent whose reply the note marks; absent, the chair's own reply. */
export type Note = { id: number; kind: (typeof KINDS)[number]; rowId: string; quote: string; text: string; at: number; agent?: string };

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
    && typeof r.quote === "string" && typeof r.text === "string" && typeof r.at === "number"
    && (r.agent === undefined || typeof r.agent === "string");
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

/** The one message the chair gets: each note numbered under its full quote; a note on a child's
 *  reply starts with the agent's name, as `1. [reviewer] > quote`. */
export function noteMessage(notes: readonly Note[]): string {
  const parts = notes.map((n, i) => {
    const head = `${i + 1}. `;
    const pad = " ".repeat(head.length);
    const quote = (n.kind === "reply" ? ["[whole reply]", ...n.quote.split("\n")] : n.quote.split("\n")).map((l) => (l ? `> ${l}` : ">"));
    if (n.agent) quote[0] = `[${n.agent}] ${quote[0]}`;
    const text = n.text.trim() ? n.text.trim().split("\n") : [];
    return [...quote, ...text].map((l, j) => (j === 0 ? head : pad) + l).join("\n");
  });
  const head = notes.some((n) => n.agent)
    ? `Notes on replies (${notes.length}); [name] marks a child agent's reply, no name marks yours:`
    : `Notes on your reply (${notes.length}):`;
  return [head, ...parts].join("\n\n");
}

const NO_NOTES: readonly Note[] = [];

export class Notes {
  private chats: Map<string, Note[]>;
  private seq: number;
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

  continueNotes(from: string, to: string) {
    const saved = readNotes();
    const old = saved.get(from) ?? (this.saveRefused ? this.list(from) : NO_NOTES);
    const next = saved.get(to) ?? this.list(to);
    if (!old.length) {
      if (saved.has(to)) this.chats.set(to, [...next]);
      return;
    }

    const ids = new Set(next.map((note) => note.id));
    this.seq = Math.max(this.seq, ...old.map((note) => note.id), ...ids);
    const moved = old.map((note) => ({
      ...note,
      id: ids.has(note.id) ? ++this.seq : note.id,
      rowId: scopedRowId(note.rowId) ? note.rowId : `${from}:${note.rowId}`,
    }));
    const merged = [...moved, ...next];
    saved.delete(from);
    saved.set(to, merged);
    const ok = writeStore(STORE_KEY, savedNotes([...saved]));
    this.chats.set(to, merged);
    if (ok) this.chats.delete(from);
    else if (!this.saveRefused) this.app.notice("The browser could not save the moved notes. The original notes were not deleted.", true);
    this.saveRefused = !ok;
    this.changed();
  }

  /** Where the notes of chat `id` live: a child's column keeps them in its main chat, under
   *  the child's name. */
  home(id: string): { chat: string; agent?: string } {
    const { session, agent } = this.app.composing.target(id);
    return agent === "chair" ? { chat: session } : { chat: session, agent };
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

  /** True once for a new card, so only it takes the caret. */
  takeFocus(id: number): boolean {
    if (this.focusId !== id) return false;
    this.focusId = null;
    return true;
  }

  add(chat: string, note: Pick<Note, "kind" | "rowId" | "quote" | "agent">): Note | null {
    if (!note.quote.trim()) return null;
    if (this.list(chat).length >= CAP) {
      this.app.notice(`This chat has ${CAP} notes, the most it keeps. Add them to the message or remove some first.`, true);
      return null;
    }
    const added: Note = { ...note, id: ++this.seq, text: "", at: Date.now() };
    this.chats.set(chat, [...this.list(chat), added]);
    this.focusId = added.id;
    this.changed(chat);
    return added;
  }

  setText(chat: string, id: number, text: string) {
    this.chats.set(chat, this.list(chat).map((n) => (n.id === id ? { ...n, text } : n)));
    this.changed(chat);
  }

  remove(chat: string, id: number) {
    this.chats.set(chat, this.list(chat).filter((n) => n.id !== id));
    this.changed(chat);
  }

  /** True when the chat's message box holds text, so a move of the notes asks first. */
  boxHasText(chat: string): boolean {
    return this.app.draft(chat).trim() !== "";
  }

  /** Puts every note of the chat in its message box as one message, and ends annotate mode. The
   *  owner reads and sends it. `replace` moves the box's text to its stash, as Reuse does; `append` adds the notes under it. */
  toBox(chat: string, how: "replace" | "append") {
    const notes = this.list(chat);
    if (!notes.length) return;
    const text = noteMessage(notes);
    const draft = this.app.draft(chat);
    if (how === "append" && draft.trim()) this.app.composing.replaceDraft(chat, `${draft.trimEnd()}\n\n${text}`);
    else this.app.composing.reuse(chat, text);
    this.app.panes.revealChat();
    this.dismiss(chat);
  }

  /** Removes every note of the chat and ends annotate mode. */
  dismiss(chat: string) {
    this.chats.set(chat, []);
    this.mode = false;
    this.changed(chat);
  }
}
