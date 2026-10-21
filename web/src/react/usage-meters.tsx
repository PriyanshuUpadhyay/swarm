// Usage meters and the refresh wait shown in Settings › Accounts.

import type { App } from "../core/app";
import { untilText, waitText, type Meter } from "../core/settings";
import { useSettings } from "./settings-shared";

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

/** When the next refresh of a provider is allowed, as text, or null when it is now. */
function useWait(app: App, provider: string, now: number) {
  const next = useSettings(app, "nextRefresh")[provider] ?? 0;
  return next > now ? waitText(next - now) : null;
}

export { MeterLine, useWait };
