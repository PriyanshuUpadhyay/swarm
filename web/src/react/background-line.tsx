import { XIcon } from "lucide-react";
import { createContext, useContext, useEffect, useState, useSyncExternalStore, type ReactNode } from "react";
import type { App } from "../core/app";
import { backgroundRows, elapsed, OPEN_BACKGROUND, poll, runningLine, type BackgroundRow, type BackgroundTask, type OpenBackgroundDetail, type WorkspaceShell } from "../core/background";
import { plural } from "./cut-line";
import { Glyph } from "./glyph";
import { Offline } from "./status";

const POLL_MS = 3000;
type Lists = { rows: BackgroundRow[]; addShell: (shell: WorkspaceShell) => void; refreshShells: () => Promise<void> };
const Background = createContext<Lists | null>(null);

export function useBackground() {
  const lists = useContext(Background);
  if (!lists) throw new Error("Missing background lists");
  return lists;
}

/** The chat and Cmd+J share the same two polling reads, also while the panel is closed. */
export function BackgroundLists({ app, session, children }: { app: App; session: string; children: ReactNode }) {
  const snapshot = useSyncExternalStore(app.subscribe, () => app.snapshot);
  const [tasks, setTasks] = useState<{ session: string; rows: BackgroundTask[] }>({ session, rows: [] });
  const [shells, setShells] = useState<{ session: string; rows: WorkspaceShell[] }>({ session, rows: [] });

  useEffect(() => {
    let live = true;
    setShells({ session, rows: [] });
    const read = () => Promise.allSettled([
      app.api.get<{ tasks: BackgroundTask[] }>("/api/background", { session }).then((r) => { if (live) setTasks({ session, rows: r.tasks }); }),
      app.api.get<{ shells: WorkspaceShell[] }>("/api/shells", { session }).then((r) => { if (live) setShells({ session, rows: r.shells }); }),
    ]);
    const stop = poll(read, POLL_MS, () => document.hidden);
    return () => { live = false; stop(); };
  }, [app, session]);

  const rows = backgroundRows(snapshot?.sessions.find((s) => s.id === session), snapshot?.agents[session], tasks.session === session ? tasks.rows : [], shells.session === session ? shells.rows : []);

  const addShell = (shell: WorkspaceShell) => setShells((list) => list.session === session && !list.rows.some((row) => row.id === shell.id)
    ? { ...list, rows: [...list.rows, shell] } : list);
  const refreshShells = async () => {
    const { shells } = await app.api.get<{ shells: WorkspaceShell[] }>("/api/shells", { session });
    setShells((list) => list.session === session ? { session, rows: shells } : list);
  };
  return <Background.Provider value={{ rows, addShell, refreshShells }}>{children}</Background.Provider>;
}

export function BackgroundLine({ session }: { session: string }) {
  const line = runningLine(useContext(Background)?.rows ?? []);
  const offline = useContext(Offline);
  const [now, setNow] = useState(() => Date.now());
  const startedAt = line?.startedAt ?? null;
  useEffect(() => {
    if (startedAt === null) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(timer);
  }, [startedAt]);

  if (!line) return null;
  const label = ["In background", line.agents ? plural(line.agents, "agent") : "", line.shells ? plural(line.shells, "shell") : ""].filter(Boolean).join(" · ");

  return (
    <div className="flex h-7 items-center gap-2 text-sm text-muted-foreground" data-testid="background-line">
      <Glyph form={!offline && (line.agents || line.shells) ? "working" : "unlit"} size={12} className="shrink-0" aria-hidden />
      <span role="status" className="min-w-0 truncate">{label}</span>
      {line.failed > 0 && <span className="flex shrink-0 items-center gap-1 text-failed"><XIcon className="size-3" aria-hidden />{line.failed} failed</span>}
      {startedAt !== null && <span className="shrink-0 font-mono tabular-nums">{elapsed(now - startedAt)}</span>}
      <span className="flex-1" />
      <button type="button" className="t-link underline" onClick={() => window.dispatchEvent(new CustomEvent<OpenBackgroundDetail>(OPEN_BACKGROUND, { detail: { session, key: line.key } }))}>Open</button>
    </div>
  );
}
