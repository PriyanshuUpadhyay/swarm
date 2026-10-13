// Prints the budget table for results that `SwarmWeb --measure` wrote.
// usage: node web/perf/shell-table.mjs react.json [solid.json]
import fs from "node:fs";

const results = process.argv.slice(2).map((file) => JSON.parse(fs.readFileSync(file, "utf8")));
if (!results.length) {
  console.error("usage: shell-table.mjs RESULT.json ...");
  process.exit(2);
}

// [label, key, unit, budget, the throttle-check phases that the row depends on]
const rows = [
  ["key-to-paint p95", "key_p95", "ms", 50, ["key-to-paint"]],
  ["warm switch p95", "switch_p95", "ms", 100, ["warm switches"]],
  ["cold open p95", "open_p95", "ms", 150, ["cold opens"]],
  ["scroll dropped frames", "scroll_drop", "%", 0.01, ["scroll"]],
  ["giant output open p95", "giant_open_p95", "ms", 150, ["giant"]],
  ["live 12x50/s dropped frames", "live_drop", "%", 0.01, ["live", "live end"]],
  ["WebContent memory growth over soak", "webcontent_growth", "%", 0.1, ["soak", "soak samples"]],
];

const fmt = (v, unit) => (v == null ? "n/a" : unit === "%" ? `${(v * 100).toFixed(2)}%` : `${v.toFixed(1)} ms`);
// A value from a hidden or throttled page is not a pass, whatever the number says.
function verdict(r, key, budget, phases) {
  if (r[key] == null) return "not run";
  const checks = phases.map((name) => r.checks.find((c) => c.phase === name));
  if (checks.some((c) => !c || !c.valid)) return "INVALID (page throttled)";
  return r[key] <= budget ? "pass" : "FAIL";
}

let out = `| Metric | Budget | ${results.map((r) => `${r.app} shell`).join(" | ")} |\n|---|---|${results.map(() => "---|").join("")}\n`;
for (const [label, key, unit, budget, phases] of rows)
  out += `| ${label} | ${unit === "%" ? `<= ${budget * 100}%` : `<= ${budget} ms`} | ${results.map((r) => `${fmt(r[key], unit)} ${verdict(r, key, budget, phases)}`).join(" | ")} |\n`;

const each = (f) => results.map((r) => `${r.app} ${f(r)}`).join(", ");
out += `\nwindow: ${each((r) => `${r.window.width}x${r.window.height}`)}\n`;
out += `key p50: ${each((r) => fmt(r.key_p50, "ms"))}; NSEvent stamp to keydown handler p95: ${each((r) => fmt(r.key_lag_p95, "ms"))}\n`;
out += `scroll: ${each((r) => `${r.scroll_older_pages} older pages in ${r.scroll_seconds?.toFixed(1)} s, ${r.scroll_frames} frames, worst ${fmt(r.scroll_worst_ms, "ms")}`)}\n`;
out += `live: ${each((r) => `${r.live_rows_added} rows appended, ${r.live_frames} frames, worst ${fmt(r.live_worst_ms, "ms")}`)}\n`;
out += `JS heap: not measured, WKWebView has no heap API\n`;
out += `memory growth compares ${each((r) => `${r.webcontent_early_mb?.toFixed(1)} MB with ${r.webcontent_late_mb?.toFixed(1)} MB`)}\n`;
out += `soak: ${each((r) => `${r.soak_opens ?? "n/a"} chat opens, ${r.soak_stalls?.length ?? "n/a"} stalls${(r.soak_stalls ?? []).map((s) => ` [${s.min.toFixed(1)}m ${s.visibility} ${s.raf_hz} Hz occluded=${s.occluded} load ${s.load1} front=${s.frontmost}]`).join("")}`)}\n`;
out += `\n| Phase | ${results.map((r) => `${r.app} visibility, rAF Hz`).join(" | ")} |\n|---|${results.map(() => "---|").join("")}\n`;
for (const name of [...new Set(results.flatMap((r) => r.checks.map((c) => c.phase)))])
  out += `| ${name} | ${results.map((r) => r.checks.find((c) => c.phase === name)).map((c) => (c ? `${c.visibility}, ${c.raf_hz}${c.valid ? "" : " INVALID"}${c.load1 == null ? "" : ` (load ${c.load1})`}` : "n/a")).join(" | ")} |\n`;
for (const r of results.filter((r) => r.webcontent_mb))
  out += `\nWebContent MB per minute, ${r.app}: ${r.webcontent_mb.map((s) => `${s.min.toFixed(1)}m ${s.mb?.toFixed(1)}${s.valid ? "" : " INVALID"}`).join(", ")}\n`;
process.stdout.write(out);
