import { expect, test } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { row, rows, open, mockRows } from "./ux-helpers";


test("Find steps through occurrences inside a long answer and paints the selected word", async ({ page: tab }, info) => {
  await mockRows(tab, [row("a1", `needle\n\n${"A paragraph with ordinary text.\n\n".repeat(70)}needle`)]);
  await open(tab);
  await tab.keyboard.press("Meta+f");
  await tab.getByTestId("find-input").fill("needle");
  await expect(tab.getByTestId("find-count")).toHaveText("1 of 2");
  await expect.poll(() => tab.evaluate(() => CSS.highlights.get("find-current")?.size)).toBe(1);
  const newest = await tab.getByTestId("transcript").evaluate((el) => el.scrollTop);
  await tab.getByTestId("find-input").press("Enter");
  await expect(tab.getByTestId("find-count")).toHaveText("2 of 2");
  await expect.poll(() => tab.getByTestId("transcript").evaluate((el) => el.scrollTop)).toBeLessThan(newest - 500);
  await expect.poll(() => tab.evaluate(() => {
    const range = [...CSS.highlights.get("find-current")!][0] as Range;
    const rect = range.getBoundingClientRect();
    const view = document.querySelector('[data-testid="transcript"]')!.getBoundingClientRect();
    return rect.top >= view.top && rect.bottom <= view.bottom;
  })).toBe(true);
  await tab.screenshot({ path: info.outputPath("find-occurrence.png") });
});

test("Find searches loaded full output and reveals its virtual line", async ({ page: tab }, info) => {
  const r = row("r1", "result", "toolResult");
  r.output = { id: "full-output", text: "preview", full_lines: 1000, hidden_lines: 999, full_bytes: 20000, is_truncated: true };
  await mockRows(tab, [r]);
  await tab.route("**/api/tool-output?*", (route) => route.fulfill({ json: { text: Array.from({ length: 1000 }, (_, i) => i === 900 ? "unique needle" : `line ${i}`).join("\n") } }));
  await open(tab);
  await tab.getByTestId("show-full").click();
  await tab.keyboard.press("Meta+f");
  await tab.getByTestId("find-input").fill("unique needle");
  await expect(tab.getByTestId("find-count")).toHaveText("1 of 1");
  await expect(tab.getByTestId("full-output").getByText("unique needle", { exact: true })).toBeVisible();
  await tab.screenshot({ path: info.outputPath("find-full-output.png") });
});

test("older history shows progress, error and Retry; empty Find offers earlier search", async ({ page: tab }, info) => {
  let fail = true;
  let release!: () => void;
  const wait = new Promise<void>((r) => { release = r; });
  await tab.route("**/api/rows?*", async (route) => {
    if (!new URL(route.request().url()).searchParams.has("before")) return route.fulfill({ json: rows([row("a2", "Current message")], 100) });
    if (fail) { await wait; return route.fulfill({ status: 503, body: "History offline" }); }
    return route.fulfill({ json: rows([row("a1", "old needle")]) });
  });
  await open(tab);
  await tab.keyboard.press("Meta+f");
  await tab.getByTestId("find-input").fill("needle");
  await tab.getByRole("button", { name: "Search earlier messages" }).click();
  await expect(tab.getByTestId("load-older")).toHaveText("Loading earlier messages…");
  release();
  await expect(tab.getByRole("alert")).toContainText("History offline");
  fail = false;
  await tab.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(tab.getByTestId("find-count")).toHaveText("1 of 1");
  await expect(tab.getByRole("alert")).toHaveCount(0);
  await tab.screenshot({ path: info.outputPath("history-retry.png") });
});

test("a failed transcript has a working Retry button", async ({ page: tab }, info) => {
  let fail = true;
  await tab.route("**/api/rows?*", (route) => route.fulfill(fail ? { status: 503, body: "Rows offline" } : { json: rows([row("a1", "Recovered transcript")]) }));
  await open(tab);
  await expect(tab.getByText("Could not load this chat")).toBeVisible();
  fail = false;
  await tab.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(tab.getByText("Recovered transcript", { exact: true })).toBeVisible();
  await tab.screenshot({ path: info.outputPath("transcript-retry.png") });
});

