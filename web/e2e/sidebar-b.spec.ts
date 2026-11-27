import { expect, test, type Page } from "@playwright/test";
import type { SidebarRow, TreeView } from "../src/core/nav";
import { clickTarget, readTree, shoot } from "./lifecycle-helpers";

async function load(page: Page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await page.goto(`${url.origin}/${url.hash}`);
  await page.waitForFunction(() => window.__swarmReady === true);
}

test("head B puts Home and project controls above New chat and palette search", async ({ page, browserName }) => {
  await load(page);
  const head = page.getByTestId("sidebar-head");
  const home = head.getByTestId("home-button");
  await expect(home).toHaveAccessibleName("Swarm");
  const add = head.getByRole("button", { name: "Add project" });
  const settings = head.getByRole("button", { name: "Settings" });
  const chat = head.getByRole("button", { name: "New chat", exact: true });
  const search = head.getByRole("button", { name: "Search", exact: true });
  await expect(home).toHaveText("Swarm");

  const boxes = await Promise.all([home, add, settings, chat, search].map((control) => control.boundingBox()));
  const [homeBox, addBox, settingsBox, chatBox, searchBox] = boxes.map((box) => box!);
  expect(addBox.height).toBe(24);
  expect(settingsBox.height).toBe(24);
  expect(homeBox.y + homeBox.height).toBeLessThan(chatBox.y);
  expect(chatBox.y + chatBox.height).toBeLessThan(searchBox.y);
  expect(searchBox.height).toBe(32);

  await shoot(page, "sidebar-head", browserName);

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
  await page.getByRole("button", { name: "Search", exact: true }).click();
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

test("a PR chip is a keyboard link and the open chat alone shows its branch line", async ({ page, browserName }) => {
  const session = process.env.SWARM_E2E_GIT!;
  const url = "https://github.com/example/project/pull/7";

  await page.route("**/api/tree?view=workspaces", async (route) => {
    const response = await route.fetch();
    const tree: TreeView = await response.json();
    const path = tree.chats.find((row) => row.selection?.chat === session)?.selection?.workspace;
    if (path) Object.assign(tree.workspaces[path], {
      branch: "feature", pr: { number: 7, url, state: "draft", ci: "passed" }, diff: { addedLines: 31, removedLines: 1 },
    });
    await route.fulfill({ response, json: tree });
  });
  await page.context().route(url, (route) => route.fulfill({ body: "Pull request" }));

  await load(page);
  await page.getByTestId("home-button").click();
  const sidebar = page.getByTestId("sidebar");
  const row = sidebar.locator(`[data-session="${session}"]`);
  const link = row.getByTestId("pr-chip");
  await expect(link).toHaveAccessibleName("PR #7, draft, CI passed");
  await expect(row.getByTestId("sidebar-branch")).toHaveCount(0);
  await expect(link).toHaveAttribute("href", url);
  expect(await link.evaluate((element) => element.tabIndex)).toBe(-1);
  await link.focus();
  const [popup] = await Promise.all([page.waitForEvent("popup"), link.press("Enter")]);
  await expect(popup).toHaveURL(url);
  await popup.close();
  await expect(row).not.toHaveAttribute("aria-current", "true");

  await row.locator(".chat-title").click();
  await expect(row).toHaveAttribute("aria-current", "true");
  const branch = sidebar.getByTestId("sidebar-branch");
  await expect(branch).toHaveCount(1);
  await expect(branch).toHaveText("feature+31−1");
  await expect(branch.getByLabel("31 lines added, 1 line removed")).toBeVisible();

  const strip = page.getByTestId("tab-strip");
  const tab = strip.getByRole("tab", { selected: true });
  const bar = page.getByTestId("branch-bar");
  await expect(bar).toContainText("feature");
  await expect(bar).toContainText("+31−1");
  const barLink = bar.getByRole("link", { name: "PR #7, draft, CI passed" });
  await expect(barLink).toHaveAttribute("href", url);
  expect(await barLink.evaluate((element) => element.tabIndex)).toBe(0);
  const [stripBox, tabBox, barBox] = await Promise.all([strip, tab, bar].map((item) => item.boundingBox()));
  expect(stripBox?.height).toBe(40);
  expect(tabBox).toMatchObject({ width: 224, height: 32 });
  expect(barBox?.height).toBe(36);

  await page.mouse.move(700, 100);
  await shoot(page, "sidebar-chrome", browserName);
});

test("the menu on a waiting chat in a shared worktree has only chat actions", async ({ page }) => {
  await page.route("**/api/tree?view=workspaces", async (route) => {
    const response = await route.fetch();
    const tree: TreeView = await response.json();
    const waiting = tree.chats.find((r) => r.selection?.workspace?.endsWith("/ws-tabs"))!;
    tree.sections = [
      { id: "needs-you", kind: "needs-you", path: null, folder: false, expanded: true, collapseId: "needs-you",
        title: "Needs you", status: "blocked", fields: [], rows: [{ ...waiting, parentId: null, status: "blocked" }] },
    ];
    await route.fulfill({ response, json: tree });
  });

  await load(page);
  const row = page.getByTestId("sidebar").locator('[data-kind="chat"]');
  await row.click({ button: "right" });
  const menu = page.getByTestId("row-menu");
  await expect(menu.getByRole("menuitem", { name: "Archive chat", exact: true })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "Archive workspace", exact: true })).toHaveCount(0);
  await page.unrouteAll({ behavior: "wait" });
});

