// Archived chat reopening and external chat import.

import { useEffect, useState } from "react";
import type { App } from "../core/app";
import type { ClosedChat, ImportableChat } from "../core/lifecycle";
import { Button } from "../components/ui/button";
import { FIELD, message, useLife, Frame, ErrorLine } from "./lifecycle-shared";

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
  const [query, setQuery] = useState("");
  const [attempt, setAttempt] = useState(0);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    let live = true;
    setLoading(true);
    setError(null);
    app.life.closedChats().then((data) => live && setChats(data), (err) => live && setError(message(err))).finally(() => live && setLoading(false));
    return () => { live = false; };
  }, [app, attempt]);
  const shown = chats?.filter((c) => `${c.title ?? ""} ${c.cwd} ${c.id}`.toLowerCase().includes(query.trim().toLowerCase()));
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
      <input data-autofocus className={FIELD} type="search" aria-label="Filter chats" placeholder="Filter chats…" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => {
        if (e.key === "ArrowDown") { e.preventDefault(); e.currentTarget.nextElementSibling?.querySelector<HTMLButtonElement>("button")?.focus(); }
      }} />
      <div role="list" className="grid max-h-[320px] overflow-y-auto" onKeyDown={(e) => {
        const by = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
        if (!by) return;
        const buttons = [...e.currentTarget.querySelectorAll<HTMLButtonElement>("button")];
        const index = buttons.indexOf(e.target as HTMLButtonElement);
        if (index < 0) return;
        e.preventDefault();
        buttons[Math.max(0, Math.min(buttons.length - 1, index + by))]?.focus();
      }}>
        {!chats && !error && <p className="text-sm text-muted-foreground">Reading closed chats…</p>}
        {chats?.length === 0 && <p className="text-sm text-muted-foreground">No archived chats.</p>}
        {shown?.map((c) => (
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
              {c.title || c.cwd.split("/").pop()} <span className="font-mono text-xs text-muted-foreground">{c.id.slice(0, 8)}</span>
            </span>
            <span className="truncate text-xs text-muted-foreground">
              {c.chair_provider ?? "chat"} · closed {ageText(c.archivedAt)} ago · {c.cwd}
            </span>
          </button>
        ))}
      </div>
      {chats && chats.length > 0 && shown?.length === 0 && <p role="status" className="text-sm text-muted-foreground">No chat matches.</p>}
      <ErrorLine text={error} />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="outline" size="sm" disabled={loading || busy} onClick={() => setAttempt(attempt + 1)}>{loading ? "Reading…" : error ? "Retry" : "Refresh"}</Button>
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
  const [query, setQuery] = useState("");
  const [attempt, setAttempt] = useState(0);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    let live = true;
    setLoading(true);
    setError(null);
    app.life.importableChats().then((data) => live && setChats(data), (err) => live && setError(message(err))).finally(() => live && setLoading(false));
    return () => { live = false; };
  }, [app, attempt]);
  const shown = chats?.filter((c) => `${c.title} ${c.workspace} ${c.id}`.toLowerCase().includes(query.trim().toLowerCase()));
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
      <input data-autofocus className={FIELD} type="search" aria-label="Filter chats" placeholder="Filter chats…" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => {
        if (e.key === "ArrowDown") { e.preventDefault(); e.currentTarget.nextElementSibling?.querySelector<HTMLButtonElement>("button")?.focus(); }
      }} />
      <div role="list" className="grid max-h-[320px] overflow-y-auto" onKeyDown={(e) => {
        const by = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
        if (!by) return;
        const buttons = [...e.currentTarget.querySelectorAll<HTMLButtonElement>("button")];
        const index = buttons.indexOf(e.target as HTMLButtonElement);
        if (index < 0) return;
        e.preventDefault();
        buttons[Math.max(0, Math.min(buttons.length - 1, index + by))]?.focus();
      }}>
        {!chats && !error && <p className="text-sm text-muted-foreground">Reading chats…</p>}
        {chats?.length === 0 && <p className="text-sm text-muted-foreground">No chats to import.</p>}
        {shown?.map((c) => (
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
      {chats && chats.length > 0 && shown?.length === 0 && <p role="status" className="text-sm text-muted-foreground">No chat matches.</p>}
      <ErrorLine text={error} />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="outline" size="sm" disabled={loading || busy} onClick={() => setAttempt(attempt + 1)}>{loading ? "Reading…" : error ? "Retry" : "Refresh"}</Button>
        <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("imports")}>
          Close
        </Button>
      </div>
    </>
  );
}

export { ClosedDialog, ImportDialog };
