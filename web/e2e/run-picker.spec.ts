import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { holdChatStart, open, readTree } from "./lifecycle-helpers";

// Live snapshot reads can still be in flight when the test page closes.
test.afterEach(({ page }) => page.unrouteAll({ behavior: "ignoreErrors" }));

async function shots(tab: Page, unit: number | "fix1" | "fix2", name: string) {
  if (!process.env.E2E_SHOTS) return;
  const dir = resolve(import.meta.dirname, `../../tmp/shots/${unit}`);
  mkdirSync(dir, { recursive: true });
  for (const theme of ["dark", "light"]) {
    await tab.evaluate((value) => { document.documentElement.dataset.theme = value; }, theme);
    await tab.screenshot({ path: join(dir, `${name}-${theme}.png`) });
  }
  await tab.evaluate(() => { delete document.documentElement.dataset.theme; });
}

test("the same tab pages both CLI logs and reads tool output from its original session", async ({ page: tab }) => {
  const errors = await open(tab);
  const root = process.env.SWARM_E2E_GIANT!;
  const rootRow = tab.getByTestId("tree-row").and(tab.locator(`[data-session="${root}"]`));
  await rootRow.click();
  const priorTab = tab.getByTestId("tab").and(tab.locator(`[data-session="${root}"]`));
  await expect(priorTab).toBeVisible();
  const tabId = await priorTab.getAttribute("data-id");
  const rowId = await rootRow.getAttribute("data-id");

  const env = { PATH: process.env.PATH, HOME: process.env.SWARM_E2E_HOME, SWARM_HOME: process.env.SWARM_E2E_HOME, SWARM_ADAPTER: "tmux-swarm" };
  const swarm = process.env.SWARM_E2E_SWARM!;
  const dir = process.env.SWARM_E2E_DIR!;
  const call = (...args: string[]) => execFileSync(swarm, args, { cwd: join(dir, "ws-giant"), env, encoding: "utf8" }).trim().split("\n").at(-1)!;
  const leaf = call("session", "new", "lane", "--chair", `codex:${randomUUID()}`);
  const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
  const sql = (statement: string) => execFileSync("sqlite3", ["-cmd", ".timeout 5000", process.env.SWARM_E2E_DB!, statement]);
  const log = join(dir, `${leaf}.jsonl`);
  try {
    const line = { type: "response_item", payload: { type: "message", role: "user", content: [{ type: "input_text", text: "After the CLI switch" }] } };
    writeFileSync(log, `${JSON.stringify(line)}\n`);
    sql(`UPDATE session SET chair_log = ${quote(log)} WHERE id = ${quote(leaf)};
          INSERT INTO agent (id, session_id, role, provider, created_at, log) VALUES ('orchestrator', ${quote(leaf)}, 'orchestrator', 'codex', unixepoch(), ${quote(log)});`);
    call("session", "continue", leaf, root);
    await expect.poll(async () => (await readTree()).sections.flatMap((s) => s.rows).some((row) => row.selection?.chat === leaf)).toBe(true);
    await tab.reload();
    const row = tab.getByTestId("tree-row").and(tab.locator(`[data-session="${leaf}"]`));
    await expect(row).toHaveAttribute("data-id", rowId!);
    await row.click();
    await expect(tab.getByTestId("tab").and(tab.locator(`[data-session="${leaf}"]`))).toHaveAttribute("data-id", tabId!);
    const transcript = tab.getByTestId("transcript").and(tab.locator(`[data-chat="${leaf}"]`));
    await expect(transcript.getByText("After the CLI switch")).toBeVisible();

    const loadOlder = tab.getByTestId("load-older");
    await expect(loadOlder).toBeVisible();
    await loadOlder.click();
    await expect(transcript.getByText("Show the build log")).toBeVisible();
    await expect(transcript.getByTestId("model-change")).toHaveText("Claude → Codex");
    await shots(tab, 5, "linked-transcript");
    await transcript.getByTestId("fold").click();
    const card = transcript.getByTestId("tool-card").filter({ hasText: "cat big.log" });
    await card.getByRole("button", { name: /cat big\.log/ }).click();
    await card.getByTestId("show-full").click();
    await expect(card.getByTestId("full-output")).toContainText("line 0 of the build log");
  } finally {
    sql(`DELETE FROM agent WHERE session_id = ${quote(leaf)}; DELETE FROM session WHERE id = ${quote(leaf)};`);
    rmSync(log, { force: true });
    await expect.poll(async () => (await readTree()).sections.flatMap((s) => s.rows).some((row) => row.selection?.chat === root)).toBe(true);
  }
  expect(errors).toEqual([]);
});

async function pickerCatalog(tab: Page) {
  const providers = ["claude", "codex", "agy", "pi"].map((id) => ({ id, accounts: id === "claude" || id === "codex", efforts: ["low", "high"], default_effort: "high" }));
  await tab.route("**/api/providers", (route) => route.fulfill({ json: { providers } }));
  await tab.route("**/api/models?*", (route) => {
    const cli = new URL(route.request().url()).searchParams.get("provider");
    const models = cli === "claude" ? [{ id: "model-a", label: "Model A" }, { id: "model-b", label: "Model B" }] : [{ id: "other", label: "Other model" }];
    return route.fulfill({ json: { models } });
  });
  await tab.route("**/api/accounts?*", (route) => route.fulfill({ json: { accounts: [] } }));
}

