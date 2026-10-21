// A row's actions, for its right-click menu and for the ⌘ bar that shows them as icons while ⌘ is
// held over the row. The tree and the Recent list share them, so the two cannot drift.

import { ArchiveIcon, ArchiveRestoreIcon, ChevronsDownUpIcon, ChevronsUpDownIcon, CopyIcon, FolderMinusIcon, MessageSquareIcon, PinIcon, PinOffIcon, PencilIcon, PlusIcon, SquareIcon, TerminalIcon, Trash2Icon, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState, type RefObject } from "react";
import type { App } from "../core/app";
import { archivedSession, type NavRow } from "../core/nav";
import { ContextMenuContent, ContextMenuItem, ContextMenuLabel, ContextMenuSeparator } from "../components/ui/context-menu";
import { copyText as copy } from "./copy";

export const ROW = '[data-testid="tree-row"]';

export const rowOf = (app: App, el: Element | null): NavRow | undefined => {
  const id = el?.closest<HTMLElement>(ROW)?.dataset.id;
  return id ? app.view.rows.find((r) => r.id === id) : undefined;
};

export const SEP = "-";
/** One row action. `menuOnly` keeps it out of the ⌘ bar, where the row itself or its chevron does it. */
type RowAction = { label: string; icon?: LucideIcon; run: () => void; disabled?: boolean; menuOnly?: boolean };

/** The actions of one row, for its context menu and its ⌘ bar. SEP marks a menu separator. */
export function rowActions(app: App, row: NavRow): (RowAction | typeof SEP)[] {
  const r = row.row;
  const archive = app.view.treeName === "archived";
  const fold: RowAction | false = row.expanded !== null && {
    label: row.expanded ? "Collapse" : "Expand",
    icon: row.expanded ? ChevronsDownUpIcon : ChevronsUpDownIcon,
    run: () => app.toggle(row.id),
    menuOnly: true,
  };
  let list: (RowAction | typeof SEP | false | undefined)[] = [];
  if (!r && row.kind === "project") {
    const { path, title } = row.section;
    list = [
      !archive && !!path && { label: "New Workspace…", icon: PlusIcon, run: () => app.life.newWorkspace(path) },
      { label: "Rename Project…", icon: PencilIcon, run: () => app.renameProject(path ?? "", title) },
      !archive && !!path && { label: "Setup script…", icon: TerminalIcon, run: () => app.life.editSetupScript(path, title) },
      !!path && { label: "Remove Project…", icon: FolderMinusIcon, run: () => app.life.removeProject(path, title) },
      SEP,
      fold,
    ];
  } else if (!r && row.kind === "pinned") list = [fold];
  else if (r?.kind === "archived-chat") list = [{ label: "Unarchive chat", icon: ArchiveRestoreIcon, run: () => app.unarchive(archivedSession(r.id)) }];
  else if (r?.kind === "chat" && !r.archived) list = chatActions(app, r.selection?.chat ?? undefined, fold);
  else if (r?.kind === "workspace" && (r.archived || archive))
    list = [
      { label: "Restore workspace", icon: ArchiveRestoreIcon, run: () => app.life.restoreWorkspace(r.id) },
      { label: "Delete workspace…", icon: Trash2Icon, run: () => void app.life.deleteWorkspace(r.id) },
    ];
  else if (r?.kind === "workspace") {
    const up = app.workspaceMoveTarget(r.id, -1);
    const down = app.workspaceMoveTarget(r.id, 1);
    list = [
      { label: "New chat", icon: PlusIcon, disabled: !r.newChatEnabled, run: () => void app.startChat(r.id) },
      r.pinned
        ? { label: "Unpin workspace", icon: PinOffIcon, run: () => void app.choose({ op: "unpin", workspace: r.id }) }
        : { label: "Pin workspace", icon: PinIcon, run: () => void app.pin(r.id) },
      { label: "Rename workspace…", icon: PencilIcon, run: () => app.renameWorkspace(r.id) },
      { label: "Move Up", disabled: !up, run: () => up && void app.choose({ op: "reorder_workspace", workspace: r.id, target: up }) },
      { label: "Move Down", disabled: !down, run: () => down && void app.choose({ op: "reorder_workspace", workspace: r.id, target: down }) },
      { label: "Archive workspace", icon: ArchiveIcon, run: () => void app.life.archiveWorkspace(r.id) },
      { label: "Delete workspace…", icon: Trash2Icon, run: () => void app.life.deleteWorkspace(r.id) },
      r.missing && { label: "Prune worktree…", run: () => void app.life.pruneWorkspace(r.id, row.section.title) },
      SEP,
      fold,
      { label: "Copy path", icon: CopyIcon, run: () => void copy(r.id, "path") },
    ];
  }
  // Drop the gaps, then any separator with nothing on one side of it.
  const out = list.filter((a): a is RowAction | typeof SEP => !!a);
  return out.filter((a, i) => a !== SEP || (i > 0 && i < out.length - 1 && out[i - 1] !== SEP));
}