test("Prune names the project for a pinned worktree", async ({ page }) => {
  const session = process.env.SWARM_E2E_GIT!;
  let project = "";
  await page.route("**/api/tree?view=workspaces", async (route) => {
    const response = await route.fetch();
    const tree: TreeView = await response.json();
    const chat = tree.chats.find((r) => r.selection?.chat === session)!;
    const facts = tree.workspaces[chat.selection!.workspace!];
    project = tree.projects.find((p) => p.path === facts.projectPath)!.title;
    facts.missing = true;
    tree.sections = [
      { id: "pinned", kind: "pinned", path: null, folder: false, expanded: true, collapseId: "pinned",
        title: "Pinned", status: null, fields: [], rows: [{ ...chat, pinned: true, parentId: null }] },
    ];
    await route.fulfill({ response, json: tree });
  });
  await page.route("**/api/workspace/impact?*", (route) => route.fulfill({ json: { prunePaths: ["/missing"] } }));

  await load(page);
  const row = page.getByTestId("sidebar").locator(`[data-session="${session}"]`);
  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Prune worktree…" }).click();
  await expect(page.getByTestId("confirm")).toContainText(`missing folders in ${project}:`);
  await page.unrouteAll({ behavior: "wait" });
});

async function withChatFacts(page: Page, change: (tree: TreeView, row: SidebarRow) => void) {
  const session = process.env.SWARM_E2E_GIT!;
  await page.route("**/api/tree?view=workspaces", async (route) => {
    const response = await route.fetch();
    const tree: TreeView = await response.json();
    const row = tree.sections.flatMap((section) => section.rows).find((item) => item.selection?.chat === session)!;
    change(tree, row);
    await route.fulfill({ response, json: tree });
  });
  await load(page);
  return page.getByTestId("sidebar").locator(`[data-session="${session}"]`);
}

const demoPr = { number: 7, url: "https://github.com/example/project/pull/7", state: "open", ci: "unknown" } as const;

test("the archive row has a 24 px Unarchive target without growing", async ({ page }) => {
  const tree = await readTree();
  const section = tree.sections.find((item) => item.rows.some((row) => row.kind === "chat"));
  const chat = section?.rows.find((row) => row.kind === "chat");
  if (!section || !chat) throw new Error("Missing a chat for the archive fixture");
  const archived: SidebarRow = { ...chat, id: "archived-chat:fixture", kind: "archived-chat", depth: 0, parentId: null, fields: [] };

  try {
    await page.route("**/api/tree?view=archived", (route) => route.fulfill({
      json: { ...tree, view: "archived", sections: [{ ...section, rows: [archived] }] },
    }));
    await load(page);
    await page.getByTestId("archive-toggle").click();
    const row = page.locator('[data-id="archived-chat:fixture"]');
    const button = row.getByRole("button", { name: /^Unarchive / });
    await expect(button).toBeVisible();
    await expect(button).toHaveCSS("height", "18px");
    await expect(row).toHaveCSS("height", "28px");
    expect(await clickTarget(button)).toEqual({ width: 24, height: 24 });
  } finally {
    await page.unrouteAll({ behavior: "ignoreErrors" });
  }
});

