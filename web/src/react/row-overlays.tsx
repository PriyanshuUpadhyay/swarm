// One context menu, one tooltip, one hover card and one new-chat menu for a whole list of plain rows
// (decision 3). A Radix root per row would cost every key move and scroll frame. These components keep
// their own state, so a hover or a menu never re-renders the rows; they find the row from the DOM event.

import { ArchiveIcon, ArchiveRestoreIcon, ChevronsDownUpIcon, ChevronsUpDownIcon, CopyIcon, MessageSquareIcon, PinIcon, PinOffIcon, PencilIcon, PlusIcon, SquareIcon, TerminalIcon, Trash2Icon } from "lucide-react";
import { useEffect, useState, type RefObject } from "react";
import type { App } from "../core/app";
import type { Profile } from "../core/lifecycle";
import { archivedSession, type NavRow, type SidebarRow } from "../core/nav";
import { ContextMenuContent, ContextMenuItem, ContextMenuLabel, ContextMenuSeparator } from "../components/ui/context-menu";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from "../components/ui/dropdown-menu";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "../components/ui/hover-card";
import { Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { copyText as copy } from "./copy";
import { StatusGlyph } from "./status";

const ROW = '[data-testid="tree-row"]';
const TIP_DELAY = 400;
const CARD_DELAY = 800;

const rowOf = (app: App, el: Element | null): NavRow | undefined => {
  const id = el?.closest<HTMLElement>(ROW)?.dataset.id;
  return id ? app.view.rows.find((r) => r.id === id) : undefined;
};

const rowEl = (list: HTMLElement, id: string) => list.querySelector<HTMLElement>(`${ROW}[data-id="${CSS.escape(id)}"]`);

/** Content for the list's one ContextMenu; the row is the one under the right click. */
export function RowMenu({ app, listRef }: { app: App; listRef: RefObject<HTMLDivElement | null> }) {
  const [row, setRow] = useState<NavRow | null>(null);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    // A native listener runs before React's root listener, so Radix opens with the row already set.
    const onContextMenu = (e: MouseEvent) => {
      const target = rowOf(app, e.target as Element);
      if (!target || ["more", "child", "archived"].includes(target.kind)) {
        // Off a row: no menu at all, not an empty one.
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      list.querySelector('[data-menu="true"]')?.removeAttribute("data-menu");
      rowEl(list, target.id)?.setAttribute("data-menu", "true");
      setRow(target);
    };
    // Shift+F10 and the ContextMenu key open the focused row's menu. WebKit fires no contextmenu
    // for either, and Chromium none for Shift+F10, so one is sent at the row's corner.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "ContextMenu" && !(e.key === "F10" && e.shiftKey)) return;
      const row = (e.target as Element).closest<HTMLElement>(ROW);
      if (!row) return;
      e.preventDefault();
      const r = row.getBoundingClientRect();
      row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 24, clientY: r.bottom, button: 2 }));
    };
    list.addEventListener("contextmenu", onContextMenu);
    list.addEventListener("keydown", onKey);
    return () => {
      list.removeEventListener("contextmenu", onContextMenu);
      list.removeEventListener("keydown", onKey);
    };
  }, [app, listRef]);

  const onClose = () => listRef.current?.querySelector('[data-menu="true"]')?.removeAttribute("data-menu");

  if (!row) return null;
  const r = row.row;
  const archive = app.view.treeName === "archived";
  const fold = row.expanded !== null && (
    <ContextMenuItem onSelect={() => app.toggle(row.id)}>
      {row.expanded ? <ChevronsDownUpIcon /> : <ChevronsUpDownIcon />} {row.expanded ? "Collapse" : "Expand"}
    </ContextMenuItem>
  );
  return (
    <ContextMenuContent onCloseAutoFocus={onClose} onEscapeKeyDown={onClose} onPointerDownOutside={onClose} data-testid="row-menu">
      <ContextMenuLabel className="max-w-[260px] truncate normal-case">{r ? r.title : row.section.title}</ContextMenuLabel>
      {!r && row.kind === "project" && (
        <>
          {!archive && row.section.path && (
            <ContextMenuItem onSelect={() => app.life.newWorkspace(row.section.path!)}>
              <PlusIcon /> New Workspace…
            </ContextMenuItem>
          )}
          <ContextMenuItem onSelect={() => app.renameProject(row.section.path ?? "", row.section.title)}>
            <PencilIcon /> Rename Project…
          </ContextMenuItem>
          {!archive && row.section.path && (
            <ContextMenuItem onSelect={() => app.life.editSetupScript(row.section.path!, row.section.title)}>
              <TerminalIcon /> Setup script…
            </ContextMenuItem>
          )}
          {row.section.path && (
            <ContextMenuItem onSelect={() => app.life.removeProject(row.section.path!, row.section.title)}>
              Remove Project…
            </ContextMenuItem>
          )}
          {fold && <ContextMenuSeparator />}
          {fold}
        </>
      )}
      {!r && row.kind === "pinned" && fold}
      {r?.kind === "archived-chat" && (
        <ContextMenuItem onSelect={() => app.unarchive(archivedSession(r.id))}>
          <ArchiveRestoreIcon /> Unarchive chat
        </ContextMenuItem>
      )}
      {r?.kind === "chat" && <ChatItems app={app} row={r} fold={fold} />}
      {r?.kind === "workspace" && <WorkspaceItems app={app} row={r} fold={fold} archive={archive} project={row.section.title} />}
    </ContextMenuContent>
  );
}

