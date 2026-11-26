// The run chip `account · model · thinking` and its popover, which changes the chat's run
// (ADR 0110). Inside one CLI the session restarts on the new run; another CLI asks what to bring.

import { ChevronDownIcon, LoaderCircleIcon } from "lucide-react";
import { useEffect, useId, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import type { App } from "../core/app";
import { accountColor, applyBlock, applyLabel, CARRY_OPTIONS, outcomeText, runLabel, runTip, savedCarry, switchOutcome, type AgentInput, type Carry, type Run } from "../core/composer";
import { isSettled } from "../core/lifecycle";
import { Button } from "../components/ui/button";
import { useLife, ErrorLine } from "./lifecycle-shared";
import { SwitchProgress } from "./lifecycle-switch-model";
import { Segmented } from "./settings-shared";
import { RunPicker, runModel, useRunCatalog } from "./run-picker";

export function RunChip({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const chosen = useLife(app, "switchModel");
  const handoff = useLife(app, "handoffs")[sessionId];
  const open = chosen === sessionId && !chair.who && !chair.unreachable;
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
        {chair.account && <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: accountColor(chair.account) }} />}
        <span className="truncate" data-testid="model-chip-label">
          {switching ?? runLabel(chair)}
        </span>
        {!chair.who && <ChevronDownIcon className="size-3 shrink-0" />}
      </button>
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
          className="m-0 w-[min(640px,calc(100vw-32px))] rounded-xl border border-border-strong bg-popover p-3 text-sm text-popover-foreground shadow-panel"
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
  const provider = current.provider;
  const [draft, setDraft] = useState<Run>(current);
  const [carry, setCarry] = useState<Carry>(saved);
  const catalog = useRunCatalog(app, provider);
  const running = handoff !== undefined && !isSettled(handoff.phase);

  const { label } = runModel(catalog, draft);
  const outcome = switchOutcome(current, draft);
  const blocked = applyBlock(chair, draft);
  const error = catalog.error;

  const apply = () => {
    if (!draft.model || outcome === "none") return;
    const how = outcome === "resume" ? "resume" : carry;
    // switchModel keeps its own failure in the handoff state, so nothing waits for it.
    void app.life.switchModel(sessionId, draft.provider, draft.model, draft.account, draft.effort, how);
  };

  return (
    <div className="grid gap-3">
      <RunPicker app={app} catalog={catalog} current={current} draft={draft} setDraft={setDraft} disabled={running} note={(p) => (p !== current.provider ? (carry === "summary" ? "new chat + summary" : "new chat") : null)} />
      {outcome === "handoff" && !running && (
        <div className="flex flex-wrap items-center gap-2" data-testid="run-carry">
          <span className="text-xs text-muted-foreground">Bring over</span>
          <Segmented<Carry>
            label="Bring over"
            testid="run-carry-choice"
            value={carry}
            options={CARRY_OPTIONS}
            onChange={setCarry}
          />
          {/* setPref shows its own notice on failure, so the promise never rejects. */}
          {carry !== saved && (
            <Button type="button" variant="ghost" size="sm" data-testid="run-carry-default" onClick={() => void app.settings.setPref("switchCarry", carry)}>
              Make default
            </Button>
          )}
        </div>
      )}
      {handoff && <SwitchProgress handoff={handoff} />}
      <ErrorLine text={handoff?.error ?? error} />
      <div className={`rounded-xl border px-3 py-2 text-xs ${outcome === "none" && !running ? "border-dashed border-border text-muted-foreground" : "border-border bg-hover"}`} role="status">
        {outcome !== "none" && !running && !blocked && <b className="mr-1.5 font-medium text-foreground">What happens</b>}
        <span data-testid="run-outcome">{running ? "Switching…" : (blocked ?? outcomeText(outcome, current, draft, carry, label))}</span>
      </div>
      <div className="flex items-center justify-end gap-2">
        {running ? (
          <Button type="button" variant="outline" size="sm" data-testid="run-cancel" disabled={!handoff?.canCancel} onClick={() => void app.life.cancelSwitch(sessionId)}>
            Cancel switch
          </Button>
        ) : (
          <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("switchModel")}>
            Cancel
          </Button>
        )}
        <Button type="button" size="sm" data-testid="run-apply" disabled={running || blocked !== null || outcome === "none"} onClick={apply}>
          {running && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
          {applyLabel(outcome)}
        </Button>
      </div>
    </div>
  );
}
