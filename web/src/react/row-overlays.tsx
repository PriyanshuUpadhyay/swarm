// One context menu, one tooltip and one hover card for a whole list of plain rows (decision 3).
// A Radix root per row would cost every key move and scroll frame. These components keep their own
// state, so a hover or a menu never re-renders the rows; they find the row from the DOM event.

import { ChevronsDownUpIcon, ChevronsUpDownIcon, CopyIcon, MessageSquareIcon } from "lucide-react";
import { useEffect, useState, type RefObject } from "react";
import type { App } from "../core/app";
import type { TreeRow } from "../core/tree";
import { ContextMenuContent, ContextMenuItem, ContextMenuLabel, ContextMenuSeparator } from "../components/ui/context-menu";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "../components/ui/hover-card";
import { Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { copyText as copy } from "./copy";
import { StatusGlyph } from "./status";

const ROW = '[data-testid="tree-row"]';
const TIP_DELAY = 400;
const CARD_DELAY = 800;

const rowOf = (app: App, el: Element | null): TreeRow | undefined => {
  const id = el?.closest<HTMLElement>(ROW)?.dataset.id;
  return id ? app.view.rows.find((r) => r.id === id) : undefined;
};

const rowEl = (list: HTMLElement, id: string) => list.querySelector<HTMLElement>(`${ROW}[data-id="${CSS.escape(id)}"]`);

/** Content for the list's one ContextMenu; the row is the one under the right click. */
export function RowMenu({ app, listRef }: { app: App; listRef: RefObject<HTMLDivElement | null> }) {
  const [row, setRow] = useState<TreeRow | null>(null);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    // A native listener runs before React's root listener, so Radix opens with the row already set.
    const onContextMenu = (e: MouseEvent) => {
      const target = rowOf(app, e.target as Element);
      if (!target) {
        // Off a row: no menu at all, not an empty one.
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      list.querySelector('[data-menu="true"]')?.removeAttribute("data-menu");
      rowEl(list, target.id)?.setAttribute("data-menu", "true");
      setRow(target);
    };
    list.addEventListener("contextmenu", onContextMenu);
    return () => list.removeEventListener("contextmenu", onContextMenu);
  }, [app, listRef]);

  const onClose = () => listRef.current?.querySelector('[data-menu="true"]')?.removeAttribute("data-menu");

  if (!row) return null;
  return (
    <ContextMenuContent onCloseAutoFocus={onClose} onEscapeKeyDown={onClose} onPointerDownOutside={onClose}>
      <ContextMenuLabel className="max-w-[260px] truncate normal-case">{row.label}</ContextMenuLabel>
      {row.sessionId ? (
        <>
          <ContextMenuItem onSelect={() => app.clickRow(row.id)}>
            <MessageSquareIcon /> Open chat
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => void copy(row.sessionId!, "session id")}>
            <CopyIcon /> Copy session id
          </ContextMenuItem>
        </>
      ) : (
        <>
          <ContextMenuItem onSelect={() => app.toggle(row.id)}>
            {row.expanded ? <ChevronsDownUpIcon /> : <ChevronsUpDownIcon />} {row.expanded ? "Collapse" : "Expand"}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => void copy(row.title, "path")}>
            <CopyIcon /> Copy path
          </ContextMenuItem>
        </>
      )}
    </ContextMenuContent>
  );
}

type Anchor = { rect: DOMRect };
type Tip = Anchor & { text: string };
type Card = Anchor & { row: TreeRow };

/** A fixed, invisible box over the hovered element; the Radix popup positions against it.
 *  The Radix trigger passes its ref and props through `asChild`, so they must reach the span. */
function Ghost({ rect, ...props }: Anchor & React.ComponentProps<"span">) {
  return (
    <span
      {...props}
      aria-hidden
      className="pointer-events-none fixed"
      style={{ left: rect.left, top: rect.top, width: rect.width, height: rect.height }}
    />
  );
}

