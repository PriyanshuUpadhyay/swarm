// The chat page, then columns of child agent panes that scroll in from the right with no snap
// (ADR 0030). The owner drags a column's edge to size every column and the line in a two-pane
// column to split it; a double-click resets (ADR 0026). A finished child stays dimmed until it is
// dismissed (ADR 0056). One context menu serves every pane header (decision 3).

import { CopyIcon, Maximize2Icon, Minimize2Icon, PowerIcon, SquareIcon, TerminalIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";
import { toast } from "sonner";
import type { App, Pane, Strip } from "../core/app";
import { attachCommand, chatWidth, columns, columnWidth, midTurn, saveSplit, splitShare, waitingMessage, type PaneCell, type PaneWidths } from "../core/panes";
import { Button } from "../components/ui/button";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuLabel, ContextMenuTrigger } from "../components/ui/context-menu";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../components/ui/dialog";
import { PromptCard } from "./composer";
import { copyText } from "./copy";
import { StatusGlyph } from "./status";
import { store } from "./store";
import { ChatView, PaneBody } from "./transcript";

// Kept in the browser until P2 serves view state.
const WIDTHS_KEY = "swarm.panes.widths";
/** A grip's width in px; it sits on the trailing edge of the chat page and of each column. */
const GRIP = 7;

function readWidths(): PaneWidths {
  try {
    return JSON.parse(store.getItem(WIDTHS_KEY) ?? "{}") ?? {};
  } catch {
    return {};
  }
}

type Action = { title: string; label: string; icon: React.ReactNode; disabled?: boolean; run: () => void };

const failed = (err: unknown) => toast.error(err instanceof Error ? err.message : String(err));

