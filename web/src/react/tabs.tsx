// The chat tab strip on the Rust tab view (P2a, ADR 0054) in jean's look: owner order, drag to
// reorder, close, an overflow menu, and children badges. Tab groups are not shown (ADR 0076). One
// context menu serves every tab.

import { ChevronDownIcon, CopyIcon, LoaderCircleIcon, PencilIcon, PlusIcon, TriangleAlertIcon, XIcon } from "lucide-react";
import { memo, useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type DragEvent, type MouseEvent } from "react";
import type { App } from "../core/app";
import { workspaceRow, type Tab, type Tabs } from "../core/nav";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuShortcut, ContextMenuTrigger } from "../components/ui/context-menu";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "../components/ui/dropdown-menu";
import { copyText as copy } from "./copy";
import { NewChatMenu, type NewChatAnchor } from "./row-overlays";
import { StatusGlyph } from "./status";

const cap = (s: string) => s[0].toUpperCase() + s.slice(1);

export function TabStrip({ app }: { app: App }) {
  // Only the strip's own fields: a rows frame in the open chat must not render the strip.
  const tabs = useSyncExternalStore(app.subscribe, () => app.view.tabs);
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const workspace = useSyncExternalStore(app.subscribe, () => app.view.workspace);
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const scroller = useRef<HTMLDivElement>(null);
  const dragged = useRef<string | null>(null);
  const newChat = useRef<((anchor: NewChatAnchor) => void) | null>(null);
  const [overflow, setOverflow] = useState<Tab[]>([]);
  const ws = workspaceRow(tree, workspace);
  const list = tabs?.tabs ?? [];
  const selected = list.find((t) => t.session === openId);

  // The open tab comes into view when it changes, not on every refresh of the strip.
  const selectedId = selected?.id;
  useLayoutEffect(() => {
    if (selectedId) scroller.current?.querySelector(`[data-testid="tab"][data-id="${CSS.escape(selectedId)}"]`)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selectedId]);

  const tabOf = (e: { target: EventTarget }) => {
    const el = (e.target as Element).closest<HTMLElement>("[data-testid=tab]");
    return el ? list.find((t) => t.id === el.dataset.id) : undefined;
  };
  const onClick = (e: MouseEvent) => {
    const tab = tabOf(e);
    if (!tab) return;
    const action = (e.target as Element).closest("[data-action]")?.getAttribute("data-action");
    if (action === "close") return void app.closeTab(tab.id);
    if (tab.session !== openId) app.openChat(tab.session);
    if (action === "children" && tab.children?.firstWaiting) app.focusPane(tab.children.firstWaiting);
  };

  // Drop a tab on another tab to move it there.
  const dragProps = {
    onDragStart: (e: DragEvent) => {
      const tab = tabOf(e);
      if (!tab || tab.pending !== null) return;
      dragged.current = tab.id;
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", tab.id);
    },
    onDragOver: (e: DragEvent) => {
      const tab = tabOf(e);
      if (tab && dragged.current && tab.id !== dragged.current) e.preventDefault();
    },
    onDrop: (e: DragEvent) => {
      const tab = tabOf(e);
      const chat = dragged.current;
      if (!tab || !chat || !tabs || tab.id === chat) return;
      e.preventDefault();
      void app.choose({ op: "move_tab", workspace: tabs.workspace, chat, target: tab.id });
    },
    onDragEnd: () => (dragged.current = null),
  };

  // Tabs past either edge of the strip, read when the menu opens (ChatTab.overflow).
  const readOverflow = () => {
    const box = scroller.current?.getBoundingClientRect();
    if (!box) return setOverflow([]);
    const out: Tab[] = [];
    scroller.current!.querySelectorAll<HTMLElement>("[data-testid=tab]").forEach((el) => {
      const r = el.getBoundingClientRect();
      const tab = list.find((t) => t.id === el.dataset.id);
      if (tab && (r.right > box.right + 1 || r.left < box.left - 1)) out.push(tab);
    });
    setOverflow(out);
  };

  if (!workspace) return null;
  return (
    <div className="tab-bar" data-testid="tab-strip">
      <span className="tab-title" title={workspace}>
        {ws?.title ?? workspace.split("/").pop()}
      </span>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={scroller}
            role="tablist"
            aria-label="Open chats"
            data-testid="tabs"
            data-previous-recent={tabs?.previousRecent ?? undefined}
            data-next-recent={tabs?.nextRecent ?? undefined}
            className="flex h-full min-w-0 flex-1 items-stretch overflow-x-auto [scrollbar-width:none]"
            onClick={onClick}
            {...dragProps}
          >
            {list.map((tab) => (
              <TabView key={tab.id} tab={tab} active={tab === selected} />
            ))}
          </div>
        </ContextMenuTrigger>
        <TabMenu app={app} tabs={tabs} scroller={scroller} />
      </ContextMenu>
      <DropdownMenu onOpenChange={(o) => o && readOverflow()}>
        <DropdownMenuTrigger asChild>
          <button className="icon-btn" aria-label="More tabs" title="Tabs past the edge">
            <ChevronDownIcon />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" data-testid="tab-overflow">
          {overflow.length === 0 && <DropdownMenuItem disabled>All tabs are in view</DropdownMenuItem>}
          {overflow.map((t) => (
            <DropdownMenuItem key={t.id} onSelect={() => app.openChat(t.session)}>
              {t.status && <StatusGlyph status={t.status} />}
              <span className="max-w-[260px] truncate">{t.title}</span>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <button
        className="icon-btn"
        aria-label="New chat in this workspace"
        title="New chat in this workspace"
        disabled={ws ? !ws.newChatEnabled : true}
        onClick={(e) => ws && newChat.current?.({ rect: e.currentTarget.getBoundingClientRect(), workspace: ws, inTab: true })}
      >
        <PlusIcon />
      </button>
      <NewChatMenu app={app} open={newChat} />
    </div>
  );
}

/** One tab. Memoized on its wire object, so a strip refresh renders only the tabs that changed. */
const TabView = memo(function TabView({ tab, active }: { tab: Tab; active: boolean }) {
  return (
    <div
      role="tab"
      data-testid="tab"
      data-id={tab.id}
      data-session={tab.session}
      aria-selected={active}
      tabIndex={-1}
      draggable={tab.pending === null ? true : undefined}
      title={tab.title}
      className="tab"
    >
      {tab.pending === "starting" || tab.pending === "closing" ? (
        <LoaderCircleIcon className="glyph working" aria-label={cap(tab.pending)} />
      ) : tab.pending === "failed" ? (
        <TriangleAlertIcon className="glyph failed" aria-label="Failed" />
      ) : null}
      {tab.pending !== null || !tab.fields.length ? (
        <span className="label">{tab.title}</span>
      ) : (
        tab.fields.map((f, i) =>
          f.status ? (
            <StatusGlyph key={i} status={f.status} />
          ) : f.field === "title" ? (
            <span key={i} className="label">
              {f.text}
            </span>
          ) : f.field === "unread" ? (
            <span key={i} className="unread" role="img" aria-label="Unread" />
          ) : (
            <span key={i} className={`fld ${f.field}`}>
              {f.field === "provider" ? f.text || tab.badge : f.text}
            </span>
          ),
        )
      )}
      {tab.children && (
        <button
          data-action="children"
          className={`fld rounded-sm px-1 hover:bg-hover ${tab.children.waiting ? "text-waiting" : ""}`}
          title={tab.children.waiting ? "Show first waiting child" : "Show chat"}
          aria-label={`${tab.title}, ${tab.children.count} agents, ${tab.children.waiting} waiting`}
        >
          {tab.children.text}
        </button>
      )}
      {tab.canHide && (
        <button data-action="close" aria-label={`Close tab ${tab.title}`} title="Close tab" className="tab-close">
          <XIcon className="size-3" />
        </button>
      )}
    </div>
  );
});

/** The strip's one context menu, for the tab under the pointer. */
function TabMenu({ app, tabs, scroller }: { app: App; tabs: Tabs | null; scroller: React.RefObject<HTMLDivElement | null> }) {
  const [tab, setTab] = useState<Tab | null>(null);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const onContextMenu = (e: globalThis.MouseEvent) => {
      const hit = (e.target as Element).closest<HTMLElement>("[data-testid=tab]");
      const found = hit && app.view.tabs?.tabs.find((t) => t.id === hit.dataset.id);
      if (found && found.pending === null) setTab(found);
      else {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    el.addEventListener("contextmenu", onContextMenu);
    return () => el.removeEventListener("contextmenu", onContextMenu);
  }, [app, scroller]);
  if (!tab || !tabs) return null;
  const workspace = tabs.workspace;
  const [left, right] = tabs.stepTargets[tab.id] ?? [null, null];
  const later = (label: string, phase: string) => (
    <ContextMenuItem disabled>
      {label}
      <ContextMenuShortcut>{phase}</ContextMenuShortcut>
    </ContextMenuItem>
  );
  return (
    <ContextMenuContent data-testid="tab-menu">
      <ContextMenuItem onSelect={() => app.renameChat(tab.session)}>
        <PencilIcon /> Rename chat…
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => app.life.openSwitchModel(tab.session)}>Switch model…</ContextMenuItem>
      <ContextMenuItem disabled={!left} onSelect={() => left && void app.choose({ op: "step_tab", workspace, chat: tab.id, target: left })}>
        Move Left
      </ContextMenuItem>
      <ContextMenuItem disabled={!right} onSelect={() => right && void app.choose({ op: "step_tab", workspace, chat: tab.id, target: right })}>
        Move Right
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void copy(tab.session, "session id")}>
        <CopyIcon /> Copy session id
      </ContextMenuItem>
      {later("Reveal folder in Finder", "S6")}
      {later("Open in Terminal", "S6")}
      <ContextMenuSeparator />
      <ContextMenuItem disabled={!tab.canHide} onSelect={() => void app.closeTab(tab.id)}>
        Close tab
      </ContextMenuItem>
      <ContextMenuItem disabled={!tab.canClose} onSelect={() => void app.life.endChat(tab.session, false)}>
        End chat…
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void app.life.endChat(tab.session, true)}>Archive chat</ContextMenuItem>
    </ContextMenuContent>
  );
}
