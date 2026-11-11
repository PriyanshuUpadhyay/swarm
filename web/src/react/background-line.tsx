// One status line at the end of the open chat while background work runs (ADR 0104). It is text, not
// a terminal (ADR 0099); Open shows the task in the Cmd+J panel.

import { useEffect, useState } from "react";
import type { App } from "../core/app";
import { OPEN_BACKGROUND, elapsed, type OpenBackgroundDetail, poll, runningLine, type BackgroundTask } from "../core/background";
import { Button } from "../components/ui/button";

const POLL_MS = 3000;

export function BackgroundLine({ app, session }: { app: App; session: string }) {
  const [tasks, setTasks] = useState<BackgroundTask[]>([]);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let stop = false;
    setTasks([]);
    // A failed poll keeps the last list, and the next tick tries again.
    const end = poll(() => app.api.get<{ tasks: BackgroundTask[] }>("/api/background", { session }).then((r) => !stop && setTasks(r.tasks)), POLL_MS, () => document.hidden);
    return () => {
      stop = true;
      end();
    };
  }, [app, session]);

  const line = runningLine(tasks);

  useEffect(() => {
    if (!line?.startedAt) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [line?.startedAt]);

  if (!line) return null;

  return (
    <div role="status" data-testid="background-line" className="flex items-center gap-2 border-t px-3 py-1 text-xs text-muted-foreground">
      <span className="min-w-0 flex-1 truncate">Running · {line.label}</span>
      {line.startedAt !== null && <span className="shrink-0 font-mono tabular-nums">{elapsed(now - line.startedAt)}</span>}
      <Button variant="ghost" size="sm" onClick={() => window.dispatchEvent(new CustomEvent<OpenBackgroundDetail>(OPEN_BACKGROUND, { detail: { session, key: line.key } }))}>
        Open
      </Button>
    </div>
  );
}
