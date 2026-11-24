// One context menu and one source popover for the whole transcript list (decision 3). Rows stay
// plain elements; the menu finds the row from the DOM event.

import { BracesIcon, CopyIcon, MessageSquarePlusIcon, XIcon } from "lucide-react";
import { useEffect, useRef, useState, type RefObject } from "react";
import type { Row } from "../core/api";
import type { Line } from "../core/transcript";
import { ContextMenuContent, ContextMenuItem, ContextMenuLabel, ContextMenuSeparator } from "../components/ui/context-menu";
import { copyText } from "./copy";

const ROW = '[data-testid="row"]';
/** The text of an agent reply, without its buttons. */
export const REPLY_BODY = '[data-kind="assistant"] .msg .md, [data-kind="assistant"] .msg > .whitespace-pre-wrap';

/** A note's anchor: the row, and the text the owner selected in it. */
export type NoteSpot = { rowId: string; quote: string };

/** The selected text per agent reply, when both ends of the selection sit in agent replies. A
 *  selection over 2 replies gives 2 spots; rows between them that are not replies give none.
 *  A note card under a block sits inside the reply's markup but is not reply text: the
 *  selection's own text skips it, because its slot takes no selection (styles-notes.css), and a
 *  part of a range drops its slots. */
function selectedSpots(list: HTMLElement): NoteSpot[] {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || !sel.rangeCount) return [];
  const inReply = (node: Node | null) => {
    const el = node && list.contains(node) ? (node instanceof Element ? node : node.parentElement) : null;
    return !!el?.closest(REPLY_BODY) && !el.closest(".ncard");
  };
  if (!inReply(sel.anchorNode) || !inReply(sel.focusNode)) return [];
  const range = sel.getRangeAt(0);
  const bodies = [...list.querySelectorAll<HTMLElement>(REPLY_BODY)].filter((b) => range.intersectsNode(b));
  if (bodies.length === 1) {
    const rowId = bodies[0].closest<HTMLElement>(ROW)?.dataset.id;
    return rowId ? [{ rowId, quote: sel.toString() }] : [];
  }
  return bodies.flatMap((body) => {
    const rowId = body.closest<HTMLElement>(ROW)?.dataset.id;
    const part = document.createRange();
    part.selectNodeContents(body);
    if (range.compareBoundaryPoints(Range.START_TO_START, part) > 0) part.setStart(range.startContainer, range.startOffset);
    if (range.compareBoundaryPoints(Range.END_TO_END, part) < 0) part.setEnd(range.endContainer, range.endOffset);
    const text = part.cloneContents();
    for (const slot of text.querySelectorAll(".note-under")) slot.remove();
    const quote = text.textContent ?? "";
    return rowId && quote.trim() ? [{ rowId, quote }] : [];
  });
}

export type Source = { row: Row; rect: DOMRect };


/** Content for the list's one ContextMenu: Copy, Add note on selected reply text, and Show source
 *  for the row under the click. */
export function TranscriptMenu({ lines, listRef, onSource, onNote }: { lines: Line[]; listRef: RefObject<HTMLDivElement | null>; onSource: (s: Source) => void; onNote: (spots: NoteSpot[]) => number | null }) {
  const [target, setTarget] = useState<{ row: Row; el: HTMLElement; selection: string; spots: NoteSpot[] } | null>(null);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    // A native listener runs before React's root listener, so Radix opens with the row already set.
    const onContextMenu = (e: MouseEvent) => {
      const el = (e.target as Element).closest<HTMLElement>(ROW);
      const line = el && lines.find((l) => l.id === el.dataset.id);
      if (!el || !line) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      list.querySelector('[data-menu="true"]')?.removeAttribute("data-menu");
      el.setAttribute("data-menu", "true");
      const sel = window.getSelection();
      const inRow = !!sel && !sel.isCollapsed && el.contains(sel.anchorNode) && el.contains(sel.focusNode);
      setTarget({ row: line.row, el, selection: inRow ? sel.toString() : "", spots: selectedSpots(list) });
    };
    list.addEventListener("contextmenu", onContextMenu);
    return () => list.removeEventListener("contextmenu", onContextMenu);
  }, [lines, listRef]);

  const onClose = () => listRef.current?.querySelector('[data-menu="true"]')?.removeAttribute("data-menu");
  // A new note's box takes the caret once the menu has closed; its focus trap holds it until then.
  const noted = useRef<number | null>(null);

  if (!target) return null;
  const { row } = target;
  return (
    <ContextMenuContent
      onCloseAutoFocus={(e) => {
        onClose();
        if (noted.current === null) return;
        e.preventDefault();
        listRef.current?.querySelector<HTMLElement>(`[data-note="${noted.current}"] textarea`)?.focus({ preventScroll: true });
        noted.current = null;
      }}
      onEscapeKeyDown={onClose}
      onPointerDownOutside={onClose}
    >
      <ContextMenuLabel className="max-w-[260px] truncate normal-case">{row.kind === "fold" ? row.fold?.text : row.text.split("\n", 1)[0]}</ContextMenuLabel>
      <ContextMenuItem onSelect={() => void copyText(target.selection || row.text, target.selection ? "selection" : "row")}>
        <CopyIcon /> Copy
      </ContextMenuItem>
      {target.spots.length > 0 && (
        <ContextMenuItem
          onSelect={() => {
            noted.current = onNote(target.spots);
          }}
        >
          <MessageSquarePlusIcon /> Add note
        </ContextMenuItem>
      )}
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => onSource({ row, rect: target.el.getBoundingClientRect() })}>
        <BracesIcon /> Show source
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

/** The row's JSON next to the row. Esc, a click outside, or a scroll of the list closes it. */
export function SourcePopover({ source, listRef, onClose }: { source: Source; listRef: RefObject<HTMLDivElement | null>; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const json = JSON.stringify(source.row, null, 2);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    const onDown = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onClose();
    const list = listRef.current;
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown);
    list?.addEventListener("scroll", onClose, { passive: true });
    ref.current?.focus();
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown);
      list?.removeEventListener("scroll", onClose);
    };
  }, [listRef, onClose]);
  const width = Math.min(560, window.innerWidth - 32);
  const left = Math.max(16, Math.min(source.rect.left + 24, window.innerWidth - width - 16));
  const top = Math.max(16, Math.min(source.rect.top, window.innerHeight * 0.4));
  return (
    <div ref={ref} role="dialog" aria-label="Row source" tabIndex={-1} className="source" data-testid="source" style={{ left, top, width }}>
      <div className="flex items-center gap-2 border-b px-3 py-1.5 text-xs text-muted-foreground">
        <span className="min-w-0 flex-1 truncate font-mono">{source.row.id}</span>
        <button type="button" className="t-link" onClick={() => void copyText(json, "source")}>
          <CopyIcon className="size-3" /> Copy
        </button>
        <button type="button" className="icon-btn" aria-label="Close source" onClick={onClose}>
          <XIcon />
        </button>
      </div>
      <pre className="source-body">{json}</pre>
    </div>
  );
}
