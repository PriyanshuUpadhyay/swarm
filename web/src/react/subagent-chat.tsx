// A subagent row, and the read-only chat of the subagent's own work (ADR 0140).
import { ArrowLeftIcon } from "lucide-react";
import { useEffect, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { TAIL, paneId } from "../core/app-types";
import { closeSubagent, currentSubagent, openSubagent, subagentAgent, subscribeSubagent } from "../core/subagent";
import { Dialog, DialogContent, DialogTitle } from "../components/ui/dialog";
import { PaneBody } from "./transcript";
import { StatusGlyph } from "./transcript-tool";
import type { Ctx } from "./transcript-row";

/** How often a child chat of a running subagent reads its log again, since it follows no stream. */
const REFRESH_MS = 3000;

export function SubagentRow({ line }: Ctx) {
  const run = line.row.subagent;
  if (!run) return null;
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
          if (chatId) openSubagent({ chatId, callId: run.call_id, title: line.row.text, running: run.state === "waiting" });
        }}
      >
        <StatusGlyph state={run.state} />
        <span className="truncate">{line.row.text}</span>
        {result && <span className="truncate text-muted-foreground">{result}</span>}
      </button>
    </div>
  );
}

/** The child chat in a dialog, with a bar that says it runs on its own. */
export function SubagentOverlay({ app }: { app: App }) {
  const open = useSyncExternalStore(subscribeSubagent, currentSubagent);
  useSyncExternalStore(app.subscribe, () => app.view);
  const parent = open ? app.chats.get(open.chatId) : undefined;
  const agent = parent && open ? subagentAgent(parent.agent, open.callId) : undefined;
  const sessionId = parent?.sessionId;
  const running = open?.running ?? false;

  useEffect(() => {
    if (!sessionId || !agent) return;
    const child = app.reading.childChat(sessionId, agent);
    // The overlay loads this chat itself, so the pane must not wait for a column to load it.
    child.loaded = true;
    let stopped = false;
    let reading = false;

    const load = async () => {
      if (reading) return;
      reading = true;
      await app.reading.loadTail(child);
      reading = false;
      if (!stopped) app.notify();
    };
    void load();

    // A finished subagent's log no longer grows, and a read that failed would fail again. A tail
    // read drops older pages, so a reader who loaded them keeps the rows they have.
    const timer = running
      ? setInterval(() => {
          if (child.state === "ready" && child.rows.length <= TAIL) void load();
        }, REFRESH_MS)
      : undefined;

    return () => {
      stopped = true;
      clearInterval(timer);
      app.chats.delete(child.id);
    };
  }, [app, sessionId, agent, running]);

  if (!open || !sessionId || !agent) return null;

  const child = app.chats.get(paneId(sessionId, agent));
  // The child has no composer: it takes no input.
  const pane = child ? { ...app.panes.paneView(child, "return", true), composer: null } : null;

  return (
    <Dialog open onOpenChange={(next) => !next && closeSubagent()}>
      <DialogContent className="top-6 flex h-[calc(100vh-48px)] w-[min(960px,calc(100vw-32px))] flex-col" aria-describedby={undefined} data-testid="subagent-chat">
        <div className="flex items-center gap-2 border-b px-3 py-2 text-sm" data-testid="subagent-bar">
          <button type="button" className="card-head w-auto gap-1 px-2" onClick={closeSubagent} data-testid="open-parent">
            <ArrowLeftIcon className="size-3.5" aria-hidden /> Open parent
          </button>
          <DialogTitle className="truncate text-sm font-medium">{open.title}</DialogTitle>
          <span className="ml-auto text-muted-foreground">Runs on its own</span>
        </div>
        {pane && <PaneBody app={app} pane={pane} empty="This subagent has no messages." />}
      </DialogContent>
    </Dialog>
  );
}