test("right click leaves the PR chip's native link menu available", async ({ page }) => {
  const row = await withChatFacts(page, (tree, row) => { tree.workspaces[row.selection!.workspace!].pr = demoPr; });
  const chip = row.getByTestId("pr-chip");
  expect(await chip.evaluate((link) => link.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 })))).toBe(true);
  await expect(page.getByTestId("row-menu")).toHaveCount(0);
  await page.unrouteAll({ behavior: "wait" });
});

test("a branchless chat still reserves the same 36 px below the tabs", async ({ page }) => {
  const row = await withChatFacts(page, (tree, row) => { tree.workspaces[row.selection!.workspace!].branch = null; });
  await row.locator(".chat-title").click();
  const strip = page.getByTestId("branch-bar");
  await expect(strip).toHaveCSS("height", "36px");
  await expect(strip).toHaveAttribute("aria-hidden", "true");
  await page.unrouteAll({ behavior: "wait" });
});

test("the PR chip responds to a click within its 24 px hit area", async ({ page }) => {
  try {
    const row = await withChatFacts(page, (tree, row) => {
      const path = row.selection?.workspace;
      if (!path || !tree.workspaces[path]) throw new Error("PR fixture chat has no workspace");
      tree.workspaces[path].pr = demoPr;
    });
    const chip = row.getByTestId("pr-chip");
    await expect(chip).toBeVisible();
    expect(await chip.evaluate((el) => getComputedStyle(el, "::after").height)).toBe("24px");
  } finally {
    await page.unrouteAll({ behavior: "ignoreErrors" });
  }
});

test("the chat agent count has a 24 px click area", async ({ page }) => {
  const row = await withChatFacts(page, (_tree, row) => { row.childrenSummary = "1"; });
  const count = row.locator("button.sidebar-count");
  const box = (await count.boundingBox())!;
  expect(box.height).toBeGreaterThanOrEqual(24);
  expect(box.width).toBeGreaterThanOrEqual(24);
  await page.unrouteAll({ behavior: "wait" });
});

test("the rail caps the chat buttons and opens Search for one more chat", async ({ page }) => {
  try {
    await withChatFacts(page, (tree, row) => {
      if (!row.selection) throw new Error("Rail fixture chat has no selection");
      const selection = row.selection;
      tree.chats = Array.from({ length: 9 }, (_, i): SidebarRow => ({
        ...row, id: `chat:rail-${i}`, title: `Rail chat ${i}`, selection: { ...selection, chat: `rail-${i}` },
      }));
    });

    await page.setViewportSize({ width: 900, height: 900 });
    const rail = page.getByTestId("sidebar-rail");
    await expect(rail.getByTestId("rail-chat")).toHaveCount(8);
    const more = rail.getByRole("button", { name: /^Search / });
    await expect(more).toHaveAccessibleName("Search 1 more chat");
    await more.click();
    await expect(page.getByRole("combobox")).toBeFocused();
  } finally {
    await page.unrouteAll({ behavior: "ignoreErrors" });
  }
});

test("the rail shows initial loading and a failed workspace list", async ({ page }) => {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  let release!: () => void;
  const pending = new Promise<void>((resolve) => { release = resolve; });
  await page.route("**/api/tree?view=workspaces", async (route) => {
    await pending;
    await route.fulfill({ status: 503, json: { error: "Unavailable" } });
  });

  await page.setViewportSize({ width: 900, height: 900 });
  await page.goto(`${url.origin}/${url.hash}`);
  const status = page.getByTestId("sidebar-rail").getByRole("status");
  try {
    await expect(status).toHaveText("Loading workspaces…");
  } finally {
    release();
  }
  await expect(status).toHaveText("Could not load the workspaces");
  await page.unrouteAll({ behavior: "wait" });
});

test("the rail replaces the sidebar only below 1024 px and keeps desktop collapse", async ({ page, browserName }) => {
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

  await shoot(page, "sidebar-rail", browserName);

  await page.setViewportSize({ width: 1024, height: 900 });
  await expect(rail).toHaveCount(0);
  expect(await main.evaluate((el) => el.isConnected)).toBe(true);
  await expect.poll(async () => (await sidePanel.boundingBox())?.width).toBe(0);
  await page.keyboard.press("Meta+b");
  await expect(sidebar).toBeVisible();
  await expect(sidebar.locator(`[data-session="${process.env.SWARM_E2E_GIT}"]`)).toHaveAttribute("aria-current", "true");
});
