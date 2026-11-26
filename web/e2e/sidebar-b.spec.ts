import { expect, test, type Page } from "@playwright/test";
import { resolve } from "node:path";
import type { SidebarRow, TreeView } from "../src/core/nav";

async function load(page: Page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await page.goto(`${url.origin}/${url.hash}`);
  await page.waitForFunction(() => window.__swarmReady === true);
}

test("head B puts Home and project controls above New chat and palette search", async ({ page }) => {
  await load(page);
  const head = page.getByTestId("sidebar-head");
  const home = head.getByRole("button", { name: "Home" });
  const add = head.getByRole("button", { name: "Add project" });
  const settings = head.getByRole("button", { name: "Settings" });
  const chat = head.getByRole("button", { name: "New chat", exact: true });
  const search = head.getByRole("button", { name: "Command palette" });
  await expect(home).toHaveText("Swarm");
  const boxes = await Promise.all([home, add, settings, chat, search].map((control) => control.boundingBox()));
  const [homeBox, addBox, settingsBox, chatBox, searchBox] = boxes.map((box) => box!);
  expect(addBox.height).toBe(24);
  expect(settingsBox.height).toBe(24);
  expect(homeBox.y + homeBox.height).toBeLessThan(chatBox.y);
  expect(chatBox.y + chatBox.height).toBeLessThan(searchBox.y);
  expect(searchBox.height).toBe(32);

  for (const colorScheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme });
    await page.screenshot({ path: resolve(import.meta.dirname, `../../tmp/shots/U2/head-${colorScheme}.png`) });
  }

  await search.click();
  await page.getByRole("combobox").fill("Skills");
  await page.getByRole("option", { name: "Skills…" }).click();
  await expect(page.getByTestId("skills")).toBeVisible();
});

test("the counted cut keeps catalog chats in Search and opens by keyboard", async ({ page }) => {
  const session = process.env.SWARM_E2E_GIT!;
  let expanded = false;
  let listId = "";
  await page.route("**/api/tree?view=workspaces", async (route) => {
    const response = await route.fetch();
    const tree: TreeView = await response.json();
    const hidden = tree.chats.find((row) => row.selection?.chat === session)!;
    const section = tree.sections.find((s) => s.rows.some((row) => row.id === hidden.id))!;
    listId = section.collapseId;
    if (!expanded) {
      const more: SidebarRow = { ...hidden, id: `more:${listId}`, kind: "more", title: "… 1 more", selection: null };
      section.rows = [...section.rows.filter((row) => row.id !== hidden.id), more];
    }
    await route.fulfill({ response, json: tree });
  });
  await page.route("**/api/choices", async (route) => {
    const body = route.request().postDataJSON();
    if (body.op !== "expand_list") return route.continue();
    expect(body.id).toBe(listId);
    expanded = true;
    await route.continue();
  });
  await load(page);
  const sidebar = page.getByTestId("sidebar");
  const hidden = sidebar.locator(`[data-session="${session}"]`);
  await expect(hidden).toHaveCount(0);
  await page.getByRole("button", { name: "Command palette" }).click();
  await expect(page.locator(`[cmdk-item][data-value="chat:${session}"]`)).toBeVisible();
  await page.keyboard.press("Escape");
  const more = sidebar.getByRole("treeitem").filter({ hasText: "… 1 more" });
  const rows = sidebar.getByRole("treeitem");
  await rows.first().focus();
  await page.keyboard.press("Home");
  const index = await rows.evaluateAll((els) => els.findIndex((el) => el.getAttribute("data-kind") === "more"));
  expect(index).toBeGreaterThan(0);
  for (let i = 0; i < index; i++) await page.keyboard.press("ArrowDown");
  await expect(more).toBeFocused();
  const written = page.waitForResponse((r) => r.url().endsWith("/api/choices") && r.request().postDataJSON().op === "expand_list");
  await page.keyboard.press("Enter");
  expect((await written).ok()).toBe(true);
  await expect(hidden).toBeVisible();
  await expect(more).toHaveCount(0);
  await page.getByTestId("archive-toggle").click();
  await expect(sidebar).toHaveAttribute("aria-label", "Archived workspaces");
  await page.getByRole("button", { name: "Archived chats…" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
});

test("a PR chip is a keyboard link and the open chat alone shows its branch line", async ({ page }) => {
  const session = process.env.SWARM_E2E_GIT!;
  const url = "https://github.com/example/project/pull/7";
  await page.route("**/api/tree?view=workspaces", async (route) => {
    const response = await route.fetch();
    const tree: TreeView = await response.json();
    const path = tree.chats.find((row) => row.selection?.chat === session)?.selection?.workspace;
    if (path) Object.assign(tree.workspaces[path], {
      branch: "feature", pr: { number: 7, url, state: "open", ci: "passed" }, diff: { addedLines: 31, removedLines: 4 },
    });
    await route.fulfill({ response, json: tree });
  });
  await page.context().route(url, (route) => route.fulfill({ body: "Pull request" }));
  await load(page);
  await page.getByTestId("home-button").click();
  const sidebar = page.getByTestId("sidebar");
  const row = sidebar.locator(`[data-session="${session}"]`);
  const link = row.getByRole("link", { name: "PR #7, open, CI passed" });
  await expect(row.getByTestId("sidebar-branch")).toHaveCount(0);
  await expect(link).toHaveAttribute("href", url);
  await link.focus();
  const [popup] = await Promise.all([page.waitForEvent("popup"), link.press("Enter")]);
  await expect(popup).toHaveURL(url);
  await popup.close();
  await expect(row).not.toHaveAttribute("aria-current", "true");
  await row.locator(".chat-title").click();
  await expect(row).toHaveAttribute("aria-current", "true");
  const branch = sidebar.getByTestId("sidebar-branch");
  await expect(branch).toHaveCount(1);
  await expect(branch).toHaveText("feature+31−4");
  const strip = page.getByTestId("tab-strip");
  const tab = strip.getByRole("tab", { selected: true });
  const bar = page.getByTestId("branch-bar");
  await expect(bar).toContainText("feature");
  await expect(bar).toContainText("+31−4");
  await expect(bar.getByRole("link", { name: "PR #7, open, CI passed" })).toHaveAttribute("href", url);
  const [stripBox, tabBox, barBox] = await Promise.all([strip, tab, bar].map((item) => item.boundingBox()));
  expect(stripBox?.height).toBe(40);
  expect(tabBox).toMatchObject({ width: 224, height: 32 });
  expect(barBox?.height).toBe(36);
  for (const colorScheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme });
    await page.mouse.move(700, 100);
    await page.screenshot({ path: resolve(import.meta.dirname, `../../tmp/shots/U5/chrome-${colorScheme}.png`) });
  }
});

