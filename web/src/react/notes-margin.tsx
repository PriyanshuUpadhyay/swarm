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
