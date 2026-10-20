// Usage meters and the refresh wait shown in settings and the sidebar.

import { ChevronRightIcon, RefreshCwIcon, TriangleAlertIcon } from "lucide-react";
import { useEffect, useState } from "react";
import type { App } from "../core/app";
import { meterGroups, untilText, USAGE_PROVIDERS, waitText, type Meter } from "../core/settings";
import { Button } from "../components/ui/button";
import { store } from "./store";
import { label, useSettings, useNow } from "./settings-shared";

// ---- meters ----

/** One window: its name, a bar of what is left, the percent and the time to its reset. */
function MeterLine({ meter, now }: { meter: Meter; now: number }) {
  const left = meter.used_pct === null ? null : 100 - meter.used_pct;
  if (left === null || meter.state === "missing" || meter.state === "failed")
    return (
      <div className="meter-line" data-testid="meter" data-state={meter.state}>
        <span className="meter-window">{meter.window ?? ""}</span>
        <span className="min-w-0 flex-1 truncate text-faint-foreground">{meter.reason ?? meter.state}</span>
      </div>
    );
  const reset = meter.reset_time_seconds === null ? null : meter.reset_time_seconds - now / 1000;
  const tone = left <= 5 ? "bg-failed" : left <= 20 ? "bg-waiting" : "bg-foreground/70";
  return (
    <div className="meter-line" data-testid="meter" data-state={meter.state} data-old={meter.state === "stale" || undefined}>
      <span className="meter-window">{meter.window ?? ""}</span>
      <span className="meter-bar" role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={left} aria-label={`${meter.label} ${meter.window ?? ""} left`}>
        <span className={tone} style={{ width: `${left}%` }} />
      </span>
      <span className="w-8 text-right tabular-nums">{left}%</span>
      <span className="min-w-10 text-right whitespace-nowrap text-faint-foreground tabular-nums">{reset !== null && reset > 0 ? untilText(reset) : ""}</span>
    </div>
  );
}

function RefreshButton({ app, provider, now }: { app: App; provider: string; now: number }) {
  const refreshing = useSettings(app, "refreshing");
  const next = useSettings(app, "nextRefresh")[provider] ?? 0;
  const wait = next - now;
  const name = `Refresh ${label(provider)} usage`;
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      data-testid={`usage-refresh-${provider}`}
      aria-label={name}
      aria-busy={refreshing === provider}
      title={wait > 0 ? `Next refresh in ${waitText(wait)}` : name}
      disabled={refreshing !== null || wait > 0}
      onClick={() => void app.settings.refreshUsage(provider)}
    >
      <RefreshCwIcon className={refreshing === provider ? "animate-spin motion-reduce:animate-none" : undefined} />
    </Button>
  );
}

/** When the next refresh of a provider is allowed, as text, or null when it is now. */
function useWait(app: App, provider: string, now: number) {
  const next = useSettings(app, "nextRefresh")[provider] ?? 0;
  return next > now ? waitText(next - now) : null;
}

const OPEN_KEY = "swarm.usage.open";

/** The sidebar footer: one line per account, and every window when open. */
export function UsageMeters({ app }: { app: App }) {
  const usage = useSettings(app, "usage");
  const nextRefresh = useSettings(app, "nextRefresh");
  const [open, setOpen] = useState(store.getItem(OPEN_KEY) === "1");
  const cooling = Object.values(nextRefresh).some((t) => t > Date.now());
  const now = useNow(cooling ? 1000 : 60_000);
  useEffect(() => {
    if (app.settings.state.usage.state === "idle") void app.settings.loadUsage();
  }, [app]);
  const groups = meterGroups(usage.state === "ready" ? usage.data : null);
  const toggle = () => {
    store.setItem(OPEN_KEY, open ? "0" : "1");
    setOpen(!open);
  };
  return (
    <section aria-label="Usage" data-testid="usage-meters" className="usage-meters">
      <div className="flex items-center gap-1">
        <button className="flex min-w-0 flex-1 items-center gap-1 text-left" aria-expanded={open} onClick={toggle}>
          <ChevronRightIcon className={`size-3 shrink-0 transition-transform ${open ? "rotate-90" : ""}`} />
          <span className="font-medium text-muted-foreground">Usage</span>
          {!open && <span className="min-w-0 flex-1 truncate text-faint-foreground">{groups.map((g) => `${g.label} ${summary(g.meters)}`).join(" · ")}</span>}
        </button>
        {usage.state === "error" && <TriangleAlertIcon className="size-3 text-failed" aria-label={usage.error} />}
      </div>
      {open &&
        USAGE_PROVIDERS.map((provider) => {
          const mine = groups.filter((g) => g.provider === provider);
          if (!mine.length) return null;
          return (
            <div key={provider} className="grid gap-0.5" data-testid={`usage-${provider}`}>
              <div className="flex items-center gap-1">
                <span className="min-w-0 flex-1 truncate text-muted-foreground">{label(provider)}</span>
                <Wait app={app} provider={provider} now={now} />
                <RefreshButton app={app} provider={provider} now={now} />
              </div>
              {mine.map((g) => (
                <div key={g.key} className="grid gap-0.5">
                  {(mine.length > 1 || g.label !== "default") && <span className="truncate pl-1 text-faint-foreground">{g.label}</span>}
                  {g.meters.map((m, i) => (
                    <MeterLine key={`${m.window}:${i}`} meter={m} now={now} />
                  ))}
                </div>
              ))}
            </div>
          );
        })}
    </section>
  );
}

function Wait({ app, provider, now }: { app: App; provider: string; now: number }) {
  const refreshing = useSettings(app, "refreshing") === provider;
  const wait = useWait(app, provider, now);
  return refreshing || wait ? (
    <span className="text-faint-foreground tabular-nums" data-testid={`usage-wait-${provider}`}>
      {refreshing ? "refreshing…" : `next ${wait}`}
    </span>
  ) : null;
}

/** "87% left": the least left of an account's windows, or its reason. */
function summary(meters: Meter[]): string {
  const left = meters.filter((m) => m.used_pct !== null && (m.state === "fresh" || m.state === "stale")).map((m) => 100 - m.used_pct!);
  return left.length ? `${Math.min(...left)}%` : (meters[0]?.reason ?? meters[0]?.state ?? "");
}

export { MeterLine, useWait };