test("the rail replaces the sidebar only below 1024 px and keeps desktop collapse", async ({ page }) => {
  await load(page);
  await page.getByTestId("home-button").click();
  const sidebar = page.getByTestId("sidebar");
  const sidePanel = page.getByTestId("side-panel");
  const rail = page.getByTestId("sidebar-rail");
  const width = async () => Math.round((await sidePanel.boundingBox())?.width ?? -1);
  await expect.poll(width).toBe(264);
  await expect(sidebar).toBeVisible();
  await expect(rail).toHaveCount(0);
  const handle = page.locator('[data-side] > [data-slot="resizable-handle"]');
  await handle.press("ArrowRight");
  await expect.poll(width).toBeGreaterThan(264);
  const saved = await width();
  await page.setViewportSize({ width: 1023, height: 900 });
  await expect(rail).toBeVisible();
  await page.setViewportSize({ width: 1024, height: 900 });
  await expect.poll(width).toBe(saved);
  await page.keyboard.press("Meta+b");
  await expect.poll(async () => (await sidePanel.boundingBox())?.width).toBe(0);

  await page.setViewportSize({ width: 1023, height: 900 });
  await expect(rail).toBeVisible();
  expect((await rail.boundingBox())?.width).toBe(52);
  await expect(sidebar).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(1023);
  const main = (await page.getByTestId("main-panel").elementHandle())!;
  const live = rail.locator(`[data-session="${process.env.SWARM_E2E_GIT}"]`);
  await live.click();
  await expect(live).toHaveAttribute("aria-current", "true");
  await rail.getByRole("button", { name: "New chat" }).click();
  await expect(page.getByTestId("new-chat-prompt")).toBeFocused();
  await page.keyboard.press("Escape");
  for (const colorScheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme });
    await page.screenshot({ path: resolve(import.meta.dirname, `../../tmp/shots/U6/rail-${colorScheme}.png`) });
  }

  await page.setViewportSize({ width: 1024, height: 900 });
  await expect(rail).toHaveCount(0);
  expect(await main.evaluate((el) => el.isConnected)).toBe(true);
  await expect.poll(async () => (await sidePanel.boundingBox())?.width).toBe(0);
  await page.keyboard.press("Meta+b");
  await expect(sidebar).toBeVisible();
  await expect(sidebar.locator(`[data-session="${process.env.SWARM_E2E_GIT}"]`)).toHaveAttribute("aria-current", "true");
});