test("image upload blocks Send and preserves edits made while it waits", async ({ page: tab }, info) => {
  let release!: () => void;
  const wait = new Promise<void>((r) => { release = r; });
  await tab.route("**/api/chat/image", async (route) => { await wait; await route.fulfill({ json: { path: "/tmp/uploads/12345678-1234-1234-1234-123456789abc.png" } }); });
  await open(tab);
  const composer = tab.getByTestId("composer");
  await composer.fill("replace this tail");
  await composer.evaluate((el: HTMLTextAreaElement) => {
    el.setSelectionRange(0, 12);
    const data = new DataTransfer();
    data.items.add(new File(["image"], "screen.png", { type: "image/png" }));
    el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true }));
  });
  await expect(composer).toHaveValue(/Uploading image/);
  await expect(tab.getByTestId("send")).toBeDisabled();
  await composer.press("End");
  await composer.press("Meta+ArrowRight");
  await tab.keyboard.insertText(" more text");
  release();
  await expect(composer).toHaveValue(/123456789abc.png.*tail more text/);
  await expect(tab.getByTestId("send")).toBeEnabled();
  await tab.getByRole("button", { name: "Remove image", exact: true }).focus();
  await expect(tab.getByRole("button", { name: "Remove image", exact: true })).toBeVisible();
  await tab.screenshot({ path: info.outputPath("image-upload.png") });
});

test("run picker accepts Down and Enter, retains Codex account wording and reloads cached usage", async ({ page: tab }, info) => {
  await tab.route("**/api/providers", (route) => route.fulfill({ json: { providers: [{ id: "codex", label: "Codex", accounts: true, efforts: ["high"], default_effort: "high" }] } }));
  await tab.route("**/api/models?*", (route) => route.fulfill({ json: { models: [{ id: "model-a", label: "Model A" }, { id: "model-b", label: "Model B" }] } }));
  await tab.route("**/api/accounts?*", (route) => route.fulfill({ json: { accounts: [{ name: "work", email: null, summary: null }] } }));
  await tab.route("**/api/snapshot", async (route) => {
    const snap = await (await route.fetch()).json();
    const session = process.env.SWARM_E2E_LIVE!;
    snap.sessions.find((s: { id: string }) => s.id === session).chair_provider = "codex";
    Object.assign(snap.agents[session][0], { provider: "codex", account: "work", model: "model-a" });
    await route.fulfill({ json: snap });
  });
  let reads = 0;
  await tab.route("**/api/usage", (route) => { reads++; return route.fulfill({ json: { meters: [] } }); });
  await open(tab);
  await tab.getByTestId("model-chip").click();
  await tab.getByTestId("run-query").fill("model-b");
  await tab.getByTestId("run-query").press("ArrowDown");
  await tab.keyboard.press("Enter");
  await expect(tab.getByTestId("run-models").locator('button[aria-pressed="true"]')).toContainText("Model B");
  await expect(tab.getByTestId("run-apply")).toBeEnabled();
  await expect(tab.getByRole("button", { name: "Keep current account (work)", exact: true })).toBeVisible();
  const first = reads;
  await tab.getByTestId("run-popover").getByRole("button", { name: "Cancel", exact: true }).click();
  await tab.getByTestId("model-chip").click();
  await expect.poll(() => reads).toBeGreaterThan(first);
  await tab.getByTestId("run-query").fill("custom-id");
  await tab.getByTestId("run-query").press("Enter");
  await expect(tab.getByTestId("run-custom-codex")).toHaveAttribute("aria-pressed", "true");
  await tab.screenshot({ path: info.outputPath("model-picker.png") });
});

test("stream loss shows Reconnecting and clears it after hello", async ({ page: tab }, info) => {
  await tab.route("**/api/stream", (route) => route.fulfill({ contentType: "text/event-stream", body: "" }));
  await open(tab);
  await expect(tab.getByTestId("reconnecting")).toBeVisible();
  await tab.screenshot({ path: info.outputPath("reconnecting.png") });
  await tab.unroute("**/api/stream");
  await expect(tab.getByTestId("reconnecting")).toHaveCount(0);
});

test("an arriving question keeps an active draft and cannot turn a typed digit into approval", async ({ page: tab }) => {
  const screen = join(process.env.SWARM_E2E_HOME!, "screen");
  const initial = readFileSync(screen, "utf8");
  const waiting = readFileSync(resolve(import.meta.dirname, "../../tests/fixtures/screens/claude-waiting.txt"), "utf8");
  await open(tab);
  const composer = tab.getByTestId("composer");
  await composer.fill("I am writing step ");
  let approvals = 0;
  tab.on("request", (r) => { if (r.url().includes("/api/chat/answer")) approvals++; });
  try {
    writeFileSync(screen, waiting);
    await expect(tab.getByTestId("prompt-card")).toBeVisible();
    await expect(composer).toBeFocused();
    await tab.keyboard.type("1");
    await expect(composer).toHaveValue("I am writing step 1");
    expect(approvals).toBe(0);
  } finally { writeFileSync(screen, initial); }
});

