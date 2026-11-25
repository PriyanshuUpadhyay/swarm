// Chat notes (ADR 0161): the numbered cards of one reply, and the send bar above the composer.

import { TextCursorInputIcon, XIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { plural } from "../core/lifecycle-state";
import { noteMessage, type Note, type Notes } from "../core/notes";
import { Button } from "../components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../components/ui/dialog";
import { copyText } from "./copy";
import { NOTE_CARD, NOTE_SLOT, replyText } from "./note-marks";
import { REPLY_BODY } from "./transcript-overlays";

const SOURCE: Record<Note["kind"], string> = { text: "On selected text", item: "On item", reply: "On whole reply" };
/** A quote longer than this folds to 3 lines with Show all. */
const LONG_QUOTE = 240;

/** One card: its number, the quote it marks, and the note box, which saves on each change. */
export function NoteCard({ notes, chat, note, n }: { notes: Notes; chat: string; note: Note; n: number }) {
  const box = useRef<HTMLTextAreaElement>(null);
  const [all, setAll] = useState(false);
  useEffect(() => {
    if (notes.takeFocus(note.id)) box.current?.focus({ preventScroll: true });
  }, [notes, note.id]);

  const long = note.quote.length > LONG_QUOTE || note.quote.split("\n").length > 3;
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

/** "N notes on this chat", the exact text the chair gets, and the move of all notes into the main
 *  chat's message box. `chat` is the main chat; in a child's column (`child`), the bar still counts
 *  and moves every note of the main chat. A box that holds text asks first. */
export function NotesBar({ notes, chat, list, child, blocked }: { notes: Notes; chat: string; list: readonly Note[]; child: boolean; blocked: string | null }) {
  const [preview, setPreview] = useState(false);
  const [asking, setAsking] = useState(false);
  if (!list.length) return null;

  const count = plural(list.length, "note");
  const move = (how: "replace" | "append") => {
    setAsking(false);
    notes.toBox(chat, how);
    // The chat's box is the first composer on the page, as ⌘L finds it.
    requestAnimationFrame(() => document.querySelector<HTMLElement>('[data-testid="composer"]')?.focus());
  };

  return (
    <div className="notes-bar" data-testid="notes-bar">
      <div className="notes-bar-row">
        <span className="font-medium">{count} {child ? "for the main chat" : "on this chat"}</span>
        <span className="flex-1" />
        <button type="button" className="t-link" aria-expanded={preview} onClick={() => setPreview(!preview)}>
          {preview ? "Hide what the agent gets" : "Show what the agent gets"}
        </button>
        <Button
          size="sm"
          data-testid="notes-send"
          disabled={blocked !== null}
          title={blocked ?? "Put every note of the chat, also those on its agents' columns, in the chat's message box"}
          onClick={() => (notes.boxHasText(chat) ? setAsking(true) : move("replace"))}
        >
          <TextCursorInputIcon /> Add {count} to message
        </Button>
      </div>
      {preview && <pre className="notes-preview" data-testid="notes-preview">{noteMessage(list)}</pre>}
      <Dialog open={asking} onOpenChange={setAsking}>
        <DialogContent className="top-1/3 grid w-[min(460px,calc(100vw-32px))] gap-3 p-5" data-testid="notes-confirm">
          <DialogTitle>The message box has text</DialogTitle>
          <DialogDescription>Add the {count} under that text, or replace it. Replace keeps the old text in the box's stash, as Reuse does.</DialogDescription>
          <div className="flex flex-wrap justify-end gap-2">
            <Button variant="outline" onClick={() => setAsking(false)}>Cancel</Button>
            <Button
              variant="outline"
              title="Copy the notes as one message and remove them"
              onClick={() => {
                void copyText(noteMessage(list), "notes");
                setAsking(false);
                notes.dismiss(chat);
              }}
            >
              Copy and Dismiss
            </Button>
            <Button variant="outline" onClick={() => move("replace")}>Replace Text</Button>
            <Button autoFocus onClick={() => move("append")}>Append</Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** The blocks of a reply that annotate mode offers as one item each. */
const BLOCKS = "p, h1, h2, h3, h4, h5, h6, li, pre, blockquote, table";

/** The item under `el` in an agent reply: the innermost list item or quote that holds the block. */
export function itemAt(el: Element): HTMLElement | null {
  // A card under a block sits inside the reply's markup, but it is not reply text.
  if (el.closest(NOTE_CARD)) return null;
  const block = el.closest<HTMLElement>(BLOCKS);
  const body = block?.closest(REPLY_BODY);
  if (!block || !body) return null;
  const outer = block.closest<HTMLElement>("li, blockquote");
  return outer && body.contains(outer) ? outer : block;
}

/** An item's quote: its text; an item of an ordered list keeps its number, counted from the list's
 *  start over its items only. Not innerText, which is empty while the list keeps a new row hidden
 *  to measure it. */
export function itemQuote(item: HTMLElement): string {
  const text = replyText(item).trim();
  const list = item.parentElement;
  if (item.tagName !== "LI" || list?.tagName !== "OL") return text;
  const n = [...list.children].filter((el) => el.tagName === "LI").indexOf(item);
  return `${(list as HTMLOListElement).start + n}. ${text}`;
}

/** The block that a note marks in its row: the item, or the block where a text note's passage
 *  starts. A whole reply, or a passage the shown fold leaves out, marks none. */
function markedBlock(row: HTMLElement, note: Note): HTMLElement | null {
  const blocks = [...row.querySelectorAll<HTMLElement>(BLOCKS)].filter((b) => itemAt(b) === b);
  if (note.kind === "item") return blocks.find((b) => itemQuote(b) === note.quote) ?? null;
  if (note.kind === "reply") return null;
  const start = note.quote.trim().split("\n", 1)[0].slice(0, 60);
  return blocks.find((b) => replyText(b).includes(start)) ?? null;
}

/** Marks the items and replies that have notes, and in annotate mode makes items take Tab.
 *  Narrow (`wide` false): each card of a marked block gets a slot right under that block. The map
 *  that comes back holds, for each note of a shown row, its slot, or null for the end of the row.
 *  A card shows only once it is in the map, so it mounts once, in its place, and keeps its focus. Wide: the cards sit in the row's gutter,
 *  each at its block's top, pushed down to clear the card above.
 *  Rows remount as the list scrolls, so this runs again after each change of the list's rows. */
export function paintNotes(list: HTMLElement, marks: readonly { note: Note; n: number }[], annotating: boolean, wide: boolean): Map<number, HTMLElement | null> {
  for (const el of list.querySelectorAll("[data-inote]")) el.removeAttribute("data-inote");
  for (const el of list.querySelectorAll(REPLY_BODY)) {
    for (const block of el.querySelectorAll<HTMLElement>(BLOCKS)) {
      if (annotating && itemAt(block) === block) block.tabIndex = 0;
      else block.removeAttribute("tabindex");
    }
  }

  const slots = new Map<number, HTMLElement | null>();
  const marked = new Map<number, HTMLElement>();
  marks.forEach(({ note, n }) => {
    const row = list.querySelector<HTMLElement>(`[data-testid="row"][data-id="${CSS.escape(note.rowId)}"]`);
    if (!row) return;
    if (note.kind === "reply") row.dataset.inote = String(n);
    const block = markedBlock(row, note);
    slots.set(note.id, null);
    if (!block) return;
    if (note.kind === "item") block.dataset.inote = String(n);
    marked.set(note.id, block);
    if (wide) return;

    // The slots of one block follow it in note order. A slot in place is not moved, so a repaint
    // makes no change for the list's observer to see.
    const slot = list.querySelector<HTMLElement>(`.${NOTE_SLOT}[data-for="${note.id}"]`) ?? Object.assign(document.createElement("div"), { className: NOTE_SLOT });
    slot.dataset.for = String(note.id);
    let end: Element = block;
    let placed = false;
    while (end.nextElementSibling?.classList.contains(NOTE_SLOT)) {
      end = end.nextElementSibling;
      placed ||= end === slot;
    }
    if (!placed) end.after(slot);
    slots.set(note.id, slot);
  });

  for (const slot of list.querySelectorAll<HTMLElement>(`.${NOTE_SLOT}`)) if (!slots.get(Number(slot.dataset.for))) slot.remove();

  if (wide) {
    for (const gutter of list.querySelectorAll<HTMLElement>(".note-gutter")) {
      let top = gutter.getBoundingClientRect().top;
      let gap = 0;
      for (const card of gutter.querySelectorAll<HTMLElement>(NOTE_CARD)) {
        const block = marked.get(Number(card.dataset.note));
        const want = Math.max(block ? block.getBoundingClientRect().top : top, top + gap);
        card.style.marginTop = `${want - top}px`;
        top = card.getBoundingClientRect().bottom;
        gap = 8;
      }
    }
  } else for (const card of list.querySelectorAll<HTMLElement>(`.note-gutter ${NOTE_CARD}`)) card.style.marginTop = "";
  return slots;
}
