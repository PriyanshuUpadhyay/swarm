import { expect, test } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { row, rows, open, mockRows } from "./ux-helpers";
import fencePainter from "../../tests/fixtures/s6c/painter.json" with { type: "json" };
import type { Display, PatchInfo } from "../src/core/api";
import previewPatch from "./fixtures/s6a-preview.json" with { type: "json" };
import fullLines0 from "./fixtures/s6a-full-lines-0.json" with { type: "json" };
import fullLines1 from "./fixtures/s6a-full-lines-1.json" with { type: "json" };
import fullLines2 from "./fixtures/s6a-full-lines-2.json" with { type: "json" };
import fullSplit0 from "./fixtures/s6a-full-split-0.json" with { type: "json" };


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

test("Find reaches repeated matches in a loaded full patch in Unified and Split views", async ({ page: tab }) => {
  const r = row("d1", "", "diff");
  r.diff = { id: "patch-output", path: "app.ts", patch: "@@ -1 +1 @@\n-old\n+preview", added: 1, removed: 1, omitted_lines: 900, shortened_lines: 0, full_lines: 1001, full_bytes: 20000 };
  await mockRows(tab, [r], 0, previewPatch as Record<string, PatchInfo>, { d1: [{ f: "diff.patch", p: "patch-output" }] });
  const patch = ["@@ -1,1000 +1,1000 @@", ...Array.from({ length: 1000 }, (_, i) => ` ${i === 100 || i === 900 ? "repeated needle" : `line ${i}`}`)].join("\n");
  await tab.route("**/api/tool-output?*", (route) => route.fulfill({ json: { text: patch, patch: { lines: fullLines0.concat(fullLines1, fullLines2), split: fullSplit0 } } }));
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

// S6C-FENCES-V1 T2. Written only; no native clipboard and no browser run.
test("a fence and a tool share wire rows and layout while Copy keeps full text", async ({ page: tab }) => {
  await tab.addInitScript(() => {
    localStorage.setItem("swarm.diff.split", "0");
    const captured: string[] = [];
    Object.defineProperty(window, "s6cCopies", { value: captured });
    Object.defineProperty(navigator, "clipboard", { value: { writeText: async (text: string) => { captured.push(text); } } });
  });
  const fence = row("fence1", "raw Markdown deliberately differs from display");
  const tool = row("tool2", "", "diff");
  tool.diff = { id: "fence-tool", path: "x", patch: "@@ -1 +1 @@\n-tool-raw\n+tool-raw\n", added: 1, removed: 1, omitted_lines: 0, shortened_lines: 0, full_lines: 5, full_bytes: 48 };
  const page = rows([fence, tool], 0, { "fence-tool": fencePainter.patch as PatchInfo });
  page.display![fence.id] = fencePainter.display as Display;
  await tab.route("**/api/rows?*", route => route.fulfill({ json: page }));
  await open(tab);
  const hunk = tab.locator('[data-id="fence1"] .md-code').nth(0);
  const prefix = tab.locator('[data-id="fence1"] .md-code').nth(1);
  const card = tab.getByTestId("diff");
  for (const box of [hunk, card]) {
    await expect(box.locator(".patch")).toBeVisible();
    await expect(box.locator(".pl.del .code")).toHaveText("<script>x</script>");
    await expect(box.locator(".pl.add .code")).toHaveText("<b>&new</b>");
    await expect(box.locator(".pl.del .ln")).toHaveText(["1", ""]);
    await expect(box.locator(".pl.add .ln")).toHaveText(["", "1"]);
    await expect(box.locator("script, b")).toHaveCount(0);
  }
  await hunk.locator('[data-diff-layout="1"]').click();
  await expect(card.getByRole("button", { name: "Split", exact: true })).toHaveAttribute("aria-pressed", "true");
  for (const box of [hunk, card]) {
    await expect(box.locator(".pl.split")).toHaveCount(1);
    await expect(box.locator(".half.del .ln")).toHaveText("1");
    await expect(box.locator(".half.add .ln")).toHaveText("1");
    await expect(box.locator(".half.del .code")).toHaveText("<script>x</script>");
    await expect(box.locator(".half.add .code")).toHaveText("<b>&new</b>");
  }
  await card.getByRole("button", { name: "Unified", exact: true }).click();
  for (const box of [hunk, card]) {
    await expect(box.locator(".pl.split")).toHaveCount(0);
    await expect(box.locator(".pl.del .code")).toHaveText("<script>x</script>");
  }
  await expect(prefix.locator("[data-diff-layout]")).toHaveCount(0);
  await expect(prefix.locator("pre")).toHaveText(fencePainter.prefixCopy);
  await hunk.getByRole("button", { name: "Copy code", exact: true }).click();
  await prefix.getByRole("button", { name: "Copy code", exact: true }).click();
  expect(await tab.evaluate(() => (window as unknown as { s6cCopies: string[] }).s6cCopies))
    .toEqual([fencePainter.raw, fencePainter.prefixCopy]);
});
