// The notice card on Home (ADR 0102): when macOS blocks or holds swarm's notices, the steps to fix
// it and a button to the System Settings pane that does.

import { useEffect, useRef, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { noticesCard, type CardState, type NoticeAction } from "../core/notices";
import { Button } from "../components/ui/button";

const TITLE: Record<CardState, string> = {
  allow: "Allow swarm's notices",
  blocked: "Swarm's notices are off",
  held: "A Focus is holding swarm's notices",
  unknown: "Swarm could not check its notices",
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
const FIXED = "Notices work. Swarm can tell you when an agent needs you.";

export function NoticesCard({ app }: { app: App }) {
  const card = useSyncExternalStore(app.notices.subscribe, () => noticesCard(app.notices.state));
  const pending = useSyncExternalStore(app.notices.subscribe, () => app.notices.state.pending);
  const done = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    // Fire-and-forget is safe: load keeps its own error in the notices state.
    void app.notices.load(true);
    const again = () => void app.notices.load();
    addEventListener("focus", again);
    return () => removeEventListener("focus", again);
  }, [app]);
  // The pressed button leaves the card, so focus goes to Done rather than to the page body.
  useEffect(() => {
    if (card === "fixed") done.current?.focus();
  }, [card]);
  if (!card) return null;
  const actions: NoticeAction[] = card !== "fixed" && card.action ? [card.action] : [];
  if (card !== "fixed" && card.state === "held") actions.push("test");
  return (
    <section className="grid gap-2 rounded-md border border-border px-4 py-3" aria-label="Notices" data-testid="notices-card">
      {card !== "fixed" && (
        <>
          <h2 className="font-semibold">{TITLE[card.state]}</h2>
          <p className="text-sm text-muted-foreground">{card.reason}</p>
          {card.steps.length > 0 && (
            <ol className="list-decimal pl-5 text-sm">
              {card.steps.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          )}
        </>
      )}
      {/* One live region for both forms, so a screen reader hears the wait and the result. */}
      <p role="status" className="text-sm">
        {pending ? WAIT[pending] : card === "fixed" ? FIXED : ""}
      </p>
      <div className="flex flex-wrap gap-2">
        {actions.map((action, i) => (
          // aria-disabled, not disabled: a disabled button drops keyboard focus to the page body.
          // Fire-and-forget is safe: fix shows its own error as a notice.
          <Button key={action} size="sm" variant={i === 0 ? "default" : "outline"} aria-disabled={pending !== null || undefined} onClick={() => void app.notices.fix(action)}>
            {BUTTON[action]}
          </Button>
        ))}
        <Button ref={done} size="sm" variant="ghost" onClick={() => app.notices.hide()}>
          {card === "fixed" ? "Done" : "Not now"}
        </Button>
      </div>
    </section>
  );
}