export function PaneStrip({ app }: { app: App }) {
  const strip = useSyncExternalStore(app.subscribe, () => app.view.strip);
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const { cells } = strip;
  const hasPanes = cells.length > 0;
  const scroller = useRef<HTMLDivElement>(null);
  const aside = useRef<HTMLElement>(null);
  // A mutable box, not state: a drag changes widths without a render.
  const [widths] = useState<{ current: PaneWidths }>(() => ({ current: readWidths() }));
  const [menu, setMenu] = useState<string | null>(null);
  const [closing, setClosing] = useState<string | null>(null);
  const cols = columns(cells.length).map((col) => col.map((i) => cells[i]));

  const save = () => store.setItem(WIDTHS_KEY, JSON.stringify(widths.current));
  const main = () => scroller.current?.clientWidth ?? 0;
  /** Sets the chat page and column widths from the main area and the saved drags. */
  const layout = () => {
    const el = scroller.current;
    if (!el) return;
    if (!hasPanes) return el.style.removeProperty("--chat-w");
    el.style.setProperty("--chat-w", `${chatWidth(main(), widths.current.chat) - GRIP}px`);
    el.style.setProperty("--col-w", `${columnWidth(main(), widths.current.column) - GRIP}px`);
  };

  useLayoutEffect(() => {
    layout();
    const observer = new ResizeObserver(layout);
    observer.observe(scroller.current!);
    return () => observer.disconnect();
  }, [hasPanes]);

  // Another chat starts at its chat page.
  useLayoutEffect(() => {
    if (scroller.current) scroller.current.scrollLeft = 0;
  }, [openId]);

  // A key or "Show column" moved focus: scroll to the pane with no animation, and move the keyboard.
  const seen = useRef(strip.revealSeq);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el || seen.current === strip.revealSeq) return;
    seen.current = strip.revealSeq;
    const pane = strip.reveal === null ? null : el.querySelector<HTMLElement>(`[data-pane="${CSS.escape(strip.reveal)}"]`);
    if (!pane) {
      el.scrollLeft = 0;
      // ⌘L has already put the keyboard in the chat's composer.
      if (!el.querySelector(".strip-chat")?.contains(document.activeElement)) el.querySelector<HTMLElement>('[data-testid="transcript"]')?.focus();
      return;
    }
    const col = pane.closest<HTMLElement>(".strip-col")!;
    if (col.offsetLeft < el.scrollLeft) el.scrollLeft = col.offsetLeft;
    else if (col.offsetLeft + col.offsetWidth + GRIP > el.scrollLeft + el.clientWidth) el.scrollLeft = col.offsetLeft + col.offsetWidth + GRIP - el.clientWidth;
    (pane.querySelector<HTMLElement>('[data-testid="composer"]:not(:disabled)') ?? pane.querySelector<HTMLElement>('[data-testid="pane-transcript"]'))?.focus();
  }, [strip.revealSeq]);

  // The menu is for the header under the right click. This listener runs before React's root
  // listener, so the trigger is already off when the click is not on a header, and the transcript's
  // own menu or the browser's shows instead.
  useEffect(() => {
    const el = aside.current;
    if (!el) return;
    const onContextMenu = (e: MouseEvent) => {
      const id = (e.target as Element).closest<HTMLElement>("[data-pane-head]")?.dataset.paneHead ?? null;
      flushSync(() => setMenu(id));
    };
    el.addEventListener("contextmenu", onContextMenu);
    return () => el.removeEventListener("contextmenu", onContextMenu);
  }, [openId]);

  const actions = (cell: PaneCell): Action[] => {
    const zoomed = strip.zoomed === cell.id;
    const list: Action[] = [
      { title: "Stop", label: `Stop ${cell.id}`, icon: <SquareIcon className="fill-current" />, disabled: !midTurn(cell.status), run: () => void app.stopPane(cell.id).catch(failed) },
      {
        title: "Close agent…",
        label: `Close agent ${cell.id}`,
        icon: <PowerIcon />,
        // Only a child in a turn asks first (SwarmAgentCell.requiresCloseConfirmation).
        run: () => (midTurn(cell.status) ? setClosing(cell.id) : void app.closePane(cell.id).catch(failed)),
      },
      { title: "Copy id", label: `Copy id of ${cell.id}`, icon: <CopyIcon />, run: () => void copyText(cell.id, "agent id") },
      { title: "Copy attach command", label: `Copy attach command for ${cell.id}`, icon: <TerminalIcon />, run: () => void copyText(attachCommand(cell.id), "attach command") },
      {
        title: zoomed ? "Return to the strip (⇧⌘↩)" : "Zoom pane (⇧⌘↩)",
        label: `${zoomed ? "Unzoom" : "Zoom"} ${cell.id}`,
        icon: zoomed ? <Minimize2Icon /> : <Maximize2Icon />,
        run: () => app.toggleZoom(cell.id),
      },
    ];
    return list;
  };

  const dragStart = useRef(0);
  const dragColumn = useRef({ width: 0, offset: 0 });
  /** Every column changes width, so the strip scrolls by the change of the columns before this
   *  one. Its leading edge stays put, and the dragged edge follows the pointer. */
  const resizeColumns = (preferred: number | null, index: number, start: { width: number; offset: number }) => {
    const width = columnWidth(main(), preferred);
    widths.current = { ...widths.current, column: preferred === null ? undefined : width };
    layout();
    scroller.current!.scrollLeft = start.offset + index * (width - start.width);
  };
  const columnStart = () => ({ width: columnWidth(main(), widths.current.column), offset: scroller.current!.scrollLeft });
  const setSplit = (el: HTMLElement, column: string, share: number | null) => {
    el.style.setProperty("--split", String(splitShare(share)));
    widths.current = saveSplit(widths.current, openId!, cols.map((c) => c[0].id), column, share === null ? null : splitShare(share));
  };

  const menuCell = cells.find((c) => c.id === menu);
  const pane = (cell: PaneCell) => (
    <PaneView
      key={cell.id}
      app={app}
      cell={cell}
      pane={strip.panes.get(cell.id)!}
      focused={strip.focused === cell.id}
      zoomed={strip.zoomed === cell.id}
      actions={actions(cell)}
    />
  );

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      {/* A zoomed pane fills this box, over the strip. */}
      <div className="relative min-h-0 flex-1">
        <div ref={scroller} className="strip" data-panes={hasPanes} data-testid="strip">
          <section className="strip-chat" onPointerDownCapture={() => app.paneFocused(null)} onFocusCapture={() => app.paneFocused(null)}>
            <Asking app={app} strip={strip} />
            <ChatView app={app} />
          </section>
          {hasPanes && (
            <Grip
              axis="x"
              label="Chat page width"
              onStart={() => (dragStart.current = chatWidth(main(), widths.current.chat))}
              onDrag={(dx) => {
                widths.current = { ...widths.current, chat: chatWidth(main(), dragStart.current + dx) };
                layout();
              }}
              onEnd={save}
              onReset={() => {
                widths.current = { ...widths.current, chat: undefined };
                layout();
                save();
              }}
            />
          )}
          <ContextMenu>
            <ContextMenuTrigger asChild disabled={!menuCell}>
              <aside ref={aside} key={openId} data-testid="pane-strip" aria-label="Agent panes" className="flex empty:hidden">
                {cols.map((col, index) => (
                  <div key={col[0].id} className="contents">
                    <div className="strip-col" style={{ "--split": splitShare(widths.current.splits?.[openId!]?.[col[0].id]) } as React.CSSProperties}>
                      {pane(col[0])}
                      {col[1] && (
                        <Grip
                          axis="y"
                          label={`Split of ${col[0].id}`}
                          onStart={(el) => (dragStart.current = Number(el.parentElement!.style.getPropertyValue("--split")))}
                          onDrag={(dy, el) => setSplit(el.parentElement!, col[0].id, dragStart.current + dy / Math.max(el.parentElement!.clientHeight, 1))}
                          onEnd={save}
                          onReset={(el) => {
                            setSplit(el.parentElement!, col[0].id, null);
                            save();
                          }}
                        />
                      )}
                      {col[1] && pane(col[1])}
                    </div>
                    <Grip
                      axis="x"
                      label="Agent column width"
                      onStart={() => (dragColumn.current = columnStart())}
                      onDrag={(dx) => resizeColumns(dragColumn.current.width + dx, index, dragColumn.current)}
                      onEnd={save}
                      onReset={() => {
                        resizeColumns(null, index, columnStart());
                        save();
                      }}
                    />
                  </div>
                ))}
              </aside>
            </ContextMenuTrigger>
            {menuCell && (
              <ContextMenuContent>
                <ContextMenuLabel className="max-w-[260px] truncate normal-case">{menuCell.id}</ContextMenuLabel>
                {actions(menuCell).map((a) => (
                  <ContextMenuItem key={a.label} disabled={a.disabled} onSelect={a.run}>
                    {a.icon} {a.title}
                  </ContextMenuItem>
                ))}
              </ContextMenuContent>
            )}
          </ContextMenu>
        </div>
      </div>
      <Dialog open={closing !== null} onOpenChange={(open) => !open && setClosing(null)}>
        <DialogContent className="top-1/3 grid w-[min(420px,calc(100vw-32px))] gap-3 p-5" data-testid="close-agent">
          <DialogTitle>Close “{closing}”?</DialogTitle>
          <DialogDescription>“{closing}” is in a turn. Closing the agent stops it.</DialogDescription>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setClosing(null)}>
              Cancel
            </Button>
            <Button
              className="bg-failed text-white hover:bg-failed/90"
              onClick={() => {
                void app.closePane(closing!).catch(failed);
                setClosing(null);
              }}
            >
              Close Agent
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** Each child's question also shows on the chat page, so the owner can answer it while the strip
 *  is scrolled away (ADR 0029). */
