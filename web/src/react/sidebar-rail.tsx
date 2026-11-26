import { PlusIcon } from "lucide-react";
import { useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { chatRows } from "../core/nav";
import { Glyph } from "./glyph";
import { StatusGlyph } from "./status";

export function SidebarRail({ app }: { app: App }) {
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const rows = useSyncExternalStore(app.subscribe, () => app.view.rows);
  const loaded = useSyncExternalStore(app.subscribe, () => app.view.loaded);
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const catalog = chatRows(tree);
  const byId = new Map(catalog.map((row) => [row.id, row]));
  const visible = rows.map((row) => row.id).filter((id) => byId.has(id));
  const order = [...new Set([...visible, ...byId.keys()])];
  return (
    <nav aria-label="Chat rail" className="sidebar-rail" data-testid="sidebar-rail" data-loaded={String(loaded)}>
      <button type="button" aria-label="Home" title="Home" onClick={() => app.showHome()}><Glyph aria-hidden /></button>
      <button type="button" aria-label="New chat" title="New chat (⌘N)" onClick={() => app.newChat.open()}><PlusIcon aria-hidden /></button>
      <div className="rail-chats">
        {order.map((id) => {
          const row = byId.get(id);
          const session = row?.selection?.chat;
          if (!row || !session) return null;
          return (
            <button key={id} type="button" aria-label={row.title} title={row.title} data-testid="rail-chat" data-session={session}
              aria-current={openId === session || undefined} onClick={() => app.openChat(session)}>
              <StatusGlyph status={row.status ?? "ended"} />
            </button>
          );
        })}
      </div>
    </nav>
  );
}
