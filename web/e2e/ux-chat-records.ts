import { expect, test } from "@playwright/test";
import { resolve } from "node:path";
import type { Prefs } from "../src/core/nav";
import { api, holdChatStart, mockRunCatalog, patchLiveChair, patchTreePrefs, readTree, shootThemes } from "./lifecycle-helpers";
import { row, open, mockRows } from "./ux-helpers";

test("a long moved-file header keeps its controls in view", async ({ page: tab }, info) => {
  const old = "src/view/transcript/normalize/" + "previous_location/".repeat(8) + "codex_changes.rs";
  const path = "src/view/transcript/normalize/" + "next_location/".repeat(8) + "codex_changes_old.rs";
  const moved = row("move", "move", "diff");
  moved.diff = { id: "move-diff", old_path: old, path, patch: "", full_lines: 0, full_bytes: 0, omitted_lines: 0, shortened_lines: 0, added: 0, removed: 0 };
  await mockRows(tab, [moved]);
  await open(tab);

  const diff = tab.getByTestId("diff");
  await diff.evaluate((el: HTMLElement) => { el.style.width = "480px"; });
  const name = diff.locator(".diff-head > span").first();
  await expect(name).toHaveAttribute("title", `${old} -> ${path}`);
  const bounds = await diff.evaluate((el) => {
    const head = el.querySelector(".diff-head")!;
    const name = head.querySelector("span")!;
    const right = head.getBoundingClientRect().right;
    return {
      clipped: name.scrollWidth > name.clientWidth && getComputedStyle(name).textOverflow === "ellipsis",
      controlsFit: [...head.querySelectorAll("button")].every((button) => button.getBoundingClientRect().right <= right),
    };
  });
  await tab.screenshot({ path: info.outputPath("moved-file-header.png") });
  expect(bounds).toEqual({ clipped: true, controlsFit: true });
});

test("an interrupted result without its call shows its state", async ({ page: tab }, info) => {
  const result = row("partial", "partial patch output", "toolResult");
  result.tool_status = "interrupted";
  await mockRows(tab, [result]);
  await open(tab);
  await expect(tab.getByTestId("output")).toContainText("Interrupted tool result");
  await tab.screenshot({ path: info.outputPath("interrupted-tool-result.png") });
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
    await setDefault(previous);
  }
});

test("a model preview stays readable without scrolling or changing its announcement", async ({ page: tab }) => {
  await patchLiveChair(tab, (chair) => Object.assign(chair, { provider: "claude", model: "model-a", state: "done", prompt: null }));
  await mockRunCatalog(tab, { providers: [{ id: "claude" }, { id: "codex" }], models: [{ id: "model-a", label: "Model A" }, { id: "model-b", label: "Model B" }] });
  await open(tab);
  await tab.getByTestId("model-chip").click();

  await tab.setViewportSize({ width: 320, height: 800 });
  const picker = tab.getByTestId("run-popover");
  const status = picker.getByRole("status");
  const before = (await status.textContent())!;
  await picker.getByRole("button", { name: "Codex", exact: true }).click();
  await picker.getByRole("button", { name: "Model B", exact: true }).hover();
  const outcome = picker.getByTestId("run-outcome");
  await expect(outcome).toContainText("Model B");
  await expect(outcome).toHaveCSS("overflow-y", "visible");
  await expect(status).toHaveText(before);
  await shootThemes(tab, resolve(import.meta.dirname, "../../tmp/shots/fix5/model-preview"));
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

test("the cold picker keeps its rows still and announces the catalog", async ({ page: tab }) => {
  await patchLiveChair(tab, (chair) => Object.assign(chair, { provider: "claude", model: "model-1" }));
  await patchTreePrefs(tab, (prefs) => { prefs.recentModels = [{ provider: "claude", model: "model-1" }]; });

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
    const recent = picker.getByTestId("run-recent");
    const controls = [recent.getByRole("button", { name: /favorites/i }), recent.getByText("now", { exact: true })];
    const positions = () => Promise.all(controls.map(async (item) => (await item.boundingBox())!.x));
    const before = await positions();

    release();
    await expect(picker.getByRole("button", { name: "Model 0", exact: true })).toBeVisible();
    expect(await positions()).toEqual(before);
    expect((await search.boundingBox())!.y).toBe(y);
    await expect(picker.getByTestId("run-model-status")).toHaveText("24 models listed.");
  } finally { release(); }
});
