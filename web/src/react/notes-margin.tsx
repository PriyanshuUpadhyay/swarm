// Chat notes (ADR 0162): the numbered cards, their places, and the notes bar above the composer.

import { TextCursorInputIcon, XIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { plural } from "../core/lifecycle-state";
import { noteMessage, type Note, type Notes } from "../core/notes";
import { Button } from "../components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../components/ui/dialog";
import { copyText } from "./copy";
import { NOTE_CARD } from "./note-marks";
import { REPLY_BODY } from "./transcript-overlays";

const SOURCE: Record<Note["kind"], string> = { text: "On selected text", item: "On item", reply: "On whole reply" };
/** A quote longer than this folds to 3 lines with Show all. */
const LONG_QUOTE = 240;

/** One card: its number, the quote it marks, and the note box, which saves on each change. In a
 *  narrow list only the number shows until the card holds the focus (styles-notes.css); a click
 *  on the number puts the caret in the box. */
export function NoteCard({ notes, chat, note, n }: { notes: Notes; chat: string; note: Note; n: number }) {
  const box = useRef<HTMLTextAreaElement>(null);
  const [all, setAll] = useState(false);
  useEffect(() => {
    if (notes.takeFocus(note.id)) box.current?.focus({ preventScroll: true });
  }, [notes, note.id]);

  const long = note.quote.length > LONG_QUOTE || note.quote.split("\n").length > 3;
  const lines = Math.min(8, Math.max(2, note.text.split("\n").length));
  return (
    <article className="ncard" data-testid="note-card" data-note={note.id} aria-label={`Note ${n}`} hidden>
      <div className="ncard-head">
        <button type="button" className="nbadge num" aria-label={`Open note ${n}`} onClick={() => box.current?.focus({ preventScroll: true })}>
          {n}
        </button>
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
  const text = (item.textContent ?? "").trim();
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
  return blocks.find((b) => b.textContent?.includes(start)) ?? null;
}

/** Marks the items and replies that have notes, and in annotate mode makes items take Tab. The
 *  map that comes back holds, for each note of a shown row, the element its card sits beside: the
 *  marked block, or the row for a whole reply or a passage that the shown fold leaves out. Rows
 *  remount as the list scrolls, so this runs again after each change of the list's rows. */
export function paintNotes(list: HTMLElement, marks: readonly { note: Note; n: number }[], annotating: boolean): Map<number, HTMLElement> {
  for (const el of list.querySelectorAll("[data-inote]")) el.removeAttribute("data-inote");
  for (const el of list.querySelectorAll(REPLY_BODY)) {
    for (const block of el.querySelectorAll<HTMLElement>(BLOCKS)) {
      if (annotating && itemAt(block) === block) block.tabIndex = 0;
      else block.removeAttribute("tabindex");
    }
  }

  const anchors = new Map<number, HTMLElement>();
  for (const { note, n } of marks) {
    const row = list.querySelector<HTMLElement>(`[data-testid="row"][data-id="${CSS.escape(note.rowId)}"]`);
    if (!row) continue;
    if (note.kind === "reply") row.dataset.inote = String(n);
    const block = markedBlock(row, note);
    if (block && note.kind === "item") block.dataset.inote = String(n);
    anchors.set(note.id, block ?? row);
  }
  return anchors;
}

/** The height a collapsed card takes in a narrow list: its number badge and a gap. */
const BADGE = 22;

/** Places the cards of the note layer, which scrolls with the rows but sits outside them, so a
 *  card never changes a row's height and the list never jumps. Each card goes to the top of its
 *  anchor, pushed down to clear the card above. Wide, cards sit in the gutter right of the rows;
 *  narrow, each collapsed card sits on its block's right edge. A card whose row is not mounted
 *  keeps its last place, or stays hidden until its row first shows. */
export function placeCards(layer: HTMLElement, anchors: ReadonlyMap<number, HTMLElement>, wide: boolean) {
  const box = layer.getBoundingClientRect();
  const cards = [...layer.querySelectorAll<HTMLElement>(NOTE_CARD)].map((card) => {
    const anchor = anchors.get(Number(card.dataset.note));
    if (!anchor) return { card, top: card.hidden ? null : parseFloat(card.style.top) };
    const at = anchor.getBoundingClientRect();
    card.style.right = wide ? "0px" : `${Math.max(0, box.right - at.right)}px`;
    card.hidden = false;
    return { card, top: at.top - box.top };
  });

  let floor = -Infinity;
  for (const { card, top } of cards.filter((c) => c.top !== null).sort((a, b) => a.top! - b.top!)) {
    const at = Math.max(top!, floor);
    card.style.top = `${at}px`;
    floor = at + (wide ? card.offsetHeight + 8 : BADGE);
  }
}