test("Continue keeps the prior provider, model and account and supports keyboard model selection", async ({ page: tab }) => {
  await tab.route("**/api/snapshot", async (route) => {
    const snap = await (await route.fetch()).json();
    Object.assign(snap.agents[process.env.SWARM_E2E_LIVE!][0], { provider: "codex", account: "work", model: "model-a", alive: false });
    await route.fulfill({ json: snap });
  });
  await tab.route("**/api/providers", (route) => route.fulfill({ json: { providers: [{ id: "codex", label: "Codex", accounts: true }] } }));
  await tab.route("**/api/models?*", (route) => route.fulfill({ json: { models: [{ id: "model-a", label: "Model A" }, { id: "model-b", label: "Model B" }] } }));
  await tab.route("**/api/accounts?*", (route) => route.fulfill({ json: { accounts: [{ name: "work", email: null }] } }));
  await open(tab);
  await tab.getByTestId("continue-chat").click();
  await expect(tab.getByTestId("sm-provider")).toHaveValue("codex");
  await expect(tab.getByTestId("sm-account")).toHaveValue("work");
  await expect(tab.getByTestId("sm-models").locator('[aria-pressed="true"]')).toHaveText("Model A");
  await tab.getByTestId("sm-query").fill("model-b");
  await tab.getByTestId("sm-query").press("ArrowDown");
  await tab.keyboard.press("Enter");
  await expect(tab.getByTestId("sm-models").locator('[aria-pressed="true"]')).toHaveText("Model B");
  await expect(tab.getByTestId("sm-start")).toBeEnabled();
});

test("modified Up keeps queued messages and slash Send returns to the newest message", async ({ page: tab }) => {
  await mockRows(tab, [...Array.from({ length: 60 }, (_, i) => row(`a${i}`, `message ${i} ` + "paragraph ".repeat(35))), row("q99", "queued text", "queued")]);
  let pulls = 0;
  await tab.route("**/api/chat/pull-back", (route) => { pulls++; return route.fulfill({ json: { outcome: "pulled", text: "queued text" } }); });
  await tab.route("**/api/chat/send", (route) => route.fulfill({ json: {} }));
  await open(tab);
  const composer = tab.getByTestId("composer");
  await composer.focus();
  await composer.press("Meta+ArrowUp");
  await composer.press("Shift+ArrowUp");
  expect(pulls).toBe(0);
  await expect(composer).toHaveValue("");
  await composer.press("ArrowUp");
  await expect.poll(() => pulls).toBe(1);
  await expect(composer).toHaveValue("queued text");
  await tab.getByTestId("transcript").hover();
  await tab.mouse.wheel(0, -2000);
  await expect(tab.getByTestId("jump-latest")).toBeVisible();
  await composer.fill("/cle");
  await expect(tab.getByTestId("slash-row").first()).toContainText("/clear");
  await composer.press("Enter");
  await expect(tab.getByTestId("jump-latest")).toHaveCount(0);
});

test("returning to a chat keeps its reading position", async ({ page: tab }) => {
  await mockRows(tab, Array.from({ length: 80 }, (_, i) => row(`a${i}`, `message ${i} ` + "paragraph ".repeat(35))));
  await open(tab);
  const transcript = tab.getByTestId("transcript");
  await transcript.evaluate((el) => { el.scrollTop = 1200; });
  await expect(tab.getByTestId("jump-latest")).toBeVisible();
  const offset = await transcript.evaluate((el) => el.scrollTop);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_GIT!}"]`).click();
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_LIVE!}"]`).click();
  await expect.poll(() => transcript.evaluate((el) => el.scrollTop)).toBeCloseTo(offset, -1);
});

test("Find reaches repeated matches in a loaded full patch in Unified and Split views", async ({ page: tab }) => {
  const r = row("d1", "", "diff");
  r.diff = { id: "patch-output", path: "app.ts", patch: "@@ -1 +1 @@\n-old\n+preview", added: 1, removed: 1, omitted_lines: 900, shortened_lines: 0, full_lines: 1001, full_bytes: 20000 };
  await mockRows(tab, [r]);
  const patch = ["@@ -1,1000 +1,1000 @@", ...Array.from({ length: 1000 }, (_, i) => ` ${i === 100 || i === 900 ? "repeated needle" : `line ${i}`}`)].join("\n");
  await tab.route("**/api/tool-output?*", (route) => route.fulfill({ json: { text: patch } }));
  await open(tab);
  await tab.getByRole("button", { name: "Show full patch", exact: true }).click();
  for (const mode of ["Unified", "Split"]) {
    await tab.getByRole("button", { name: mode, exact: true }).click();
    await tab.keyboard.press("Meta+f");
    await tab.getByTestId("find-input").fill("repeated needle");
    await expect(tab.getByTestId("find-count")).toHaveText("1 of 2");
    await expect(tab.locator('[data-output-line="901"]').first()).toBeVisible();
    await tab.getByTestId("find-input").press("Enter");
    await expect(tab.getByTestId("find-count")).toHaveText("2 of 2");
    await expect(tab.locator('[data-output-line="101"]').first()).toBeVisible();
    await tab.getByTestId("find-input").press("Escape");
  }
});