test("one idle picker opens by shortcut, keeps an unlisted current model and applies picks at once", async ({ page: tab }) => {
  await pickerCatalog(tab);
  const live = process.env.SWARM_E2E_LIVE!;
  const picked: Record<string, unknown>[] = [];
  await tab.route("**/api/chat/continue", (route) => {
    picked.push(route.request().postDataJSON());
    return route.fulfill({ json: { session: live } });
  });
  await tab.route("**/api/snapshot", async (route) => {
    const snap = await (await route.fetch()).json();
    Object.assign(snap.agents[live][0], { provider: "claude", model: picked.at(-1)?.model ?? "unlisted-model", effort: "high", state: "done", prompt: null });
    await route.fulfill({ json: snap });
  });
  await tab.route("**/api/tree?*", async (route) => {
    const tree = await (await route.fetch()).json();
    tree.prefs = { ...tree.prefs, favoriteModels: [{ provider: "claude", model: "model-b" }], recentModels: [{ provider: "codex", model: "other" }], newChatModel: { provider: "codex", model: "other" } };
    await route.fulfill({ json: tree });
  });
  await open(tab);
  await tab.getByTestId("tree-row").and(tab.locator(`[data-session="${live}"]`)).click();
  await tab.keyboard.press("Shift+Meta+m");
  const picker = tab.getByTestId("run-popover");
  await expect(picker).toBeVisible();
  await expect(picker.getByTestId("run-unavailable")).toContainText("unlisted-model");
  await expect(picker.getByTestId("run-recent")).toContainText("Other model");
  await expect(picker.getByTestId("run-apply")).toHaveCount(0);
  await shots(tab, 6, "picker");
  await picker.getByRole("button", { name: "Model A", exact: true }).click();
  await expect.poll(() => picked.map((run) => run.model)).toEqual(["model-a"]);
  await expect(tab.getByTestId("model-chip-label")).toContainText("model-a");
  await tab.keyboard.press("Control+Meta+m");
  await expect.poll(() => picked.map((run) => run.model)).toEqual(["model-a", "model-b"]);
  await expect(tab.getByTestId("run-favorite-toast")).toHaveText("Model B");
  await expect(picker).toBeHidden();

  const starts = await holdChatStart(tab);
  await tab.keyboard.press("Meta+t");
  await expect.poll(() => starts.map((start) => [start.provider, start.model])).toEqual([["codex", "other"]]);
});

test("a typed model stays available in the new-chat picker", async ({ page: tab }) => {
  await pickerCatalog(tab);
  await open(tab);
  await tab.getByTestId("tree-row").and(tab.locator(`[data-session="${process.env.SWARM_E2E_LIVE}"]`)).click();
  await tab.getByTestId("tab-new-chat").click();
  await tab.getByTestId("chip-run").click();
  const query = tab.getByTestId("run-query");
  await expect(tab.getByRole("button", { name: "Model A", exact: true })).toBeVisible();
  await query.fill("custom-model");
  await tab.getByRole("button", { name: "Use “custom-model”" }).click();
  await query.clear();
  const row = tab.getByTestId("run-models").getByRole("button", { name: "custom-model", exact: true });
  await expect(row).toBeEnabled();
  await expect(row).toHaveAttribute("aria-pressed", "true");
});

test("the new-chat picker gives a catalog error space inside its footer", async ({ page: tab }) => {
  await tab.route("**/api/providers", (route) => route.fulfill({ status: 503, json: { error: "catalog offline" } }));
  await open(tab);
  await tab.getByTestId("tree-row").and(tab.locator(`[data-session="${process.env.SWARM_E2E_LIVE}"]`)).click();
  await tab.getByTestId("tab-new-chat").click();
  await tab.getByTestId("chip-run").click();
  const overlay = tab.getByTestId("composer-overlay");
  const error = overlay.getByRole("alert").filter({ hasText: "Could not read the CLIs" });
  await expect(error).toBeVisible();
  const rail = overlay.getByRole("group", { name: "Model groups" });
  expect((await error.boundingBox())!.x - (await rail.boundingBox())!.x).toBeGreaterThanOrEqual(8);
});

