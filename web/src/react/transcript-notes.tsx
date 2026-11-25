// Chat notes in the transcript (ADR 0162): annotate mode, the note layer, and the notes bar.
// A child's column shows its own notes, which live in the main chat and go to its chair.

import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type RefObject } from "react";
import type { Pane, Reader } from "../core/app";
import { REPLY_HANDLE } from "./note-marks";
import { itemAt, itemQuote, NoteCard, NotesBar, paintNotes, placeCards } from "./notes-margin";
import type { NoteSpot } from "./transcript-overlays";

/** A list at least this wide shows cards in a gutter right of the rows; styles-notes.css owns it. */
const wideAt = (list: HTMLElement) => parseFloat(getComputedStyle(list).getPropertyValue("--wide-notes"));

/** The notes of the open chat for one transcript list. */
export function useTranscriptNotes(app: Reader, view: Pane, listRef: RefObject<HTMLDivElement | null>) {
  const { lines, id: openId, chatState, find } = view;
  const { notes } = app;
  const { chat, agent } = notes.home(openId);
  // Snapshots, not reads in render: the React compiler keeps a render's result while its props stay the same.
  const readList = () => notes.list(chat);
  const readMode = () => notes.annotating();
  const noteList = useSyncExternalStore(notes.subscribe, readList, readList);
  const annotating = useSyncExternalStore(notes.subscribe, readMode, readMode);

  // This column's notes; each card shows its number in the main chat's order.
  const own = noteList.flatMap((note, i) => (note.agent === agent ? [{ note, n: i + 1 }] : []));

  /** Adds a text note per spot; returns the last card's id, whose box takes the caret. */
  const addNotes = (spots: NoteSpot[]) => {
    let last: number | null = null;
    for (const spot of spots) last = notes.add(chat, { kind: "text", ...spot, agent })?.id ?? last;
    return last;
  };

  /** Annotate mode: a note on the item under `target`, or on the whole reply from its handle.
   *  True when the mode takes the click, also when add refuses the note (the cap, or an item with
   *  no text), so the item's own link or button never runs. */
  const annotate = (target: Element) => {
    if (!annotating) return false;
    const whole = target.closest(REPLY_HANDLE);
    const rowEl = target.closest<HTMLElement>('[data-testid="row"]');
    const row = rowEl && lines.find((l) => l.id === rowEl.dataset.id)?.row;
    if (whole && row) {
      notes.add(chat, { kind: "reply", rowId: row.id, quote: row.text, agent });
      return true;
    }
    const item = itemAt(target);
    if (!item || !row || window.getSelection()?.isCollapsed === false) return false;
    notes.add(chat, { kind: "item", rowId: row.id, quote: itemQuote(item), agent });
    return true;
  };

  // Before the item's own handlers: in annotate mode a click on an item, also on a link or a
  // button in it, only adds a note.
  const onClickCapture = (e: React.MouseEvent) => {
    if (!annotate(e.target as Element)) return;
    e.preventDefault();
    e.stopPropagation();
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    const el = e.target;
    if ((e.key === "Enter" || e.key === " ") && el instanceof HTMLElement && el.tabIndex === 0 && itemAt(el) === el && annotate(el)) e.preventDefault();
  };

  const [wideList, setWideList] = useState(false);
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list || chatState !== "ready") return;
    const watch = new ResizeObserver(() => setWideList(list.clientWidth >= wideAt(list)));
    watch.observe(list);
    return () => watch.disconnect();
  }, [chatState, listRef]);
  const wide = wideList && own.length > 0;
  const layerRef = useRef<HTMLDivElement>(null);

  // Marks follow the rows, which remount as the list scrolls, and cards follow their blocks, which
  // move when a row above them changes height. With no notes and the mode off, one paint clears
  // what an earlier state left, and no observer runs.
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list || chatState !== "ready") return;

    const paint = () => {
      const anchors = paintNotes(list, own, annotating);
      if (layerRef.current) placeCards(layerRef.current, anchors, wide);
    };
    paint();

    if (own.length === 0 && !annotating) return;
    const rows = new MutationObserver(paint);
    rows.observe(list, { childList: true, subtree: true });
    // Placing a card writes only its top, so a resize that it causes cannot ring again.
    const sizes = new ResizeObserver(paint);
    for (const el of list.children) if (el !== layerRef.current) sizes.observe(el);
    for (const card of layerRef.current?.children ?? []) sizes.observe(card);
    return () => {
      rows.disconnect();
      sizes.disconnect();
    };
    // The React compiler keeps `own` while `noteList` and `agent` stay the same.
  }, [own, annotating, chatState, lines, wide, listRef]);

  // Esc leaves annotate mode first, also from the composer, whose own Esc would put its draft
  // away. An open find bar, dialog, popover or menu keeps its own Esc.
  useEffect(() => {
    if (!annotating) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || find.open || document.querySelector('[role="dialog"], [role="menu"]')) return;
      e.preventDefault();
      e.stopPropagation();
      notes.setMode(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [annotating, find.open, notes]);

  // One layer in the scrolling list holds every card of this column; placeCards sets each top.
  const layer = own.length > 0 && (
    <div ref={layerRef} className={`note-layer ${wide ? "wide" : "narrow"}`}>
      {own.map(({ note, n }) => <NoteCard key={note.id} notes={notes} chat={chat} note={note} n={n} />)}
    </div>
  );

  const overlay = chatState === "ready" && (
    <>
      <button type="button" className="annotate-pill" data-testid="annotate-mode" aria-pressed={annotating} title="Annotate Chat (⇧⌘A)" onClick={() => notes.toggleMode()}>
        {annotating ? "Annotate · Esc" : "Annotate"}
      </button>
      {(!agent || own.length > 0) && <NotesBar notes={notes} chat={chat} list={noteList} child={!!agent} blocked={view.noteComposer ? null : "This chat takes no messages."} />}
    </>
  );

  const className = `${annotating ? " annotating" : ""}${wide ? " wide-notes" : ""}`;
  return { addNotes, onClickCapture, onKeyDown, layer, overlay, className };
}
