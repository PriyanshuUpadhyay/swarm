// The run chip `account · model · thinking` and its instant picker (ADR 0167).

import { ChevronDownIcon } from "lucide-react";
import { useEffect, useId, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import type { App } from "../core/app";
import { accountCli, accountColor, outcomeText, runLabel, runTip, savedCarry, switchOutcome, type AgentInput, type Run } from "../core/composer";
import { isSettled } from "../core/lifecycle";
import { Button } from "../components/ui/button";
import { useLife, ErrorLine } from "./lifecycle-shared";
import { SwitchProgress } from "./lifecycle-switch-model";
import { RunPicker, runModel, useRunCatalog } from "./run-picker";

export function RunChip({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const chosen = useLife(app, "switchModel");
  const handoff = useLife(app, "handoffs")[sessionId];
  const open = chosen === sessionId && !chair.who && !chair.unreachable;
  const favorite = useLife(app, "favoritePick");
  const login = !chair.who && chair.account ? accountCli(chair.provider ?? "", chair.model) : null;
  const [single, setSingle] = useState<string | null>(null);
  useEffect(() => {
    if (!login) return;
    let live = true;
    void app.life.runs.accounts(login).then(({ accounts }) => { if (live) setSingle(accounts.length === 1 ? login : null); }, () => {});
    return () => { live = false; };
  }, [app, login]);
  const multipleAccounts = !login || single !== login;
  const pop = useRef<HTMLDivElement>(null);
  // An anchor name must be a CSS ident; useId gives characters outside one.
  const anchor = `--run-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const switching = handoff && !isSettled(handoff.phase) ? (handoff.carry === "resume" ? "Restarting…" : "Switching…") : null;
  const run: Run = { provider: chair.provider ?? "claude", model: chair.model, effort: chair.effort, account: chair.account };

  useEffect(() => {
    const el = pop.current;
    if (!el) return;
    if (open && !el.matches(":popover-open")) {
      el.showPopover();
      // A popover takes focus only from an `autofocus` child at the moment it opens.
      el.querySelector<HTMLInputElement>("[data-testid=run-query]")?.focus();
    }
    if (!open && el.matches(":popover-open")) el.hidePopover();
  }, [open]);
  // A closed pane swaps the composer for Resume and Continue chat, which removes this chip with
  // no toggle event; the open state must not outlive it.
  // A child's chip never opens, so only the chair's chip closes it.
  const own = chair.who === null;
  useEffect(() => () => void (own && app.life.state.switchModel === sessionId && app.life.close("switchModel")), [app, sessionId, own]);

  return (
    <>
      <button
        type="button"
        data-testid="model-chip"
        className="flex min-w-0 items-center gap-1 rounded-md px-1.5 py-1 hover:bg-hover disabled:pointer-events-none"
        title={chair.who ? "Change the child from its parent agent." : runTip(run, chair.account && app.life.runs.email(run.provider, chair.account))}
        disabled={chair.who !== null}
        aria-expanded={open}
        style={{ anchorName: anchor } as CSSProperties}
        onClick={() => (open ? app.life.close("switchModel") : app.life.openSwitchModel(sessionId))}
      >
        {chair.account && multipleAccounts && <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: accountColor(chair.account) }} />}
        <span className="truncate" data-testid="model-chip-label">
          {switching ?? runLabel(multipleAccounts ? chair : { ...chair, account: null })}
        </span>
        {!chair.who && <ChevronDownIcon className="size-3 shrink-0" />}
      </button>
      {favorite?.session === sessionId && !chair.who && <span data-testid="run-favorite-toast" role="status" className="absolute z-10 -translate-y-full rounded-md border border-menu-border bg-popover px-2 py-1 text-xs" style={{ positionAnchor: anchor, positionArea: "top" } as CSSProperties}>{favorite.label}</span>}
      {/* A button's children are presentational, so a running switch is announced from outside it.
          The open popover has its own status. A child's chip never switches, and its composer keeps
          one status for the prompt caption. */}
      {!chair.who && (
        <span role="status" className="sr-only" data-testid="run-chip-status">
          {open ? "" : (switching ?? "")}
        </span>
      )}
      {!chair.who && (
        <div
          ref={pop}
          popover="auto"
          // Only an open dialog has the role; ⌘K waits for no dialog before it focuses the composer.
          role={open ? "dialog" : undefined}
          data-testid="run-popover"
          aria-label="Change model, thinking, or account"
          className="m-0 w-[min(560px,calc(100vw-32px))] rounded-xl border border-menu-border bg-popover text-sm text-popover-foreground shadow-2"
          style={{ positionAnchor: anchor, positionArea: "top span-right", marginBottom: 6, positionTryFallbacks: "flip-block" } as CSSProperties}
          onToggle={(e) => e.newState === "closed" && app.life.state.switchModel === sessionId && app.life.close("switchModel")}
        >
          {open && <RunBody app={app} sessionId={sessionId} chair={chair} key={sessionId} />}
        </div>
      )}
    </>
  );
}

function RunBody({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const handoff = useLife(app, "handoffs")[sessionId];
  const saved = useSyncExternalStore(app.subscribe, () => savedCarry(app.view.tree?.prefs));
  const current: Run = { provider: chair.provider ?? "claude", model: chair.model, effort: chair.effort, account: chair.account };
  const [draft, setDraft] = useState<Run>(current);
  const [preview, setPreview] = useState<Run | null>(null);
  const catalog = useRunCatalog(app, current.provider);
  const switching = handoff !== undefined && !isSettled(handoff.phase);
  const proposed = preview ?? draft;
  const { label } = runModel(catalog, proposed);
  const outcome = switchOutcome(current, proposed);
  const pick = (run: Run) => {
    setDraft(run);
    app.life.pickRun(sessionId, run);
  };

  const help = outcome !== "none" ? outcomeText(outcome, current, proposed, saved, label)
    : chair.running ? "Wait for this turn to end, or stop it."
    : "A pick applies now. The next reply may read the whole chat once, so it costs more.";
  return (
    <>
      <RunPicker app={app} catalog={catalog} current={current} draft={draft} setDraft={pick} disabled={switching} preview={setPreview} />
      <div className="grid gap-2 border-t border-menu-border p-2 text-xs text-muted-foreground">
        {handoff && <SwitchProgress handoff={handoff} />}
        <ErrorLine text={handoff?.error ?? catalog.error} />
        <span data-testid="run-outcome">{help}</span>
        {switching && <Button className="justify-self-end" type="button" size="sm" variant="outline" data-testid="run-cancel" disabled={!handoff?.canCancel} onClick={() => void app.life.cancelSwitch(sessionId)}>Cancel switch</Button>}
      </div>
    </>
  );
}
