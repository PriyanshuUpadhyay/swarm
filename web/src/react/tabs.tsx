// The chat tab strip: the chats the owner opened in the current workspace, in opening order
// (ADR 0054). P2 adds drag, groups, overflow and the key map; this unit holds the open set in memory.

import { XIcon } from "lucide-react";
import { useState, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { workspaces, type TreeNode } from "../core/tree";
import { StatusGlyph } from "./status";

export function TabStrip({ app }: { app: App }) {
  const view = useSyncExternalStore(app.subscribe, () => app.view);
  const { roots, openId } = view;
  const [open, setOpen] = useState<Record<string, string[]>>({});
  const [seen, setSeen] = useState<string | null>(null);

  const all = workspaces(roots);
  const ws = openId ? all.find((w) => w.children.some((c) => c.sessionId === openId)) : undefined;
  const project = ws && roots.find((p) => p.children.includes(ws));

  // A newly opened chat gets a tab to the right of the tab seen before it.
  if (openId !== seen) {
    setSeen(openId);
    if (ws && openId) {
      const list = open[ws.id] ?? [];
      if (!list.includes(openId)) {
        const at = seen ? list.indexOf(seen) + 1 || list.length : list.length;
        setOpen({ ...open, [ws.id]: [...list.slice(0, at), openId, ...list.slice(at)] });
      }
    }
  }

  const chats = new Map((ws?.children ?? []).map((c) => [c.sessionId!, c]));
  const tabs = (ws ? (open[ws.id] ?? []) : []).map((id) => chats.get(id)).filter((c): c is TreeNode => c !== undefined);

  const close = (sessionId: string) => {
    if (!ws) return;
    const i = tabs.findIndex((t) => t.sessionId === sessionId);
    const rest = tabs.filter((t) => t.sessionId !== sessionId);
    setOpen({ ...open, [ws.id]: rest.map((t) => t.sessionId!) });
    if (sessionId === openId && rest.length) app.openChat(rest[Math.max(0, i - 1)].sessionId!);
  };

  return (
    <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3">
      {ws && (
        <div className="flex min-w-0 shrink-0 items-center gap-1 text-sm" title={ws.title}>
          <span className="text-muted-foreground">{project?.label}</span>
          <span className="text-faint-foreground">/</span>
          <span className="max-w-[180px] truncate font-medium">{ws.label}</span>
        </div>
      )}
      <div role="tablist" aria-label="Open chats" data-testid="tabs" className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
        {tabs.map((tab) => {
          const active = tab.sessionId === openId;
          // The open tab can close only when another tab can take its place.
          const closable = !active || tabs.length > 1;
          return (
            <div
              key={tab.id}
              role="tab"
              data-testid="tab"
              data-id={tab.sessionId}
              aria-selected={active}
              tabIndex={-1}
              onClick={() => !active && app.openChat(tab.sessionId!)}
              className={`group flex h-7 max-w-[220px] min-w-[120px] shrink-0 cursor-default items-center gap-1.5 rounded-md pr-1 pl-2.5 text-sm ${active ? "bg-selected text-foreground" : "text-muted-foreground hover:bg-hover hover:text-foreground"}`}
            >
              <StatusGlyph status={tab.state} />
              <span className="min-w-0 flex-1 truncate">{tab.label}</span>
              {closable && (
                <button
                  aria-label="Close tab"
                  onClick={(e) => {
                    e.stopPropagation();
                    close(tab.sessionId!);
                  }}
                  className="grid size-4 shrink-0 place-items-center rounded-sm text-faint-foreground opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-foreground focus-visible:opacity-100"
                >
                  <XIcon className="size-3" />
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
