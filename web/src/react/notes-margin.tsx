// Chat notes (ADR 0161): the numbered cards of one reply, and the send bar above the composer.

import { SendIcon, XIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { SENT } from "../core/app";
import { noteMessage, type Note, type Notes } from "../core/notes";
import { Button } from "../components/ui/button";

const SOURCE: Record<Note["kind"], string> = { text: "On selected text", item: "On item", reply: "On whole reply" };
/** A quote longer than this folds to 3 lines with Show all. */
const LONG_QUOTE = 240;
const plural = (n: number) => (n === 1 ? "1 note" : `${n} notes`);

/** One card: its number, the quote it marks, and the note box, which saves on each change. */
export function NoteCard({ notes, chat, note, n }: { notes: Notes; chat: string; note: Note; n: number }) {
  const box = useRef<HTMLTextAreaElement>(null);
  const long = note.quote.length > LONG_QUOTE || note.quote.split("\n").length > 3;
  const [all, setAll] = useState(false);
  useEffect(() => {
    if (notes.takeFocus(note.id)) box.current?.focus({ preventScroll: true });
  }, [notes, note.id]);
  const lines = Math.min(8, Math.max(2, note.text.split("\n").length));
  return (
    <article className="ncard" data-testid="note-card" data-note={note.id} aria-label={`Note ${n}`}>
      <div className="ncard-head">
        <span className="nbadge num">{n}</span>
        <span className="src">{SOURCE[note.kind]}</span>
        <button type="button" className="icon-btn" aria-label={`Remove note ${n}`} title="Remove note" data-testid="note-remove" onClick={() => notes.remove(chat, note.id)}>
          <XIcon />
        </button>
      </div>
      <blockquote className={`nquote${long && !all ? " folded" : ""}`}>{note.quote}</blockquote>
      {long && (
        <button type="button" className="t-link" aria-expanded={all} onClick={() => setAll(!all)}>
          {all ? "Show less" : "Show all"}
        </button>
      )}
      <textarea
        ref={box}
        data-testid="note-input"
        aria-label={`Note ${n}`}
        placeholder="Write a note…"
        rows={lines}
        value={note.text}
        onChange={(e) => notes.setText(chat, note.id, e.target.value)}
      />
    </article>
  );
}

/** "N notes on this chat", the exact text the agent gets, and Send. */
export function NotesBar({ notes, chat, list, sending, blocked }: { notes: Notes; chat: string; list: readonly Note[]; sending: boolean; blocked: string | null }) {
  const [preview, setPreview] = useState(false);
  if (!list.length) return null;
  return (
    <div className="notes-bar" data-testid="notes-bar">
      <div className="notes-bar-row">
        <span className="font-medium">{plural(list.length)} on this chat</span>
        <span className="flex-1" />
        <button type="button" className="t-link" aria-expanded={preview} onClick={() => setPreview(!preview)}>
          {preview ? "Hide what the agent gets" : "Show what the agent gets"}
        </button>
        <Button size="sm" data-testid="notes-send" disabled={sending || blocked !== null} title={blocked ?? "Send every note to the chat's agent as one message"} onClick={() => {
            // The transcript follows the sent message, as after a typed send.
            window.dispatchEvent(new CustomEvent(SENT, { detail: chat }));
            void notes.send(chat);
          }}>
          <SendIcon /> {sending ? "Sending…" : `Send ${plural(list.length)}`}
        </Button>
      </div>
      {preview && <pre className="notes-preview" data-testid="notes-preview">{noteMessage(list)}</pre>}
    </div>
  );
}

/** The blocks of a reply that annotate mode offers as one item each. */
const BLOCKS = "p, h1, h2, h3, h4, h5, h6, li, pre, blockquote, table";
/** The reply text, without its buttons. */
const BODY = '[data-kind="assistant"] .msg .md';

/** The item under `el` in an agent reply: the innermost list item or quote that holds the block. */
export function itemAt(el: Element): HTMLElement | null {
  const block = el.closest<HTMLElement>(BLOCKS);
  const body = block?.closest(BODY);
  if (!block || !body) return null;
  const outer = block.closest<HTMLElement>("li, blockquote");
  return outer && body.contains(outer) ? outer : block;
}

/** An item's quote: its text; an item of an ordered list keeps its number. Not innerText, which is
 *  empty while the list keeps a new row hidden to measure it. */
export function itemQuote(item: HTMLElement): string {
  const text = (item.textContent ?? "").trim();
  const list = item.parentElement;
  if (item.tagName !== "LI" || list?.tagName !== "OL") return text;
  const start = Number((list as HTMLOListElement).start) || 1;
  return `${start + [...list.children].indexOf(item)}. ${text}`;
}

/** Marks the items and replies that have notes, and in annotate mode makes items take Tab.
 *  Rows remount as the list scrolls, so this runs again after each change of the list's rows. */
export function paintNotes(list: HTMLElement, notes: readonly Note[], annotating: boolean) {
  for (const el of list.querySelectorAll("[data-inote]")) el.removeAttribute("data-inote");
  for (const el of list.querySelectorAll(BODY)) {
    for (const block of el.querySelectorAll<HTMLElement>(BLOCKS)) {
      if (annotating && itemAt(block) === block) block.tabIndex = 0;
      else block.removeAttribute("tabindex");
    }
  }
  notes.forEach((note, i) => {
    const row = list.querySelector<HTMLElement>(`[data-testid="row"][data-id="${CSS.escape(note.rowId)}"]`);
    if (!row) return;
    if (note.kind === "reply") row.dataset.inote = String(i + 1);
    if (note.kind !== "item") return;
    const item = [...row.querySelectorAll<HTMLElement>(`.msg .md :is(${BLOCKS})`)].find((b) => itemAt(b) === b && itemQuote(b) === note.quote);
    if (item) item.dataset.inote = String(i + 1);
  });
}