/** The list's one tooltip (for `data-tip` elements) and one hover card (for chat and workspace rows). */
export function RowHints({ app, listRef }: { app: App; listRef: RefObject<HTMLDivElement | null> }) {
  const [tip, setTip] = useState<Tip | null>(null);
  const [card, setCard] = useState<Card | null>(null);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    let tipTimer = 0;
    let cardTimer = 0;
    let tipEl: Element | null = null;
    let cardEl: Element | null = null;
    // Runs on every key press, so it does nothing when nothing is hovered.
    const hide = () => {
      if (!tipEl && !cardEl) return;
      clearTimeout(tipTimer);
      clearTimeout(cardTimer);
      tipEl = cardEl = null;
      setTip(null);
      setCard(null);
    };
    const onOver = (e: PointerEvent) => {
      const target = e.target as Element;
      const nextTip = target.closest("[data-tip]");
      if (nextTip !== tipEl) {
        tipEl = nextTip;
        clearTimeout(tipTimer);
        setTip(null);
        if (nextTip)
          tipTimer = window.setTimeout(() => setTip({ rect: nextTip.getBoundingClientRect(), text: nextTip.getAttribute("data-tip")! }), TIP_DELAY);
      }
      const nextCard = target.closest(`${ROW}:is([data-kind="chat"],[data-kind="workspace"])`);
      if (nextCard !== cardEl) {
        cardEl = nextCard;
        clearTimeout(cardTimer);
        setCard(null);
        const row = rowOf(app, nextCard);
        if (nextCard && row) cardTimer = window.setTimeout(() => setCard({ rect: nextCard.getBoundingClientRect(), row }), CARD_DELAY);
      }
    };
    list.addEventListener("pointerover", onOver);
    list.addEventListener("pointerleave", hide);
    list.addEventListener("pointerdown", hide);
    list.addEventListener("contextmenu", hide);
    list.addEventListener("scroll", hide, { passive: true });
    window.addEventListener("keydown", hide);
    return () => {
      hide();
      list.removeEventListener("pointerover", onOver);
      list.removeEventListener("pointerleave", hide);
      list.removeEventListener("pointerdown", hide);
      list.removeEventListener("contextmenu", hide);
      list.removeEventListener("scroll", hide);
      window.removeEventListener("keydown", hide);
    };
  }, [app, listRef]);

  return (
    <>
      <Tooltip open={tip !== null} onOpenChange={(open) => !open && setTip(null)}>
        <TooltipTrigger asChild>{tip ? <Ghost rect={tip.rect} /> : <span hidden />}</TooltipTrigger>
        {tip && <TooltipContent side="top">{tip.text}</TooltipContent>}
      </Tooltip>
      <HoverCard open={card !== null && tip === null} onOpenChange={(open) => !open && setCard(null)}>
        <HoverCardTrigger asChild>{card ? <Ghost rect={card.rect} /> : <span hidden />}</HoverCardTrigger>
        {card && (
          <HoverCardContent side="right" align="start" sideOffset={10}>
            <RowCard row={card.row} />
          </HoverCardContent>
        )}
      </HoverCard>
    </>
  );
}

function RowCard({ row }: { row: TreeRow }) {
  return (
    <div className="grid gap-1.5">
      <div className="flex items-center gap-1.5">
        <span className="min-w-0 flex-1 truncate font-medium">{row.label}</span>
        <StatusGlyph status={row.state} />
      </div>
      <div className="font-mono text-xs break-all text-muted-foreground">{row.title}</div>
      <div className="flex gap-3 text-xs text-faint-foreground">
        <span className="capitalize">{row.kind}</span>
        <span>{row.state === "idle" ? "Idle" : row.state === "working" ? "Working" : "Waiting for you"}</span>
        {row.sessionId && <span className="font-mono">{row.sessionId.slice(0, 8)}</span>}
      </div>
    </div>
  );
}
