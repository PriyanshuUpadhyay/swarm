import "./ux-chat-records";
import "./ux-chat-images";
import "./ux-chat-diffs";
import { expect, test } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { row, rows, open, mockRows, mockStream, streamFrame, type StreamFixture } from "./ux-helpers";
import type { Snapshot } from "../src/core/api";


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
  await expect(tab.getByTestId("run-popover").getByText("Codex", { exact: true })).toBeVisible();
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

test("a malformed first hello does not subscribe the open chat", async ({ page: tab }) => {
  const subscriptions: string[] = [];
  await mockStream(tab, "accepted-hello", { build: "fixture" });
  await tab.route("**/api/subscribe", (route) => {
    subscriptions.push(route.request().postDataJSON().stream);
    return route.fulfill({ json: { ok: true } });
  });
  await mockRows(tab, [row("a1", "The chat is ready")]);
  await open(tab);
  await expect(tab.getByText("The chat is ready", { exact: true })).toBeVisible();

  await streamFrame(tab, "hello", { stream: "accepted-hello", build: "fixture" });
  await expect.poll(() => subscriptions, { timeout: 3000 }).toEqual(["accepted-hello"]);
});

test("stream loss keeps the last list and accepted update, then clears after hello", async ({ page: tab }) => {
  const session = process.env.SWARM_E2E_LIVE!;
  await tab.setViewportSize({ width: 960, height: 540 });

  await tab.route("**/api/snapshot", async (route) => {
    const snapshot = await (await route.fetch()).json() as Snapshot;
    snapshot.agents[session][0].state = "working";
    await route.fulfill({ json: snapshot });
  });
  await mockStream(tab, "offline-fixture");

  await tab.clock.setFixedTime(new Date("2030-01-01T14:02:00Z"));
  await open(tab);
  const status = tab.getByTestId("chat-status-line");
  await expect(status.locator('svg[data-form="working"]')).toBeVisible();

  const accepted = "2030-01-01T14:02:10.000Z";
  await tab.clock.setFixedTime(new Date(accepted));
  const read = tab.waitForResponse("**/api/snapshot");
  await streamFrame(tab, "snapshot", { version: 0 });
  await read;

  await tab.clock.setFixedTime(new Date("2030-01-01T14:02:30Z"));
  await tab.evaluate(() => {
    const { swarmStream } = window as unknown as { swarmStream: StreamFixture };
    swarmStream.send("unsupported", {});
    swarmStream.drop();
  });
  const banner = tab.getByTestId("reconnecting");
  const lastList = tab.locator(`[data-testid="tree-row"][data-session="${session}"]`);
  await expect(banner.locator("time")).toHaveAttribute("datetime", accepted, { timeout: 2000 });
  await expect(banner).toHaveCSS("height", "28px");
  await expect(banner.getByRole("button")).toHaveCount(0);
  await expect(tab.getByText("Offline. Showing the last list.", { exact: true })).toBeVisible();
  await expect(lastList).toBeVisible();
  await expect(status.locator('svg[data-form="unlit"]')).toBeVisible();
  if (process.env.DESIGN_B_SHOTS) for (const theme of ["dark", "light"] as const) {
    await tab.emulateMedia({ colorScheme: theme });
    await tab.screenshot({ path: resolve(process.env.DESIGN_B_SHOTS, `offline-${theme}.png`), animations: "disabled" });
  }

  await tab.evaluate(() => (window as unknown as { swarmStream: StreamFixture }).swarmStream.restore());
  await expect(banner).toHaveCount(0);
  await expect(status.locator('svg[data-form="working"]')).toBeVisible();
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

test("only an editable queued message gets the Up hint", async ({ page: tab }) => {
  const taken = { ...row("taken", "already taken"), system_kind: "queued_prompt" };
  await mockRows(tab, [taken, row("held", "still in the queue", "queued")]);
  await open(tab);
  const saved = tab.locator('[data-id="taken"]');
  await expect(saved).toContainText("already taken");
  await expect(saved).not.toContainText("to edit queued messages");
  await expect(tab.locator('[data-id="held"]')).toContainText("to edit queued messages");
});

test("modified Up keeps queued messages", async ({ page: tab }) => {
  await mockRows(tab, [...Array.from({ length: 60 }, (_, i) => row(`a${i}`, `message ${i} ` + "paragraph ".repeat(35))), row("q99", "queued text", "queued")]);
  let pulls = 0;
  await tab.route("**/api/chat/pull-back", (route) => { pulls++; return route.fulfill({ json: { outcome: "pulled", text: "queued text" } }); });
  await open(tab);
  // Up pulls back only when the chair can take it; wait for the queued message and the input hint.
  const queued = tab.getByTestId("row").filter({ hasText: "queued text" });
  await expect(queued.getByText("↑", { exact: true })).toBeVisible();
  await expect(tab.getByTestId("composer-status")).toHaveText("↑ to edit queued messages");
  if (process.env.DESIGN_B_SHOTS) for (const theme of ["dark", "light"]) {
    await tab.evaluate((theme) => { document.documentElement.dataset.theme = theme; }, theme);
    await queued.screenshot({ path: resolve(process.env.DESIGN_B_SHOTS, `queue-${theme}.png`), animations: "disabled" });
  }
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
