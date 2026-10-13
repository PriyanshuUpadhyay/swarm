// Web pilot budget suite. Builds nothing; run `pnpm -C web perf` after the release builds.
// usage: node web/perf/run.mjs [--browsers chromium,webkit] [--apps react] [--live-seconds 60] [--soak-minutes 10]
// Needs: target/release/swarm built with --features serve, the ReleaseSafe transcript tool, web/dist.
// PERF_SWARM and PERF_TOOL point at other builds of the two binaries.
import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
import { fileURLToPath } from "node:url";
import { chromium, webkit } from "@playwright/test";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../..");
const args = Object.fromEntries(process.argv.slice(2).reduce((acc, v, i, a) => (i % 2 ? acc : [...acc, [v.replace(/^--/, ""), a[i + 1]]]), []));
const browsers = (args.browsers ?? "chromium,webkit").split(",");
const apps = (args.apps ?? "react").split(",");
const liveSeconds = Number(args["live-seconds"] ?? 60);
const soakMinutes = Number(args["soak-minutes"] ?? 10);
const swarm = path.resolve(repo, process.env.PERF_SWARM ?? "target/release/swarm");
const tool = path.resolve(repo, process.env.PERF_TOOL ?? "packages/transcript/zig-out/bin/transcript");
const home = path.join(repo, "tmp/web-pilot/perf-home");

const log = (...m) => console.error(new Date().toISOString().slice(11, 19), ...m);

const BUDGET = {
  key_p95: 50,
  switch_p95: 100,
  open_p95: 150,
  scroll_drop: 0.01,
  live_drop: 0.01,
  pane_live_drop: 0.01,
  giant_open_p95: 150,
  heap_growth: 0.10,
};

const p = (xs, q) => {
  if (!xs.length) return NaN;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.ceil(q * s.length) - 1)];
};

function startServer() {
  const child = spawn(swarm, ["serve", "--port", "0", "--assets", path.join(repo, "web/dist")], {
    env: { ...process.env, SWARM_HOME: home, SWARM_TRANSCRIPT_TOOL: tool },
    stdio: ["ignore", "pipe", "inherit"],
  });
  return new Promise((resolve, reject) => {
    const lines = readline.createInterface({ input: child.stdout });
    lines.once("line", (url) => resolve({ child, url: url.trim() }));
    child.once("exit", (code) => reject(new Error(`swarm serve exited ${code}`)));
  });
}

// Frame gaps from requestAnimationFrame while `action` runs. A gap over 1.5 frames counts as dropped.
async function frames(page, action) {
  await page.evaluate(() => {
    window.__frames = [];
    let last = performance.now();
    const tick = (t) => {
      window.__frames.push(t - last);
      last = t;
      if (window.__frameRun) requestAnimationFrame(tick);
    };
    window.__frameRun = true;
    requestAnimationFrame(tick);
  });
  await action();
  const gaps = await page.evaluate(() => {
    window.__frameRun = false;
    return window.__frames.slice(1);
  });
  const dropped = gaps.filter((g) => g > 25).length;
  return { frames: gaps.length, dropped, rate: gaps.length ? dropped / gaps.length : NaN, worst: Math.max(...gaps) };
}

const perfOf = (page, kind) => page.evaluate((k) => (window.__swarmPerf ?? []).filter((r) => r.kind === k).map((r) => r.painted - r.start), kind);
const clearPerf = (page) => page.evaluate(() => { window.__swarmPerf = []; });

// The newest row of the chat, not of a child's pane.
const maxIndex = (page) => page.evaluate(() => Math.max(-1, ...[...document.querySelectorAll('[data-testid="transcript"] [data-testid="row"]')].map((r) => Number(r.getAttribute("data-index")))));
// The newest row index of each pane on screen, added up.
const paneRows = (page) => page.evaluate(() => [...document.querySelectorAll('[data-testid="pane-transcript"]')].reduce((sum, pane) => sum + Math.max(-1, ...[...pane.querySelectorAll('[data-testid="row"]')].map((r) => Number(r.getAttribute("data-index")))), 0));

async function heap(page, cdp) {
  if (!cdp) return NaN;
  await cdp.send("HeapProfiler.collectGarbage");
  const { metrics } = await cdp.send("Performance.getMetrics");
  return metrics.find((m) => m.name === "JSHeapUsedSize").value;
}

