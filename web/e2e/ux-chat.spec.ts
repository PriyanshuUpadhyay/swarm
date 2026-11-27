import "./ux-chat-records";
import "./ux-chat-images";
import "./ux-chat-diffs";
import { expect, test } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { row, rows, open, mockRows } from "./ux-helpers";
import { api, captureRunPost, holdChatStart, mockRunCatalog, patchLiveChair, readTree, shootThemes } from "./lifecycle-helpers";
import type { Prefs } from "../src/core/nav";


test("Find steps through occurrences inside a long answer and paints the selected word", async ({ page: tab }, info) => {
  await mockRows(tab, [row("a1", `needle\n\n${"A paragraph with ordinary text.\n\n".repeat(70)}needle`)], 0, {}, { a1: [{ f: "text" }] });
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

test("a code block shows syntax colours, and Find matches across colour runs", async ({ page: tab }, info) => {
  await mockRows(tab, [row("a1", "```ts\nconst needle = 1;\n```")], 0, {}, { a1: [{ f: "text" }] });
  await open(tab);
  const code = tab.locator(".md-code code.language-ts");
  await expect(code).toHaveText("const needle = 1;\n");
  expect(await code.locator("span[style]").count()).toBeGreaterThan(1);
  await tab.keyboard.press("Meta+f");
  await tab.getByTestId("find-input").fill("const needle");
  await expect(tab.getByTestId("find-count")).toHaveText("1 of 1");
  await expect.poll(() => tab.evaluate(() => String(CSS.highlights.get("find-current")?.values().next().value))).toBe("const needle");
  await code.screenshot({ path: info.outputPath("code-colours.png") });
});

test("Find searches loaded full output and reveals its virtual line", async ({ page: tab }, info) => {
  const r = row("r1", "result", "toolResult");
  r.output = { id: "full-output", text: "preview", full_lines: 1000, hidden_lines: 999, full_bytes: 20000, is_truncated: true };
  await mockRows(tab, [r], 0, {}, { r1: [{ f: "output.text", p: "full-output" }] });
  await tab.route("**/api/tool-output?*", (route) => route.fulfill({ json: { text: Array.from({ length: 1000 }, (_, i) => i === 900 ? "unique needle" : `line ${i}`).join("\n") } }));
  await open(tab);
  await tab.getByTestId("show-full").click();
  await tab.keyboard.press("Meta+f");
  await tab.getByTestId("find-input").fill("unique needle");
  await expect(tab.getByTestId("find-count")).toHaveText("1 of 1");
  await expect(tab.getByTestId("full-output").getByText("unique needle", { exact: true })).toBeVisible();
  await tab.screenshot({ path: info.outputPath("find-full-output.png") });
});

test("empty Find offers earlier search", async ({ page: tab }, info) => {
  await tab.route("**/api/rows?*", async (route) => {
    if (!new URL(route.request().url()).searchParams.has("before")) return route.fulfill({ json: rows([row("a2", "Current message")], 100, {}, { a2: [{ f: "text" }] }) });
    return route.fulfill({ json: rows([row("a1", "old needle")], 0, {}, { a1: [{ f: "text" }] }) });
  });
  await open(tab);
  await tab.keyboard.press("Meta+f");
  await tab.getByTestId("find-input").fill("needle");
  await tab.getByRole("button", { name: "Search earlier messages" }).click();
  await expect(tab.getByTestId("find-count")).toHaveText("1 of 1");
  await tab.screenshot({ path: info.outputPath("history-search.png") });
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
  await mockRunCatalog(tab, {
    providers: [{ id: "codex", label: "Codex", accounts: true, efforts: ["high"], default_effort: "high" }],
    models: [{ id: "model-a", label: "Model A" }, { id: "model-b", label: "Model B" }],
    accounts: [{ name: "work", email: null, summary: null }],
  });
  await patchLiveChair(tab, (chair, snapshot, session) => {
    snapshot.sessions.find((s) => s.id === session)!.chair_provider = "codex";
    Object.assign(chair, { provider: "codex", account: "work", model: "model-a" });
  });
  const picks = await captureRunPost(tab, "continue");
  let reads = 0;
  await tab.route("**/api/usage", (route) => { reads++; return route.fulfill({ json: { meters: [] } }); });
  await open(tab);
  await tab.getByTestId("model-chip").click();
  await expect(tab.getByTestId("run-popover").getByRole("button", { name: "Codex", exact: true })).toBeVisible();
  await expect(tab.getByRole("button", { name: "Keep current account (work)", exact: true })).toBeVisible();
  await tab.screenshot({ path: info.outputPath("model-picker.png") });
  await tab.getByTestId("run-query").fill("model-b");
  await tab.getByTestId("run-query").press("ArrowDown");
  await tab.keyboard.press("Enter");
  await expect.poll(() => picks.map((p) => p.model)).toEqual(["model-b"]);
  const first = reads;
  await expect(tab.getByTestId("run-popover")).toBeHidden();
  await tab.getByTestId("model-chip").click();
  await expect.poll(() => reads).toBeGreaterThan(first);
  await tab.getByTestId("run-query").fill("custom-id");
  await tab.getByTestId("run-query").press("Enter");
  await expect.poll(() => picks.map((p) => p.model)).toEqual(["model-b", "custom-id"]);
});

test("a removed new-chat default blocks the start and can be reset", async ({ page: tab }) => {
  const setDefault = async (model: Prefs["newChatModel"] | null) => api("/api/choices", {
    revision: (await readTree()).revision, op: "pref", key: "newChatModel", value: model,
  });

  const previous = (await readTree()).prefs.newChatModel ?? null;
  const dropped = { provider: "codex", model: "dropped-model" };
  await setDefault(dropped);
  try {
    await mockRunCatalog(tab, { providers: [{ id: "codex" }], models: [{ id: "model-b", label: "Model B" }] });
    const starts = await holdChatStart(tab);
    await open(tab);
    await tab.getByTestId("model-chip").click();
    const picker = tab.getByTestId("run-popover");
    await picker.getByRole("button", { name: "Codex", exact: true }).click();
    await expect(picker.getByRole("button", { name: "Model B", exact: true })).toBeVisible();
    await tab.keyboard.press("Escape");

    await tab.keyboard.press("Meta+t");
    await expect(tab.getByText(/The default model for new chats is no longer listed/)).toBeVisible();
    expect(starts).toEqual([]);

    await tab.getByTestId("chip-run").click();
    const first = tab.getByTestId("run-models").getByRole("button", { name: "Model B", exact: true });
    await expect(first).toBeVisible();
    const top = (await first.boundingBox())!.y;
    await shootThemes(tab, resolve(import.meta.dirname, "../../tmp/shots/fix4/default-row-set"));

    const reset = tab.getByRole("button", { name: "Reset new chat default" });
    await reset.click();
    await expect(reset).toHaveCount(0);
    await expect.poll(async () => (await readTree()).prefs.newChatModel).toBeUndefined();
    await expect.poll(async () => (await first.boundingBox())?.y).toBe(top);
    await shootThemes(tab, resolve(import.meta.dirname, "../../tmp/shots/fix4/default-row-empty"));
  } finally {
    await tab.unrouteAll({ behavior: "ignoreErrors" });
    await setDefault(previous);
  }
});

test("a model preview changes the visible outcome but not its announcement", async ({ page: tab }) => {
  await patchLiveChair(tab, (chair) => Object.assign(chair, { provider: "claude", model: "model-a", state: "done", prompt: null }));
  await mockRunCatalog(tab, { providers: [{ id: "claude" }], models: [{ id: "model-a", label: "Model A" }, { id: "model-b", label: "Model B" }] });
  try {
    await open(tab);
    await tab.getByTestId("model-chip").click();
    const picker = tab.getByTestId("run-popover");
    const status = picker.getByRole("status");
    const before = (await status.textContent())!;
    await picker.getByRole("button", { name: "Model B", exact: true }).hover();
    await expect(picker.getByTestId("run-outcome")).toContainText("Model B");
    await expect(status).toHaveText(before);
  } finally { await tab.unrouteAll({ behavior: "ignoreErrors" }); }
});

test("a refused live switch restores the current highlight", async ({ page: tab }) => {
  await patchLiveChair(tab, (chair) => Object.assign(chair, { provider: "claude", model: "model-a", state: "done", prompt: null }));
  await mockRunCatalog(tab, { providers: [{ id: "claude" }, { id: "codex" }], models: (cli) => cli === "claude" ? [{ id: "model-a", label: "Model A" }] : [{ id: "model-b", label: "Model B" }] });
  await tab.route("**/api/chat/continue", (route) => route.fulfill({ status: 409, json: { error: "The switch was refused" } }));
  await open(tab);
  await tab.getByTestId("model-chip").click();
  const picker = tab.getByTestId("run-popover");
  await picker.getByRole("button", { name: "Codex", exact: true }).click();
  const picked = picker.getByRole("button", { name: "Model B", exact: true });
  await picked.click();
  await expect(picker.getByRole("alert")).toHaveText("The switch was refused");
  await expect(picked).toHaveAttribute("aria-pressed", "false");
});

test.describe("picker on a touch screen", () => {
  test.use({ hasTouch: true });

  test("favorite and default controls show before touch", async ({ page: tab }) => {
    await mockRunCatalog(tab, { providers: [{ id: "claude" }], models: [{ id: "model-a", label: "Model A" }] });
    await open(tab);
    await tab.getByTestId("model-chip").tap();
    const picker = tab.getByTestId("run-popover");
    const row = picker.locator('[data-model-id="model-a"]').locator("..");
    await expect(row.getByRole("button", { name: /favorite/i })).toHaveCSS("opacity", "1");
    await expect(row.getByRole("button", { name: "Use for new chats" })).toHaveCSS("opacity", "1");
    await expect(picker.getByTestId("run-query")).toHaveCSS("font-size", "16px");
  });
});

test("the cold picker keeps its search field still and announces the catalog", async ({ page: tab }) => {
  let release!: () => void;
  const ready = new Promise<void>((resolve) => { release = resolve; });
  await mockRunCatalog(tab, { providers: [{ id: "claude" }], models: [] });
  await tab.route("**/api/models?**", async (route) => {
    await ready;
    await route.fulfill({ json: { models: Array.from({ length: 24 }, (_, i) => ({ id: `model-${i}`, label: `Model ${i}` })) } });
  });
  await open(tab);
  await tab.getByTestId("model-chip").click();
  const picker = tab.getByTestId("run-popover");
  const search = picker.getByTestId("run-query");
  try {
    await expect(picker.getByTestId("run-models").getByText("Reading models…")).toBeVisible();
    const y = (await search.boundingBox())!.y;
    release();
    await expect(picker.getByRole("button", { name: "Model 0", exact: true })).toBeVisible();
    expect((await search.boundingBox())!.y).toBe(y);
    await expect(picker.getByTestId("run-model-status")).toHaveText("24 models listed.");
  } finally { release(); }
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

test("Ended Resume keeps the prior provider and account and uses the keyboard model pick", async ({ page: tab }) => {
  await patchLiveChair(tab, (chair) => { Object.assign(chair, { provider: "codex", account: "work", model: "model-a", alive: false }); });
  await mockRunCatalog(tab, {
    providers: [{ id: "codex", label: "Codex", accounts: true }],
    models: [{ id: "model-a", label: "Model A" }, { id: "model-b", label: "Model B" }],
    accounts: [{ name: "work", email: null }],
  });

  const resumes = await captureRunPost(tab, "resume");
  await open(tab);
  await tab.getByTestId("model-chip").click();
  const picker = tab.getByTestId("run-popover");
  await expect(picker.getByRole("button", { name: "Codex", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(picker.getByRole("button", { name: "work", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(picker.getByTestId("run-models").locator('[aria-pressed="true"]')).toContainText("Model A");
  await picker.getByTestId("run-query").fill("model-b");
  await picker.getByTestId("run-query").press("ArrowDown");
  await tab.keyboard.press("Enter");
  await expect(picker.getByTestId("run-models").locator('[aria-pressed="true"]')).toContainText("Model B");
  expect(resumes).toEqual([]);
  await tab.keyboard.press("Escape");
  await tab.getByTestId("resume-chat").click();
  await expect.poll(() => resumes).toEqual([{ session: process.env.SWARM_E2E_LIVE, model: "model-b", account: "work" }]);
  await tab.unrouteAll({ behavior: "ignoreErrors" });
});

test("modified Up keeps queued messages", async ({ page: tab }) => {
  await mockRows(tab, [...Array.from({ length: 60 }, (_, i) => row(`a${i}`, `message ${i} ` + "paragraph ".repeat(35))), row("q99", "queued text", "queued")]);
  let pulls = 0;
  await tab.route("**/api/chat/pull-back", (route) => { pulls++; return route.fulfill({ json: { outcome: "pulled", text: "queued text" } }); });
  await open(tab);
  // Up pulls back only when the chair can take it; the hint shows that state, so wait for it before the first key.
  await expect(tab.getByText("↑ to edit queued messages")).toBeVisible();
  const composer = tab.getByTestId("composer");
  await composer.focus();
  await composer.press("Meta+ArrowUp");
  await composer.press("Shift+ArrowUp");
  expect(pulls).toBe(0);
  await expect(composer).toHaveValue("");
  await composer.press("ArrowUp");
  await expect.poll(() => pulls).toBe(1);
  await expect(composer).toHaveValue("queued text");
});

test("returning to a chat keeps its reading position", async ({ page: tab }) => {
  await mockRows(tab, Array.from({ length: 80 }, (_, i) => row(`a${i}`, `message ${i} ` + "paragraph ".repeat(35))));
  await open(tab);
  const transcript = tab.getByTestId("transcript");
  // Scroll as a user does, after the last row shows: a scrollTop set during the first layout can be undone by the stick-to-bottom pass.
  await expect(tab.getByText("message 79", { exact: false })).toBeVisible();
  await transcript.hover();
  await tab.mouse.wheel(0, -2000);
  await expect(tab.getByTestId("jump-latest")).toBeVisible();
  const offset = await transcript.evaluate((el) => el.scrollTop);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_GIT!}"]`).click();
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_LIVE!}"]`).click();
  await expect.poll(() => transcript.evaluate((el) => el.scrollTop)).toBeCloseTo(offset, -1);
});

test("a table column keeps its longest word and a wide table scrolls inside its frame", async ({ page: tab }) => {
  const table = "| CLI | Name form | Note |\n| --- | --- | --- |\n| Claude | `swarm:plan` | " + "plain words that wrap ".repeat(40) + " |";
  await mockRows(tab, [row("a1", table)]);
  await open(tab);
  const heads = tab.locator(".md-table th");
  await expect(heads).toHaveCount(3);
  for (const [i, word] of ["CLI", "Name", "Note"].entries()) {
    const wide = await heads.nth(i).evaluate((el, w) => {
      const probe = document.createElement("span");
      probe.style.cssText = "position:absolute;visibility:hidden;white-space:nowrap";
      probe.textContent = w;
      el.append(probe);
      const need = probe.getBoundingClientRect().width;
      probe.remove();
      return el.getBoundingClientRect().width >= need;
    }, word);
    expect(wide, `${word} column is as wide as its longest word`).toBe(true);
  }
  expect(await tab.locator(".md-table td").first().evaluate((el) => getComputedStyle(el).overflowWrap)).toBe("normal");
});