function ChatItems({ app, row, fold }: { app: App; row: SidebarRow; fold: React.ReactNode }) {
  const session = row.selection?.chat;
  if (row.archived) return null;
  return (
    <>
      <ContextMenuItem disabled={!session} onSelect={() => session && app.renameChat(session)}>
        <PencilIcon /> Rename chat…
      </ContextMenuItem>
      <ContextMenuItem disabled={!session} onSelect={() => session && app.life.endChat(session, false)}>
        <SquareIcon /> End chat…
      </ContextMenuItem>
      <ContextMenuItem disabled={!session} onSelect={() => session && app.life.endChat(session, true)}>
        <ArchiveIcon /> Archive chat
      </ContextMenuItem>
      <ContextMenuSeparator />
      {session && (
        <ContextMenuItem onSelect={() => app.openChat(session)}>
          <MessageSquareIcon /> Open chat
        </ContextMenuItem>
      )}
      {fold}
      {session && (
        <ContextMenuItem onSelect={() => void copy(session, "session id")}>
          <CopyIcon /> Copy session id
        </ContextMenuItem>
      )}
    </>
  );
}

function WorkspaceItems({ app, row, fold, archive, project }: { app: App; row: SidebarRow; fold: React.ReactNode; archive: boolean; project: string }) {
  if (row.archived || archive)
    return (
      <>
        <ContextMenuItem onSelect={() => app.life.restoreWorkspace(row.id)}>
          <ArchiveRestoreIcon /> Restore workspace
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => void app.life.deleteWorkspace(row.id)}>
          <Trash2Icon /> Delete workspace…
        </ContextMenuItem>
      </>
    );
  const up = app.workspaceMoveTarget(row.id, -1);
  const down = app.workspaceMoveTarget(row.id, 1);
  return (
    <>
      <ContextMenuItem disabled={!row.newChatEnabled} onSelect={() => void app.startChat(row.id)}>
        <PlusIcon /> New chat
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => (row.pinned ? void app.choose({ op: "unpin", workspace: row.id }) : void app.pin(row.id))}>
        {row.pinned ? <PinOffIcon /> : <PinIcon />} {row.pinned ? "Unpin workspace" : "Pin workspace"}
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => app.renameWorkspace(row.id)}>
        <PencilIcon /> Rename workspace…
      </ContextMenuItem>
      <ContextMenuItem disabled={!up} onSelect={() => up && void app.choose({ op: "reorder_workspace", workspace: row.id, target: up })}>
        Move Up
      </ContextMenuItem>
      <ContextMenuItem disabled={!down} onSelect={() => down && void app.choose({ op: "reorder_workspace", workspace: row.id, target: down })}>
        Move Down
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void app.life.archiveWorkspace(row.id)}>
        <ArchiveIcon /> Archive workspace
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void app.life.deleteWorkspace(row.id)}>
        <Trash2Icon /> Delete workspace…
      </ContextMenuItem>
      {row.missing && <ContextMenuItem onSelect={() => void app.life.pruneWorkspace(row.id, project)}>Prune worktree…</ContextMenuItem>}
      <ContextMenuSeparator />
      {fold}
      <ContextMenuItem onSelect={() => void copy(row.id, "path")}>
        <CopyIcon /> Copy path
      </ContextMenuItem>
    </>
  );
}

/** `inTab` keeps the new chat out of the sidebar, as the tab strip's "+" does (ADR 0085). */
export type NewChatAnchor = { rect: DOMRect; workspace: SidebarRow; inTab?: boolean };

