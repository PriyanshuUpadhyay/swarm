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

test("body keys do not act on hidden rows and Recent left and right do not fold workspaces", async ({ page }) => {
  await load(page);
  await page.getByTestId("home-button").click();
  const before = await tree();
  await page.evaluate(() => { (document.activeElement as HTMLElement)?.blur(); document.body.focus(); });
  for (const key of ["ArrowDown", "ArrowDown", "ArrowLeft", "Enter"]) await page.keyboard.press(key);
  await expect(page.getByTestId("home")).toBeVisible();
  expect((await tree()).revision).toBe(before.revision);
  await page.keyboard.press("Alt+Meta+2");
  const recent = await tree();
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowRight");
  expect((await tree()).revision).toBe(recent.revision);
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

test("Search shows shortcut help and blocks unavailable Find while New Chat explains Home", async ({ page }) => {
  await load(page);
  await page.getByTestId("home-button").click();
  await page.keyboard.press("Meta+t");
  await expect(page.getByText("Pick a workspace to start a chat.", { exact: true })).toBeVisible();
  await page.keyboard.press("Meta+k");
  await page.getByTestId("keyboard-shortcuts").locator("summary").click();
  await expect(page.getByTestId("keyboard-shortcuts")).toContainText("Reopen Closed Tab");
  await page.getByPlaceholder("Search actions, workspaces, chats, agents, and prompts").fill("Find");
  await expect(page.locator('[cmdk-item][data-value="action:find.open"]')).toHaveAttribute("aria-disabled", "true");
  await expect(page.getByTestId("palette")).toBeVisible();
});

test("archived workspaces filter leaves live Search and the new-chat control available", async ({ page }) => {
  await load(page);
  await tabs(page);
  await page.getByTestId("archive-toggle").click();
  await expect(page.getByRole("button", { name: "New chat in this workspace", exact: true })).toBeEnabled();
  await page.keyboard.press("Meta+k");
  await page.getByPlaceholder("Search actions, workspaces, chats, agents, and prompts").fill("ws-live");
  await expect(page.locator('[cmdk-item][data-value^="workspace:"]').filter({ hasText: "ws-live" })).toBeVisible();
});

test("tree row buttons leave one tree Tab stop and Enter on a focused button keeps its action", async ({ page }) => {
  await load(page);
  const sidebar = page.getByTestId("sidebar");
  expect(await sidebar.locator("button").evaluateAll((buttons) => buttons.every((button) => button.tabIndex === -1))).toBe(true);
  const workspace = page.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-live" });
  await workspace.getByRole("button", { name: "New chat in ws-live", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("new-chat-menu")).toBeVisible();
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
  await page.keyboard.press("Enter");
  await expect(page.locator(`[data-testid="tab"][data-session="${other}"]`)).toHaveAttribute("aria-selected", "true");
  await expect(composer(page)).toBeFocused();
  await page.locator(`[data-testid="tab"][data-session="${old}"]`).click({ button: "middle" });
  await expect(list).toHaveCount(1);
});

test("saving the same rename leaves the auto title unset and a bad chat link shows a notice", async ({ page }) => {
  await load(page);
  const session = process.env.SWARM_E2E_LIVE!;
  let renames = 0;
  page.on("request", (request) => {
    if (request.url().endsWith("/api/choices") && request.method() === "POST" && request.postDataJSON().op === "rename_chat") renames++;
  });
  await row(page, session).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Rename chat…", exact: true }).click();
  await page.getByTestId("ask-save").click();
  await expect(page.getByTestId("ask-name")).toHaveCount(0);
  expect(renames).toBe(0);
  await load(page, "/chat/missing-chat");
  await expect(page.getByText("This chat link is unavailable. The chat may be archived or removed.", { exact: true })).toBeVisible();
});

test("Home orders Recent work by activity while the sidebar keeps its dragged order", async ({ page }) => {
  const data = await tree();
  const sessions = data.chats!.slice(0, 2).map((chat) => chat.selection!.chat!);
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

test("the tab strip shows a drag target and a mouse wheel scrolls long tabs", async ({ page }) => {
  await page.route("**/api/tabs?*", async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    if (body.view.tabs.length === 2) {
      const template = body.view.tabs[0];
      body.view.tabs.push(...Array.from({ length: 10 }, (_, i) => ({ ...template, id: `overflow-${i}`, session: `overflow-${i}`, title: `Long overflow chat ${i}` })));
    }
    await route.fulfill({ response, json: body });
  });
  await load(page);
  await page.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-tabs" }).click();
  const list = page.getByTestId("tabs");
  await expect(list.getByTestId("tab")).toHaveCount(12);
  await list.evaluate((element) => { element.scrollLeft = 0; });
  await list.hover();
  await page.mouse.wheel(0, 180);
  await expect.poll(() => list.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);
  await list.evaluate((element) => { element.scrollLeft = 0; });
  const transfer = await page.evaluateHandle(() => new DataTransfer());
  await list.getByTestId("tab").nth(0).dispatchEvent("dragstart", { dataTransfer: transfer });
  await list.getByTestId("tab").nth(1).dispatchEvent("dragover", { dataTransfer: transfer });
  const target = list.getByTestId("tab").nth(1);
  await expect(target).toHaveAttribute("data-drop-target", "true");
  expect(await target.evaluate((element) => getComputedStyle(element).boxShadow)).not.toBe("none");
  await list.getByTestId("tab").nth(0).dispatchEvent("dragend", { dataTransfer: transfer });
  await expect(target).not.toHaveAttribute("data-drop-target", "true");
});