// Opens the chat at sidebar position `index` by keyboard: Home, then j `index` times, then Enter.
async function openChat(page, index) {
  await page.getByTestId("sidebar").focus();
  await page.keyboard.press("Home");
  for (let i = 0; i < index; i++) await page.keyboard.press("j");
  // The app keeps only its newest records, so match this Enter by time, not by count.
  const before = await page.evaluate(() => performance.now());
  await page.keyboard.press("Enter");
  // Done when the app records the paint of this open or switch, not when old rows are on screen.
  try {
    await page.waitForFunction((t) => (window.__swarmPerf ?? []).some((r) => r.kind !== "key" && r.start >= t), before, { timeout: 30000 });
  } catch (error) {
    const state = await page.evaluate(() => ({
      selected: document.querySelector('[data-testid="tree-row"][aria-selected="true"]')?.outerHTML.slice(0, 300),
      open: document.querySelector('[data-testid="transcript"]')?.getAttribute("data-chat"),
      state: document.querySelector('[data-testid="transcript"]')?.getAttribute("data-state"),
      rows: document.querySelectorAll('[data-testid="tree-row"]').length,
      last: (window.__swarmPerf ?? []).slice(-3),
    }));
    log("openChat timeout", index, JSON.stringify(state));
    throw error;
  }
}

