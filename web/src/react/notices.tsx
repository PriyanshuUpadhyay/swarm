// The notice card (ADR 0102): when macOS blocks or holds swarm's notices, the steps to fix it and
// a button to the System Settings pane that does.

import { useEffect, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { noticesCard, type NoticeAction } from "../core/notices";
import { Button } from "../components/ui/button";

const TITLE: Record<string, string> = {
  allow: "Allow swarm's notices",
  blocked: "Swarm's notices are off",
  held: "A Focus is holding swarm's notices",
};
const BUTTON: Record<NoticeAction, string> = {
  allow: "Allow notices",
  notifications: "Open Notification settings",
  focus: "Open Focus settings",
  test: "Send a test notice",
};
const WAIT: Record<NoticeAction, string> = {
  allow: "Answer the macOS prompt…",
  notifications: "Opening System Settings…",
  focus: "Opening System Settings…",
  test: "Sending a test notice…",
};

export function NoticesCard({ app }: { app: App }) {
  const card = useSyncExternalStore(app.notices.subscribe, () => noticesCard(app.notices.state));
  const pending = useSyncExternalStore(app.notices.subscribe, () => app.notices.state.pending);
  useEffect(() => {
    // Fire-and-forget is safe: load keeps its own error in the notices state.
    void app.notices.load(true);
    const again = () => void app.notices.load();
    addEventListener("focus", again);
    return () => removeEventListener("focus", again);
  }, [app]);
  if (!card) return null;
  if (card === "fixed")
    return (
      <section className="m-3 flex items-center gap-3 rounded-md border border-border px-4 py-3" aria-label="Notices" data-testid="notices-card">
        <p className="flex-1 text-sm">Notices work. Swarm can tell you when an agent needs you.</p>
        <Button size="sm" variant="ghost" onClick={() => app.notices.hide()}>
          Done
        </Button>
      </section>
    );
  const actions: NoticeAction[] = card.action ? [card.action] : [];
  if (card.state === "held") actions.push("test");
  return (
    <section className="m-3 grid gap-2 rounded-md border border-border px-4 py-3" aria-label="Notices" data-testid="notices-card">
      <h2 className="font-semibold">{TITLE[card.state]}</h2>
      <p className="text-sm text-muted-foreground">{card.reason}</p>
      <ol className="list-decimal pl-5 text-sm">
        {card.steps.map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
      <div className="flex gap-2">
        {actions.map((action, i) => (
          // aria-disabled, not disabled: a disabled button drops keyboard focus to the page body.
          <Button key={action} size="sm" variant={i === 0 ? "default" : "outline"} aria-disabled={pending !== null || undefined} onClick={() => void app.notices.fix(action)}>
            {BUTTON[action]}
          </Button>
        ))}
        <Button size="sm" variant="ghost" onClick={() => app.notices.hide()}>
          Not now
        </Button>
      </div>
      <p role="status" className="text-sm text-muted-foreground">
        {pending ? WAIT[pending] : ""}
      </p>
    </section>
  );
}
