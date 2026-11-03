import { expect, test, type Page } from "@playwright/test";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { TreeView } from "../src/core/nav";

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const token = () => new URLSearchParams(server().hash.slice(1)).get("token")!;
const row = (page: Page, session: string) => page.locator(`[data-testid="tree-row"][data-session="${session}"]`);
const composer = (page: Page) => page.locator(".strip-chat .chat-input").getByTestId("composer");
let saved: { path: string; body: string | null }[] = [];
test.beforeEach(() => {
  saved = ["choices.json", "view-state.json"].map((name) => {
    const path = join(process.env.SWARM_E2E_HOME!, ".swarm", name);
    return { path, body: existsSync(path) ? readFileSync(path, "utf8") : null };
  });
});
test.afterEach(async ({ page }, info) => {
  try { await page.screenshot({ path: info.outputPath("navigation.png") }); }
  finally {
    await page.close();
    for (const { path, body } of saved) {
      if (body === null) rmSync(path, { force: true });
      else writeFileSync(path, body);
    }
  }
});
async function load(page: Page, path = "/") {
  await page.goto(`${server().origin}${path}${server().hash}`);
  await page.waitForFunction(() => window.__swarmReady === true);
}
async function tree(): Promise<TreeView> {
  const response = await fetch(`${server().origin}/api/tree?view=workspaces`, { headers: { Authorization: `Bearer ${token()}` } });
  return response.json();
}
async function tabs(page: Page) {
  await page.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-tabs" }).click();
  await expect(page.getByTestId("tab")).toHaveCount(2);
  await expect(composer(page)).toBeFocused();
  return page.getByTestId("tab");
}

test("a folded workspace keeps its chats in Search and Home", async ({ page }) => {
  await load(page);
  const session = process.env.SWARM_E2E_LIVE!;
  const chat = (await tree()).chats!.find((item) => item.selection?.chat === session)!;
  const workspace = page.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-live" });
  await workspace.locator('[data-action="toggle"]').click();
  await expect(row(page, session)).toBeHidden();
  await page.keyboard.press("Meta+k");
  await page.getByPlaceholder("Search actions, workspaces, chats, agents, and prompts").fill(chat.title);
  await expect(page.locator(`[cmdk-item][data-value="chat:${session}"]`)).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByTestId("home-button").click();
  await expect(page.getByTestId("recent-chat").filter({ hasText: chat.title }).first()).toBeVisible();
});

test("Enter in Recent focuses the opened chat and reload keeps the Recent keyboard row", async ({ page }) => {
  await load(page);
  const session = process.env.SWARM_E2E_LIVE!;
  await row(page, session).click();
  await page.keyboard.press("Alt+Meta+2");
  const current = page.locator(`[data-testid="recent-row"][data-session="${session}"]`);
  await expect(current).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(composer(page)).toBeFocused();
  await page.reload();
  await page.waitForFunction(() => window.__swarmReady === true);
  await expect(current).toHaveAttribute("tabindex", "0");
  await expect(composer(page)).toBeFocused();
  await page.keyboard.type("keep going");
  await expect(composer(page)).toHaveValue("keep going");
});

test("switching to Recent takes the next keys before another animation frame", async ({ page }) => {
  await load(page);
  await row(page, process.env.SWARM_E2E_LIVE!).click();
  await expect(composer(page)).toBeFocused();
  const start = new Date("2030-01-01T00:00:00Z");
  await page.clock.install({ time: start });
  await page.clock.pauseAt(new Date(start.getTime() + 1_000));
  try {
    await page.keyboard.press("Alt+Meta+2");
    await expect(page.locator('[data-testid="recent-row"][aria-current="true"]')).toBeFocused();
    await page.keyboard.press("Home");
    await page.keyboard.press("j");
    await expect(page.getByTestId("recent-row").nth(1)).toHaveAttribute("aria-selected", "true");
  } finally { await page.clock.resume(); }
});

