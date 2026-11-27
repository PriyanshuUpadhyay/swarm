import { LoaderCircleIcon, MessageSquareIcon, PlusIcon, TriangleAlertIcon } from "lucide-react";
import { useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { chatRows } from "../core/nav";
import { Glyph } from "./glyph";
import { StatusGlyph } from "./status";

const CHAT_LIMIT = 8; // Same first-list limit as Home's recent chats.

export function SidebarRail({ app }: { app: App }) {
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const rows = useSyncExternalStore(app.subscribe, () => app.view.rows);
  const loaded = useSyncExternalStore(app.subscribe, () => app.view.loaded);
  const error = useSyncExternalStore(app.subscribe, () => app.view.error);
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);

  const catalog = chatRows(tree);
  const byId = new Map(catalog.map((row) => [row.id, row]));
  const visible = rows.flatMap(({ id }) => {
    const chat = byId.get(id);
    return chat ? [chat] : [];
  });
  const order = [...new Set([...visible, ...byId.values()])];
  const shown = order.slice(0, CHAT_LIMIT);
  const current = order.findIndex((row) => row.selection?.chat === openId);
  if (current >= CHAT_LIMIT) shown[CHAT_LIMIT - 1] = order[current];
  const hidden = order.length - shown.length;
  const notice = error ? (order.length ? "Offline. Showing the last list." : "Could not load the workspaces") :
    tree?.choicesError ? `Choices: ${tree.choicesError}` : !loaded ? "Loading workspaces…" : !order.length ? "No chats" : null;

  return (
    <nav aria-label="Chat rail" className="sidebar-rail" data-testid="sidebar-rail" data-loaded={String(loaded)}>
      <button type="button" aria-label="Home" title="Home" onClick={() => app.showHome()}><Glyph aria-hidden /></button>
      <button type="button" aria-label="New chat" title="New chat (⌘N)" onClick={() => app.newChat.open()}><PlusIcon aria-hidden /></button>
      {notice && <div className="mt-1 text-muted-foreground" role="status" title={notice}>
        {error || tree?.choicesError ? <TriangleAlertIcon className="text-failed" aria-hidden /> :
          !loaded ? <LoaderCircleIcon className="motion-safe:animate-spin" aria-hidden /> : <MessageSquareIcon aria-hidden />}
        <span className="sr-only">{notice}</span>
      </div>}
      <div className="rail-chats">
        {shown.map((row) => {
          const session = row.selection?.chat;
          if (!session) return null;
          return (
            <button key={row.id} type="button" aria-label={row.title} title={row.title} data-testid="rail-chat" data-session={session}
              aria-current={openId === session || undefined} onClick={() => app.openChat(session)}>
              <StatusGlyph status={row.status ?? "ended"} />
            </button>
          );
        })}
      </div>
      {hidden > 0 && <button type="button" className="text-xs tabular-nums" aria-label={`Search ${hidden} more chats`} title={`Search ${hidden} more chats`} onClick={() => app.setPalette(true)}>
        <span className="max-w-full truncate">+{hidden}</span>
      </button>}
    </nav>
  );
}