async function suite(browserName, app, url) {
  const launcher = browserName === "chromium" ? chromium : webkit;
  const browser = await launcher.launch();
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const cdp = browserName === "chromium" ? await page.context().newCDPSession(page) : null;
  if (cdp) await cdp.send("Performance.enable");
  const entry = "/";
  const [origin, hash] = url.split("#");
  const open = async (query = "") => {
    await page.goto(`${origin.replace(/\/$/, "")}${entry}${query}#${hash}`);
    await page.waitForFunction(() => window.__swarmReady === true, null, { timeout: 15000 });
  };
  const result = { browser: browserName, app };

  await open();
  // The pane chat has its own scenario; the other scenarios keep the chats they had before it.
  const paneChat = `c:${JSON.parse(fs.readFileSync(path.join(home, "manifest.json"), "utf8")).panes.id}`;
  const chatRows = await page.getByTestId("tree-row").evaluateAll((rows, skip) => rows.map((r, i) => ({ i, chat: r.getAttribute("data-kind") === "chat" && r.getAttribute("data-id") !== skip })).filter((r) => r.chat).map((r) => r.i), paneChat);

  log(browserName, app, "cold opens");
  // Cold opens: cycle through every chat, so the 4-chat LRU always misses.
  await clearPerf(page);
  for (let round = 0; round < 3; round++) for (const i of chatRows.slice(2)) await openChat(page, i);
  result.open_p95 = p(await perfOf(page, "open"), 0.95);

  log(browserName, app, "warm switches");
  // Warm switches between two cached chats.
  await openChat(page, chatRows[2]);
  await openChat(page, chatRows[3]);
  await clearPerf(page);
  for (let k = 0; k < 30; k++) await openChat(page, chatRows[2 + (k % 2)]);
  result.switch_p95 = p(await perfOf(page, "switch"), 0.95);

  log(browserName, app, "key-to-paint");
  // Key-to-paint: j/k selection moves in the sidebar.
  await page.getByTestId("sidebar").focus();
  await clearPerf(page);
  for (let k = 0; k < 120; k++) {
    await page.keyboard.press(k % 20 < 10 ? "j" : "k");
    await page.waitForTimeout(20);
  }
  result.key_p95 = p(await perfOf(page, "key"), 0.95);
  result.key_p50 = p(await perfOf(page, "key"), 0.5);

  log(browserName, app, "scroll");
  // Scroll the 100k-event chat upward, paging older rows in as it goes.
  await openChat(page, chatRows[0]);
  let olderPages = 0;
  const countOlder = (request) => { if (request.url().includes("/api/rows") && request.url().includes("before=")) olderPages++; };
  page.on("request", countOlder);
  const scroll = await frames(page, async () => {
    // Scroll until 10k log lines are paged in (pages of TAIL = 200 lines), capped at 90 s.
    const end = Date.now() + 90000;
    const list = page.getByTestId("transcript");
    await list.hover();
    while (Date.now() < end && olderPages * 200 < 10000) {
      await page.mouse.wheel(0, -600);
      await page.waitForTimeout(16);
    }
  });
  result.scroll_drop = scroll.rate;
  page.off("request", countOlder);
  result.scroll_rows = await page.locator('[data-testid="transcript"] [data-testid="row"]').count();
  result.scroll_older_pages = olderPages;

  log(browserName, app, "giant");
  // Giant tool output chat.
  await clearPerf(page);
  for (let k = 0; k < 10; k++) {
    await openChat(page, chatRows[1]);
    await openChat(page, chatRows[4 + (k % 4)]);
  }
  const giant = await perfOf(page, "open").then(async (opens) => opens.concat(await perfOf(page, "switch")));
  result.giant_open_p95 = p(giant, 0.95);

  log(browserName, app, "live");
  // Live load: every chair log grows at 50 lines/s while the page follows all 12 chats.
  await open("?live=all");
  await openChat(page, chatRows[2]);
  const liveStart = await maxIndex(page);
  const gen = spawn("node", [path.join(here, "gen.mjs"), "live", "--home", home, "--rate", "50", "--seconds", String(liveSeconds)], { stdio: "inherit" });
  // Listen before the frame loop: the generator can exit before the loop ends.
  const genExit = new Promise((r) => gen.once("exit", r));
  const live = await frames(page, async () => {
    const end = Date.now() + liveSeconds * 1000;
    while (Date.now() < end) {
      await page.keyboard.press("j");
      await page.waitForTimeout(250);
    }
  });
  await genExit;
  await page.waitForTimeout(500);
  result.live_rows_added = (await maxIndex(page)) - liveStart;
  result.live_drop = live.rate;
  result.live_worst_ms = live.worst;

  log(browserName, app, "pane live");
  // Pane live load: the chat's chair and four child logs grow at 50 lines/s while all four panes
  // are on screen (two columns of two) and follow their logs.
  await open();
  await page.locator(`[data-testid="tree-row"][data-id="${paneChat}"]`).click();
  await page.waitForFunction(() => document.querySelectorAll('[data-testid="pane"]').length === 4);
  await page.getByTestId("strip").evaluate((el) => (el.scrollLeft = el.scrollWidth));
  await page.waitForFunction(() => {
    const panes = [...document.querySelectorAll('[data-testid="pane"]')];
    const shown = (el) => el.getBoundingClientRect().left >= 0 && el.getBoundingClientRect().right <= innerWidth;
    return panes.length === 4 && panes.every((p) => shown(p) && p.querySelector('[data-testid="pane-transcript"][data-state="ready"] [data-testid="row"]'));
  });
  await page.waitForTimeout(500);
  const paneStart = await paneRows(page);
  const paneGen = spawn("node", [path.join(here, "gen.mjs"), "live", "--home", home, "--rate", "50", "--seconds", String(liveSeconds), "--target", "panes"], { stdio: "inherit" });
  const paneGenExit = new Promise((r) => paneGen.once("exit", r));
  const paneLive = await frames(page, async () => {
    const end = Date.now() + liveSeconds * 1000;
    while (Date.now() < end) {
      await page.keyboard.press("j");
      await page.waitForTimeout(250);
    }
  });
  await paneGenExit;
  await page.waitForTimeout(500);
  result.pane_live_rows_added = (await paneRows(page)) - paneStart;
  result.pane_live_drop = paneLive.rate;
  result.pane_live_worst_ms = paneLive.worst;

  log(browserName, app, "soak");
  // Memory soak: switch chats for `soakMinutes`; compare the heap at 20% of the run with the end.
  // Samples are taken only after a full cycle of chats, so the 4-chat LRU holds the same chats each
  // time; the ~9 MB giant output is in the LRU for 4 of 12 positions and would swing the heap.
  if (cdp && soakMinutes > 0) {
    await open();
    const start = Date.now();
    const end = start + soakMinutes * 60000;
    const samples = [];
    let k = 0;
    while (Date.now() < end || k % chatRows.length) {
      await openChat(page, chatRows[k++ % chatRows.length]);
      if (k % chatRows.length === 0 && Date.now() >= start + samples.length * 60000)
        samples.push({ k, min: (Date.now() - start) / 60000, mb: (await heap(page, cdp)) / 1e6 });
    }
    if (samples.at(-1)?.k !== k) samples.push({ k, min: (Date.now() - start) / 60000, mb: (await heap(page, cdp)) / 1e6 });
    const early = samples.find((s) => s.min >= soakMinutes * 0.2).mb;
    const late = samples.at(-1).mb;
    result.heap_growth = (late - early) / early;
    result.heap_mb = late;
    result.heap_samples = samples;
  } else {
    result.heap_growth = NaN;
  }
  await browser.close();
  return result;
}