test("a workspace landing lists chats that its closed tabs can reopen", async ({ page }) => {
  await load(page);
  const list = await tabs(page);
  const titles = await list.locator(".label").allTextContents();
  await page.keyboard.press("Meta+w");
  await expect(list).toHaveCount(1);
  await page.keyboard.press("Meta+w");
  const landing = page.getByTestId("workspace-landing");
  await expect(landing).toBeVisible();
  for (const title of titles) await expect(landing.getByTestId("workspace-chat").filter({ hasText: title })).toBeVisible();
  await landing.getByTestId("workspace-chat").first().click();
  await expect(composer(page)).toBeFocused();
});

test("modifier clicks select rows and keep action bars hidden until the modifier is released", async ({ page }) => {
  await load(page);
  await page.clock.install();
  const session = process.env.SWARM_E2E_LIVE!;
  const chat = row(page, session);
  await chat.hover();
  // This gesture is faster than the action bar's hold delay.
  await page.keyboard.down("Meta");
  await chat.click({ position: { x: 30, y: 12 }, noWaitAfter: true });
  await expect(chat).toHaveAttribute("data-marked", "true");
  await expect(page.getByTestId("row-bar")).toBeHidden();
  await page.mouse.move(0, 0);
  await chat.hover();
  await page.clock.runFor(500);
  await expect(page.getByTestId("row-bar")).toBeHidden();
  await page.keyboard.up("Meta");
  await page.keyboard.down("Meta");
  await expect(page.getByTestId("row-bar")).toBeVisible();
  await page.keyboard.up("Meta");
});

test("tabs have a keyboard stop, arrows move focus, Enter opens, and middle click closes", async ({ page }) => {
  await load(page);
  const list = await tabs(page);
  const selected = page.locator('[data-testid="tab"][aria-selected="true"]');
  await expect(selected).toHaveAttribute("tabindex", "0");
  await selected.focus();
  const old = await selected.getAttribute("data-session");
  await page.keyboard.press("ArrowRight");
  const focused = page.locator('[data-testid="tab"]:focus');
  const other = await focused.getAttribute("data-session");
  expect(other).not.toBe(old);
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator(`[data-testid="tab"][data-session="${old}"]`)).toBeFocused();
  await page.keyboard.press("End");
  await expect(list.last()).toBeFocused();
  await page.keyboard.press("Home");
  await expect(list.first()).toBeFocused();
  await page.locator(`[data-testid="tab"][data-session="${old}"]`).focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(`[data-testid="tab"][data-session="${other}"]`)).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator(`[data-testid="tab"][data-session="${other}"]`)).toHaveAttribute("aria-selected", "true");
  await expect(composer(page)).toBeFocused();
  await page.locator(`[data-testid="tab"][data-session="${old}"]`).focus();
  await page.keyboard.press("Space");
  await expect(page.locator(`[data-testid="tab"][data-session="${old}"]`)).toHaveAttribute("aria-selected", "true");
  await expect(composer(page)).toBeFocused();
  await page.locator(`[data-testid="tab"][data-session="${old}"]`).click({ button: "middle" });
  await expect(list).toHaveCount(1);
});

test("Home orders Recent work by activity while the sidebar keeps its dragged order", async ({ page }) => {
  const data = await tree();
  // New chats go above the saved order, so seed every known chat before checking a drag order.
  const sessions = data.chats!.map((chat) => chat.selection!.chat!);
  await page.addInitScript((order) => localStorage.setItem("swarm.recentOrder", JSON.stringify(order)), sessions);
  await page.route("**/api/snapshot", async (route) => {
    const response = await route.fetch();
    const snapshot = await response.json();
    for (const item of snapshot.sessions) {
      if (item.id === sessions[1]) item.last_message_at = 9_000_000_000;
      else if (item.id === sessions[0]) item.last_message_at = 8_000_000_000;
    }
    await route.fulfill({ response, json: snapshot });
  });
  await load(page);
  await page.getByTestId("home-button").click();
  await expect(page.getByTestId("recent-chat").first()).toContainText(data.chats!.find((chat) => chat.selection?.chat === sessions[1])!.title);
  await page.keyboard.press("Alt+Meta+2");
  await expect(page.getByTestId("recent-row").first()).toHaveAttribute("data-session", sessions[0]);
});

