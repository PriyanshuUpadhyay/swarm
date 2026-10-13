// One context menu and one source popover for the whole transcript list (decision 3). Rows stay
// plain elements; the menu finds the row from the DOM event.

import { BracesIcon, CopyIcon, XIcon } from "lucide-react";
import { useEffect, useRef, useState, type RefObject } from "react";
import type { Row } from "../core/api";
import type { App } from "../core/app";
import { ContextMenuContent, ContextMenuItem, ContextMenuLabel, ContextMenuSeparator } from "../components/ui/context-menu";
import { copyText } from "./copy";

const ROW = '[data-testid="row"]';

export type Source = { row: Row; rect: DOMRect };

const lineOf = (app: App, el: Element | null) => {
  const id = el?.closest<HTMLElement>(ROW)?.dataset.id;
  return id ? app.view.lines.find((l) => l.id === id) : undefined;
};

/** Content for the list's one ContextMenu: Copy and Show source for the row under the click. */
export function TranscriptMenu({ app, listRef, onSource }: { app: App; listRef: RefObject<HTMLDivElement | null>; onSource: (s: Source) => void }) {
  const [target, setTarget] = useState<{ row: Row; el: HTMLElement; selection: string } | null>(null);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    // A native listener runs before React's root listener, so Radix opens with the row already set.
    const onContextMenu = (e: MouseEvent) => {
      const el = (e.target as Element).closest<HTMLElement>(ROW);
      const line = lineOf(app, el);
      if (!el || !line) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      list.querySelector('[data-menu="true"]')?.removeAttribute("data-menu");
      el.setAttribute("data-menu", "true");
      const sel = window.getSelection();
      setTarget({ row: line.row, el, selection: sel && !sel.isCollapsed && el.contains(sel.anchorNode) ? sel.toString() : "" });
    };
    list.addEventListener("contextmenu", onContextMenu);
    return () => list.removeEventListener("contextmenu", onContextMenu);
  }, [app, listRef]);

  const onClose = () => listRef.current?.querySelector('[data-menu="true"]')?.removeAttribute("data-menu");

  if (!target) return null;
  const { row } = target;
  return (
    <ContextMenuContent onCloseAutoFocus={onClose} onEscapeKeyDown={onClose} onPointerDownOutside={onClose}>
      <ContextMenuLabel className="max-w-[260px] truncate normal-case">{row.kind === "fold" ? row.fold?.text : row.text.split("\n", 1)[0]}</ContextMenuLabel>
      <ContextMenuItem onSelect={() => void copyText(target.selection || row.text, target.selection ? "selection" : "row")}>
        <CopyIcon /> Copy
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => onSource({ row, rect: target.el.getBoundingClientRect() })}>
        <BracesIcon /> Show source
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

/** The row's JSON next to the row. Esc, a click outside, or a scroll of the list closes it. */
export function SourcePopover({ source, onClose }: { source: Source; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const json = JSON.stringify(source.row, null, 2);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    const onDown = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onClose();
    const list = document.querySelector('[data-testid="transcript"]');
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown);
    list?.addEventListener("scroll", onClose, { passive: true });
    ref.current?.focus();
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown);
      list?.removeEventListener("scroll", onClose);
    };
  }, [onClose]);
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
