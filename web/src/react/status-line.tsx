import { useEffect, useEffectEvent, useState } from "react";
import { Button } from "../components/ui/button";
import type { App, Composer } from "../core/app";
import { Glyph } from "./glyph";

const SHOW_AFTER_MS = 200;
const HOLD_MS = 400;

type Work = { shownAt: number; label: string; clock: string | null };

function clockSince(startS: number | undefined): string | null {
  if (typeof startS !== "number" || !Number.isSafeInteger(startS) || startS <= 0) return null;
  const elapsed = Math.floor(Date.now() / 1000) - startS;
  return elapsed < 0 ? null : `${Math.floor(elapsed / 60)}:${String(elapsed % 60).padStart(2, "0")}`;
}

export function StatusLine({ app, composer }: { app: App; composer: Composer }) {
  const { id, agent, state, workLabel = "Working" } = composer;
  const running = agent.running && !agent.waiting && !agent.unreachable;
  const [work, setWork] = useState<Work | null>(null);
  const shownAt = work?.shownAt ?? null;
  const sample = useEffectEvent(() => ({ label: workLabel, clock: clockSince(agent.turnStartTimeS) }));
  const reveal = useEffectEvent(() => setWork({ ...sample(), shownAt: performance.now() }));
  const refresh = useEffectEvent(() => {
    const next = sample();
    setWork((previous) => previous ? { ...next, shownAt: previous.shownAt } : null);
  });
  useEffect(() => {
    if (running && shownAt === null) {
      const timer = window.setTimeout(reveal, SHOW_AFTER_MS);
      return () => window.clearTimeout(timer);
    }
    if (running) {
      const timer = window.setInterval(refresh, 1_000);
      return () => window.clearInterval(timer);
    }
    if (shownAt === null) return;
    const timer = window.setTimeout(() => setWork(null), Math.max(0, HOLD_MS - (performance.now() - shownAt)));
    return () => window.clearTimeout(timer);
  }, [running, shownAt]);
  useEffect(() => {
    if (running && shownAt !== null) refresh();
  }, [running, shownAt, workLabel, agent.turnStartTimeS]);

  if (agent.unreachable || (!agent.waiting && !work)) return null;
  const label = agent.waiting ? "Waiting for your answer" : running ? workLabel : work?.label;
  const clock = agent.waiting ? null : work?.clock;
  const target = agent.who ?? (agent.provider ? `the ${agent.provider} agent` : "the agent");
  return (
    <div data-testid="chat-status-line" className="flex h-7 items-center gap-2 text-sm text-muted-foreground">
      {agent.waiting ? <span className="mx-0.5 size-2 shrink-0 rounded-full bg-waiting" aria-hidden /> : <Glyph form="working" size={12} className="shrink-0" aria-hidden />}
      <span role="status" className="min-w-0 truncate">{label}</span>
      {clock && <span className="shrink-0 font-mono tabular-nums" aria-label={`Turn elapsed: ${clock}`}>· {clock}</span>}
      <span className="flex-1" />
      {running && (
        <Button type="button" variant="outline" size="sm" className="h-6 px-2" data-testid="stop" aria-label={`Stop ${target}`} title={agent.who ? `Stop ${target}` : `Stop ${target} (⌘.)`} disabled={state.stopping} onClick={() => void app.interrupt(id)}>
          Stop
        </Button>
      )}
    </div>
  );
}