/** A chat's actions, in the tree and in the Recent list. */
function chatActions(app: App, session: string | undefined, fold: RowAction | false = false): (RowAction | typeof SEP | false)[] {
  return [
    { label: "Rename chat…", icon: PencilIcon, disabled: !session, run: () => session && app.renameChat(session) },
    { label: "End chat…", icon: SquareIcon, disabled: !session, run: () => session && void app.life.endChat(session, false) },
    { label: "Archive chat", icon: ArchiveIcon, disabled: !session, run: () => session && void app.life.endChat(session, true) },
    SEP,
    !!session && { label: "Open chat", icon: MessageSquareIcon, run: () => app.openChat(session), menuOnly: true },
    fold,
    !!session && { label: "Copy session id", icon: CopyIcon, run: () => void copy(session, "session id") },
  ];
}

const RECENT = '[data-testid="recent-row"]';
type Target = { title: string; actions: (RowAction | typeof SEP)[] };

export const treeTarget = (app: App) => (el: Element): Target | undefined => {
  const row = rowOf(app, el);
  return row && { title: row.row?.title ?? row.section.title, actions: rowActions(app, row) };
};

export const recentTarget = (app: App) => (el: Element): Target | undefined => {
  const session = el.closest<HTMLElement>(RECENT)?.dataset.session;
  const chat = session && app.view.recent.find((c) => c.session === session);
  return chat ? { title: chat.title, actions: chatActions(app, session).filter((a): a is RowAction | typeof SEP => !!a) } : undefined;
};

/** Content for the Recent list's one ContextMenu: the chat under the right click. */
export function RecentMenu({ app, listRef }: { app: App; listRef: RefObject<HTMLDivElement | null> }) {
  const [target, setTarget] = useState<Target | null>(null);
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const onContextMenu = (e: MouseEvent) => {
      const t = recentTarget(app)(e.target as Element);
      if (!t) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      setTarget(t);
    };
    list.addEventListener("contextmenu", onContextMenu);
    return () => list.removeEventListener("contextmenu", onContextMenu);
  }, [app, listRef]);
  if (!target) return null;
  return (
    <ContextMenuContent data-testid="row-menu">
      <ContextMenuLabel className="max-w-[260px] truncate normal-case">{target.title}</ContextMenuLabel>
      {target.actions.map((a, i) =>
        a === SEP ? (
          <ContextMenuSeparator key={i} />
        ) : (
          <ContextMenuItem key={i} disabled={a.disabled} onSelect={a.run}>
            {a.icon && <a.icon />} {a.label}
          </ContextMenuItem>
        ),
      )}
    </ContextMenuContent>
  );
}

/** While ⌘ (or Ctrl) is held, the hovered row's actions show as icons over its right end. Like the
 *  other overlays it lives once per list; it sits inside the list so moving onto it keeps the row. */
export function RowBar({ listRef, select, target }: { listRef: RefObject<HTMLDivElement | null>; select: string; target: (el: Element) => Target | undefined }) {
  const [bar, setBar] = useState<{ el: Element; rect: DOMRect } | null>(null);
  const reset = useRef(() => {});

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    let held = false;
    let hovered: Element | null = null;
    const update = () => {
      setBar(held && hovered ? { el: hovered, rect: hovered.getBoundingClientRect() } : null);
    };
    const onOver = (e: PointerEvent) => {
      const target = e.target as Element;
      if (target.closest(".row-bar")) return;
      const next = target.closest(select);
      const meta = e.metaKey || e.ctrlKey;
      if (next === hovered && meta === held) return;
      hovered = next;
      held = meta;
      update();
    };
    const onLeave = () => {
      hovered = null;
      update();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Meta" && e.key !== "Control") return;
      const meta = e.type === "keydown";
      if (meta === held) return;
      held = meta;
      update();
    };
    // ⌘-Tab away sends no keyup to this window.
    const onBlur = () => {
      held = false;
      update();
    };
    const onScroll = () => hovered && held && update();
    reset.current = onLeave;
    list.addEventListener("pointerover", onOver);
    list.addEventListener("pointerleave", onLeave);
    list.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKey);
    window.addEventListener("blur", onBlur);
    return () => {
      list.removeEventListener("pointerover", onOver);
      list.removeEventListener("pointerleave", onLeave);
      list.removeEventListener("scroll", onScroll);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKey);
      window.removeEventListener("blur", onBlur);
    };
  }, [listRef, select]);

  const hit = bar && target(bar.el);
  if (!bar || !hit) return null;
  const actions = hit.actions.filter((a): a is RowAction & { icon: LucideIcon } => a !== SEP && !!a.icon && !a.menuOnly);
  if (!actions.length) return null;
  return (
    <div
      role="toolbar"
      aria-label={`Actions for ${hit.title}`}
      data-testid="row-bar"
      className="row-bar"
      // Over the first line's right end, which a long title then runs under.
      style={{ top: bar.rect.top + 1, right: window.innerWidth - bar.rect.right + 4 }}
    >
      {actions.map((a) => (
        <button
          key={a.label}
          disabled={a.disabled}
          aria-label={a.label}
          data-tip={a.label}
          onClick={() => {
            reset.current();
            a.run();
          }}
        >
          <a.icon />
        </button>
      ))}
    </div>
  );
}
