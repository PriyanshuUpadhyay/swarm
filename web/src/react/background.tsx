// The Cmd+J panel: the open chat's live agents and their Claude background tasks, with a terminal
// for the selected row (ADR 0102). An agent's terminal is its tmux pane; a shell's is its output
// file, read-only, since nothing outside Claude Code can type into it. One terminal is live at a
// time, and hiding the panel or changing the row or the chat closes it.

import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { backgroundRows, poll, type BackgroundRow, type BackgroundTask } from "../core/background";
import { Button } from "../components/ui/button";
import { StatusGlyph } from "./status";
import { useTheme } from "./theme";

const POLL_MS = 3000;

export function BackgroundPanel({ app, session }: { app: App; session: string }) {
  const snapshot = useSyncExternalStore(app.subscribe, () => app.snapshot);
  const [tasks, setTasks] = useState<BackgroundTask[]>([]);
  const [picked, setPicked] = useState<string | null>(null);

  useEffect(() => {
    let stop = false;
    // A failed poll keeps the last list, and the next tick tries again.
    const end = poll(
      () => app.api.get<{ tasks: BackgroundTask[] }>("/api/background", { session }).then((r) => !stop && setTasks(r.tasks)),
      POLL_MS,
      () => document.hidden,
    );
    return () => {
      stop = true;
      end();
    };
  }, [app, session]);

  const rows = backgroundRows(
    snapshot?.sessions.find((s) => s.id === session),
    snapshot?.agents[session],
    tasks,
  );
  const row: BackgroundRow | undefined = rows.find((r) => r.key === picked) ?? rows[0];
  const pickAgent = (agent: string) => setPicked(`agent:${agent}`);

  return (
    <div className="flex h-full min-h-0 border-t bg-background" data-testid="background-panel">
      <ul className="w-64 shrink-0 overflow-y-auto border-r py-1 text-sm" aria-label="Agents and background tasks">
        {rows.map((r) => (
          <li key={r.key}>
            <button
              type="button"
              aria-current={r.key === row?.key}
              className={`flex w-full items-center gap-2 py-1 pr-2 text-left hover:bg-accent aria-[current=true]:bg-accent ${r.type === "task" ? "pl-6" : "pl-2"}`}
              onClick={() => setPicked(r.key)}
            >
              <StatusGlyph status={r.status} />
              <span className="min-w-0 flex-1 truncate">{r.type === "agent" ? r.title : r.task.title || r.task.id}</span>
              <span className="shrink-0 text-xs text-muted-foreground">{r.type === "agent" ? r.detail : r.task.kind}</span>
            </button>
          </li>
        ))}
        {!rows.length && <li className="px-2 py-1 text-muted-foreground">No live agents.</li>}
      </ul>
      <div className="flex min-w-0 flex-1 flex-col">{row && <Detail key={row.key} app={app} session={session} row={row} picked={row.key === picked} onOwner={pickAgent} />}</div>
    </div>
  );
}

function Detail({ app, session, row, picked, onOwner }: { app: App; session: string; row: BackgroundRow; picked: boolean; onOwner: (agent: string) => void }) {
  if (row.type === "agent") return <Term app={app} session={session} agent={row.agent} focus={picked} />;
  const { task } = row;
  return (
    <>
      <div className="flex items-center gap-2 border-b px-2 py-1 text-xs text-muted-foreground">
        <span className="min-w-0 flex-1 truncate">{task.summary ?? `${task.kind === "shell" ? "Shell" : "Subagent"} ${task.status}`}</span>
        <Button variant="ghost" size="sm" title="Stop it with /tasks in the owner's terminal" onClick={() => onOwner(row.agent)}>
          Open owner terminal
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        {task.kind === "shell" && task.output ? (
          <Term app={app} session={session} agent={row.agent} task={task.id} />
        ) : (
          <p className="p-2 text-sm text-muted-foreground">
            {task.kind === "shell"
              ? "The log names no output file for this shell, so there is nothing to show."
              : "A subagent's output is its own chat log; its owner reports the result."}
          </p>
        )}
      </div>
    </>
  );
}

const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const colors = () => ({ background: css("--background"), foreground: css("--foreground") });

/** One xterm on one socket. `task` makes it a read-only view of that shell's output file. `focus`
 *  is set only for a row the user chose, so a panel that opens with the chat leaves the keys there. */
function Term({ app, session, agent, task, focus = false }: { app: App; session: string; agent: string; task?: string; focus?: boolean }) {
  const shell = task !== undefined;
  const host = useRef<HTMLDivElement>(null);
  const live = useRef<Terminal | null>(null);
  // The socket opens later, so it reads the newest choice, not the one from the first render.
  const focused = useRef(focus);
  focused.current = focus;
  useEffect(() => {
    if (focus) live.current?.focus();
  }, [focus]);
  const theme = useTheme();
  useEffect(() => {
    const paint = () => {
      if (live.current) live.current.options.theme = colors();
    };
    paint();
    // On "system" the page colours follow the OS with no change to the saved choice.
    const os = matchMedia("(prefers-color-scheme: dark)");
    os.addEventListener("change", paint);
    return () => os.removeEventListener("change", paint);
  }, [theme]);
  useEffect(() => {
    const el = host.current!;
    const term = new Terminal({
      fontFamily: css("--font-mono"),
      fontSize: 12,
      convertEol: shell,
      disableStdin: shell,
      cursorBlink: !shell,
      theme: colors(),
    });
    live.current = term;
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    // App chords such as ⌘J and ⌘K go to the app; every other key goes to the terminal.
    term.attachCustomKeyEventHandler((e) => !app.registry.match(e));
    let ws: WebSocket | null = null;
    let gone = false;
    const send = (data: string | Uint8Array) => ws?.readyState === WebSocket.OPEN && ws.send(data);
    const resize = () => send(JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }));
    app.api
      .post<{ ticket: string }>("/api/terminal/ticket", { session, agent, ...(shell ? { task } : {}) })
      .then(({ ticket }) => {
        if (gone) return;
        ws = new WebSocket(`${location.origin.replace(/^http/, "ws")}/api/terminal?ticket=${encodeURIComponent(ticket)}`);
        ws.binaryType = "arraybuffer";
        ws.onopen = () => {
          fit.fit();
          resize();
          if (focused.current) term.focus();
        };
        ws.onmessage = (m) => term.write(typeof m.data === "string" ? m.data : new Uint8Array(m.data as ArrayBuffer));
        ws.onclose = () => !gone && term.write("\r\n\x1b[2m[closed]\x1b[0m\r\n");
      })
      .catch((err: unknown) => !gone && term.write(`${err instanceof Error ? err.message : String(err)}\r\n`));
    const encoder = new TextEncoder();
    const subs = [
      term.onData((d) => send(encoder.encode(d))),
      term.onBinary((d) => send(Uint8Array.from(d, (c) => c.charCodeAt(0)))),
      term.onResize(resize),
    ];
    const observer = new ResizeObserver(() => el.clientWidth && fit.fit());
    observer.observe(el);
    return () => {
      gone = true;
      observer.disconnect();
      for (const s of subs) s.dispose();
      ws?.close();
      live.current = null;
      term.dispose();
    };
  }, [app, session, agent, task]);
  return <div ref={host} className="h-full w-full overflow-hidden p-1" data-testid="terminal" />;
}
