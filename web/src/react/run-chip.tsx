// The run chip `account · model · thinking` and its instant picker (ADR 0167).

import { ChevronDownIcon } from "lucide-react";
import { useEffect, useId, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import type { App } from "../core/app";
import { accountCli, accountColor, cliName, endedCarry, outcomeText, runLabel, runTip, savedCarry, switchOutcome, type AgentInput, type Run } from "../core/composer";
import { isSettled } from "../core/lifecycle";
import { Button } from "../components/ui/button";
import { CliMark } from "./cli-mark";
import { ErrorLine } from "./lifecycle-shared";
import { SwitchProgress } from "./lifecycle-switch-model";
import { RunPicker, runModel, useRunCatalog } from "./run-picker";

export function RunChip({ app, sessionId, chair, ended = false }: { app: App; sessionId: string; chair: AgentInput; ended?: boolean }) {
  const state = useRunState(app);
  const chosen = state.switchModel;
  const handoff = state.handoffs[sessionId];
  const next = state.endedRun?.[sessionId];
  const resuming = state.resuming?.[sessionId] === true;
  const open = chosen === sessionId && !chair.who && (ended || !chair.unreachable) && !resuming;
  const shown = ended && next ? { ...chair, ...next } : chair;
  const favorite = state.favoritePick;

  const login = !chair.who && shown.account ? accountCli(shown.provider ?? "", shown.model) : null;
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
  const run: Run = { provider: shown.provider ?? "claude", model: shown.model, effort: shown.effort, account: shown.account };
  const status = ended ? null : switching;
  const named = multipleAccounts ? shown.account : null;
  const cliLabel = runLabel({ ...shown, account: null });
  const accountLabel = runLabel(shown);

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
  // A child's chip never opens. Leaving a chair's chip closes its popover.
  const own = chair.who === null;
  useEffect(() => () => void (own && app.life.state.switchModel === sessionId && app.life.close("switchModel")), [app, sessionId, own]);

  return (
    <>
      <button
        type="button"
        data-testid="model-chip"
        className={`flex min-w-0 items-center gap-1 rounded-md px-1.5 py-1 hover:bg-hover disabled:pointer-events-none ${ended ? "border border-menu-border" : ""}`}
        title={chair.who ? "Change the child from its parent agent." : runTip(run, shown.account && app.life.runs.email(run.provider, shown.account))}
        disabled={chair.who !== null || !chair.provider || resuming}
        aria-expanded={open}
        style={{ anchorName: anchor } as CSSProperties}
        onClick={() => (open ? app.life.close("switchModel") : app.life.openSwitchModel(sessionId))}
      >
        {shown.provider && <CliMark provider={shown.provider} selected={open} />}
        <span className="grid min-w-0">
          {/* Keep the chip the same width when the account list settles. */}
          {shown.account && !status && <span aria-hidden className="invisible col-start-1 row-start-1 flex min-w-0 items-center gap-1">
            {!named && <span className="size-2 shrink-0" />}
            <span className="truncate">{named ? cliLabel : accountLabel}</span>
          </span>}
          <span className="col-start-1 row-start-1 flex min-w-0 items-center gap-1">
            {named && <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: accountColor(named) }} />}
            {named && shown.provider && !status && <span className="sr-only">{cliName(shown.provider)} · </span>}
            <span className="truncate" data-testid="model-chip-label">{status ?? (named ? accountLabel : cliLabel)}</span>
          </span>
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
          {open && <RunBody app={app} sessionId={sessionId} chair={chair} ended={ended} key={sessionId} />}
        </div>
      )}
    </>
  );
}

export function useRunState(app: App) {
  const snapshot = () => app.life.state;
  return useSyncExternalStore(app.life.subscribe, snapshot, snapshot);
}