function Asking({ app, strip }: { app: App; strip: Strip }) {
  const asking = strip.cells.flatMap((cell) => {
    const pane = strip.panes.get(cell.id);
    const prompt = pane?.composer?.agent.prompt;
    return pane?.composer && prompt ? [{ cell, pane, prompt, state: pane.composer.state }] : [];
  });
  if (!asking.length) return null;
  return (
    <div className="max-h-[40%] shrink-0 overflow-y-auto px-6 pt-3" data-testid="asking">
      <div className="mx-auto grid max-w-[820px] gap-2">
        {asking.map(({ cell, pane, prompt, state }) => (
          <PromptCard key={cell.id} app={app} sessionId={pane.id} who={cell.id} prompt={prompt} state={state} showColumn={() => app.focusPane(cell.id)} />
        ))}
      </div>
    </div>
  );
}

/** A clear grip on a pane edge. It takes no focus, so keys stay with the columns. `onDrag` gets the
 *  pointer's travel since the drag began, along the axis. */
function Grip({
  axis,
  label,
  onStart,
  onDrag,
  onEnd,
  onReset,
}: {
  axis: "x" | "y";
  label: string;
  onStart: (el: HTMLElement) => void;
  onDrag: (travel: number, el: HTMLElement) => void;
  onEnd: () => void;
  onReset: (el: HTMLElement) => void;
}) {
  const start = useRef<number | null>(null);
  const at = (e: React.PointerEvent) => (axis === "x" ? e.clientX : e.clientY);
  const end = () => {
    if (start.current === null) return;
    start.current = null;
    onEnd();
  };
  return (
    <div
      role="separator"
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      aria-label={label}
      title="Drag to resize; double-click to reset"
      className={`grip ${axis}`}
      data-testid="grip"
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        start.current = at(e);
        onStart(e.currentTarget);
      }}
      onPointerMove={(e) => start.current !== null && onDrag(at(e) - start.current, e.currentTarget)}
      onPointerUp={end}
      onPointerCancel={end}
      onDoubleClick={(e) => onReset(e.currentTarget)}
    />
  );
}

function PaneView({ app, cell, pane, focused, zoomed, actions }: { app: App; cell: PaneCell; pane: Pane; focused: boolean; zoomed: boolean; actions: Action[] }) {
  const ref = useRef<HTMLElement>(null);
  // Only a column on screen follows its log.
  useEffect(() => {
    const observer = new IntersectionObserver((entries) => app.setPaneVisible(cell.id, entries[entries.length - 1].isIntersecting));
    observer.observe(ref.current!);
    return () => {
      observer.disconnect();
      app.setPaneVisible(cell.id, false);
    };
  }, [app, cell.id]);
  const focus = () => app.paneFocused(cell.id);
  return (
    <section
      ref={ref}
      className="pane"
      data-testid="pane"
      data-pane={cell.id}
      data-status={cell.status}
      data-focused={focused}
      data-zoomed={zoomed}
      aria-label={`Agent ${cell.id}`}
      onPointerDownCapture={focus}
      onFocusCapture={focus}
    >
      <header className="pane-head" data-pane-head={cell.id} aria-label={`Controls for ${cell.id}`}>
        <StatusGlyph status={cell.status} />
        <span className="shrink-0 font-semibold">{cell.id}</span>
        <span className="min-w-0 flex-1 truncate text-muted-foreground">
          {cell.role} · {cell.model}
        </span>
        {actions.map((a) => (
          <button key={a.label} type="button" className="icon-btn" aria-label={a.label} title={a.title} disabled={a.disabled} onClick={a.run}>
            {a.icon}
          </button>
        ))}
      </header>
      <div className="pane-body">
        <PaneBody app={app} pane={pane} empty={waitingMessage(cell.provider)} />
      </div>
    </section>
  );
}