const fmt = (v, unit) => (Number.isNaN(v) || v === undefined ? "n/a" : unit === "%" ? `${(v * 100).toFixed(2)}%` : `${v.toFixed(1)} ms`);
const verdict = (v, budget) => (Number.isNaN(v) || v === undefined ? "not run" : v <= budget ? "pass" : "FAIL");

execFileSync("node", [path.join(here, "gen.mjs"), "setup", "--home", home, "--swarm", swarm], { stdio: "inherit" });
const results = [];
for (const b of browsers) {
  for (const a of apps) {
    const { child, url } = await startServer();
    try {
      results.push(await suite(b, a, url));
    } finally {
      child.kill();
    }
    // Live load appended to the logs; rebuild the home so each run sees the same data.
    execFileSync("node", [path.join(here, "gen.mjs"), "setup", "--home", home, "--swarm", swarm], { stdio: "inherit" });
  }
}

const rows = [
  ["key-to-paint p95", "key_p95", "ms"],
  ["warm switch p95", "switch_p95", "ms"],
  ["cold open p95", "open_p95", "ms"],
  ["scroll dropped frames", "scroll_drop", "%"],
  ["giant output open p95", "giant_open_p95", "ms"],
  ["live 12x50/s dropped frames", "live_drop", "%"],
  ["live 4 panes x50/s dropped frames", "pane_live_drop", "%"],
  ["heap growth over soak", "heap_growth", "%"],
];
let out = `| Metric | Budget | ${results.map((r) => `${r.app} ${r.browser}`).join(" | ")} |\n|---|---|${results.map(() => "---|").join("")}\n`;
for (const [label, key, unit] of rows) {
  out += `| ${label} | ${unit === "%" ? `<= ${BUDGET[key] * 100}%` : `<= ${BUDGET[key]} ms`} | ${results.map((r) => `${fmt(r[key], unit)} ${verdict(r[key], BUDGET[key])}`).join(" | ")} |\n`;
}
out += `\nkey p50: ${results.map((r) => `${r.app} ${r.browser} ${fmt(r.key_p50, "ms")}`).join(", ")}\n`;
out += `rows in DOM after scroll: ${results.map((r) => `${r.app} ${r.browser} ${r.scroll_rows}`).join(", ")}\n`;
out += `older 200-line pages fetched while scrolling (50 = 10k lines): ${results.map((r) => `${r.app} ${r.browser} ${r.scroll_older_pages}`).join(", ")}\n`;
out += `rows appended to the open chat during live load: ${results.map((r) => `${r.app} ${r.browser} ${r.live_rows_added}`).join(", ")}\n`;
out += `live worst frame: ${results.map((r) => `${r.app} ${r.browser} ${fmt(r.live_worst_ms, "ms")}`).join(", ")}\n`;
out += `rows appended to the four panes during pane live load: ${results.map((r) => `${r.app} ${r.browser} ${r.pane_live_rows_added}`).join(", ")}\n`;
out += `pane live worst frame: ${results.map((r) => `${r.app} ${r.browser} ${fmt(r.pane_live_worst_ms, "ms")}`).join(", ")}\n`;
for (const r of results.filter((r) => r.heap_samples))
  out += `heap MB per minute, ${r.app} ${r.browser}: ${r.heap_samples.map((s) => `${s.min.toFixed(1)}m ${s.mb.toFixed(2)}`).join(", ")}\n`;
console.log(out);
fs.writeFileSync(path.join(repo, "tmp/web-pilot/perf-result.md"), out);
fs.writeFileSync(path.join(repo, "tmp/web-pilot/perf-result.json"), JSON.stringify(results, null, 2));
