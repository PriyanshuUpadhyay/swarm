import { useEffect, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { Button } from "../components/ui/button";

/** Settings always shows health, including a card the owner dismissed on Home. */
export function NoticesStatus({ app }: { app: App }) {
  const state = useSyncExternalStore(app.notices.subscribe, () => app.notices.state);
  useEffect(() => { void app.notices.load(); }, [app]);
  const health = state.health.state === "ready" ? state.health.data : null;
  return <section className="grid gap-2 rounded-md border border-border p-3" aria-label="System notification status">
    <p role="status" className="text-sm">{health ? `${health.state}: ${health.reason}` : state.health.state === "error" ? state.health.error : "Reading macOS notification status…"}</p>
    {health?.steps.map((step) => <p key={step} className="text-xs text-muted-foreground">{step}</p>)}
    <div className="flex flex-wrap gap-2">
      {health?.action && health.action !== "test" && <Button size="sm" variant="outline" disabled={!!state.pending} onClick={() => void app.notices.fix(health.action!)}>{health.action === "allow" ? "Allow notices" : health.action === "focus" ? "Open Focus settings" : "Open Notification settings"}</Button>}
      <Button size="sm" variant="outline" disabled={!!state.pending} onClick={() => void app.notices.fix("test")}>Send a test notice</Button>
    </div>
  </section>;
}
