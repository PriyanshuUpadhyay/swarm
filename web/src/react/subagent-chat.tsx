// A subagent row, and the read-only chat of the subagent's own work (ADR 0140).
import { ArrowLeftIcon } from "lucide-react";
import { useEffect, useState, useSyncExternalStore, type MouseEvent } from "react";
import type { App } from "../core/app";
import type { SubagentChild, SubagentRun, ToolState } from "../core/api";
import { TAIL, paneId } from "../core/app-types";
import { closeSubagent, currentSubagent, openSubagent, subagentAgent, subscribeSubagent } from "../core/subagent";
import { Dialog, DialogContent, DialogTitle } from "../components/ui/dialog";
import { CutLine } from "./cut-line";
import { PaneBody } from "./transcript";
import { StatusGlyph } from "./transcript-tool";

/** How often an open child chat reads its log again until its last row ends the turn, since it
 *  follows no stream. */
const REFRESH_MS = 3000;
/** Failed reads in a row after which the dialog stops trying and shows the error with Retry. */
const MAX_FAILED_READS = 10;

const CHILD_AT = 5;
const STATES = ["failed", "started", "done", "unknown"] as const;
const GLYPHS: Record<typeof STATES[number], ToolState> = { failed: "failed", started: "waiting", done: "finished", unknown: "unreported" };

function openChild(e: MouseEvent<HTMLButtonElement>, callId: string, title: string, sourceSession?: string) {
  // Pane columns open a child from their own chat, not the main chat.
  const chatId = e.currentTarget.closest<HTMLElement>("[data-chat]")?.dataset.chat;
  if (chatId) openSubagent({ chatId, callId, title, sourceSession });
}

function ChildGroup({ children, title, state, sourceSession }: { children: SubagentChild[]; title: string; state: ToolState; sourceSession?: string }) {
  const [showAll, setShowAll] = useState(false);

  const groups = STATES.map((state) => ({ state, items: children.filter((c) => state === "unknown" ? !STATES.slice(0, 3).some((s) => c.state === s) : c.state === state) }));
  const ordered = groups.flatMap(({ state, items }) => items.map((child) => ({ ...child, state })));

  return (
    <>
      <div className="card-head flex-wrap">
        <span className="min-w-0 truncate font-semibold" title={title}>{title}</span>
        {(state === "waiting" || state === "interrupted") && <span className="flex items-center gap-1"><StatusGlyph state={state} />{state === "waiting" ? "Working" : "Interrupted"}</span>}
        <span className="font-mono text-muted-foreground">{groups.filter((g) => g.items.length).map((g) => `${g.items.length} ${g.state}`).join(" · ")}</span>
      </div>
      <ul className="m-0 list-none px-2 pb-1">
        {(showAll ? ordered : ordered.slice(0, CHILD_AT)).map((child) => (
          <li key={child.call_id}>
            <button type="button" className="card-head" onClick={(e) => openChild(e, child.call_id, child.description, sourceSession)}>
              <StatusGlyph state={GLYPHS[child.state]} />
              <span className="min-w-0 flex-1 truncate">{child.description}</span>
              <span className="shrink-0 text-xs text-muted-foreground capitalize">{child.state}</span>
            </button>
          </li>
        ))}
      </ul>
      {!showAll && ordered.length > CHILD_AT && <div className="px-2 pb-2"><CutLine count={`${ordered.length - CHILD_AT} more`} show={() => setShowAll(true)} /></div>}
    </>
  );
}

export function SubagentRow({ run, title, sourceSession }: { run: SubagentRun; title: string; sourceSession?: string }) {
  const result = run.result?.text.trim().split("\n", 1)[0] ?? "";
  const children = run.children?.map((child, i) => ({ ...child, description: child.description || `Child ${i + 1}` }))
    ?? (run.other_call_ids?.length ? [run.call_id, ...run.other_call_ids].map((call_id, i) => ({ call_id, description: i ? `Child ${i + 1}` : run.description || title })) : []);

  return (
    <div className="card" data-testid="subagent-row" data-state={run.state}>
      {children.length > 1 ? <ChildGroup children={children} title={title} state={run.state} sourceSession={sourceSession} /> : (
        <button type="button" className="card-head" title="Open the subagent's own work" onClick={(e) => openChild(e, run.call_id, title, sourceSession)}>
          <StatusGlyph state={run.state} />
          <span className="truncate">{title}</span>
          {result && <span className="truncate text-muted-foreground">{result}</span>}
        </button>
      )}
    </div>
  );
}

/** The child chat in a dialog, with a bar that says it runs on its own. */
export function SubagentOverlay({ app }: { app: App }) {
  const open = useSyncExternalStore(subscribeSubagent, currentSubagent);
  const parent = open ? app.chats.get(open.chatId) : undefined;
  const agent = parent && open ? subagentAgent(parent.agent, open.callId) : undefined;
  const sessionId = open?.sourceSession ?? parent?.sessionId;
  const pane = useSyncExternalStore(app.subscribe, () => {
    const child = sessionId && agent ? app.chats.get(paneId(sessionId, agent)) : undefined;
    return child ? app.panes.paneView(child, true) : null;
  });

  useEffect(() => {
    if (!sessionId || !agent) return;
    const child = app.reading.childChat(sessionId, agent);
    // The overlay loads this chat itself, so the pane must not wait for a column to load it.
    child.loaded = true;
    // The chat exists now, so the dialog can show its loading state while the first read runs.
    app.notify();
    let stopped = false;
    let reading = false;
    let failed = 0;

    const load = async () => {
      if (reading) return;
      reading = true;
      await app.reading.loadTail(child);
      reading = false;
      failed = child.state === "error" ? failed + 1 : 0;
      if (!stopped) app.notify();
    };
    void load();

    // A subagent that started a moment ago may have no log yet, so a failed read tries again, up
    // to MAX_FAILED_READS times in a row at REFRESH_MS. A good read, also one that the Retry button
    // makes, ends the count and the reads start again. A tail read drops older pages, so a reader who loaded them keeps the rows they have. A log
    // whose last row ends the turn no longer grows. `load` never rejects: `loadTail` catches.
    const timer = setInterval(() => {
      if (child.state === "ready") failed = 0;
      if (failed < MAX_FAILED_READS && !child.rows.at(-1)?.ends_turn && child.rows.length <= TAIL) void load();
    }, REFRESH_MS);

    return () => {
      stopped = true;
      clearInterval(timer);
      app.chats.delete(child.id);
    };
  }, [app, sessionId, agent]);

  if (!open || !sessionId || !agent) return null;

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
        {pane && <PaneBody app={app} pane={{ ...pane, composer: null }} empty="This subagent has no messages." />}
      </DialogContent>
    </Dialog>
  );
}