test("a busy pick waits for a fresh Switch now after Stop", async ({ page: tab }) => {
  await pickerCatalog(tab);
  const live = process.env.SWARM_E2E_LIVE!;
  let working = true;
  let stops = 0;
  const switches: Record<string, unknown>[] = [];
  await tab.route("**/api/snapshot", async (route) => {
    const snap = await (await route.fetch()).json();
    Object.assign(snap.agents[live][0], { provider: "claude", model: "model-a", state: working ? "working" : "done", prompt: null });
    await route.fulfill({ json: snap });
  });
  await tab.route("**/api/chat/interrupt", async (route) => {
    stops++;
    working = false;
    await route.continue();
  });
  await tab.route("**/api/chat/continue", (route) => {
    switches.push(route.request().postDataJSON());
    return route.fulfill({ json: { session: live } });
  });
  await open(tab);
  await tab.getByTestId("tree-row").and(tab.locator(`[data-session="${live}"]`)).click();
  await tab.getByTestId("model-chip").click();
  const picker = tab.getByTestId("run-popover");
  await picker.getByRole("button", { name: "Model B", exact: true }).click();
  await expect(picker.getByRole("status")).toContainText("model-b");
  const stop = picker.getByRole("button", { name: "Stop", exact: true });
  await expect(stop).toBeVisible();
  await expect(tab.getByTestId("composer-status").getByRole("button", { name: "Stop", exact: true })).toBeVisible();
  await shots(tab, 7, "busy-picker");
  await stop.click();
  await expect.poll(() => stops).toBe(1);
  const banner = tab.getByTestId("switch-pending");
  await expect(banner).toBeVisible();
  await expect(banner.getByText("Switch to model-b?")).toHaveCSS("font-size", "12px");
  expect(switches).toEqual([]);
  await shots(tab, "fix1", "switch-confirmation");
  await tab.getByRole("button", { name: "Switch now" }).click();
  await expect.poll(() => switches).toEqual([expect.objectContaining({ old_session: live, model: "model-b", carry: "resume" })]);
});

test("reload drops a busy pick rather than applying it on the now-idle chat", async ({ page: tab }) => {
  await pickerCatalog(tab);
  const live = process.env.SWARM_E2E_LIVE!;
  let working = true;
  let switches = 0;
  await tab.route("**/api/snapshot", async (route) => {
    const snap = await (await route.fetch()).json();
    Object.assign(snap.agents[live][0], { provider: "claude", model: "model-a", state: working ? "working" : "done", prompt: null });
    await route.fulfill({ json: snap });
  });
  await tab.route("**/api/chat/continue", (route) => {
    switches++;
    return route.fulfill({ json: { session: live } });
  });
  await open(tab);
  const row = tab.getByTestId("tree-row").and(tab.locator(`[data-session="${live}"]`));
  await row.click();
  await tab.getByTestId("model-chip").click();
  const picker = tab.getByTestId("run-popover");
  await picker.getByRole("button", { name: "Model B", exact: true }).click();
  await expect(picker.getByTestId("run-outcome")).toContainText("does not apply until you confirm");
  working = false;
  await tab.reload();
  await row.click();
  await expect(tab.getByTestId("model-chip-label")).toContainText("model-a");
  await expect(tab.getByTestId("switch-pending")).toHaveCount(0);
  expect(switches).toBe(0);
});

test("an ended chat keeps a foreign-CLI pick until Resume and uses File by default", async ({ page: tab }) => {
  await pickerCatalog(tab);
  const live = process.env.SWARM_E2E_LIVE!;
  const attempts: Record<string, unknown>[] = [];
  let release: () => void;
  const finishing = new Promise<void>((resolve) => { release = resolve; });
  await tab.route("**/api/snapshot", async (route) => {
    const snap = await (await route.fetch()).json();
    Object.assign(snap.agents[live][0], { provider: "claude", model: "model-a", state: "done", alive: false });
    await route.fulfill({ json: snap });
  });
  await tab.route("**/api/tree?*", async (route) => {
    const tree = await (await route.fetch()).json();
    delete tree.prefs.switchCarry;
    await route.fulfill({ json: tree });
  });
  await tab.route("**/api/chat/continue", async (route) => {
    attempts.push(route.request().postDataJSON());
    await finishing;
    await route.fulfill({ json: { session: live } });
  });
  await open(tab);
  await tab.getByTestId("tree-row").and(tab.locator(`[data-session="${live}"]`)).click();
  const caption = tab.getByText("Resume goes on in this chat", { exact: true });
  await expect(caption).toBeVisible();
  await expect(caption).toHaveCSS("font-size", "12px");
  const closed = await tab.getByTestId("resume-chat").boundingBox();
  const captionBox = await caption.boundingBox();
  expect(captionBox!.y - (closed!.y + closed!.height)).toBe(8);
  await shots(tab, "fix2", "ended-caption");
  await tab.getByTestId("model-chip").click();
  const picker = tab.getByTestId("run-popover");
  await expect(caption).toBeHidden();
  await picker.getByRole("button", { name: "Codex", exact: true }).click();
  await picker.getByRole("button", { name: "Other model", exact: true }).click();
  await expect(tab.getByTestId("model-chip-label")).toContainText("other");
  expect(attempts).toEqual([]);
  await shots(tab, 7, "ended-pick");
  const resume = tab.getByTestId("resume-chat");
  try {
    await resume.click();
    await expect(resume).toHaveText("Resuming…");
    await expect(resume).toBeDisabled();
    await expect.poll(() => attempts).toEqual([expect.objectContaining({ old_session: live, provider: "codex", model: "other", carry: "file" })]);
  } finally { release!(); }
});
