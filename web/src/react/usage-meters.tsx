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
        <span className="min-w-0 flex-1 truncate text-muted-foreground">{meter.reason ?? meter.state}</span>
      </div>
    );
  const reset = meter.reset_time_seconds === null ? null : meter.reset_time_seconds - now / 1000;
  const tone = meterColor(left);
  return (
    <div className="meter-line" data-testid="meter" data-state={meter.state} data-old={meter.state === "stale" || undefined}>
      <span className="meter-window">{meter.window ?? ""}</span>
      <span className="meter-bar" role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={left} aria-label={`${meter.label} ${meter.window ?? ""} left`}>
        <span className={tone.fill} style={{ width: `${left}%` }} />
      </span>
      <span className={`w-8 text-right tabular-nums ${tone.label}`}>{left}%</span>
      <span className="min-w-10 text-right whitespace-nowrap text-muted-foreground tabular-nums">{reset !== null && reset > 0 ? untilText(reset) : ""}</span>
    </div>
  );
}

function meterColor(left: number) {
  if (left <= 5) return { fill: "bg-failed", label: "text-failed" };
  if (left <= 20) return { fill: "bg-waiting", label: "text-waiting" };
  return { fill: "bg-foreground", label: "text-foreground" };
}

/** The fill for what is left, shared by Settings and the run popover. */
function meterTone(left: number) {
  return meterColor(left).fill;
}

/** When the next refresh of a provider is allowed, as text, or null when it is now. */
function useWait(app: App, provider: string, now: number) {
  const next = useSettings(app, "nextRefresh")[provider] ?? 0;
  return next > now ? waitText(next - now) : null;
}

export { MeterLine, meterTone, useWait };
