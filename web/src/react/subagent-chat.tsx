// A subagent row, and the read-only chat of the subagent's own work (ADR 0140).
import { ArrowLeftIcon } from "lucide-react";
import { useEffect, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { subagentAgent, closeSubagent, currentSubagent, openSubagent, subscribeSubagent } from "../core/subagent";
import { PaneBody } from "./transcript";
import { StatusGlyph } from "./transcript-tool";
import type { Ctx } from "./transcript-row";

/** How often a child chat that is open reads its log again, since it follows no stream. */
const REFRESH_MS = 3000;

export function SubagentRow({ line }: Ctx) {
  const run = line.row.subagent!;
  const result = run.result?.text.trim().split("\n", 1)[0] ?? "";
  return (
    <div className="card" data-testid="subagent-row" data-state={run.state}>
      <button
        type="button"
        className="card-head"
        title="Open the subagent's own work"
        onClick={(e) => {
          // The transcript above names its chat; a pane column's rows open from their own chat.
          const chatId = e.currentTarget.closest<HTMLElement>("[data-chat]")?.dataset.chat;
          if (chatId) openSubagent({ chatId, callId: run.call_id, title: line.row.text });
        }}
      >
        <StatusGlyph state={run.state} />
        <span className="truncate">{line.row.text}</span>
        {result && <span className="truncate text-muted-foreground">{result}</span>}
      </button>
    </div>
  );
}

/** The child chat over the page, with a bar that says it runs on its own. */
export function SubagentOverlay({ app }: { app: App }) {
  const open = useSyncExternalStore(subscribeSubagent, currentSubagent);
  useSyncExternalStore(app.subscribe, () => app.view);
  const parent = open ? app.chats.get(open.chatId) : undefined;
  const agent = parent && open ? subagentAgent(parent.agent, open.callId) : undefined;
  const sessionId = parent?.sessionId;

  useEffect(() => {
    if (!sessionId || !agent) return;
    const child = app.reading.childChat(sessionId, agent);
    let stopped = false;
    const load = () => void app.reading.loadTail(child).then(() => !stopped && app.notify());
    load();
    const timer = setInterval(load, REFRESH_MS);
    return () => {
      stopped = true;
      clearInterval(timer);
      app.chats.delete(child.id);
    };
  }, [app, sessionId, agent]);

  if (!open || !sessionId || !agent) return null;
  const child = app.reading.childChat(sessionId, agent);
  // The child has no composer: it takes no input.
  const pane = { ...app.panes.paneView(child, "return", true), composer: null };
  return (
    <div className="fixed inset-0 z-50 flex min-h-0 flex-col bg-background" data-testid="subagent-chat">
      <div className="flex items-center gap-2 border-b px-3 py-2 text-sm" data-testid="subagent-bar">
        <button type="button" className="icon-btn" onClick={closeSubagent} data-testid="open-parent" aria-label="Open parent">
          <ArrowLeftIcon /> Open parent
        </button>
        <span className="truncate font-medium">{open.title}</span>
        <span className="ml-auto text-muted-foreground">Runs on its own</span>
      </div>
      <PaneBody app={app} pane={pane} empty="This subagent's log is not available." />
    </div>
  );
}