function RunSwitchStatus({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const state = useRunState(app);
  const open = state.switchModel === sessionId;
  if (chair.who || chair.unreachable || !open || !app.life.runBusy(sessionId)) return null;
  return <StopRun app={app} sessionId={sessionId} variant="ghost" />;
}

function StopRun({ app, sessionId, variant }: { app: App; sessionId: string; variant: "ghost" | "outline" }) {
  // interrupt puts a refusal in the composer's error banner.
  return <Button type="button" size="sm" variant={variant} disabled={app.composing.input(sessionId).stopping} onClick={() => void app.composing.interrupt(sessionId)}>Stop</Button>;
}

function RunSwitchBanner({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const pick = useRunState(app).pendingSwitch?.[sessionId];
  if (!pick || chair.who || chair.unreachable || app.life.runBusy(sessionId)) return null;
  return (
    <div data-testid="switch-pending" className="flex min-w-0 items-center gap-2 rounded-lg border border-menu-border p-1.5 text-xs text-foreground">
      <span className="min-w-0 flex-1 truncate font-mono text-[12px]" title={pick.model ?? undefined}>Switch to {pick.model}?</span>
      <Button type="button" size="sm" variant="outline" onClick={() => app.life.confirmSwitch(sessionId)}>Switch now</Button>
      <Button type="button" size="sm" variant="ghost" onClick={() => app.life.dismissSwitch(sessionId)}>Dismiss</Button>
    </div>
  );
}

RunChip.Status = RunSwitchStatus;
RunChip.Pending = RunSwitchBanner;

function RunBody({ app, sessionId, chair, ended }: { app: App; sessionId: string; chair: AgentInput; ended: boolean }) {
  const state = useRunState(app);
  const handoff = state.handoffs[sessionId];
  const saved = useSyncExternalStore(app.subscribe, () => savedCarry(app.view.tree?.prefs), () => savedCarry(app.view.tree?.prefs));
  const current: Run = { provider: chair.provider ?? "claude", model: chair.model, effort: chair.effort, account: chair.account };
  const selected = state.endedRun?.[sessionId];
  const pending = state.pendingSwitch?.[sessionId];
  const [local, setDraft] = useState<Run>(current);
  const draft = (ended ? selected : pending) ?? local;
  const [preview, setPreview] = useState<Run | null>(null);
  const catalog = useRunCatalog(app, ended ? draft.provider : current.provider);

  const readBusy = () => !ended && app.life.runBusy(sessionId);
  const busy = useSyncExternalStore(app.subscribe, readBusy, readBusy);
  const wasBusy = useRef(busy);
  useEffect(() => {
    if (wasBusy.current && !busy && pending && app.life.state.switchModel === sessionId) app.life.close("switchModel");
    wasBusy.current = busy;
  }, [app, busy, pending, sessionId]);
  const switching = handoff !== undefined && !isSettled(handoff.phase);
  const proposed = preview ?? draft;
  const { label } = runModel(catalog, proposed);
  const outcome = switchOutcome(current, proposed);
  const pick = (run: Run) => {
    setDraft(run);
    app.life.pickRun(sessionId, run);
  };

  const help = busy ? (pending ? `The agent is working. ${pending.model} does not apply until you confirm after the turn.` : "The agent is working. A pick does not apply now.")
    : ended ? (outcome === "handoff" ? `Resume: ${outcomeText(outcome, current, proposed, endedCarry(saved), label)}` : "A pick sets the run. Resume opens the same chat.")
    : outcome !== "none" ? outcomeText(outcome, current, proposed, saved, label)
    : "A pick applies now. The next reply may read the whole chat once, so it costs more.";
  return (
    <RunPicker app={app} catalog={catalog} current={current} draft={draft} setDraft={pick} disabled={switching} preview={setPreview} foot={
      <div className="grid gap-2 p-2">
        {handoff && <SwitchProgress handoff={handoff} />}
        <ErrorLine text={handoff?.error ?? catalog.error} />
        <div className="flex items-center justify-between gap-2">
          <span role="status" data-testid="run-outcome" className="h-[3lh] overflow-auto">{help}</span>
          {busy && <StopRun app={app} sessionId={sessionId} variant="outline" />}
        </div>
        {/* cancelSwitch posts a refusal as an app notice. */}
        {switching && <Button className="justify-self-end" type="button" size="sm" variant="outline" data-testid="run-cancel" disabled={!handoff?.canCancel} onClick={() => void app.life.cancelSwitch(sessionId)}>Cancel switch</Button>}
      </div>
    } />
  );
}