/** The list's one "+" menu (NewChatProfileMenu): the default profile, then each saved profile. */
export function NewChatMenu({ app, open }: { app: App; open: RefObject<((anchor: NewChatAnchor) => void) | null> }) {
  const [anchor, setAnchor] = useState<NewChatAnchor | null>(null);
  const [profiles, setProfiles] = useState<Profile[] | string | null>(null);
  useEffect(() => {
    open.current = setAnchor;
    return () => {
      open.current = null;
    };
  }, [open]);
  useEffect(() => {
    if (anchor) app.life.profiles().then(setProfiles, (err) => setProfiles(String(err instanceof Error ? err.message : err)));
  }, [app, anchor]);
  return (
    <DropdownMenu open={anchor !== null} onOpenChange={(o) => !o && setAnchor(null)}>
      <DropdownMenuTrigger asChild>{anchor ? <Ghost rect={anchor.rect} /> : <span hidden />}</DropdownMenuTrigger>
      {anchor && (
        <DropdownMenuContent align="start" data-testid="new-chat-menu">
          <DropdownMenuLabel className="max-w-[240px] truncate normal-case">New chat in {anchor.workspace.title}</DropdownMenuLabel>
          <DropdownMenuItem onSelect={() => void app.startChat(anchor.workspace.id, {}, "", anchor.inTab)}>
            <PlusIcon /> Default profile
          </DropdownMenuItem>
          {profiles === null && <DropdownMenuItem disabled>Reading profiles…</DropdownMenuItem>}
          {typeof profiles === "string" && <DropdownMenuItem disabled>Profiles unavailable: {profiles}</DropdownMenuItem>}
          {Array.isArray(profiles) &&
            profiles.map((p) => (
              <DropdownMenuItem key={p.name} data-testid="profile-item" onSelect={() => void app.startChat(anchor.workspace.id, { role: p.name }, "", anchor.inTab)}>
                <span className="flex-1">{p.name}</span>
                {p.runners[0] && <span className="text-xs text-faint-foreground">{[p.runners[0].provider, p.runners[0].model].filter(Boolean).join(" ")}</span>}
              </DropdownMenuItem>
            ))}
        </DropdownMenuContent>
      )}
    </DropdownMenu>
  );
}

type Anchor = { rect: DOMRect };
type Tip = Anchor & { text: string };
type Card = Anchor & { row: SidebarRow };

/** A fixed, invisible box over the hovered element; the Radix popup positions against it.
 *  The Radix trigger passes its ref and props through `asChild`, so they must reach the span. */
export function Ghost({ rect, ...props }: Anchor & React.ComponentProps<"span">) {
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
        const row = rowOf(app, nextCard)?.row;
        if (nextCard && row) cardTimer = window.setTimeout(() => setCard({ rect: nextCard.getBoundingClientRect(), row }), CARD_DELAY);
      }
    };
    list.addEventListener("pointerover", onOver);
    list.addEventListener("pointerleave", hide);
    list.addEventListener("pointerdown", hide);
    list.addEventListener("contextmenu", hide);
    list.addEventListener("dragstart", hide);
    list.addEventListener("scroll", hide, { passive: true });
    window.addEventListener("keydown", hide);
    return () => {
      hide();
      list.removeEventListener("pointerover", onOver);
      list.removeEventListener("pointerleave", hide);
      list.removeEventListener("pointerdown", hide);
      list.removeEventListener("contextmenu", hide);
      list.removeEventListener("dragstart", hide);
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

/** The row's help text (its title and path), status counts and detail. */
function RowCard({ row }: { row: SidebarRow }) {
  const [title, ...rest] = row.help.split("\n");
  return (
    <div className="grid gap-1.5">
      <div className="flex items-center gap-1.5">
        <span className="min-w-0 flex-1 truncate font-medium">{title || row.title}</span>
        {row.status && <StatusGlyph status={row.status} />}
      </div>
      {rest.length > 0 && <div className="font-mono text-xs break-all whitespace-pre-wrap text-muted-foreground">{rest.join("\n")}</div>}
      <div className="flex flex-wrap gap-3 text-xs text-faint-foreground">
        <span className="capitalize">{row.kind}</span>
        {row.detail && <span>{row.detail}</span>}
        {row.counts.map((c) => (
          <span key={c.status} className="inline-flex items-center gap-1">
            <StatusGlyph status={c.status} /> {c.count}
          </span>
        ))}
        {row.selection?.chat && <span className="font-mono">{row.selection?.chat.slice(0, 8)}</span>}
      </div>
    </div>
  );
}
