// Archived chat reopening and external chat import.

import { useEffect, useState } from "react";
import type { App } from "../core/app";
import type { ClosedChat, ImportableChat } from "../core/lifecycle";
import { Button } from "../components/ui/button";
import { message, useLife, Frame, ErrorLine } from "./lifecycle-shared";

// ---- Archived chats ----

/** "5m", "3h", "2d" since a Unix time in seconds. */
function ageText(seconds: number, now = Date.now() / 1000): string {
  const s = Math.max(0, now - seconds);
  if (s < 3600) return `${Math.max(1, Math.floor(s / 60))}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

function ClosedDialog({ app }: { app: App }) {
  const open = useLife(app, "closed");
  return (
    <Frame open={open} onClose={() => app.life.close("closed")} title="Archived chats" description="Open one to bring it back." testid="recently-closed">
      {open && <ClosedBody app={app} />}
    </Frame>
  );
}

function ClosedBody({ app }: { app: App }) {
  const [chats, setChats] = useState<ClosedChat[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    app.life.closedChats().then(setChats, (err) => setError(message(err)));
  }, [app]);
  const reopen = async (id: string) => {
    setBusy(true);
    try {
      await app.life.reopen(id);
    } catch (err) {
      setError(message(err));
    }
    setBusy(false);
  };
  return (
    <>
      <div role="list" className="grid max-h-[320px] overflow-y-auto">
        {!chats && !error && <p className="text-sm text-muted-foreground">Reading closed chats…</p>}
        {chats?.length === 0 && <p className="text-sm text-muted-foreground">No archived chats.</p>}
        {chats?.map((c) => (
          <button
            key={c.id}
            role="listitem"
            type="button"
            disabled={busy}
            data-testid="closed-chat"
            data-session={c.id}
            title={c.cwd}
            className="grid gap-0.5 rounded-md px-2 py-1.5 text-left hover:bg-hover focus-visible:bg-selected focus-visible:outline-none"
            onClick={() => void reopen(c.id)}
          >
            <span className="truncate text-sm font-medium">
              {c.cwd.split("/").pop()} <span className="font-mono text-xs text-muted-foreground">{c.id.slice(0, 8)}</span>
            </span>
            <span className="truncate text-xs text-muted-foreground">
              {c.chair_provider ?? "chat"} · closed {ageText(c.archivedAt)} ago · {c.cwd}
            </span>
          </button>
        ))}
      </div>
      <ErrorLine text={error} />
      <div className="flex justify-end">
        <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("closed")}>
          Close
        </Button>
      </div>
    </>
  );
}

// ---- Import chats ----

function ImportDialog({ app }: { app: App }) {
  const open = useLife(app, "imports");
  return (
    <Frame open={open} onClose={() => app.life.close("imports")} title="Import chats" description="Chats that a CLI started outside the app, in Herdr, tmux or another host. Import one to list it." testid="import-chats">
      {open && <ImportBody app={app} />}
    </Frame>
  );
}

function ImportBody({ app }: { app: App }) {
  const [chats, setChats] = useState<ImportableChat[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    app.life.importableChats().then(setChats, (err) => setError(message(err)));
  }, [app]);
  const pick = async (chat: ImportableChat) => {
    setBusy(true);
    try {
      await app.life.importChat(chat);
    } catch (err) {
      setError(message(err));
    }
    setBusy(false);
  };
  return (
    <>
      <div role="list" className="grid max-h-[320px] overflow-y-auto">
        {!chats && !error && <p className="text-sm text-muted-foreground">Reading chats…</p>}
        {chats?.length === 0 && <p className="text-sm text-muted-foreground">No chats to import.</p>}
        {chats?.map((c) => (
          <button
            key={c.id}
            role="listitem"
            type="button"
            disabled={busy}
            data-testid="importable-chat"
            data-session={c.id}
            title={c.workspace}
            className="grid gap-0.5 rounded-md px-2 py-1.5 text-left hover:bg-hover focus-visible:bg-selected focus-visible:outline-none"
            onClick={() => void pick(c)}
          >
            <span className="truncate text-sm font-medium">{c.title}</span>
            <span className="truncate text-xs text-muted-foreground">
              {c.provider ?? "chat"} · {c.running ? "running" : `last active ${ageText(c.lastActivity)} ago`} · {c.workspace}
            </span>
          </button>
        ))}
      </div>
      <ErrorLine text={error} />
      <div className="flex justify-end">
        <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("imports")}>
          Close
        </Button>
      </div>
    </>
  );
}

export { ClosedDialog, ImportDialog };
